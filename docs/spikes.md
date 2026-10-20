# Phase 2 spikes: what `claude -p` really does

Run on 2026-10-20 against Claude Code **2.1.304** (claude.ai login, default
model `claude-sonnet-5-5`), macOS 15.7, in a throwaway workspace. Each question
comes from design §4 ("To verify in the first spike"). The recorded runs are
the parser fixtures in `src-tauri/tests/fixtures/` (paths, usernames and the
machine's command/plugin lists scrubbed; partial JSON deltas blanked).

`spikes/approve-mcp.mjs` is the throwaway stdio MCP server used to observe
permission requests; it logs every `approve` call and answers allow or deny.

## Results

| # | Question | Result | Consequence for the build |
|---|---|---|---|
| 1 | Does `ExitPlanMode` reach the permission tool, and which mode does the run continue in? | **Yes.** `tool_name: "ExitPlanMode"`, `input: { plan, planFilePath }`. In the stream, the same `tool_use` has an **empty** `input` (`{}`), so the plan text comes only from the permission request. After it was allowed, the next `Write` reached the permission tool again: the rest of the run is in **default ("ask first")** mode. | §10.4 option A applies: answer **allow** and the run carries on, asking before each change. No "deny then auto-send" step. Plan mode also writes the plan to `~/.claude/plans/` (a side effect outside the workspace). |
| 2 | Is the prompt read from stdin with no prompt argument? | **Yes.** `echo "…" \| claude -p …` works. | Prompt via stdin as designed; no `.buddy/inbox/prompt-*.md` fallback. |
| 3 | Can Claude read a workspace PNG without a permission prompt in `default` mode? | **Yes.** `Read` on a PNG made no permission call, and Claude described the image correctly. | The screenshot flow (Phase 3) can rely on `Read`. |
| 4 | Does `/compact` work under `-p --resume`? | **Yes**, by argument **and** by stdin. Emits `system/compact_boundary` (`compact_metadata`: `trigger`, `pre_tokens`, `post_tokens`, …), `result` is `""`, the **session id is unchanged**, about 14 s. | `CompactStrategy::Native` (Phase 3). The buddy-made summary path is not needed. |
| 5 | Where do context usage and window size come from? | Tokens in context = `input_tokens + cache_read_input_tokens + cache_creation_input_tokens` of the **last** `usage.iterations[]` entry (fall back to top-level `usage`) of the `result` message. Window = `modelUsage[<model>].contextWindow` (**1,000,000** for this model). The model name is in `system/init`. | The context meter ships. `Finished` carries `context_tokens` and `context_window`. Verified on a single-turn run only; multi-step runs use the last iteration. |
| 6 | Does `--fork-session` work under `-p`? | **Yes.** A new `session_id`; the original stays resumable and remembers the conversation. | Branch conversation ships (Phase 3). |
| 7 | Does `--tools ""` give a text-only turn with `--resume`? | **Yes.** It removes every built-in tool. The 83 tools still listed were MCP tools from the user's own config (GitHub, Atlassian, …). | Always pass **`--strict-mcp-config`** so only the buddy's own MCP server loads. Verified: the permission tool still works under it, and a deny comes back to Claude as the error `The user denied this action.` |

## Other things learned

- **The permission tool also receives `tool_use_id`** next to `tool_name` and
  `input`.
- **The user's own Claude Code hooks run inside the buddy's `claude -p`.**
  `system/hook_started` and `hook_response` events appear in the stream (the
  parser ignores them), and `PreToolUse` hooks can block a tool call on their
  own: it surfaces as an error `tool_result` and in `result.permission_denials`,
  not through the buddy's approval. Controlling this (for example with
  `--settings` / `--setting-sources`) is not verified yet.
- **Cost of a run:** about 36k cache-creation tokens even for "reply PONG",
  because the user's `CLAUDE.md`, rules and tools are loaded each time.
- **Stream event types seen:** `system` (`hook_started`, `hook_response`,
  `init`, `status`, `commands_changed`, `compact_boundary`), `stream_event`
  (message and content-block events), `assistant`, `user` (tool results),
  `rate_limit_event`, `result` (`subtype`, `is_error`, `result`, `session_id`,
  `total_cost_usd`, `usage`, `modelUsage`, `permission_denials`).
- A prompt passed as an argument with no stdin prints a 3-second "no stdin
  data received" warning; irrelevant because the buddy writes to stdin.

## Still to verify

- `CGWindowListCopyWindowInfo` bounds without the Screen Recording permission
  (Phase 3, standing on windows).
- `system/api_retry` is parsed per the documentation but was not observed.

## Fixtures

| File | What it covers |
|---|---|
| `text_run.jsonl` | A plain text answer: `init`, text deltas, `result` with usage; one hook event pair to prove they are ignored. |
| `denied_tool_run.jsonl` | A `Write` the permission tool **denied**: `tool_use`, then an error `tool_result`. |
| `read_image_run.jsonl` | A `Read` of an image (the image bytes are omitted). |
| `plan_run.jsonl` | Plan mode: the `ExitPlanMode` `tool_use` (partial stream events removed). |
| `compact_run.jsonl` | `/compact`: `compact_boundary` and an empty `result`. |
