//! When the buddy sends a macOS notification (design §8.3). The rules are
//! pure so they can be tested; `buddy.rs` hands the result to
//! `tauri-plugin-notification`.

use std::time::Duration;

use crate::perm_broker::ApprovalRequest;

/// A run that took longer than this gets a notification even if you were
/// watching.
pub const LONG_RUN: Duration = Duration::from_secs(30);
const MAX_BODY_CHARS: usize = 120;
const TITLE: &str = "Desktop Buddy";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub title: String,
    pub body: String,
}

/// How a run ended, as far as the rules care.
#[derive(Debug, Clone, Copy)]
pub struct RunSummary<'a> {
    pub elapsed: Duration,
    pub is_error: bool,
    pub cancelled: bool,
    pub reply: &'a str,
}

fn first_line_cut(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    if line.chars().count() <= MAX_BODY_CHARS {
        return line.to_string();
    }
    let kept: String = line.chars().take(MAX_BODY_CHARS - 1).collect();
    format!("{kept}…")
}

fn notification(body: String) -> Notification {
    Notification {
        title: TITLE.to_string(),
        body,
    }
}

/// A notification for a finished run: only when the run was long or the
/// bubble was minimized, and never for a run you cancelled yourself.
pub fn on_finished(run: &RunSummary, minimized: bool, enabled: bool) -> Option<Notification> {
    if !enabled || run.cancelled || !(minimized || run.elapsed > LONG_RUN) {
        return None;
    }
    if run.is_error {
        return Some(notification("⚠️ Something went wrong".to_string()));
    }
    let first_line = first_line_cut(run.reply);
    if first_line.is_empty() {
        return Some(notification("✅ Done".to_string()));
    }
    Some(notification(format!("✅ Done: {first_line}")))
}

/// A notification for an approval request: only while the bubble is minimized.
pub fn on_approval(
    request: &ApprovalRequest,
    minimized: bool,
    enabled: bool,
) -> Option<Notification> {
    if !enabled || !minimized {
        return None;
    }
    Some(notification(format!(
        "🙋 I need your OK: {}",
        request.headline
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perm_broker::ApprovalKind;

    fn run(elapsed_secs: u64, reply: &str) -> RunSummary<'_> {
        RunSummary {
            elapsed: Duration::from_secs(elapsed_secs),
            is_error: false,
            cancelled: false,
            reply,
        }
    }

    fn approval() -> ApprovalRequest {
        ApprovalRequest {
            id: "1".to_string(),
            tool_name: "Bash".to_string(),
            kind: ApprovalKind::Action,
            headline: "💻 running npm install".to_string(),
            detail: None,
        }
    }

    #[test]
    fn a_short_run_you_watched_is_silent() {
        assert_eq!(on_finished(&run(5, "Hi!"), false, true), None);
    }

    #[test]
    fn a_run_over_thirty_seconds_notifies() {
        let shown = on_finished(&run(31, "All set.\nMore."), false, true).unwrap();

        assert_eq!(shown.body, "✅ Done: All set.");
        assert_eq!(shown.title, "Desktop Buddy");
    }

    #[test]
    fn exactly_thirty_seconds_is_not_over() {
        assert_eq!(on_finished(&run(30, "Hi"), false, true), None);
    }

    #[test]
    fn a_minimized_bubble_notifies_even_for_a_short_run() {
        let shown = on_finished(&run(2, "Done it"), true, true).unwrap();

        assert_eq!(shown.body, "✅ Done: Done it");
    }

    #[test]
    fn an_error_says_something_went_wrong() {
        let failed = RunSummary {
            is_error: true,
            ..run(40, "boom")
        };

        assert_eq!(
            on_finished(&failed, false, true).unwrap().body,
            "⚠️ Something went wrong"
        );
    }

    #[test]
    fn a_cancelled_run_is_silent() {
        let cancelled = RunSummary {
            cancelled: true,
            ..run(99, "")
        };

        assert_eq!(on_finished(&cancelled, true, true), None);
    }

    #[test]
    fn notifications_can_be_turned_off() {
        assert_eq!(on_finished(&run(99, "x"), true, false), None);
        assert_eq!(on_approval(&approval(), true, false), None);
    }

    #[test]
    fn an_empty_reply_is_just_done() {
        assert_eq!(
            on_finished(&run(99, "  \n"), false, true).unwrap().body,
            "✅ Done"
        );
    }

    #[test]
    fn a_long_first_line_is_cut() {
        let long = "x".repeat(500);

        let body = on_finished(&run(99, &long), false, true).unwrap().body;

        assert!(body.chars().count() < 140);
        assert!(body.ends_with('…'));
    }

    #[test]
    fn an_approval_notifies_only_while_minimized() {
        assert_eq!(on_approval(&approval(), false, true), None);
        assert_eq!(
            on_approval(&approval(), true, true).unwrap().body,
            "🙋 I need your OK: 💻 running npm install"
        );
    }
}
