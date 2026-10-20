//! Turns `claude -p --output-format stream-json` lines into app events
//! (design §9.1). Pure: no I/O, so it is tested against real recorded runs in
//! `tests/fixtures/` (see docs/spikes.md).
//!
//! One line can carry several events (an assistant message with two tool
//! calls), so [`parse_line`] returns a list; ignored, unknown and malformed
//! lines give an empty list rather than an error, because the stream also
//! carries hook, status and rate-limit noise the buddy has no use for.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum BuddyEvent {
    /// The session started (or was resumed): its id and the model in use.
    SessionStarted {
        id: String,
        model: String,
    },
    /// A piece of Claude's streaming answer.
    TextDelta {
        text: String,
    },
    /// Claude is calling a tool; `id` matches the later [`Self::ToolFinished`].
    ToolStarted {
        id: String,
        name: String,
        input: Value,
    },
    ToolFinished {
        id: String,
        ok: bool,
    },
    /// The API request is being retried.
    Retry {
        attempt: u64,
    },
    /// The run is over (always the last event).
    Finished {
        text: String,
        is_error: bool,
        cost_usd: Option<f64>,
        /// Tokens now in the conversation's context, when the stream says.
        context_tokens: Option<u64>,
        /// The model's context window, when the stream says.
        context_window: Option<u64>,
    },
}

pub fn parse_line(line: &str) -> Vec<BuddyEvent> {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };
    match value.get("type").and_then(Value::as_str) {
        Some("system") => parse_system(&value),
        Some("stream_event") => parse_stream_event(&value),
        Some("assistant") => parse_assistant(&value),
        Some("user") => parse_user(&value),
        Some("result") => vec![parse_result(&value)],
        _ => Vec::new(),
    }
}

fn text_of<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn parse_system(value: &Value) -> Vec<BuddyEvent> {
    match text_of(value, "subtype") {
        Some("init") => match (text_of(value, "session_id"), text_of(value, "model")) {
            (Some(id), Some(model)) => vec![BuddyEvent::SessionStarted {
                id: id.to_string(),
                model: model.to_string(),
            }],
            _ => Vec::new(),
        },
        Some("api_retry") => {
            let attempt = value.get("attempt").and_then(Value::as_u64).unwrap_or(1);
            vec![BuddyEvent::Retry { attempt }]
        }
        _ => Vec::new(),
    }
}

/// True for output that belongs to a sub-agent, not Claude's own answer.
fn is_from_subagent(value: &Value) -> bool {
    value
        .get("parent_tool_use_id")
        .is_some_and(|parent| !parent.is_null())
}

fn parse_stream_event(value: &Value) -> Vec<BuddyEvent> {
    if is_from_subagent(value) {
        return Vec::new();
    }
    let delta = &value["event"]["delta"];
    match (text_of(delta, "type"), text_of(delta, "text")) {
        (Some("text_delta"), Some(text)) if !text.is_empty() => {
            vec![BuddyEvent::TextDelta {
                text: text.to_string(),
            }]
        }
        _ => Vec::new(),
    }
}

fn content_blocks(value: &Value) -> &[Value] {
    value["message"]["content"]
        .as_array()
        .map_or(&[], Vec::as_slice)
}

fn parse_assistant(value: &Value) -> Vec<BuddyEvent> {
    content_blocks(value)
        .iter()
        .filter(|block| text_of(block, "type") == Some("tool_use"))
        .filter_map(|block| {
            Some(BuddyEvent::ToolStarted {
                id: text_of(block, "id")?.to_string(),
                name: text_of(block, "name")?.to_string(),
                input: block.get("input").cloned().unwrap_or(Value::Null),
            })
        })
        .collect()
}

fn parse_user(value: &Value) -> Vec<BuddyEvent> {
    content_blocks(value)
        .iter()
        .filter(|block| text_of(block, "type") == Some("tool_result"))
        .filter_map(|block| {
            let is_error = block
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Some(BuddyEvent::ToolFinished {
                id: text_of(block, "tool_use_id")?.to_string(),
                ok: !is_error,
            })
        })
        .collect()
}

fn token_count(usage: &Value, key: &str) -> u64 {
    usage.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// Tokens in context after the run: input + cache reads + cache writes of
/// the *last* model call (the top-level `usage` adds up every call of a
/// multi-step run).
fn context_tokens(result: &Value) -> Option<u64> {
    let usage = result.get("usage")?;
    let last_call = usage
        .get("iterations")
        .and_then(Value::as_array)
        .and_then(|calls| calls.last())
        .unwrap_or(usage);
    let total = [
        "input_tokens",
        "cache_read_input_tokens",
        "cache_creation_input_tokens",
    ]
    .iter()
    .map(|key| token_count(last_call, key))
    .sum();
    (total > 0).then_some(total)
}

/// The largest context window among the models used.
fn context_window(result: &Value) -> Option<u64> {
    result
        .get("modelUsage")?
        .as_object()?
        .values()
        .filter_map(|model| model.get("contextWindow").and_then(Value::as_u64))
        .max()
}

fn result_text(result: &Value) -> String {
    if let Some(text) = text_of(result, "result") {
        return text.to_string();
    }
    let errors: Vec<&str> = result["errors"]
        .as_array()
        .map(|list| list.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if errors.is_empty() {
        String::new()
    } else {
        errors.join("\n")
    }
}

fn parse_result(result: &Value) -> BuddyEvent {
    let is_error = result
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| text_of(result, "subtype") != Some("success"));
    BuddyEvent::Finished {
        text: result_text(result),
        is_error,
        cost_usd: result.get("total_cost_usd").and_then(Value::as_f64),
        context_tokens: context_tokens(result),
        context_window: context_window(result),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT_RUN: &str = include_str!("../tests/fixtures/text_run.jsonl");
    const DENIED_RUN: &str = include_str!("../tests/fixtures/denied_tool_run.jsonl");
    const READ_RUN: &str = include_str!("../tests/fixtures/read_image_run.jsonl");
    const PLAN_RUN: &str = include_str!("../tests/fixtures/plan_run.jsonl");
    const COMPACT_RUN: &str = include_str!("../tests/fixtures/compact_run.jsonl");

    fn events(run: &str) -> Vec<BuddyEvent> {
        run.lines().flat_map(parse_line).collect()
    }

    fn finished(run: &str) -> BuddyEvent {
        events(run).pop().expect("a run always ends")
    }

    #[test]
    fn a_text_run_streams_start_text_and_finish() {
        let all = events(TEXT_RUN);

        assert_eq!(
            all.first(),
            Some(&BuddyEvent::SessionStarted {
                id: "9c9d51f2-d4e8-466c-9840-26a353275d95".into(),
                model: "claude-sonnet-5-5".into(),
            })
        );
        assert!(all.contains(&BuddyEvent::TextDelta {
            text: "PONG".into()
        }));
        assert!(matches!(all.last(), Some(BuddyEvent::Finished { .. })));
    }

    #[test]
    fn hook_status_and_rate_limit_lines_are_ignored() {
        for line in TEXT_RUN.lines() {
            let kind = serde_json::from_str::<Value>(line).unwrap();
            let noisy = matches!(
                kind["subtype"].as_str(),
                Some("hook_started" | "hook_response" | "status" | "commands_changed")
            ) || kind["type"] == "rate_limit_event";
            if noisy {
                assert!(parse_line(line).is_empty(), "should ignore: {line}");
            }
        }
    }

    #[test]
    fn finished_reads_text_cost_and_context_from_the_result() {
        let BuddyEvent::Finished {
            text,
            is_error,
            cost_usd,
            context_tokens,
            context_window,
        } = finished(TEXT_RUN)
        else {
            panic!("last event should be Finished");
        };

        assert_eq!(text, "PONG");
        assert!(!is_error);
        assert!(cost_usd.unwrap() > 0.0);
        assert_eq!(context_tokens, Some(2 + 8857 + 36078));
        assert_eq!(context_window, Some(1_000_000));
    }

    #[test]
    fn a_denied_tool_call_starts_then_finishes_with_an_error() {
        let all = events(DENIED_RUN);

        let write = all
            .iter()
            .find_map(|event| match event {
                BuddyEvent::ToolStarted { id, name, input } if name == "Write" => {
                    Some((id.clone(), input.clone()))
                }
                _ => None,
            })
            .expect("the Write call is reported");
        assert!(write.1["file_path"].as_str().unwrap().ends_with("note.txt"));
        assert!(all.contains(&BuddyEvent::ToolFinished {
            id: write.0,
            ok: false
        }));
    }

    #[test]
    fn a_denied_run_still_finishes_normally() {
        assert!(matches!(
            finished(DENIED_RUN),
            BuddyEvent::Finished { text, is_error: false, .. } if text == "Refused."
        ));
    }

    #[test]
    fn a_successful_read_finishes_ok() {
        let all = events(READ_RUN);

        let (id, name) = all
            .iter()
            .find_map(|event| match event {
                BuddyEvent::ToolStarted { id, name, .. } => Some((id.clone(), name.clone())),
                _ => None,
            })
            .expect("the Read call is reported");
        assert_eq!(name, "Read");
        assert!(all.contains(&BuddyEvent::ToolFinished { id, ok: true }));
    }

    #[test]
    fn plan_mode_reports_exit_plan_mode_with_an_empty_input() {
        let exit = events(PLAN_RUN).into_iter().find_map(|event| match event {
            BuddyEvent::ToolStarted { name, input, .. } if name == "ExitPlanMode" => Some(input),
            _ => None,
        });

        assert_eq!(exit, Some(serde_json::json!({})));
    }

    #[test]
    fn compaction_produces_only_a_finished_event() {
        let all = events(COMPACT_RUN);

        assert!(matches!(
            all.last(),
            Some(BuddyEvent::Finished { text, is_error: false, .. }) if text.is_empty()
        ));
    }

    #[test]
    fn context_uses_the_last_model_call_not_the_sum_of_a_multi_step_run() {
        let line = r#"{"type":"result","subtype":"success","is_error":false,"result":"x",
            "usage":{"input_tokens":30,"cache_read_input_tokens":5000,"cache_creation_input_tokens":0,
              "iterations":[{"input_tokens":10,"cache_read_input_tokens":1000,"cache_creation_input_tokens":0},
                            {"input_tokens":20,"cache_read_input_tokens":4000,"cache_creation_input_tokens":0}]}}"#
            .replace('\n', "");

        let BuddyEvent::Finished { context_tokens, .. } = parse_line(&line).remove(0) else {
            panic!("expected Finished");
        };

        assert_eq!(context_tokens, Some(4020));
    }

    #[test]
    fn context_falls_back_to_top_level_usage_without_iterations() {
        let line = r#"{"type":"result","is_error":false,"result":"x","usage":{"input_tokens":7,"cache_read_input_tokens":3}}"#;

        let BuddyEvent::Finished { context_tokens, .. } = parse_line(line).remove(0) else {
            panic!("expected Finished");
        };

        assert_eq!(context_tokens, Some(10));
    }

    #[test]
    fn missing_usage_means_no_context_figures() {
        let line = r#"{"type":"result","subtype":"success","is_error":false,"result":"x"}"#;

        assert_eq!(
            parse_line(line),
            vec![BuddyEvent::Finished {
                text: "x".into(),
                is_error: false,
                cost_usd: None,
                context_tokens: None,
                context_window: None,
            }]
        );
    }

    #[test]
    fn an_error_result_without_text_uses_the_errors_list() {
        let line = r#"{"type":"result","subtype":"error_during_execution","is_error":true,
            "errors":["No conversation found with session ID: abc"]}"#
            .replace('\n', "");

        assert!(matches!(
            parse_line(&line).remove(0),
            BuddyEvent::Finished { text, is_error: true, .. }
                if text == "No conversation found with session ID: abc"
        ));
    }

    #[test]
    fn a_non_success_result_is_an_error_even_without_the_flag() {
        let line = r#"{"type":"result","subtype":"error_max_turns","result":"stopped"}"#;

        assert!(matches!(
            parse_line(line).remove(0),
            BuddyEvent::Finished { is_error: true, .. }
        ));
    }

    #[test]
    fn api_retry_reports_the_attempt() {
        let line = r#"{"type":"system","subtype":"api_retry","attempt":2}"#;

        assert_eq!(parse_line(line), vec![BuddyEvent::Retry { attempt: 2 }]);
    }

    #[test]
    fn one_assistant_message_can_start_several_tools() {
        let line = r#"{"type":"assistant","message":{"content":[
            {"type":"text","text":"ok"},
            {"type":"tool_use","id":"a","name":"Read","input":{"file_path":"/x"}},
            {"type":"tool_use","id":"b","name":"Bash","input":{"command":"ls"}}]}}"#
            .replace('\n', "");

        let started = parse_line(&line);

        assert_eq!(started.len(), 2);
    }

    #[test]
    fn text_from_a_subagent_is_not_streamed() {
        let line = r#"{"type":"stream_event","parent_tool_use_id":"toolu_1",
            "event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"hi"}}}"#
            .replace('\n', "");

        assert!(parse_line(&line).is_empty());
    }

    #[test]
    fn empty_text_deltas_are_dropped() {
        let line = r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":""}}}"#;

        assert!(parse_line(line).is_empty());
    }

    #[test]
    fn input_json_deltas_are_not_text() {
        let line = r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"input_json_delta","partial_json":"{"}}}"#;

        assert!(parse_line(line).is_empty());
    }

    #[test]
    fn malformed_unknown_and_blank_lines_give_nothing() {
        for line in [
            "",
            "   ",
            "not json",
            "{",
            "[]",
            "42",
            r#"{"type":"mystery"}"#,
            r#"{"no":"type"}"#,
        ] {
            assert!(parse_line(line).is_empty(), "should ignore {line:?}");
        }
    }

    #[test]
    fn an_init_without_a_model_is_ignored() {
        let line = r#"{"type":"system","subtype":"init","session_id":"s"}"#;

        assert!(parse_line(line).is_empty());
    }
}
