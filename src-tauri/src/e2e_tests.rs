//! Opt-in end-to-end checks against the real `claude` CLI. They spend a little
//! of your Claude usage, so they are skipped by default:
//!
//!     cargo test --manifest-path src-tauri/Cargo.toml -- --ignored real_claude
//!
//! They wire the real pieces together: the runner, the MCP permission server,
//! the broker, and your installed, logged-in `claude`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::claude_runner::{
    ClaudeRunner, PermissionMode, RunOptions, RunRequest, STOP_GRACE, check_login, locate_claude,
};
use crate::perm_broker::{
    APPROVAL_TIMEOUT, ApprovalRequest, Broker, Decision, USER_DENIED_MESSAGE,
};
use crate::perm_server;
use crate::platform::ShellEnv;
use crate::platform::macos::{MacProcessCtl, MacShellEnv};
use crate::stream_parser::BuddyEvent;

struct Outcome {
    asked: Vec<ApprovalRequest>,
    events: Vec<BuddyEvent>,
    workspace: tempfile::TempDir,
}

/// Runs one prompt through the real stack, answering every approval with
/// `decision`.
async fn run_real_claude(prompt: &str, decision: Decision) -> Outcome {
    let path_env = MacShellEnv::default().path();
    let home = PathBuf::from(std::env::var("HOME").expect("HOME is set"));
    let claude_bin = locate_claude(&path_env, &home).expect("claude is installed");
    check_login(&claude_bin, &path_env).expect("claude is logged in");

    let (broker, mut ui) = Broker::new(APPROVAL_TIMEOUT);
    let server = perm_server::start(Arc::clone(&broker))
        .await
        .expect("server starts");

    let asked = Arc::new(Mutex::new(Vec::new()));
    let answerer = {
        let (broker, asked) = (Arc::clone(&broker), Arc::clone(&asked));
        tokio::spawn(async move {
            while let Some(request) = ui.recv().await {
                broker.answer(&request.id, decision.clone());
                asked.lock().unwrap().push(request);
            }
        })
    };

    let workspace = tempfile::tempdir().unwrap();
    let request = RunRequest {
        claude_bin,
        path_env,
        workspace: workspace.path().to_path_buf(),
        prompt: prompt.to_string(),
        options: RunOptions {
            session_id: None,
            mode: PermissionMode::Default,
            model: None,
            effort: None,
            add_dirs: Vec::new(),
            mcp_config: server.mcp_config(),
            persona_path: None,
        },
    };
    let events = tokio::task::spawn_blocking(move || {
        let runner = ClaudeRunner::new(Arc::new(MacProcessCtl), STOP_GRACE);
        let mut events = Vec::new();
        runner
            .run(request, |event| events.push(event))
            .expect("the run starts");
        events
    })
    .await
    .unwrap();

    answerer.abort();
    drop(server);
    let asked = asked.lock().unwrap().clone();
    Outcome {
        asked,
        events,
        workspace,
    }
}

const WRITE_PROMPT: &str = "Create a file named hello.txt in the current directory containing the text hi. Then reply done.";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real claude CLI and uses your Claude usage"]
async fn real_claude_asks_the_buddy_and_writes_the_file_when_allowed() {
    let outcome = run_real_claude(WRITE_PROMPT, Decision::Allow).await;

    let write = outcome
        .asked
        .iter()
        .find(|request| request.tool_name == "Write")
        .expect("claude should have asked to Write through the buddy's permission server");
    assert!(
        write
            .detail
            .as_deref()
            .is_some_and(|path| path.ends_with("hello.txt"))
    );
    let written = std::fs::read_to_string(outcome.workspace.path().join("hello.txt"))
        .expect("the allowed write should have created the file");
    assert!(written.contains("hi"));
    assert!(matches!(
        outcome.events.last(),
        Some(BuddyEvent::Finished {
            is_error: false,
            ..
        })
    ));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real claude CLI and uses your Claude usage"]
async fn real_claude_cannot_write_the_file_when_the_buddy_denies() {
    let outcome = run_real_claude(WRITE_PROMPT, Decision::deny(USER_DENIED_MESSAGE)).await;

    assert!(
        outcome
            .asked
            .iter()
            .any(|request| request.tool_name == "Write")
    );
    assert!(
        !outcome.workspace.path().join("hello.txt").exists(),
        "a denied write must not create the file"
    );
    assert!(
        outcome
            .events
            .iter()
            .any(|event| matches!(event, BuddyEvent::ToolFinished { ok: false, .. }))
    );
}
