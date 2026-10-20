//! Holds the approval requests Claude is waiting on (design §9.1, §10.2): each
//! is shown to the user, and the answer (or a timeout, a cancelled run, or
//! the app quitting) goes back to Claude Code through the MCP server.
//!
//! The UI side is a plain channel: [`Broker::new`] hands back a receiver of
//! [`ApprovalRequest`]s, and the UI answers with [`Broker::answer`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};

/// An approval nobody answers within this long is denied (design §13).
pub const APPROVAL_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// The message Claude sees when the user clicks Deny.
pub const USER_DENIED_MESSAGE: &str = "The user denied this action.";

const PLAN_TOOL: &str = "ExitPlanMode";
const MAX_DETAIL_CHARS: usize = 600;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny { message: String },
}

impl Decision {
    pub fn deny(message: &str) -> Decision {
        Decision::Deny {
            message: message.to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalKind {
    /// Claude wants to run a tool: Allow / Deny.
    Action,
    /// Claude finished planning: approve, approve with auto-accepted edits,
    /// or keep planning (design §10.4).
    Plan,
}

/// What the bubble shows for a pending approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApprovalRequest {
    pub id: String,
    pub tool_name: String,
    pub kind: ApprovalKind,
    /// One line, like the activity label ("✍️ writing hello.txt").
    pub headline: String,
    /// The specifics: the full command, path, URL, or the plan text.
    pub detail: Option<String>,
}

fn cut(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let kept: String = text.chars().take(max_chars - 1).collect();
    format!("{kept}…")
}

fn string_field<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    input
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

fn detail_for(tool_name: &str, input: &Value) -> Option<String> {
    let specific = match tool_name {
        "Bash" => string_field(input, "command").map(str::to_string),
        "Write" | "Edit" | "Read" => string_field(input, "file_path").map(str::to_string),
        "NotebookEdit" => string_field(input, "notebook_path").map(str::to_string),
        "WebFetch" => string_field(input, "url").map(str::to_string),
        "WebSearch" => string_field(input, "query").map(str::to_string),
        _ => None,
    };
    let detail = specific.or_else(|| match input {
        Value::Object(map) if !map.is_empty() => Some(input.to_string()),
        _ => None,
    })?;
    Some(cut(&detail, MAX_DETAIL_CHARS))
}

fn describe_request(id: String, tool_name: &str, input: &Value) -> ApprovalRequest {
    if tool_name == PLAN_TOOL {
        return ApprovalRequest {
            id,
            tool_name: tool_name.to_string(),
            kind: ApprovalKind::Plan,
            headline: "📋 Here's my plan".to_string(),
            detail: string_field(input, "plan").map(str::to_string),
        };
    }
    let activity = crate::activity::describe(tool_name, input);
    ApprovalRequest {
        id,
        tool_name: tool_name.to_string(),
        kind: ApprovalKind::Action,
        headline: format!("{} {}", activity.icon, activity.label),
        detail: detail_for(tool_name, input),
    }
}

/// The JSON Claude Code expects back from the permission tool: allow (with
/// the input it may run, unchanged) or deny (with a reason it will see).
pub fn verdict_json(input: &Value, decision: &Decision) -> String {
    match decision {
        Decision::Allow => json!({ "behavior": "allow", "updatedInput": input }),
        Decision::Deny { message } => json!({ "behavior": "deny", "message": message }),
    }
    .to_string()
}

pub struct Broker {
    pending: Mutex<HashMap<String, oneshot::Sender<Decision>>>,
    timeout: Duration,
    requests: mpsc::UnboundedSender<ApprovalRequest>,
}

impl Broker {
    /// The broker, and the stream of requests the UI should show.
    pub fn new(timeout: Duration) -> (Arc<Broker>, mpsc::UnboundedReceiver<ApprovalRequest>) {
        let (requests, receiver) = mpsc::unbounded_channel();
        let broker = Broker {
            pending: Mutex::new(HashMap::new()),
            timeout,
            requests,
        };
        (Arc::new(broker), receiver)
    }

    /// Shows the request to the user and waits for their answer. Denies when
    /// nobody answers in time, the run is cancelled, or there is no UI to ask.
    pub async fn ask(&self, tool_name: &str, input: &Value) -> Decision {
        let id = uuid::Uuid::new_v4().to_string();
        let (answer_tx, answer_rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id.clone(), answer_tx);

        let request = describe_request(id.clone(), tool_name, input);
        if self.requests.send(request).is_err() {
            self.pending.lock().unwrap().remove(&id);
            return Decision::deny("There is nobody to ask right now.");
        }

        match tokio::time::timeout(self.timeout, answer_rx).await {
            Ok(Ok(decision)) => decision,
            Ok(Err(_)) => Decision::deny("The request was cancelled."),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Decision::deny("There was no answer in time, so the action was not allowed.")
            }
        }
    }

    /// Delivers the user's answer. False if the request is unknown or was
    /// already answered, timed out, or cancelled.
    pub fn answer(&self, id: &str, decision: Decision) -> bool {
        let sender = self.pending.lock().unwrap().remove(id);
        sender.is_some_and(|sender| sender.send(decision).is_ok())
    }

    /// Denies everything still pending: the run was cancelled or the app is
    /// quitting.
    pub fn cancel_all(&self) {
        let pending: Vec<_> = self.pending.lock().unwrap().drain().collect();
        for (_, sender) in pending {
            let _ = sender.send(Decision::deny("The run was cancelled."));
        }
    }

    pub fn pending_count(&self) -> usize {
        self.pending.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUICK: Duration = Duration::from_millis(60);

    async fn next_request(rx: &mut mpsc::UnboundedReceiver<ApprovalRequest>) -> ApprovalRequest {
        tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("a request should arrive")
            .expect("channel open")
    }

    #[tokio::test]
    async fn an_allowed_request_returns_allow() {
        let (broker, mut ui) = Broker::new(APPROVAL_TIMEOUT);
        let asker = Arc::clone(&broker);
        let asked = tokio::spawn(async move {
            asker
                .ask(
                    "Write",
                    &json!({"file_path": "/w/hello.txt", "content": "hi"}),
                )
                .await
        });

        let request = next_request(&mut ui).await;
        assert!(broker.answer(&request.id, Decision::Allow));

        assert_eq!(asked.await.unwrap(), Decision::Allow);
        assert_eq!(broker.pending_count(), 0);
    }

    #[tokio::test]
    async fn a_denied_request_carries_the_reason() {
        let (broker, mut ui) = Broker::new(APPROVAL_TIMEOUT);
        let asker = Arc::clone(&broker);
        let asked =
            tokio::spawn(async move { asker.ask("Bash", &json!({"command": "rm -rf x"})).await });

        let request = next_request(&mut ui).await;
        broker.answer(&request.id, Decision::deny(USER_DENIED_MESSAGE));

        assert_eq!(asked.await.unwrap(), Decision::deny(USER_DENIED_MESSAGE));
    }

    #[tokio::test]
    async fn an_unanswered_request_is_denied_after_the_timeout() {
        let (broker, mut ui) = Broker::new(QUICK);

        let decision = broker.ask("Bash", &json!({"command": "ls"})).await;

        assert!(matches!(decision, Decision::Deny { message } if message.contains("no answer")));
        assert_eq!(broker.pending_count(), 0);
        let request = next_request(&mut ui).await;
        assert!(
            !broker.answer(&request.id, Decision::Allow),
            "too late to answer"
        );
    }

    #[tokio::test]
    async fn cancelling_denies_every_pending_request() {
        let (broker, mut ui) = Broker::new(APPROVAL_TIMEOUT);
        let tasks: Vec<_> = (0..3)
            .map(|n| {
                let asker = Arc::clone(&broker);
                tokio::spawn(async move {
                    asker
                        .ask("Bash", &json!({"command": format!("cmd {n}")}))
                        .await
                })
            })
            .collect();
        for _ in 0..3 {
            next_request(&mut ui).await;
        }
        assert_eq!(broker.pending_count(), 3);

        broker.cancel_all();

        for task in tasks {
            assert!(
                matches!(task.await.unwrap(), Decision::Deny { message } if message.contains("cancelled"))
            );
        }
        assert_eq!(broker.pending_count(), 0);
    }

    #[tokio::test]
    async fn answering_twice_or_an_unknown_id_does_nothing() {
        let (broker, mut ui) = Broker::new(APPROVAL_TIMEOUT);
        let asker = Arc::clone(&broker);
        let asked = tokio::spawn(async move { asker.ask("Write", &json!({})).await });
        let request = next_request(&mut ui).await;

        assert!(!broker.answer("no-such-id", Decision::Allow));
        assert!(broker.answer(&request.id, Decision::Allow));
        assert!(!broker.answer(&request.id, Decision::deny("late")));
        assert_eq!(asked.await.unwrap(), Decision::Allow);
    }

    #[tokio::test]
    async fn with_no_ui_listening_it_denies_at_once() {
        let (broker, ui) = Broker::new(APPROVAL_TIMEOUT);
        drop(ui);

        let decision = broker.ask("Bash", &json!({"command": "ls"})).await;

        assert!(matches!(decision, Decision::Deny { message } if message.contains("nobody")));
        assert_eq!(broker.pending_count(), 0);
    }

    #[tokio::test]
    async fn concurrent_requests_get_distinct_ids_and_independent_answers() {
        let (broker, mut ui) = Broker::new(APPROVAL_TIMEOUT);
        let (a, b) = (Arc::clone(&broker), Arc::clone(&broker));
        let first = tokio::spawn(async move { a.ask("Write", &json!({"file_path": "/a"})).await });
        let second = tokio::spawn(async move { b.ask("Write", &json!({"file_path": "/b"})).await });
        let one = next_request(&mut ui).await;
        let two = next_request(&mut ui).await;
        assert_ne!(one.id, two.id);

        let (allow_id, deny_id) = if one.detail.as_deref() == Some("/a") {
            (one.id, two.id)
        } else {
            (two.id, one.id)
        };
        broker.answer(&allow_id, Decision::Allow);
        broker.answer(&deny_id, Decision::deny("no"));

        assert_eq!(first.await.unwrap(), Decision::Allow);
        assert_eq!(second.await.unwrap(), Decision::deny("no"));
    }

    #[test]
    fn an_action_is_summarised_like_the_activity_label() {
        let request = describe_request(
            "1".into(),
            "Write",
            &json!({"file_path": "/Users/me/BuddyWorkspace/hello.txt", "content": "hi"}),
        );

        assert_eq!(request.kind, ApprovalKind::Action);
        assert_eq!(request.headline, "✍️ writing hello.txt");
        assert_eq!(
            request.detail.as_deref(),
            Some("/Users/me/BuddyWorkspace/hello.txt")
        );
    }

    #[test]
    fn a_command_shows_in_full() {
        let request = describe_request(
            "1".into(),
            "Bash",
            &json!({"command": "npm install && npm test"}),
        );

        assert_eq!(request.headline, "💻 running npm install && npm test");
        assert_eq!(request.detail.as_deref(), Some("npm install && npm test"));
    }

    #[test]
    fn web_requests_show_their_target() {
        let fetch = describe_request(
            "1".into(),
            "WebFetch",
            &json!({"url": "https://example.com/a?b=1"}),
        );
        let search = describe_request("2".into(), "WebSearch", &json!({"query": "tauri tray"}));

        assert_eq!(fetch.detail.as_deref(), Some("https://example.com/a?b=1"));
        assert_eq!(search.detail.as_deref(), Some("tauri tray"));
    }

    #[test]
    fn an_unknown_tool_shows_its_input_as_json_or_nothing() {
        let with_input = describe_request("1".into(), "mcp__x__y", &json!({"a": 1}));
        let without = describe_request("2".into(), "mcp__x__y", &json!({}));

        assert_eq!(with_input.detail.as_deref(), Some(r#"{"a":1}"#));
        assert_eq!(without.detail, None);
        assert_eq!(with_input.headline, "🔧 using a tool");
    }

    #[test]
    fn exit_plan_mode_is_a_plan_with_the_plan_text() {
        let request = describe_request(
            "1".into(),
            "ExitPlanMode",
            &json!({"plan": "# Plan\n1. Do it", "planFilePath": "/x/plan.md"}),
        );

        assert_eq!(request.kind, ApprovalKind::Plan);
        assert_eq!(request.detail.as_deref(), Some("# Plan\n1. Do it"));
    }

    #[test]
    fn a_plan_without_text_has_no_detail() {
        assert_eq!(
            describe_request("1".into(), "ExitPlanMode", &json!({})).detail,
            None
        );
    }

    #[test]
    fn very_long_details_are_cut() {
        let command = "x".repeat(2000);

        let request = describe_request("1".into(), "Bash", &json!({ "command": command }));

        let detail = request.detail.unwrap();
        assert_eq!(detail.chars().count(), MAX_DETAIL_CHARS);
        assert!(detail.ends_with('…'));
    }

    #[test]
    fn the_request_serialises_for_the_frontend() {
        let request = describe_request("abc".into(), "Bash", &json!({"command": "ls"}));

        let json = serde_json::to_value(&request).unwrap();

        assert_eq!(json["id"], "abc");
        assert_eq!(json["kind"], "action");
        assert_eq!(json["tool_name"], "Bash");
    }

    #[test]
    fn allow_returns_the_input_unchanged_to_claude() {
        let input = json!({"file_path": "/a", "content": "hi"});

        let verdict: Value = serde_json::from_str(&verdict_json(&input, &Decision::Allow)).unwrap();

        assert_eq!(verdict, json!({"behavior": "allow", "updatedInput": input}));
    }

    #[test]
    fn deny_returns_the_reason_to_claude() {
        let verdict: Value = serde_json::from_str(&verdict_json(
            &json!({}),
            &Decision::deny("Keep planning. User feedback: shorter"),
        ))
        .unwrap();

        assert_eq!(
            verdict,
            json!({"behavior": "deny", "message": "Keep planning. User feedback: shorter"})
        );
    }
}
