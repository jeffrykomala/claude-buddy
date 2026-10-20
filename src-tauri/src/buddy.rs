//! The buddy's brain on the Rust side (design §10): it takes a message from
//! the webview, runs it through Claude Code, and turns what happens into
//! events, approvals, notifications and the saved conversation.
//!
//! One run at a time. Everything the UI needs arrives as an event; the
//! commands here only start, stop, answer or switch things.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::claude_runner::{
    ClaudeRunner, PermissionMode, PreflightError, RunError, RunOptions, RunRequest, STOP_GRACE,
    check_login, locate_claude,
};
use crate::conversations::{self, ConversationStore};
use crate::notify::{self, Notification, RunSummary};
use crate::perm_broker::{self, APPROVAL_TIMEOUT, ApprovalRequest, Broker, Decision};
use crate::perm_server::{self, PermServer};
use crate::platform::macos::MacProcessCtl;
use crate::platform::{self, ShellEnv};
use crate::settings::SettingsStore;
use crate::slash::{self, SlashCommand};
use crate::ui_event::{self, RUN_EVENT, UiEvent};

/// An approval request for the bubble ([`ApprovalRequest`]).
pub const APPROVAL_EVENT: &str = "buddy://approval";
/// A new conversation began: clear the reply.
pub const NEW_EVENT: &str = "buddy://new";
/// Open the input bubble (hotkey, menu bar, ⌘⇧Space).
pub const TALK_EVENT: &str = "buddy://talk";
/// `/resume` was typed: show the recent conversations.
pub const PICKER_EVENT: &str = "buddy://picker";
/// A short line for the bubble ("Resumed: …").
pub const NOTICE_EVENT: &str = "buddy://notice";
/// The app was activated (e.g. a notification was clicked): reopen the reply.
pub const REOPEN_EVENT: &str = "buddy://reopen";

const MAX_MESSAGE_CHARS: usize = 20_000;
/// How long quitting waits for `claude` to stop (SIGINT, SIGTERM, SIGKILL
/// with [`STOP_GRACE`] between).
const QUIT_WAIT: Duration = Duration::from_secs(8);
const QUIT_POLL: Duration = Duration::from_millis(50);
const WORKSPACE_DIR: &str = "BuddyWorkspace";
const PERSONA_FILE: &str = "persona.md";
const PERSONA: &str = include_str!("../resources/persona.md");

const NO_APPROVALS_MESSAGE: &str = "I can't start my approval helper, so I won't run anything. \
     Please quit and open me again.";
const NOT_LOGGED_IN_MESSAGE: &str =
    "You're not logged in to Claude Code. Open Terminal, run `claude auth login`, then try again.";
const UNAVAILABLE_MESSAGE: &str =
    "I couldn't check your Claude Code login. Is `claude` working in Terminal?";
const BUSY_MESSAGE: &str = "I'm still working on your last message.";

/// Where `claude` is and the PATH to give it, once both are known to work.
#[derive(Clone)]
struct Toolchain {
    claude_bin: PathBuf,
    path_env: String,
}

pub struct Buddy {
    runner: Arc<ClaudeRunner<MacProcessCtl>>,
    broker: Arc<Broker>,
    /// `None` when it couldn't start: prompts must not run without it.
    server: Option<PermServer>,
    workspace: PathBuf,
    persona: Option<PathBuf>,
    /// The bubble is closed while a run is going (design §8.3).
    minimized: AtomicBool,
    quitting: AtomicBool,
    toolchain: Mutex<Option<Toolchain>>,
}

impl Buddy {
    /// Sets up the workspace, the persona file, the approval server and the
    /// stream of approval requests. Call from `setup`.
    pub fn start(app: &AppHandle) -> Buddy {
        let (broker, requests) = Broker::new(APPROVAL_TIMEOUT);
        let server = match tauri::async_runtime::block_on(perm_server::start(Arc::clone(&broker))) {
            Ok(server) => Some(server),
            Err(error) => {
                eprintln!("buddy: the approval server could not start: {error}");
                None
            }
        };
        spawn_approval_forwarder(app.clone(), requests);

        let home = app.path().home_dir().unwrap_or_default();
        let data_dir = app.path().app_data_dir().unwrap_or_default();
        Buddy {
            runner: Arc::new(ClaudeRunner::new(Arc::new(MacProcessCtl), STOP_GRACE)),
            broker,
            server,
            workspace: prepare_workspace(&home),
            persona: write_persona(&data_dir),
            minimized: AtomicBool::new(false),
            quitting: AtomicBool::new(false),
            toolchain: Mutex::new(None),
        }
    }

    /// Stops the current run (and denies anything it is waiting on). False
    /// when nothing was running.
    fn stop_run(&self) -> bool {
        self.broker.cancel_all();
        self.runner.cancel()
    }

    /// Whether the app is already on its way out.
    pub fn is_quitting(&self) -> bool {
        self.quitting.load(Ordering::SeqCst)
    }

    /// Whether a run is going right now.
    pub fn is_running(&self) -> bool {
        self.runner.is_running()
    }

    /// Last-chance cleanup: nothing should outlive the app.
    pub fn shutdown(&self) {
        self.stop_run();
    }

    fn toolchain(&self, platform: &platform::Platform) -> Result<Toolchain, String> {
        if let Some(known) = self.toolchain.lock().unwrap().clone() {
            return Ok(known);
        }
        let path_env = platform.path();
        let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
        let claude_bin =
            locate_claude(&path_env, &home).map_err(|looked| not_found_message(&looked))?;
        check_login(&claude_bin, &path_env).map_err(|error| match error {
            PreflightError::NotLoggedIn => NOT_LOGGED_IN_MESSAGE.to_string(),
            PreflightError::Unavailable => UNAVAILABLE_MESSAGE.to_string(),
        })?;
        let toolchain = Toolchain {
            claude_bin,
            path_env,
        };
        *self.toolchain.lock().unwrap() = Some(toolchain.clone());
        Ok(toolchain)
    }
}

fn not_found_message(looked: &[PathBuf]) -> String {
    let places: Vec<String> = looked
        .iter()
        .take(6)
        .map(|path| path.display().to_string())
        .collect();
    format!(
        "I can't find Claude Code. Install it from https://claude.com/claude-code, then try again.\n\nI looked in: {}",
        places.join(", ")
    )
}

/// `~/BuddyWorkspace`, created if missing. If it can't be made the home
/// folder itself is used, so the buddy still works.
fn prepare_workspace(home: &std::path::Path) -> PathBuf {
    let workspace = home.join(WORKSPACE_DIR);
    match std::fs::create_dir_all(&workspace) {
        Ok(()) => workspace,
        Err(error) => {
            eprintln!("buddy: cannot create {}: {error}", workspace.display());
            home.to_path_buf()
        }
    }
}

/// Writes the persona where `--append-system-prompt-file` can read it. The
/// file is rewritten on every launch so an updated persona applies to the
/// next new conversation (design §12).
fn write_persona(data_dir: &std::path::Path) -> Option<PathBuf> {
    let path = data_dir.join(PERSONA_FILE);
    let written = std::fs::create_dir_all(data_dir).and_then(|()| std::fs::write(&path, PERSONA));
    match written {
        Ok(()) => Some(path),
        Err(error) => {
            eprintln!("buddy: cannot write {}: {error}", path.display());
            None
        }
    }
}

/// Trims a typed message and checks it can be sent.
fn validate_message(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("There's nothing to send.".to_string());
    }
    if text.chars().count() > MAX_MESSAGE_CHARS {
        return Err(format!(
            "That message is too long (the limit is {MAX_MESSAGE_CHARS} characters)."
        ));
    }
    Ok(text.to_string())
}

// ------------------------------------------------------------- the run

/// Sends one message. A `/new` or `/resume` typed as the whole message is
/// handled here instead of being sent to Claude.
#[tauri::command]
pub fn send_message(app: AppHandle, text: String) -> Result<(), String> {
    let text = validate_message(&text)?;
    match slash::parse(&text) {
        Some(SlashCommand::New) => {
            new_conversation(&app);
            return Ok(());
        }
        Some(SlashCommand::Resume) => {
            let _ = app.emit(PICKER_EVENT, ());
            return Ok(());
        }
        None => {}
    }

    let buddy = app.state::<Buddy>();
    if buddy.server.is_none() {
        return Err(NO_APPROVALS_MESSAGE.to_string());
    }
    if buddy.runner.is_running() {
        return Err(BUSY_MESSAGE.to_string());
    }
    let handle = app.clone();
    thread::spawn(move || run_message(&handle, &text));
    Ok(())
}

/// Runs one message to the end on a worker thread, reporting through events.
fn run_message(app: &AppHandle, prompt: &str) {
    use crate::stream_parser::BuddyEvent;

    let started = Instant::now();
    let buddy = app.state::<Buddy>();
    let settings = app.state::<SettingsStore>().get();

    let request = match prepare_request(app, &buddy, prompt) {
        Ok(request) => request,
        Err(message) => return report_failure(app, message),
    };
    let resumed = request.options.session_id.clone();
    let add_dirs = settings.session_add_dirs.clone();

    let mut finished = (String::new(), false);
    let mut session: Option<(String, String)> = None;
    let outcome = buddy.runner.run(request, |event| {
        match &event {
            BuddyEvent::SessionStarted { id, model } => {
                session = Some((id.clone(), model.clone()));
                remember_session(app, id, prompt, model, &add_dirs);
            }
            BuddyEvent::Finished { text, is_error, .. } => finished = (text.clone(), *is_error),
            _ => {}
        }
        if let Some(ui) = ui_event::from_buddy_event(&event) {
            let _ = app.emit(RUN_EVENT, ui);
        }
    });
    // Anything still waiting on an answer is moot now.
    buddy.broker.cancel_all();

    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(RunError::Busy) => return eprintln!("buddy: a run was already active"),
        Err(RunError::Spawn(error)) => {
            return report_failure(app, format!("I couldn't start Claude Code: {error}"));
        }
    };

    let (text, is_error) = finished;
    if is_error && !outcome.cancelled && ui_event::is_lost_conversation(&text) {
        return forget_conversation(app, resumed.as_deref());
    }
    let model = session.map(|(id, model)| {
        app.state::<ConversationStore>()
            .touch(&id, Some(&model), conversations::now_secs());
        crate::menubar::refresh_recent(app);
        model
    });
    notify_finished(
        app,
        &RunSummary {
            elapsed: started.elapsed(),
            is_error,
            cancelled: outcome.cancelled,
            reply: &text,
        },
        settings.notifications,
    );
    let _ = app.emit(
        RUN_EVENT,
        UiEvent::Finished {
            text,
            is_error,
            cancelled: outcome.cancelled,
            model,
        },
    );
}

fn prepare_request(app: &AppHandle, buddy: &Buddy, prompt: &str) -> Result<RunRequest, String> {
    let server = buddy
        .server
        .as_ref()
        .ok_or_else(|| NO_APPROVALS_MESSAGE.to_string())?;
    let toolchain = buddy.toolchain(&app.state::<platform::Platform>())?;
    let settings = app.state::<SettingsStore>().get();
    Ok(RunRequest {
        claude_bin: toolchain.claude_bin,
        path_env: toolchain.path_env,
        workspace: buddy.workspace.clone(),
        prompt: prompt.to_string(),
        options: RunOptions {
            session_id: settings.last_session_id,
            mode: PermissionMode::Default,
            model: None,
            effort: None,
            add_dirs: settings
                .session_add_dirs
                .iter()
                .map(PathBuf::from)
                .collect(),
            mcp_config: server.mcp_config(),
            persona_path: buddy.persona.clone(),
        },
    })
}

/// A run that never started: shown like a run that failed.
fn report_failure(app: &AppHandle, message: String) {
    let _ = app.emit(
        RUN_EVENT,
        UiEvent::Finished {
            text: message,
            is_error: true,
            cancelled: false,
            model: None,
        },
    );
}

/// The session started: continue it next time, and list it in Recent.
fn remember_session(app: &AppHandle, id: &str, prompt: &str, model: &str, add_dirs: &[String]) {
    app.state::<SettingsStore>()
        .update(|s| s.with_session(Some(id.to_string()), add_dirs.to_vec()));
    app.state::<ConversationStore>().record_started(
        id,
        prompt,
        Some(model),
        add_dirs,
        conversations::now_secs(),
    );
    crate::menubar::refresh_recent(app);
}

/// Claude Code no longer has the conversation: drop it from Recent, start
/// fresh, and tell the bubble to offer "Send as a new conversation".
fn forget_conversation(app: &AppHandle, session_id: Option<&str>) {
    if let Some(id) = session_id {
        app.state::<ConversationStore>().remove(id);
    }
    app.state::<SettingsStore>()
        .update(|s| s.with_session(None, Vec::new()));
    crate::menubar::refresh_recent(app);
    let _ = app.emit(RUN_EVENT, UiEvent::Lost);
}

fn notify_finished(app: &AppHandle, run: &RunSummary, enabled: bool) {
    let minimized = app.state::<Buddy>().minimized.load(Ordering::SeqCst);
    if let Some(notification) = notify::on_finished(run, minimized, enabled) {
        show_notification(app, &notification);
    }
}

fn show_notification(app: &AppHandle, notification: &Notification) {
    let shown = app
        .notification()
        .builder()
        .title(&notification.title)
        .body(&notification.body)
        .show();
    if let Err(error) = shown {
        eprintln!("buddy: notification failed: {error}");
    }
}

// ----------------------------------------------------------- approvals

/// Forwards each approval request to the bubble, and to a notification when
/// the bubble is minimized.
fn spawn_approval_forwarder(
    app: AppHandle,
    mut requests: tokio::sync::mpsc::UnboundedReceiver<ApprovalRequest>,
) {
    tauri::async_runtime::spawn(async move {
        while let Some(request) = requests.recv().await {
            let minimized = app
                .try_state::<Buddy>()
                .is_some_and(|buddy| buddy.minimized.load(Ordering::SeqCst));
            let enabled = app
                .try_state::<SettingsStore>()
                .is_some_and(|settings| settings.get().notifications);
            if let Some(notification) = notify::on_approval(&request, minimized, enabled) {
                show_notification(&app, &notification);
            }
            let _ = app.emit(APPROVAL_EVENT, request);
        }
    });
}

/// The user's answer to an approval: Allow, or Deny.
#[tauri::command]
pub fn answer_approval(buddy: tauri::State<'_, Buddy>, id: String, allow: bool) {
    let decision = if allow {
        Decision::Allow
    } else {
        Decision::deny(perm_broker::USER_DENIED_MESSAGE)
    };
    if !buddy.broker.answer(&id, decision) {
        eprintln!("buddy: approval {id} was already answered or has expired");
    }
}

// ------------------------------------------------------------- control

/// Stops the current run, if any.
#[tauri::command]
pub fn cancel_run(buddy: tauri::State<'_, Buddy>) {
    buddy.stop_run();
}

/// The frontend tells us whether the bubble is minimized, for the
/// notification rules (design §8.3).
#[tauri::command]
pub fn set_minimized(buddy: tauri::State<'_, Buddy>, minimized: bool) {
    buddy.minimized.store(minimized, Ordering::SeqCst);
}

/// Asks the frontend to open the input bubble.
pub fn talk(app: &AppHandle) {
    let _ = app.emit(TALK_EVENT, ());
}

/// Starts a new conversation: stops any run, forgets the current session
/// (it stays in Recent) and clears the reply (design §10.8).
pub fn new_conversation(app: &AppHandle) {
    app.state::<Buddy>().stop_run();
    app.state::<SettingsStore>()
        .update(|s| s.with_session(None, Vec::new()));
    let _ = app.emit(NEW_EVENT, ());
}

#[tauri::command]
pub fn start_new_conversation(app: AppHandle) {
    new_conversation(&app);
}

/// Continues a recent conversation with the next message.
pub fn resume_conversation(app: &AppHandle, id: &str) -> Result<String, String> {
    if app.state::<Buddy>().runner.is_running() {
        return Err(BUSY_MESSAGE.to_string());
    }
    let entry = app
        .state::<ConversationStore>()
        .get(id)
        .ok_or_else(|| "I can't find that conversation any more.".to_string())?;
    app.state::<SettingsStore>()
        .update(|s| s.with_session(Some(entry.id.clone()), entry.add_dirs.clone()));
    let _ = app.emit(NOTICE_EVENT, format!("Resumed: {}", entry.title));
    Ok(entry.title)
}

#[tauri::command]
pub fn resume(app: AppHandle, id: String) -> Result<String, String> {
    resume_conversation(&app, &id)
}

/// One row of Recent, ready to show.
#[derive(Debug, Serialize)]
pub struct RecentEntry {
    pub id: String,
    pub title: String,
    pub when: String,
}

pub fn recent_entries(store: &ConversationStore, now: u64) -> Vec<RecentEntry> {
    store
        .list()
        .into_iter()
        .map(|c| RecentEntry {
            when: conversations::relative_time(now, c.last_used),
            id: c.id,
            title: c.title,
        })
        .collect()
}

#[tauri::command]
pub fn list_recent(store: tauri::State<'_, ConversationStore>) -> Vec<RecentEntry> {
    recent_entries(&store, conversations::now_secs())
}

/// Quits once `claude` (and anything it started) has stopped, so no
/// `claude` process outlives the app (design §13). Gives up waiting after
/// [`QUIT_WAIT`].
pub fn quit(app: &AppHandle) {
    let buddy = app.state::<Buddy>();
    buddy.quitting.store(true, Ordering::SeqCst);
    if !buddy.stop_run() {
        return app.exit(0);
    }
    let handle = app.clone();
    thread::spawn(move || {
        let buddy = handle.state::<Buddy>();
        let deadline = Instant::now() + QUIT_WAIT;
        while buddy.runner.is_running() && Instant::now() < deadline {
            thread::sleep(QUIT_POLL);
        }
        handle.exit(0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_is_trimmed() {
        assert_eq!(
            validate_message("  hi there \n"),
            Ok("hi there".to_string())
        );
    }

    #[test]
    fn an_empty_message_is_refused() {
        assert!(validate_message("  \n\t").is_err());
    }

    #[test]
    fn a_huge_message_is_refused() {
        assert!(validate_message(&"x".repeat(MAX_MESSAGE_CHARS + 1)).is_err());
        assert!(validate_message(&"x".repeat(MAX_MESSAGE_CHARS)).is_ok());
    }

    #[test]
    fn the_not_found_message_says_where_it_looked() {
        let message = not_found_message(&[PathBuf::from("/a/claude"), PathBuf::from("/b/claude")]);

        assert!(message.contains("/a/claude, /b/claude"));
        assert!(message.contains("Install"));
    }

    #[test]
    fn the_not_found_message_stays_short() {
        let many: Vec<PathBuf> = (0..30)
            .map(|n| PathBuf::from(format!("/d{n}/claude")))
            .collect();

        let message = not_found_message(&many);

        assert!(!message.contains("/d6/claude"));
    }

    #[test]
    fn the_workspace_is_created_under_home() {
        let home = tempfile::tempdir().unwrap();

        let workspace = prepare_workspace(home.path());

        assert_eq!(workspace, home.path().join("BuddyWorkspace"));
        assert!(workspace.is_dir());
    }

    #[test]
    fn an_uncreatable_workspace_falls_back_to_home() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("home");
        std::fs::write(&blocker, "a file").unwrap();

        assert_eq!(prepare_workspace(&blocker), blocker);
    }

    #[test]
    fn the_persona_is_written_and_refreshed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("persona.md"), "stale").unwrap();

        let path = write_persona(dir.path()).unwrap();

        let text = std::fs::read_to_string(path).unwrap();
        assert!(text.contains("Shiba"));
        assert!(text.contains("macOS"));
    }

    #[test]
    fn recent_entries_show_relative_times() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConversationStore::open(dir.path());
        store.record_started("a", "Summarize the PDF", None, &[], 1000);

        let entries = recent_entries(&store, 1000 + 2 * 3600);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].title, "Summarize the PDF");
        assert_eq!(entries[0].when, "2 h ago");
    }
}
