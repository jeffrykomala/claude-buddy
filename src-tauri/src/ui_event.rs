//! What the webview hears while Claude works (design §10.1). The stream
//! parser's [`BuddyEvent`]s are the backend's view; these are the same
//! things shaped for the UI: tool calls become an activity (pose, icon,
//! label) and the end of a run says whether it was cancelled.

use serde::Serialize;

use crate::activity::{self, ActivityPose};
use crate::stream_parser::BuddyEvent;

/// Emitted for every run event, with this payload.
pub const RUN_EVENT: &str = "buddy://event";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiEvent {
    /// A session started or resumed.
    Started {
        id: String,
        model: String,
    },
    /// A piece of the streaming reply.
    Text {
        text: String,
    },
    /// Claude began using a tool: the pet acts it out.
    Activity {
        pose: ActivityPose,
        icon: &'static str,
        label: String,
    },
    ToolDone {
        ok: bool,
    },
    /// The API request is being retried.
    Retrying {
        attempt: u64,
    },
    /// The run is over (always the last event of a run).
    Finished {
        text: String,
        is_error: bool,
        cancelled: bool,
        model: Option<String>,
    },
    /// Claude Code no longer has the conversation we tried to resume.
    Lost,
}

/// The UI event for a run event, if the UI cares. `Finished` is not made
/// here: whether the run was cancelled is only known once it has ended.
pub fn from_buddy_event(event: &BuddyEvent) -> Option<UiEvent> {
    match event {
        BuddyEvent::SessionStarted { id, model } => Some(UiEvent::Started {
            id: id.clone(),
            model: model.clone(),
        }),
        BuddyEvent::TextDelta { text } => Some(UiEvent::Text { text: text.clone() }),
        BuddyEvent::ToolStarted { name, input, .. } => {
            let described = activity::describe(name, input);
            Some(UiEvent::Activity {
                pose: described.pose,
                icon: described.icon,
                label: described.label,
            })
        }
        BuddyEvent::ToolFinished { ok, .. } => Some(UiEvent::ToolDone { ok: *ok }),
        BuddyEvent::Retry { attempt } => Some(UiEvent::Retrying { attempt: *attempt }),
        BuddyEvent::Finished { .. } => None,
    }
}

/// What Claude Code says when asked to resume a conversation it has deleted.
const LOST_CONVERSATION_TEXT: &str = "No conversation found";

/// True when a failed run's text means the resumed conversation is gone.
pub fn is_lost_conversation(text: &str) -> bool {
    text.contains(LOST_CONVERSATION_TEXT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_session_start_passes_through() {
        let event = BuddyEvent::SessionStarted {
            id: "abc".into(),
            model: "opus".into(),
        };

        assert_eq!(
            from_buddy_event(&event),
            Some(UiEvent::Started {
                id: "abc".into(),
                model: "opus".into()
            })
        );
    }

    #[test]
    fn text_passes_through() {
        let event = BuddyEvent::TextDelta { text: "hi".into() };

        assert_eq!(
            from_buddy_event(&event),
            Some(UiEvent::Text { text: "hi".into() })
        );
    }

    #[test]
    fn a_tool_call_becomes_an_activity() {
        let event = BuddyEvent::ToolStarted {
            id: "t1".into(),
            name: "Read".into(),
            input: json!({ "file_path": "/a/b/main.rs" }),
        };

        assert_eq!(
            from_buddy_event(&event),
            Some(UiEvent::Activity {
                pose: ActivityPose::Read,
                icon: "📖",
                label: "reading main.rs".into()
            })
        );
    }

    #[test]
    fn tool_results_and_retries_pass_through() {
        assert_eq!(
            from_buddy_event(&BuddyEvent::ToolFinished {
                id: "t1".into(),
                ok: false
            }),
            Some(UiEvent::ToolDone { ok: false })
        );
        assert_eq!(
            from_buddy_event(&BuddyEvent::Retry { attempt: 2 }),
            Some(UiEvent::Retrying { attempt: 2 })
        );
    }

    #[test]
    fn finished_is_left_to_the_caller() {
        let event = BuddyEvent::Finished {
            text: "done".into(),
            is_error: false,
            cost_usd: None,
            context_tokens: None,
            context_window: None,
        };

        assert_eq!(from_buddy_event(&event), None);
    }

    #[test]
    fn events_serialize_with_a_type_tag() {
        let json = serde_json::to_value(UiEvent::Activity {
            pose: ActivityPose::Run,
            icon: "💻",
            label: "running ls".into(),
        })
        .unwrap();

        assert_eq!(
            json,
            json!({ "type": "activity", "pose": "run", "icon": "💻", "label": "running ls" })
        );
        assert_eq!(
            serde_json::to_value(UiEvent::Lost).unwrap(),
            json!({ "type": "lost" })
        );
    }

    #[test]
    fn a_missing_conversation_is_recognised() {
        assert!(is_lost_conversation(
            "No conversation found with session ID: 1234"
        ));
        assert!(!is_lost_conversation("Something else broke"));
    }
}
