//! The MCP permission server (design §9.1): a tiny streamable-HTTP MCP
//! endpoint with one tool, `approve(tool_name, input)`, that Claude Code calls
//! before every action (`--permission-prompt-tool mcp__buddy__approve`).
//!
//! Locked down because it decides what Claude may do on this Mac: it listens on
//! 127.0.0.1 only, on a random port, and every request must carry this
//! launch's random bearer token. Browser-style requests (any `Origin` header,
//! or a `Host` that isn't ours) are refused too, so a web page can't talk to it
//! even by DNS tricks.
//!
//! Hand-rolled on `axum` rather than an MCP SDK: the protocol surface is four
//! JSON-RPC methods, answered with plain JSON (allowed for POST by the
//! streamable-HTTP spec), and the exact shapes were checked against the real
//! `claude` CLI (docs/spikes.md).

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use crate::perm_broker::{Broker, verdict_json};

const SERVER_NAME: &str = "buddy";
const TOOL_NAME: &str = "approve";
const MCP_PATH: &str = "/mcp";
const MAX_BODY_BYTES: usize = 1_000_000;
const FALLBACK_PROTOCOL_VERSION: &str = "2025-03-26";
const START_ATTEMPTS: usize = 3;

// JSON-RPC error codes.
const PARSE_ERROR: i64 = -32700;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

struct ServerState {
    broker: Arc<Broker>,
    token: String,
    /// The only `Host` header values we answer to.
    allowed_hosts: Vec<String>,
}

fn rpc_result(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn rpc_error(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn json_response(status: StatusCode, body: &Value) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}

/// Compares in time that doesn't depend on where the first difference is.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn authorize(state: &ServerState, headers: &HeaderMap) -> Result<(), StatusCode> {
    let supplied = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    match supplied {
        Some(token) if constant_time_eq(token.as_bytes(), state.token.as_bytes()) => {}
        _ => return Err(StatusCode::UNAUTHORIZED),
    }
    let host_ok = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|host| state.allowed_hosts.iter().any(|allowed| allowed == host));
    if headers.contains_key(header::ORIGIN) || !host_ok {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

fn tool_definition() -> Value {
    json!({
        "name": TOOL_NAME,
        "description": "Ask the user whether Claude may run a tool.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "tool_name": { "type": "string" },
                "input": { "type": "object" }
            },
            "required": ["tool_name", "input"]
        }
    })
}

async fn call_tool(state: &ServerState, id: &Value, params: &Value) -> Value {
    if params.get("name").and_then(Value::as_str) != Some(TOOL_NAME) {
        return rpc_error(id, INVALID_PARAMS, "Unknown tool");
    }
    let arguments = &params["arguments"];
    let Some(tool_name) = arguments.get("tool_name").and_then(Value::as_str) else {
        return rpc_error(id, INVALID_PARAMS, "tool_name is required");
    };
    let input = arguments.get("input").cloned().unwrap_or_else(|| json!({}));

    let decision = state.broker.ask(tool_name, &input).await;
    rpc_result(
        id,
        json!({ "content": [{ "type": "text", "text": verdict_json(&input, &decision) }] }),
    )
}

/// The reply to one JSON-RPC message, or `None` for a notification.
async fn handle_message(state: &ServerState, message: &Value) -> Option<Value> {
    let id = message.get("id")?;
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let params = message.get("params").unwrap_or(&Value::Null);
    Some(match method {
        "initialize" => {
            let version = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(FALLBACK_PROTOCOL_VERSION);
            rpc_result(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") }
                }),
            )
        }
        "ping" => rpc_result(id, json!({})),
        "tools/list" => rpc_result(id, json!({ "tools": [tool_definition()] })),
        "tools/call" => call_tool(state, id, params).await,
        _ => rpc_error(id, METHOD_NOT_FOUND, "Method not found"),
    })
}

async fn mcp(State(state): State<Arc<ServerState>>, headers: HeaderMap, body: Bytes) -> Response {
    if let Err(status) = authorize(&state, &headers) {
        return status.into_response();
    }
    let Ok(message) = serde_json::from_slice::<Value>(&body) else {
        return json_response(
            StatusCode::BAD_REQUEST,
            &rpc_error(&Value::Null, PARSE_ERROR, "Parse error"),
        );
    };
    match message {
        Value::Array(batch) => {
            let mut replies = Vec::new();
            for item in &batch {
                replies.extend(handle_message(&state, item).await);
            }
            if replies.is_empty() {
                StatusCode::ACCEPTED.into_response()
            } else {
                json_response(StatusCode::OK, &Value::Array(replies))
            }
        }
        single => match handle_message(&state, &single).await {
            Some(reply) => json_response(StatusCode::OK, &reply),
            None => StatusCode::ACCEPTED.into_response(),
        },
    }
}

fn router(broker: Arc<Broker>, token: String, allowed_hosts: Vec<String>) -> Router {
    let state = Arc::new(ServerState {
        broker,
        token,
        allowed_hosts,
    });
    Router::new()
        .route(MCP_PATH, post(mcp))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(state)
}

fn new_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// A running server. Dropping it shuts the server down.
pub struct PermServer {
    port: u16,
    token: String,
    shutdown: Option<oneshot::Sender<()>>,
}

impl PermServer {
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The `--mcp-config` JSON that points Claude Code at this server.
    pub fn mcp_config(&self) -> String {
        json!({
            "mcpServers": {
                SERVER_NAME: {
                    "type": "http",
                    "url": format!("http://127.0.0.1:{}{MCP_PATH}", self.port),
                    "headers": { "Authorization": format!("Bearer {}", self.token) }
                }
            }
        })
        .to_string()
    }
}

impl Drop for PermServer {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

async fn bind_loopback() -> io::Result<TcpListener> {
    let mut last_error = None;
    for _ in 0..START_ATTEMPTS {
        // Port 0 asks the OS for a free port, so each attempt is a new one.
        match TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).await {
            Ok(listener) => return Ok(listener),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.expect("at least one attempt was made"))
}

/// Starts the server on a random loopback port. An error means prompts must
/// not run: the buddy never falls back to running without approvals
/// (design §13). Must be called inside a Tokio runtime.
pub async fn start(broker: Arc<Broker>) -> io::Result<PermServer> {
    let listener = bind_loopback().await?;
    let port = listener.local_addr()?.port();
    let token = new_token();
    let hosts = vec![format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    let app = router(broker, token.clone(), hosts);

    let (shutdown, shutdown_signal) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let serve = axum::serve(listener, app).with_graceful_shutdown(async move {
            let _ = shutdown_signal.await;
        });
        if let Err(error) = serve.await {
            eprintln!("perm_server: stopped: {error}");
        }
    });
    Ok(PermServer {
        port,
        token,
        shutdown: Some(shutdown),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perm_broker::{APPROVAL_TIMEOUT, ApprovalRequest, Decision};
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio::sync::mpsc::UnboundedReceiver;
    use tower::ServiceExt;

    const TOKEN: &str = "secret-token";
    const HOST: &str = "127.0.0.1:4000";

    fn app() -> (Router, Arc<Broker>, UnboundedReceiver<ApprovalRequest>) {
        let (broker, ui) = Broker::new(APPROVAL_TIMEOUT);
        let router = router(
            Arc::clone(&broker),
            TOKEN.to_string(),
            vec![HOST.to_string()],
        );
        (router, broker, ui)
    }

    fn request(body: &Value) -> Request<Body> {
        Request::post("/mcp")
            .header("host", HOST)
            .header("authorization", format!("Bearer {TOKEN}"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    async fn send(router: Router, request: Request<Body>) -> (StatusCode, Option<Value>) {
        let response = router.oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).ok())
    }

    async fn call(router: Router, body: Value) -> (StatusCode, Value) {
        let (status, json) = send(router, request(&body)).await;
        (status, json.expect("a JSON reply"))
    }

    // ---- who may talk to it

    #[tokio::test]
    async fn a_request_without_the_token_is_unauthorized() {
        let (router, _, _) = app();
        let request = Request::post("/mcp")
            .header("host", HOST)
            .body(Body::from(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#))
            .unwrap();

        assert_eq!(send(router, request).await.0, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn a_wrong_token_is_unauthorized() {
        let (router, _, _) = app();
        let request = Request::post("/mcp")
            .header("host", HOST)
            .header("authorization", "Bearer not-the-token")
            .body(Body::from(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#))
            .unwrap();

        assert_eq!(send(router, request).await.0, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn a_token_of_the_wrong_length_or_scheme_is_unauthorized() {
        for authorization in [
            "Bearer secret",
            "Bearer secret-token-extra",
            "Basic secret-token",
            "secret-token",
        ] {
            let (router, _, _) = app();
            let request = Request::post("/mcp")
                .header("host", HOST)
                .header("authorization", authorization)
                .body(Body::from("{}"))
                .unwrap();

            assert_eq!(
                send(router, request).await.0,
                StatusCode::UNAUTHORIZED,
                "{authorization}"
            );
        }
    }

    #[tokio::test]
    async fn a_browser_request_with_an_origin_header_is_forbidden_even_with_the_token() {
        let (router, _, _) = app();
        let mut request = request(&json!({"jsonrpc":"2.0","id":1,"method":"ping"}));
        request
            .headers_mut()
            .insert("origin", "https://evil.example".parse().unwrap());

        assert_eq!(send(router, request).await.0, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn a_foreign_host_header_is_forbidden_even_with_the_token() {
        let (router, _, _) = app();
        let request = Request::post("/mcp")
            .header("host", "evil.example:4000")
            .header("authorization", format!("Bearer {TOKEN}"))
            .body(Body::from(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#))
            .unwrap();

        assert_eq!(send(router, request).await.0, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn only_post_is_accepted() {
        let (router, _, _) = app();
        let get = Request::get("/mcp")
            .header("host", HOST)
            .header("authorization", format!("Bearer {TOKEN}"))
            .body(Body::empty())
            .unwrap();

        assert_eq!(send(router, get).await.0, StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn other_paths_are_not_found() {
        let (router, _, _) = app();
        let request = Request::post("/elsewhere")
            .header("host", HOST)
            .header("authorization", format!("Bearer {TOKEN}"))
            .body(Body::from("{}"))
            .unwrap();

        assert_eq!(send(router, request).await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn oversized_bodies_are_rejected() {
        let (router, _, _) = app();
        let huge = "x".repeat(MAX_BODY_BYTES + 10);

        let (status, _) = send(router, request(&json!({ "junk": huge }))).await;

        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    }

    // ---- the MCP handshake

    #[tokio::test]
    async fn initialize_answers_with_the_clients_protocol_version_and_our_name() {
        let (router, _, _) = app();

        let (status, reply) = call(
            router,
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(reply["id"], 1);
        assert_eq!(reply["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(reply["result"]["serverInfo"]["name"], "buddy");
        assert!(reply["result"]["capabilities"]["tools"].is_object());
    }

    #[tokio::test]
    async fn initialize_without_a_version_uses_a_default() {
        let (router, _, _) = app();

        let (_, reply) = call(
            router,
            json!({"jsonrpc":"2.0","id":"a","method":"initialize"}),
        )
        .await;

        assert_eq!(
            reply["result"]["protocolVersion"],
            FALLBACK_PROTOCOL_VERSION
        );
        assert_eq!(reply["id"], "a");
    }

    #[tokio::test]
    async fn notifications_get_an_empty_accepted_reply() {
        let (router, _, _) = app();

        let (status, body) = send(
            router,
            request(&json!({"jsonrpc":"2.0","method":"notifications/initialized"})),
        )
        .await;

        assert_eq!(status, StatusCode::ACCEPTED);
        assert_eq!(body, None);
    }

    #[tokio::test]
    async fn tools_list_offers_only_approve() {
        let (router, _, _) = app();

        let (_, reply) = call(
            router,
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        )
        .await;

        let tools = reply["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "approve");
        assert_eq!(
            tools[0]["inputSchema"]["required"],
            json!(["tool_name", "input"])
        );
    }

    #[tokio::test]
    async fn ping_is_answered() {
        let (router, _, _) = app();

        let (_, reply) = call(router, json!({"jsonrpc":"2.0","id":3,"method":"ping"})).await;

        assert_eq!(reply["result"], json!({}));
    }

    #[tokio::test]
    async fn unknown_methods_are_a_json_rpc_error() {
        let (router, _, _) = app();

        let (_, reply) = call(
            router,
            json!({"jsonrpc":"2.0","id":4,"method":"resources/list"}),
        )
        .await;

        assert_eq!(reply["error"]["code"], METHOD_NOT_FOUND);
        assert_eq!(reply["id"], 4);
    }

    #[tokio::test]
    async fn invalid_json_is_a_parse_error() {
        let (router, _, _) = app();
        let request = Request::post("/mcp")
            .header("host", HOST)
            .header("authorization", format!("Bearer {TOKEN}"))
            .body(Body::from("{ nope"))
            .unwrap();

        let (status, body) = send(router, request).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body.unwrap()["error"]["code"], PARSE_ERROR);
    }

    #[tokio::test]
    async fn a_batch_gets_one_reply_per_request() {
        let (router, _, _) = app();

        let (_, reply) = call(
            router,
            json!([
                {"jsonrpc":"2.0","id":1,"method":"ping"},
                {"jsonrpc":"2.0","method":"notifications/initialized"},
                {"jsonrpc":"2.0","id":2,"method":"tools/list"}
            ]),
        )
        .await;

        assert_eq!(reply.as_array().unwrap().len(), 2);
    }

    // ---- approvals

    async fn approve_call(router: Router, tool_name: &str, input: Value) -> Value {
        let body = json!({
            "jsonrpc": "2.0", "id": 9, "method": "tools/call",
            "params": { "name": "approve", "arguments": { "tool_name": tool_name, "input": input } }
        });
        call(router, body).await.1
    }

    fn verdict(reply: &Value) -> Value {
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
    }

    #[tokio::test]
    async fn an_approval_waits_for_the_user_then_allows() {
        let (router, broker, mut ui) = app();
        let input = json!({"file_path": "/w/hello.txt", "content": "hi"});
        let pending = tokio::spawn(approve_call(router, "Write", input.clone()));

        let shown = ui.recv().await.unwrap();
        assert_eq!(shown.tool_name, "Write");
        assert!(!pending.is_finished(), "the call must wait for the user");
        broker.answer(&shown.id, Decision::Allow);

        let reply = pending.await.unwrap();
        assert_eq!(
            verdict(&reply),
            json!({"behavior": "allow", "updatedInput": input})
        );
    }

    #[tokio::test]
    async fn an_approval_can_be_denied_with_a_reason() {
        let (router, broker, mut ui) = app();
        let pending = tokio::spawn(approve_call(router, "Bash", json!({"command": "ls"})));

        let shown = ui.recv().await.unwrap();
        broker.answer(&shown.id, Decision::deny("The user denied this action."));

        let reply = pending.await.unwrap();
        assert_eq!(
            verdict(&reply),
            json!({"behavior": "deny", "message": "The user denied this action."})
        );
    }

    #[tokio::test]
    async fn a_plan_arrives_as_a_plan_request() {
        let (router, broker, mut ui) = app();
        let pending = tokio::spawn(approve_call(
            router,
            "ExitPlanMode",
            json!({"plan": "# Plan"}),
        ));

        let shown = ui.recv().await.unwrap();
        assert_eq!(shown.detail.as_deref(), Some("# Plan"));
        broker.answer(&shown.id, Decision::Allow);

        assert_eq!(verdict(&pending.await.unwrap())["behavior"], "allow");
    }

    #[tokio::test]
    async fn cancelling_denies_a_waiting_approval() {
        let (router, broker, mut ui) = app();
        let pending = tokio::spawn(approve_call(router, "Bash", json!({"command": "ls"})));
        ui.recv().await.unwrap();

        broker.cancel_all();

        assert_eq!(verdict(&pending.await.unwrap())["behavior"], "deny");
    }

    #[tokio::test]
    async fn calling_another_tool_or_omitting_the_tool_name_is_invalid_params() {
        let (router, _, _) = app();
        let other_tool = json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"other","arguments":{}}});
        let no_name = json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"approve","arguments":{"input":{}}}});

        assert_eq!(
            call(router.clone(), other_tool).await.1["error"]["code"],
            INVALID_PARAMS
        );
        assert_eq!(
            call(router, no_name).await.1["error"]["code"],
            INVALID_PARAMS
        );
    }

    #[tokio::test]
    async fn a_missing_input_is_treated_as_empty() {
        let (router, broker, mut ui) = app();
        let body = json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"approve","arguments":{"tool_name":"Bash"}}});
        let pending = tokio::spawn(async move { call(router, body).await.1 });

        let shown = ui.recv().await.unwrap();
        broker.answer(&shown.id, Decision::Allow);

        assert_eq!(verdict(&pending.await.unwrap())["updatedInput"], json!({}));
    }

    // ---- the real server on a real socket

    async fn raw_post(port: u16, authorization: Option<&str>, body: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let auth = authorization.map_or(String::new(), |token| {
            format!("Authorization: Bearer {token}\r\n")
        });
        let request = format!(
            "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        response
    }

    fn token_of(server: &PermServer) -> String {
        server.token.clone()
    }

    #[tokio::test]
    async fn the_server_answers_with_the_token_and_refuses_without_it() {
        let (broker, _ui) = Broker::new(APPROVAL_TIMEOUT);
        let server = start(broker).await.unwrap();
        let ping = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;

        let with_token = raw_post(server.port(), Some(&token_of(&server)), ping).await;
        let without = raw_post(server.port(), None, ping).await;

        assert!(with_token.starts_with("HTTP/1.1 200"), "{with_token}");
        assert!(with_token.contains(r#""result":{}"#));
        assert!(without.starts_with("HTTP/1.1 401"), "{without}");
    }

    #[tokio::test]
    async fn each_server_gets_its_own_port_and_token() {
        let (broker, _ui) = Broker::new(APPROVAL_TIMEOUT);
        let one = start(Arc::clone(&broker)).await.unwrap();
        let two = start(broker).await.unwrap();

        assert_ne!(one.port(), two.port());
        assert_ne!(token_of(&one), token_of(&two));
        assert!(token_of(&one).len() >= 64);
    }

    #[tokio::test]
    async fn the_mcp_config_points_claude_at_the_server_with_the_token() {
        let (broker, _ui) = Broker::new(APPROVAL_TIMEOUT);
        let server = start(broker).await.unwrap();

        let config: Value = serde_json::from_str(&server.mcp_config()).unwrap();

        let entry = &config["mcpServers"]["buddy"];
        assert_eq!(entry["type"], "http");
        assert_eq!(
            entry["url"],
            format!("http://127.0.0.1:{}/mcp", server.port())
        );
        assert_eq!(
            entry["headers"]["Authorization"],
            format!("Bearer {}", token_of(&server))
        );
    }

    #[tokio::test]
    async fn dropping_the_server_stops_it() {
        let (broker, _ui) = Broker::new(APPROVAL_TIMEOUT);
        let server = start(broker).await.unwrap();
        let port = server.port();
        drop(server);

        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            if TcpStream::connect(("127.0.0.1", port)).await.is_err() {
                return;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "server still accepting connections"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[test]
    fn constant_time_eq_compares_whole_values() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(constant_time_eq(b"", b""));
    }
}
