//! Runs Claude Code (design §9.1): one `claude -p` process per message, the
//! prompt on stdin, its stream-json output turned into [`BuddyEvent`]s, and
//! a cancel that stops the whole process group.
//!
//! Blocking by design: [`ClaudeRunner::run`] holds the calling thread until
//! the run ends, so call it from `spawn_blocking` / a worker thread. Several
//! helper threads (stdin writer, stdout reader, stderr reader, process
//! waiter) keep a hung or misbehaving `claude` from ever wedging the caller.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::platform::{ProcessCtl, Signal};
use crate::stop::{Stoppable, capture_stdout, stop_gracefully};
use crate::stream_parser::{BuddyEvent, parse_line};

/// The MCP tool Claude asks before every action (`--permission-prompt-tool`).
/// The server name `buddy` matches the one in the `--mcp-config` JSON.
pub const PERMISSION_TOOL: &str = "mcp__buddy__approve";

/// After SIGINT, then SIGTERM, this long each before the next signal.
pub const STOP_GRACE: Duration = Duration::from_secs(3);
/// How long `claude auth status` may take.
const PREFLIGHT_TIMEOUT: Duration = Duration::from_secs(15);
/// After `claude` exits, how long to keep waiting for its output pipe to
/// close (something it started can hold the pipe open).
const OUTPUT_CLOSE_WAIT: Duration = Duration::from_secs(2);
/// How much of `claude`'s stderr to keep for error messages.
const STDERR_KEEP_BYTES: usize = 4000;
/// Where `claude` is usually installed, tried after PATH.
const COMMON_INSTALL_DIRS: [&str; 2] = ["/opt/homebrew/bin", "/usr/local/bin"];

// ------------------------------------------------------------- arguments

/// How much Claude may do without asking (design §7.1). There is no variant
/// for `bypassPermissions`: the buddy cannot ask for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    /// "Ask first".
    Default,
    AcceptEdits,
    Plan,
    Auto,
}

impl PermissionMode {
    fn flag(self) -> &'static str {
        match self {
            PermissionMode::Default => "default",
            PermissionMode::AcceptEdits => "acceptEdits",
            PermissionMode::Plan => "plan",
            PermissionMode::Auto => "auto",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Resume this conversation (`--resume`).
    pub session_id: Option<String>,
    pub mode: PermissionMode,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub add_dirs: Vec<PathBuf>,
    /// The `--mcp-config` JSON naming the buddy permission server. Required,
    /// so a run can never start without approvals.
    pub mcp_config: String,
    pub persona_path: Option<PathBuf>,
}

/// The command-line arguments for one run. Pure.
///
/// Always asks the permission tool, loads only the buddy's own MCP server
/// (`--strict-mcp-config`, so the user's other MCP servers don't leak in),
/// and never uses `--bare` (which ignores the subscription login) or any
/// way around permissions.
pub fn build_args(options: &RunOptions) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--permission-mode",
        options.mode.flag(),
        "--strict-mcp-config",
        "--mcp-config",
        &options.mcp_config,
        "--permission-prompt-tool",
        PERMISSION_TOOL,
    ]
    .map(String::from)
    .to_vec();

    let mut flag_with = |flag: &str, value: String| {
        args.push(flag.to_string());
        args.push(value);
    };
    if let Some(id) = &options.session_id {
        flag_with("--resume", id.clone());
    }
    if let Some(model) = &options.model {
        flag_with("--model", model.clone());
    }
    if let Some(effort) = &options.effort {
        flag_with("--effort", effort.clone());
    }
    for dir in &options.add_dirs {
        flag_with("--add-dir", dir.to_string_lossy().into_owned());
    }
    if let Some(persona) = &options.persona_path {
        flag_with(
            "--append-system-prompt-file",
            persona.to_string_lossy().into_owned(),
        );
    }
    args
}

// ------------------------------------------------------ finding `claude`

fn is_executable_file(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// Finds the `claude` executable: each PATH entry, then `~/.local/bin` and
/// the usual Homebrew locations. On failure returns every place it looked,
/// for the "where I looked" message (design §13).
pub fn locate_claude(path_env: &str, home: &Path) -> Result<PathBuf, Vec<PathBuf>> {
    let mut dirs: Vec<PathBuf> = path_env
        .split(':')
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .collect();
    dirs.push(home.join(".local/bin"));
    dirs.extend(COMMON_INSTALL_DIRS.iter().map(PathBuf::from));

    let candidates: Vec<PathBuf> = dirs.into_iter().map(|dir| dir.join("claude")).collect();
    match candidates.iter().find(|path| is_executable_file(path)) {
        Some(found) => Ok(found.clone()),
        None => Err(candidates),
    }
}

// ------------------------------------------------------------ preflight

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreflightError {
    /// Claude Code answered that nobody is logged in.
    NotLoggedIn,
    /// It could not be run or gave an answer we can't read.
    Unavailable,
}

/// `loggedIn` from `claude auth status` output.
pub fn parse_auth_status(stdout: &str) -> Option<bool> {
    serde_json::from_str::<serde_json::Value>(stdout)
        .ok()?
        .get("loggedIn")?
        .as_bool()
}

/// Runs `claude auth status` (design §9.1 "Preflight on start").
pub fn check_login(claude_bin: &Path, path_env: &str) -> Result<(), PreflightError> {
    check_login_within(claude_bin, path_env, PREFLIGHT_TIMEOUT)
}

fn check_login_within(
    claude_bin: &Path,
    path_env: &str,
    timeout: Duration,
) -> Result<(), PreflightError> {
    let mut command = Command::new(claude_bin);
    command.args(["auth", "status"]).env("PATH", path_env);
    let captured = capture_stdout(command, timeout).ok_or(PreflightError::Unavailable)?;
    match parse_auth_status(&captured.stdout) {
        Some(true) => Ok(()),
        Some(false) => Err(PreflightError::NotLoggedIn),
        None => Err(PreflightError::Unavailable),
    }
}

// --------------------------------------------------------------- runner

#[derive(Debug, Clone)]
pub struct RunRequest {
    pub claude_bin: PathBuf,
    pub path_env: String,
    /// The working directory (`~/BuddyWorkspace`).
    pub workspace: PathBuf,
    /// Sent on stdin: no length limit, no quoting problems.
    pub prompt: String,
    pub options: RunOptions,
}

#[derive(Debug)]
pub enum RunError {
    /// Only one run at a time (design §9.1).
    Busy,
    Spawn(io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutcome {
    pub exit_code: Option<i32>,
    pub cancelled: bool,
}

struct ActiveRun {
    pgid: u32,
    exited: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
}

/// The running process group, as something [`stop_gracefully`] can stop.
struct RunningGroup<P> {
    process: Arc<P>,
    pgid: u32,
    exited: Arc<AtomicBool>,
}

impl<P: ProcessCtl> Stoppable for RunningGroup<P> {
    fn send(&self, signal: Signal) {
        self.process.signal_group(self.pgid, signal);
    }

    fn has_exited(&self) -> bool {
        self.exited.load(Ordering::SeqCst)
    }
}

enum Message {
    Line(String),
    StdoutClosed,
    Exited(Option<i32>),
}

pub struct ClaudeRunner<P> {
    process: Arc<P>,
    grace: Duration,
    active: Mutex<Option<ActiveRun>>,
}

impl<P: ProcessCtl + Send + Sync + 'static> ClaudeRunner<P> {
    pub fn new(process: Arc<P>, grace: Duration) -> Self {
        ClaudeRunner {
            process,
            grace,
            active: Mutex::new(None),
        }
    }

    pub fn is_running(&self) -> bool {
        self.active.lock().unwrap().is_some()
    }

    /// Runs one message to completion, calling `on_event` for each event in
    /// order. The last event is always `Finished`: if `claude` dies or is
    /// cancelled without sending a result, one is made up with the reason.
    pub fn run(
        &self,
        request: RunRequest,
        mut on_event: impl FnMut(BuddyEvent),
    ) -> Result<RunOutcome, RunError> {
        let exited = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut child = self.start(&request, &exited, &cancelled)?;

        let (tx, rx) = mpsc::channel();
        if let Some(stdin) = child.stdin.take() {
            spawn_stdin_writer(stdin, request.prompt);
        }
        let stderr_tail = Arc::new(Mutex::new(Vec::new()));
        if let Some(stderr) = child.stderr.take() {
            spawn_stderr_reader(stderr, Arc::clone(&stderr_tail));
        }
        if let Some(stdout) = child.stdout.take() {
            spawn_stdout_reader(stdout, tx.clone());
        }
        spawn_waiter(child, Arc::clone(&exited), tx);

        let exit_code = pump_events(&rx, &mut on_event);
        *self.active.lock().unwrap() = None;

        let was_cancelled = cancelled.load(Ordering::SeqCst);
        if !exit_code.saw_finished {
            let tail = String::from_utf8_lossy(&stderr_tail.lock().unwrap()).into_owned();
            on_event(made_up_finish(was_cancelled, exit_code.code, &tail));
        }
        Ok(RunOutcome {
            exit_code: exit_code.code,
            cancelled: was_cancelled,
        })
    }

    /// Starts stopping the current run (SIGINT, then SIGTERM, then SIGKILL,
    /// each after the grace period) and returns at once. False when nothing
    /// is running.
    pub fn cancel(&self) -> bool {
        let guard = self.active.lock().unwrap();
        let Some(run) = guard.as_ref() else {
            return false;
        };
        run.cancelled.store(true, Ordering::SeqCst);
        let group = RunningGroup {
            process: Arc::clone(&self.process),
            pgid: run.pgid,
            exited: Arc::clone(&run.exited),
        };
        let grace = self.grace;
        thread::spawn(move || {
            stop_gracefully(&group, grace);
        });
        true
    }

    fn start(
        &self,
        request: &RunRequest,
        exited: &Arc<AtomicBool>,
        cancelled: &Arc<AtomicBool>,
    ) -> Result<std::process::Child, RunError> {
        let mut active = self.active.lock().unwrap();
        if active.is_some() {
            return Err(RunError::Busy);
        }
        let mut command = Command::new(&request.claude_bin);
        command
            .args(build_args(&request.options))
            .current_dir(&request.workspace)
            .env("PATH", &request.path_env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = self.process.spawn_group(command).map_err(RunError::Spawn)?;
        *active = Some(ActiveRun {
            pgid: child.id(),
            exited: Arc::clone(exited),
            cancelled: Arc::clone(cancelled),
        });
        Ok(child)
    }
}

fn spawn_stdin_writer(mut stdin: std::process::ChildStdin, prompt: String) {
    thread::spawn(move || {
        // A failure here means claude already exited; the run reports why.
        let _ = stdin.write_all(prompt.as_bytes());
    });
}

fn spawn_stderr_reader(mut stderr: std::process::ChildStderr, tail: Arc<Mutex<Vec<u8>>>) {
    thread::spawn(move || {
        let mut chunk = [0u8; 2048];
        while let Ok(read) = stderr.read(&mut chunk) {
            if read == 0 {
                break;
            }
            let mut kept = tail.lock().unwrap();
            kept.extend_from_slice(&chunk[..read]);
            if kept.len() > STDERR_KEEP_BYTES {
                let excess = kept.len() - STDERR_KEEP_BYTES;
                kept.drain(..excess);
            }
        }
    });
}

fn spawn_stdout_reader(stdout: std::process::ChildStdout, tx: mpsc::Sender<Message>) {
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            match reader.read_until(b'\n', &mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buffer).into_owned();
                    if tx.send(Message::Line(line)).is_err() {
                        return;
                    }
                }
            }
        }
        let _ = tx.send(Message::StdoutClosed);
    });
}

fn spawn_waiter(
    mut child: std::process::Child,
    exited: Arc<AtomicBool>,
    tx: mpsc::Sender<Message>,
) {
    thread::spawn(move || {
        let code = child.wait().ok().and_then(|status| status.code());
        exited.store(true, Ordering::SeqCst);
        let _ = tx.send(Message::Exited(code));
    });
}

struct PumpResult {
    code: Option<i32>,
    saw_finished: bool,
}

/// Forwards events until the process has exited and its output has closed
/// (or, if something keeps the pipe open, [`OUTPUT_CLOSE_WAIT`] after exit).
fn pump_events(rx: &mpsc::Receiver<Message>, on_event: &mut impl FnMut(BuddyEvent)) -> PumpResult {
    let mut result = PumpResult {
        code: None,
        saw_finished: false,
    };
    let mut exited = false;
    let mut stdout_closed = false;
    while !(exited && stdout_closed) {
        let message = if exited {
            match rx.recv_timeout(OUTPUT_CLOSE_WAIT) {
                Ok(message) => message,
                Err(_) => break,
            }
        } else {
            match rx.recv() {
                Ok(message) => message,
                Err(_) => break,
            }
        };
        match message {
            Message::Line(line) => {
                for event in parse_line(&line) {
                    result.saw_finished |= matches!(event, BuddyEvent::Finished { .. });
                    on_event(event);
                }
            }
            Message::StdoutClosed => stdout_closed = true,
            Message::Exited(code) => {
                exited = true;
                result.code = code;
            }
        }
    }
    result
}

fn made_up_finish(cancelled: bool, code: Option<i32>, stderr: &str) -> BuddyEvent {
    let text = if cancelled {
        "Cancelled.".to_string()
    } else if !stderr.trim().is_empty() {
        stderr.trim().to_string()
    } else {
        match code {
            Some(code) => format!("Claude stopped unexpectedly (exit code {code})."),
            None => "Claude stopped unexpectedly.".to_string(),
        }
    };
    BuddyEvent::Finished {
        text,
        is_error: true,
        cost_usd: None,
        context_tokens: None,
        context_window: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::macos::MacProcessCtl;
    use std::fs;
    use std::time::Instant;

    const TEXT_RUN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/text_run.jsonl");

    fn options() -> RunOptions {
        RunOptions {
            session_id: None,
            mode: PermissionMode::Default,
            model: None,
            effort: None,
            add_dirs: Vec::new(),
            mcp_config: r#"{"mcpServers":{}}"#.to_string(),
            persona_path: None,
        }
    }

    fn has_pair(args: &[String], flag: &str, value: &str) -> bool {
        args.windows(2)
            .any(|pair| pair[0] == flag && pair[1] == value)
    }

    // ---- build_args

    #[test]
    fn the_base_arguments_stream_json_and_ask_the_buddy_for_permission() {
        let args = build_args(&options());

        assert_eq!(args[0], "-p");
        assert!(has_pair(&args, "--output-format", "stream-json"));
        assert!(args.contains(&"--verbose".to_string()));
        assert!(args.contains(&"--include-partial-messages".to_string()));
        assert!(has_pair(&args, "--permission-prompt-tool", PERMISSION_TOOL));
        assert!(has_pair(&args, "--mcp-config", r#"{"mcpServers":{}}"#));
        assert!(args.contains(&"--strict-mcp-config".to_string()));
    }

    #[test]
    fn every_mode_maps_to_its_claude_flag() {
        for (mode, flag) in [
            (PermissionMode::Default, "default"),
            (PermissionMode::AcceptEdits, "acceptEdits"),
            (PermissionMode::Plan, "plan"),
            (PermissionMode::Auto, "auto"),
        ] {
            let args = build_args(&RunOptions { mode, ..options() });

            assert!(has_pair(&args, "--permission-mode", flag), "{mode:?}");
        }
    }

    #[test]
    fn it_never_uses_bare_or_a_way_around_permissions() {
        let everything = RunOptions {
            session_id: Some("s".into()),
            model: Some("opus".into()),
            effort: Some("high".into()),
            add_dirs: vec!["/a".into()],
            persona_path: Some("/p.md".into()),
            ..options()
        };

        for mode in [
            PermissionMode::Default,
            PermissionMode::AcceptEdits,
            PermissionMode::Plan,
            PermissionMode::Auto,
        ] {
            let args = build_args(&RunOptions {
                mode,
                ..everything.clone()
            });

            for forbidden in [
                "--bare",
                "--dangerously-skip-permissions",
                "bypassPermissions",
            ] {
                assert!(
                    !args.iter().any(|arg| arg.contains(forbidden)),
                    "{forbidden} must never appear ({mode:?})"
                );
            }
            assert!(has_pair(&args, "--permission-prompt-tool", PERMISSION_TOOL));
        }
    }

    #[test]
    fn optional_flags_appear_only_when_set() {
        let plain = build_args(&options());
        for flag in [
            "--resume",
            "--model",
            "--effort",
            "--add-dir",
            "--append-system-prompt-file",
        ] {
            assert!(
                !plain.contains(&flag.to_string()),
                "{flag} should be absent"
            );
        }

        let full = build_args(&RunOptions {
            session_id: Some("abc".into()),
            model: Some("opusplan".into()),
            effort: Some("high".into()),
            persona_path: Some("/app/persona.md".into()),
            ..options()
        });

        assert!(has_pair(&full, "--resume", "abc"));
        assert!(has_pair(&full, "--model", "opusplan"));
        assert!(has_pair(&full, "--effort", "high"));
        assert!(has_pair(
            &full,
            "--append-system-prompt-file",
            "/app/persona.md"
        ));
    }

    #[test]
    fn each_extra_folder_gets_its_own_add_dir_flag() {
        let args = build_args(&RunOptions {
            add_dirs: vec!["/a".into(), "/b c".into()],
            ..options()
        });

        assert!(has_pair(&args, "--add-dir", "/a"));
        assert!(has_pair(&args, "--add-dir", "/b c"));
    }

    // ---- locate_claude

    fn executable(path: &Path) {
        fs::write(path, "#!/bin/sh\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn locate_finds_claude_on_the_path() {
        let dir = tempfile::tempdir().unwrap();
        executable(&dir.path().join("claude"));

        let found = locate_claude(
            &format!("/nowhere:{}", dir.path().display()),
            Path::new("/h"),
        );

        assert_eq!(found, Ok(dir.path().join("claude")));
    }

    #[test]
    fn locate_prefers_the_earlier_path_entry() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        executable(&first.path().join("claude"));
        executable(&second.path().join("claude"));
        let path = format!("{}:{}", first.path().display(), second.path().display());

        assert_eq!(
            locate_claude(&path, Path::new("/h")),
            Ok(first.path().join("claude"))
        );
    }

    #[test]
    fn locate_falls_back_to_the_local_bin_in_the_home_folder() {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(home.path().join(".local/bin")).unwrap();
        executable(&home.path().join(".local/bin/claude"));

        let found = locate_claude("/usr/bin:/bin", home.path());

        assert_eq!(found, Ok(home.path().join(".local/bin/claude")));
    }

    #[test]
    fn locate_skips_files_that_are_not_executable() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("claude"), "not executable").unwrap();

        let result = locate_claude(&dir.path().to_string_lossy(), Path::new("/no-home"));

        assert!(result.is_err());
    }

    #[test]
    fn locate_reports_every_place_it_looked_when_not_found() {
        let searched = locate_claude("/aaa:/bbb", Path::new("/home/me")).unwrap_err();

        assert!(searched.contains(&PathBuf::from("/aaa/claude")));
        assert!(searched.contains(&PathBuf::from("/bbb/claude")));
        assert!(searched.contains(&PathBuf::from("/home/me/.local/bin/claude")));
        assert!(searched.contains(&PathBuf::from("/opt/homebrew/bin/claude")));
    }

    // ---- preflight

    #[test]
    fn auth_status_is_read_from_the_logged_in_field() {
        assert_eq!(
            parse_auth_status(r#"{"loggedIn": true, "authMethod": "claude.ai"}"#),
            Some(true)
        );
        assert_eq!(parse_auth_status(r#"{"loggedIn": false}"#), Some(false));
        assert_eq!(parse_auth_status("not json"), None);
        assert_eq!(parse_auth_status(r#"{"other": 1}"#), None);
    }

    fn fake_claude(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("claude");
        executable(&path);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        path
    }

    #[test]
    fn preflight_passes_when_logged_in() {
        let dir = tempfile::tempdir().unwrap();
        let bin = fake_claude(dir.path(), r#"echo '{"loggedIn": true}'"#);

        assert_eq!(
            check_login_within(&bin, "/usr/bin:/bin", Duration::from_secs(5)),
            Ok(())
        );
    }

    #[test]
    fn preflight_reports_a_logged_out_user_even_with_a_failing_exit_code() {
        let dir = tempfile::tempdir().unwrap();
        let bin = fake_claude(dir.path(), r#"echo '{"loggedIn": false}'; exit 1"#);

        assert_eq!(
            check_login_within(&bin, "/usr/bin:/bin", Duration::from_secs(5)),
            Err(PreflightError::NotLoggedIn)
        );
    }

    #[test]
    fn preflight_reports_unavailable_for_a_hang_or_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let hang = fake_claude(dir.path(), "sleep 30");
        assert_eq!(
            check_login_within(&hang, "/usr/bin:/bin", Duration::from_millis(100)),
            Err(PreflightError::Unavailable)
        );

        let garbage = fake_claude(dir.path(), "echo hello");
        assert_eq!(
            check_login_within(&garbage, "/usr/bin:/bin", Duration::from_secs(5)),
            Err(PreflightError::Unavailable)
        );
    }

    // ---- running

    fn runner() -> ClaudeRunner<MacProcessCtl> {
        ClaudeRunner::new(Arc::new(MacProcessCtl), Duration::from_millis(150))
    }

    fn request(bin: PathBuf, workspace: &Path) -> RunRequest {
        RunRequest {
            claude_bin: bin,
            path_env: "/usr/bin:/bin".to_string(),
            workspace: workspace.to_path_buf(),
            prompt: "hello there".to_string(),
            options: options(),
        }
    }

    fn collect(
        runner: &ClaudeRunner<MacProcessCtl>,
        req: RunRequest,
    ) -> (Vec<BuddyEvent>, RunOutcome) {
        let mut events = Vec::new();
        let outcome = runner.run(req, |event| events.push(event)).unwrap();
        (events, outcome)
    }

    #[test]
    fn it_streams_a_recorded_run_and_sends_the_prompt_on_stdin() {
        let dir = tempfile::tempdir().unwrap();
        let stdin_copy = dir.path().join("stdin.txt");
        let bin = fake_claude(
            dir.path(),
            &format!("cat > '{}'\ncat '{TEXT_RUN}'", stdin_copy.display()),
        );

        let (events, outcome) = collect(&runner(), request(bin, dir.path()));

        assert!(matches!(
            events.first(),
            Some(BuddyEvent::SessionStarted { .. })
        ));
        assert!(events.contains(&BuddyEvent::TextDelta {
            text: "PONG".into()
        }));
        assert!(matches!(
            events.last(),
            Some(BuddyEvent::Finished {
                is_error: false,
                ..
            })
        ));
        assert_eq!(fs::read_to_string(stdin_copy).unwrap(), "hello there");
        assert_eq!(
            outcome,
            RunOutcome {
                exit_code: Some(0),
                cancelled: false
            }
        );
    }

    #[test]
    fn it_passes_the_built_arguments_path_and_working_directory() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let out = dir.path();
        let bin = fake_claude(
            dir.path(),
            &format!(
                "cat > /dev/null\nprintf '%s\\n' \"$@\" > '{0}/args.txt'\npwd > '{0}/pwd.txt'\nprintf %s \"$PATH\" > '{0}/path.txt'",
                out.display()
            ),
        );
        let mut req = request(bin, workspace.path());
        req.path_env = "/custom/bin:/usr/bin:/bin".to_string();
        req.options.model = Some("sonnet".into());

        collect(&runner(), req.clone());

        let args: Vec<String> = fs::read_to_string(out.join("args.txt"))
            .unwrap()
            .lines()
            .map(String::from)
            .collect();
        assert_eq!(args, build_args(&req.options));
        assert_eq!(
            fs::canonicalize(fs::read_to_string(out.join("pwd.txt")).unwrap().trim()).unwrap(),
            fs::canonicalize(workspace.path()).unwrap()
        );
        assert_eq!(
            fs::read_to_string(out.join("path.txt")).unwrap(),
            "/custom/bin:/usr/bin:/bin"
        );
    }

    #[test]
    fn a_crash_without_a_result_reports_what_it_said_on_stderr() {
        let dir = tempfile::tempdir().unwrap();
        let bin = fake_claude(
            dir.path(),
            "cat > /dev/null\necho 'boom: not logged in' >&2\nexit 3",
        );

        let (events, outcome) = collect(&runner(), request(bin, dir.path()));

        assert_eq!(events.len(), 1);
        assert!(
            matches!(&events[0], BuddyEvent::Finished { text, is_error: true, .. } if text.contains("boom: not logged in"))
        );
        assert_eq!(outcome.exit_code, Some(3));
    }

    #[test]
    fn a_silent_crash_reports_the_exit_code() {
        let dir = tempfile::tempdir().unwrap();
        let bin = fake_claude(dir.path(), "cat > /dev/null\nexit 7");

        let (events, _) = collect(&runner(), request(bin, dir.path()));

        assert!(
            matches!(&events[0], BuddyEvent::Finished { text, is_error: true, .. } if text.contains('7'))
        );
    }

    #[test]
    fn output_lines_that_are_not_valid_utf8_do_not_break_the_run() {
        let dir = tempfile::tempdir().unwrap();
        let bin = fake_claude(
            dir.path(),
            &format!("cat > /dev/null\nprintf '\\377\\376 junk\\n'\ncat '{TEXT_RUN}'"),
        );

        let (events, _) = collect(&runner(), request(bin, dir.path()));

        assert!(matches!(
            events.last(),
            Some(BuddyEvent::Finished {
                is_error: false,
                ..
            })
        ));
    }

    #[test]
    fn a_missing_binary_is_a_spawn_error() {
        let dir = tempfile::tempdir().unwrap();

        let result = runner().run(
            request(dir.path().join("no-such-claude"), dir.path()),
            |_| {},
        );

        assert!(matches!(result, Err(RunError::Spawn(_))));
        assert!(!runner().is_running());
    }

    #[test]
    fn the_runner_is_free_again_after_a_run_ends() {
        let dir = tempfile::tempdir().unwrap();
        let bin = fake_claude(dir.path(), &format!("cat > /dev/null\ncat '{TEXT_RUN}'"));
        let runner = runner();

        collect(&runner, request(bin.clone(), dir.path()));
        let (events, _) = collect(&runner, request(bin, dir.path()));

        assert!(!runner.is_running());
        assert!(matches!(events.last(), Some(BuddyEvent::Finished { .. })));
    }

    /// Runs `body` as a fake claude on a thread, waits until it is running.
    fn run_in_background<'scope>(
        scope: &'scope thread::Scope<'scope, '_>,
        runner: &'scope ClaudeRunner<MacProcessCtl>,
        dir: &'scope Path,
        body: &str,
    ) -> thread::ScopedJoinHandle<'scope, (Vec<BuddyEvent>, RunOutcome)> {
        let bin = fake_claude(dir, body);
        let handle = scope.spawn(move || collect(runner, request(bin, dir)));
        let start = Instant::now();
        while !runner.is_running() {
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "run never started"
            );
            thread::sleep(Duration::from_millis(5));
        }
        handle
    }

    #[test]
    fn a_second_run_is_refused_while_one_is_active() {
        let dir = tempfile::tempdir().unwrap();
        let runner = runner();
        thread::scope(|scope| {
            let first = run_in_background(scope, &runner, dir.path(), "cat > /dev/null\nsleep 30");

            let second = runner.run(request(dir.path().join("claude"), dir.path()), |_| {});
            assert!(matches!(second, Err(RunError::Busy)));

            assert!(runner.cancel());
            first.join().unwrap();
        });
    }

    #[test]
    fn cancel_interrupts_the_run_and_reports_it() {
        let dir = tempfile::tempdir().unwrap();
        let runner = runner();
        let started = Instant::now();
        thread::scope(|scope| {
            let run = run_in_background(
                scope,
                &runner,
                dir.path(),
                "trap 'exit 130' INT\nsleep 30 &\nwait",
            );

            assert!(runner.cancel());
            let (events, outcome) = run.join().unwrap();

            assert!(outcome.cancelled);
            assert!(
                matches!(events.last(), Some(BuddyEvent::Finished { text, is_error: true, .. }) if text == "Cancelled.")
            );
        });
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(!runner.is_running());
    }

    #[test]
    fn cancel_escalates_to_kill_when_the_run_ignores_interrupt_and_terminate() {
        let dir = tempfile::tempdir().unwrap();
        let runner = runner();
        let started = Instant::now();
        thread::scope(|scope| {
            let run = run_in_background(
                scope,
                &runner,
                dir.path(),
                "trap '' INT TERM\nwhile true; do sleep 1; done",
            );

            runner.cancel();
            let (_, outcome) = run.join().unwrap();

            assert!(outcome.cancelled);
        });
        // Two grace periods (150 ms each) plus slack, nowhere near forever.
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn cancel_with_nothing_running_does_nothing() {
        assert!(!runner().cancel());
    }

    #[test]
    fn a_stray_process_holding_the_output_open_does_not_hang_the_run() {
        let dir = tempfile::tempdir().unwrap();
        let bin = fake_claude(
            dir.path(),
            &format!("cat > /dev/null\ncat '{TEXT_RUN}'\nsleep 30 &"),
        );
        let started = Instant::now();

        let (events, _) = collect(&runner(), request(bin, dir.path()));

        assert!(matches!(events.last(), Some(BuddyEvent::Finished { .. })));
        assert!(started.elapsed() < OUTPUT_CLOSE_WAIT + Duration::from_secs(3));
    }
}
