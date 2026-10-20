//! The PATH for child processes (design §9.1). An app opened from Finder
//! starts with a minimal PATH (`/usr/bin:/bin:/usr/sbin:/sbin`), so `claude`,
//! `git`, `node` and Homebrew tools wouldn't be found. We ask the user's login
//! shell for the real PATH and add the usual install locations on top.

use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use crate::platform::ShellEnv;
use crate::stop::capture_stdout;

const SHELL_TIMEOUT: Duration = Duration::from_secs(5);
/// What a `.app` launched from Finder starts with.
const MINIMAL_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";
const PRINT_PATH: &str = r#"printf %s "$PATH""#;

/// The PATH printed by `shell -ilc 'printf %s "$PATH"'`: the last non-empty
/// line, since an interactive shell may print banners or rc-file noise first.
pub fn parse_path_output(stdout: &str) -> Option<String> {
    let last = stdout
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())?;
    last.starts_with('/').then(|| last.to_string())
}

/// `path` plus the usual install directories (Homebrew on Apple Silicon and
/// Intel, `~/.local/bin`) for any that are missing from it.
pub fn with_fallbacks(path: &str, home: &str) -> String {
    let mut dirs: Vec<String> = path
        .split(':')
        .filter(|dir| !dir.is_empty())
        .map(str::to_string)
        .collect();
    let mut extras = vec![
        "/opt/homebrew/bin".to_string(),
        "/usr/local/bin".to_string(),
    ];
    if !home.is_empty() {
        extras.push(format!("{home}/.local/bin"));
    }
    for extra in extras {
        if !dirs.contains(&extra) {
            dirs.push(extra);
        }
    }
    dirs.join(":")
}

fn login_shell_path(shell: &str, timeout: Duration) -> Option<String> {
    let mut command = Command::new(shell);
    command.args(["-ilc", PRINT_PATH]);
    capture_stdout(command, timeout).and_then(|captured| parse_path_output(&captured.stdout))
}

/// The PATH to give child processes: the login shell's if it answers in time,
/// else the minimal one, plus the install-directory fallbacks either way.
pub fn resolve_path(shell: Option<&str>, home: &str, timeout: Duration) -> String {
    let base = shell
        .and_then(|shell| login_shell_path(shell, timeout))
        .unwrap_or_else(|| {
            eprintln!("env: login shell PATH unavailable, using the fallback PATH");
            MINIMAL_PATH.to_string()
        });
    with_fallbacks(&base, home)
}

/// Resolves the PATH once, on first use (it can take a moment, so not at
/// startup), and keeps it.
#[derive(Debug, Default)]
pub struct MacShellEnv {
    path: OnceLock<String>,
}

impl ShellEnv for MacShellEnv {
    fn path(&self) -> String {
        self.path
            .get_or_init(|| {
                let shell = std::env::var("SHELL").ok();
                let home = std::env::var("HOME").unwrap_or_default();
                resolve_path(shell.as_deref(), &home, SHELL_TIMEOUT)
            })
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    const HOME: &str = "/Users/me";

    #[test]
    fn parse_takes_the_last_line_after_banners() {
        let output = "Last login: Tue Oct 20\nwelcome!\n/opt/homebrew/bin:/usr/bin";

        assert_eq!(
            parse_path_output(output),
            Some("/opt/homebrew/bin:/usr/bin".to_string())
        );
    }

    #[test]
    fn parse_accepts_a_bare_path_and_ignores_trailing_blank_lines() {
        assert_eq!(parse_path_output("/a:/b\n\n"), Some("/a:/b".to_string()));
    }

    #[test]
    fn parse_keeps_spaces_inside_path_entries() {
        let path = "/Applications/Visual Studio Code.app/Contents/Resources/app/bin:/usr/bin";

        assert_eq!(parse_path_output(path), Some(path.to_string()));
    }

    #[test]
    fn parse_rejects_output_that_is_not_a_path() {
        assert_eq!(parse_path_output(""), None);
        assert_eq!(parse_path_output("  \n "), None);
        assert_eq!(parse_path_output("zsh: command not found"), None);
    }

    #[test]
    fn fallbacks_are_added_when_missing() {
        assert_eq!(
            with_fallbacks("/usr/bin:/bin", HOME),
            "/usr/bin:/bin:/opt/homebrew/bin:/usr/local/bin:/Users/me/.local/bin"
        );
    }

    #[test]
    fn fallbacks_are_not_duplicated() {
        let path = "/opt/homebrew/bin:/usr/bin:/usr/local/bin:/Users/me/.local/bin";

        assert_eq!(with_fallbacks(path, HOME), path);
    }

    #[test]
    fn fallbacks_skip_the_home_entry_without_a_home() {
        assert_eq!(
            with_fallbacks("/usr/bin", ""),
            "/usr/bin:/opt/homebrew/bin:/usr/local/bin"
        );
    }

    #[test]
    fn fallbacks_drop_empty_entries() {
        assert!(with_fallbacks("/usr/bin::/bin:", HOME).starts_with("/usr/bin:/bin:"));
    }

    /// A stand-in login shell: a script that ignores its arguments.
    fn fake_shell(dir: &tempfile::TempDir, body: &str) -> String {
        let path = dir.path().join("fake-shell");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn resolve_uses_the_login_shell_path_plus_fallbacks() {
        let dir = tempfile::tempdir().unwrap();
        let shell = fake_shell(&dir, "echo banner; printf '/custom/bin:/usr/bin'");

        let path = resolve_path(Some(&shell), HOME, Duration::from_secs(5));

        assert_eq!(
            path,
            "/custom/bin:/usr/bin:/opt/homebrew/bin:/usr/local/bin:/Users/me/.local/bin"
        );
    }

    #[test]
    fn resolve_falls_back_when_the_shell_hangs() {
        let dir = tempfile::tempdir().unwrap();
        let shell = fake_shell(&dir, "sleep 30");

        let path = resolve_path(Some(&shell), HOME, Duration::from_millis(100));

        assert!(path.starts_with(MINIMAL_PATH));
        assert!(path.contains("/opt/homebrew/bin"));
    }

    #[test]
    fn resolve_falls_back_when_the_shell_prints_nothing_useful() {
        let dir = tempfile::tempdir().unwrap();
        let shell = fake_shell(&dir, "echo 'oops'");

        assert!(resolve_path(Some(&shell), HOME, Duration::from_secs(5)).starts_with(MINIMAL_PATH));
    }

    #[test]
    fn resolve_falls_back_without_a_shell_or_with_a_missing_one() {
        assert!(resolve_path(None, HOME, Duration::from_secs(1)).starts_with(MINIMAL_PATH));
        assert!(
            resolve_path(Some("/no/such/shell"), HOME, Duration::from_secs(1))
                .starts_with(MINIMAL_PATH)
        );
    }

    #[test]
    fn the_real_login_shell_gives_a_usable_path() {
        let path = MacShellEnv::default().path();

        assert!(path.contains("/usr/bin"));
    }
}
