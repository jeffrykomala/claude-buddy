//! What the pet acts out while Claude works (design §8.2): each tool call maps
//! to a pose, an icon and a short label. Pure; the approval bubble reuses it
//! to summarise what Claude wants to do.

use serde::Serialize;
use serde_json::Value;

/// Labels are cut to this many characters (design §8.2).
const MAX_LABEL_CHARS: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityPose {
    Read,
    Write,
    Run,
    Browse,
    Think,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Activity {
    pub pose: ActivityPose,
    pub icon: &'static str,
    pub label: String,
}

fn activity(pose: ActivityPose, icon: &'static str, label: String) -> Activity {
    Activity {
        pose,
        icon,
        label: truncate(&label),
    }
}

/// Cuts `text` to [`MAX_LABEL_CHARS`] characters, ending in "…" when cut.
fn truncate(text: &str) -> String {
    if text.chars().count() <= MAX_LABEL_CHARS {
        return text.to_string();
    }
    let kept: String = text.chars().take(MAX_LABEL_CHARS - 1).collect();
    format!("{kept}…")
}

fn field<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    input
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

/// The file name of a path ("/a/b/main.rs" -> "main.rs").
fn file_name(path: &str) -> &str {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
}

/// The host of a URL ("https://example.com/a" -> "example.com").
fn host(url: &str) -> &str {
    let without_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(without_scheme)
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or("").trim()
}

fn labelled(verb: &str, target: Option<&str>) -> String {
    match target {
        Some(target) => format!("{verb} {target}"),
        None => verb.to_string(),
    }
}

/// Describes one tool call for the pet's pose and label.
pub fn describe(tool_name: &str, input: &Value) -> Activity {
    use ActivityPose::*;
    match tool_name {
        "Read" => activity(
            Read,
            "📖",
            labelled("reading", field(input, "file_path").map(file_name)),
        ),
        "Glob" => activity(Read, "📖", labelled("finding", field(input, "pattern"))),
        "Grep" => activity(Read, "📖", labelled("searching", field(input, "pattern"))),
        "Write" | "Edit" | "NotebookEdit" => {
            let path = field(input, "file_path").or_else(|| field(input, "notebook_path"));
            activity(Write, "✍️", labelled("writing", path.map(file_name)))
        }
        "Bash" => activity(
            Run,
            "💻",
            labelled("running", field(input, "command").map(first_line)),
        ),
        "WebSearch" => {
            let query = field(input, "query").map(|q| format!("\"{q}\""));
            activity(Browse, "🌐", labelled("searching", query.as_deref()))
        }
        "WebFetch" => activity(
            Browse,
            "🌐",
            labelled("fetching", field(input, "url").map(host)),
        ),
        "Agent" | "Task" => activity(Think, "🧠", "thinking hard…".to_string()),
        _ => activity(Think, "🔧", "using a tool".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn check(tool: &str, input: Value, pose: ActivityPose, icon: &str, label: &str) {
        let got = describe(tool, &input);

        assert_eq!(got.pose, pose, "{tool} pose");
        assert_eq!(got.icon, icon, "{tool} icon");
        assert_eq!(got.label, label, "{tool} label");
    }

    #[test]
    fn read_shows_the_file_name() {
        check(
            "Read",
            json!({"file_path": "/Users/me/project/src/main.rs"}),
            ActivityPose::Read,
            "📖",
            "reading main.rs",
        );
    }

    #[test]
    fn glob_and_grep_show_their_pattern_and_use_the_read_pose() {
        check(
            "Glob",
            json!({"pattern": "**/*.ts"}),
            ActivityPose::Read,
            "📖",
            "finding **/*.ts",
        );
        check(
            "Grep",
            json!({"pattern": "TODO"}),
            ActivityPose::Read,
            "📖",
            "searching TODO",
        );
    }

    #[test]
    fn write_edit_and_notebook_edit_use_the_write_pose() {
        check(
            "Write",
            json!({"file_path": "/a/notes.md"}),
            ActivityPose::Write,
            "✍️",
            "writing notes.md",
        );
        check(
            "Edit",
            json!({"file_path": "/a/b/lib.rs"}),
            ActivityPose::Write,
            "✍️",
            "writing lib.rs",
        );
        check(
            "NotebookEdit",
            json!({"notebook_path": "/a/analysis.ipynb"}),
            ActivityPose::Write,
            "✍️",
            "writing analysis.ipynb",
        );
    }

    #[test]
    fn bash_shows_the_first_line_of_the_command() {
        check(
            "Bash",
            json!({"command": "npm test"}),
            ActivityPose::Run,
            "💻",
            "running npm test",
        );
        check(
            "Bash",
            json!({"command": "  ls -la\necho done"}),
            ActivityPose::Run,
            "💻",
            "running ls -la",
        );
    }

    #[test]
    fn web_tools_use_the_browse_pose() {
        check(
            "WebSearch",
            json!({"query": "tauri drag drop"}),
            ActivityPose::Browse,
            "🌐",
            "searching \"tauri drag drop\"",
        );
        check(
            "WebFetch",
            json!({"url": "https://tauri.app/v2/guide?x=1"}),
            ActivityPose::Browse,
            "🌐",
            "fetching tauri.app",
        );
    }

    #[test]
    fn subagents_think_hard() {
        check(
            "Agent",
            json!({}),
            ActivityPose::Think,
            "🧠",
            "thinking hard…",
        );
        check(
            "Task",
            json!({}),
            ActivityPose::Think,
            "🧠",
            "thinking hard…",
        );
    }

    #[test]
    fn anything_else_is_a_generic_tool_use() {
        check(
            "mcp__github__list_issues",
            json!({}),
            ActivityPose::Think,
            "🔧",
            "using a tool",
        );
        check(
            "SomethingNew",
            json!(null),
            ActivityPose::Think,
            "🔧",
            "using a tool",
        );
    }

    #[test]
    fn missing_or_empty_fields_fall_back_to_the_bare_verb() {
        check("Read", json!({}), ActivityPose::Read, "📖", "reading");
        check(
            "Write",
            json!({"file_path": ""}),
            ActivityPose::Write,
            "✍️",
            "writing",
        );
        check(
            "Bash",
            json!({"command": 5}),
            ActivityPose::Run,
            "💻",
            "running",
        );
        check(
            "WebFetch",
            json!(null),
            ActivityPose::Browse,
            "🌐",
            "fetching",
        );
    }

    #[test]
    fn long_labels_are_cut_to_forty_characters_with_an_ellipsis() {
        let command = "cargo build --release --target aarch64-apple-darwin --features everything";

        let got = describe("Bash", &json!({ "command": command }));

        assert_eq!(got.label.chars().count(), 40);
        assert!(got.label.starts_with("running cargo build --release"));
        assert!(got.label.ends_with('…'));
    }

    #[test]
    fn a_label_of_exactly_forty_characters_is_kept_whole() {
        let command = "a".repeat(40 - "running ".len());

        let got = describe("Bash", &json!({ "command": command }));

        assert_eq!(got.label.chars().count(), 40);
        assert!(!got.label.ends_with('…'));
    }

    #[test]
    fn cutting_counts_characters_not_bytes() {
        let command = "é".repeat(60);

        let got = describe("Bash", &json!({ "command": command }));

        assert_eq!(got.label.chars().count(), 40);
    }

    #[test]
    fn file_name_handles_trailing_slashes_and_backslashes() {
        assert_eq!(file_name("/a/b/"), "b");
        assert_eq!(file_name("C:\\a\\b.txt"), "b.txt");
        assert_eq!(file_name("plain.txt"), "plain.txt");
    }

    #[test]
    fn the_pose_serialises_in_snake_case_for_the_frontend() {
        let json = serde_json::to_string(&describe("Read", &json!({}))).unwrap();

        assert!(json.contains(r#""pose":"read""#));
    }
}
