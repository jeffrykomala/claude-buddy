# Desktop Buddy: phased implementation plan

## Context
`claude-buddy/desktop-buddy-design.md` (v4.2) specifies a Shimeji-style macOS desktop pet. It is built with Tauri v2 (Rust + vanilla TS) and works by running `claude -p` per message, with MCP-based approvals. The repo (`/Users/172143.jeffry/Documents/Git/claude-buddy/claude-buddy`, already a git repo) holds only the spec. This plan turns the spec's four milestones (§17) into four phases at task level. Each phase ends with something runnable on this Mac.

**Decisions taken:**
- scaffold in this repo
- 4 phases = M1–M4
- character: a **Shiba Inu**, from a free, permissively licensed sprite sheet or Shimeji set (found in Phase 1); the placeholder shape is used until then

**Environment now:** macOS 15.7.1, Node 22.14 (mise), `claude` at `~/.local/bin/claude`. **Rust is not installed.**

**Working rules for every phase:**
- **TDD:** write each pure module's tests first (RED → GREEN → refactor), and aim for 80%+ coverage on the pure Rust/TS modules (`cargo llvm-cov`, `vitest --coverage`).
- Only `platform/` touches OS APIs. Everything else is tested with `platform/fake/`.
- Code style: immutable updates (`step()` returns a new state), functions under 50 lines, files under 400 lines, constants for every physics and timing number.
- One conventional commit per task. Run code review (rust/typescript reviewer) at the end of each phase, and a security review in Phase 2 (MCP server, process spawning).

---

## Phase 1 (M1): Alive 🐾
**Goal:** a Shiba walks, falls, climbs and can be thrown around the screen. No Claude yet.

### 1.0 Setup
- Install Rust (`rustup`), `cargo-llvm-cov`, and the Xcode CLT check.
- `npm create tauri-app@latest` (vanilla-ts, Tauri v2) into the repo root.
- Move the spec to `docs/specs/2026-10-19-desktop-buddy-design.md`.
- Add `.gitignore`, `vitest`, `cargo fmt`/`clippy` scripts.
- Bundle id `com.jeffry.desktopbuddy`.

### 1.1 Platform layer (§9.0)
- `src-tauri/src/platform/mod.rs`: the traits `Screens`, `Mouse`, `PetWindowOs` (`WindowList`, `ShellEnv` and `ProcessCtl` are declared now and implemented in later phases), plus `Display`, `Rect`, `Point`, `Buttons` types, and `current()`.
- `platform/fake/`: scripted displays, cursor and buttons, for tests.
- `platform/macos/screens.rs`: NSScreen `frame`/`visibleFrame` → top-left points. **Test first:** the pure `flip_rect(rect, main_height)` function, with secondary displays above and to the left.
- `platform/macos/mouse.rs`: `CGEvent` location and `CGEventSourceButtonState`.
- `platform/macos/app.rs`: `ActivationPolicy::Accessory`, `canJoinAllSpaces | stationary | fullScreenAuxiliary`, floating level.
- `tauri.conf.json`: `macOSPrivateApi`, transparent, no decorations or shadow, `alwaysOnTop`, `visibleOnAllWorkspaces`.

### 1.2 Physics (pure, test first; §6.2–6.5)
- `physics/world.rs`: build floor, walls and ceiling from displays; gaps between displays count as walls. *(Ledges come in Phase 3.)*
- `physics/engine.rs`: `step(state, world, input, dt) -> (State, Vec<Event>)`.
  - gravity 1000 pt/s², terminal velocity 900 pt/s
  - land vs. splat (bounce damping 0.3)
  - wall cling or bounce, using a seeded RNG for test determinism
  - Tests: §16 engine list, minus the ledge cases.
- `physics/behavior.rs`: weighted behaviors loaded from `behaviors.json` (floor, wall and ceiling sets).
- `physics/drag.rs`:
  - grab and follow the cursor
  - release detection by polling
  - throw velocity = average over the last 100 ms, capped at 1500
  - click vs. drag (4 pt / 250 ms)
  - right click

### 1.3 Pet window
- `pet_window.rs`: a 60 Hz tokio tick that refreshes the world every 250 ms, runs `step`, calls `set_position(LogicalPosition)` and emits `pose`.
- Commands `grab` and `click`.
- Handle display changes: a pet that is off every display respawns at the top center of the main display.

### 1.4 Sprites (§9.2)
- `src/sprite.ts`:
  - Format A (sheet) and Format B (Shimeji folder) loaders
  - pose fallback: alias → `think` → `stand`
  - mirror when facing right, `devicePixelRatio`, pixelated rendering
  - built-in placeholder shape
- `vitest`: character parsing, aliases, fallbacks.
- **Shiba task:** search for a CC0 or CC-BY Shiba Inu pixel sprite sheet or Shimeji set (itch.io, OpenGameArt). Record the license in `character.json`. Map its poses into `characters/shiba/`, aliasing anything missing.
- **Shimeji mapping:** read Shimeji-ee's `actions.xml` and record the `shime1…46` → pose table in code.

### 1.5 Basic menu bar
- `menubar.rs`: tray template icon with **Pause wandering**, **Character ▸** and **Quit** (⌘Q).
- Minimal `settings.rs` (load/save JSON, defaults from §9.1). It is fully completed in Phase 3.

**Exit criteria:**
- `cargo test` and `vitest` pass.
- `npm run tauri dev` shows the Shiba falling to just above the Dock, wandering, climbing walls, crawling under the menu bar and letting go, and being thrown with the mouse.
- No Dock icon, and the menu bar icon works.

---

## Phase 2 (M2): Talks 💬
**Goal:** click the Shiba, chat with Claude Code safely (Ask first), see activity poses and get a Done alert. New and Recent conversations work.

### 2.0 Spikes (scripted in `spikes/`, results written to `docs/spikes.md`)
1. ExitPlanMode reaches the permission tool, and which mode the rest of the run uses afterwards
2. prompt via stdin
3. reading a workspace PNG without a prompt
4. `/compact` under `-p --resume`
5. which usage fields to read and the context-window size
6. `--fork-session` under `-p`
7. `--tools ""` with `--resume`

*(The CGWindowList spike is in Phase 3.)* Record real `stream-json` output under `src-tauri/tests/fixtures/`.

### 2.1 Process and environment
- `platform/macos/env.rs`: `$SHELL -ilc` PATH with a 5 s timeout and the fallback PATH. The parser is tested first.
- `platform/macos/process.rs`: `process_group(0)`, SIGINT → 3 s → SIGTERM → 3 s → SIGKILL. Escalation is tested with a fake child process.

### 2.2 Claude runner
- `claude_runner.rs`:
  - pure `build_args(...)`. Tests: the permission tool is always present; `--bare`, `bypassPermissions` and `--dangerously-skip-permissions` never appear.
  - locate `claude`, write the prompt to stdin (or use the file fallback, depending on the spike)
  - one run at a time, `cancel()`, preflight `claude auth status`
- `stream_parser.rs`: `parse_line`, tested against the recorded fixtures.
- `activity.rs`: `describe(tool, input)`, tested against every row of §8.2, plus path shortening and 40-character cutting.

### 2.3 Approvals
- `perm_server.rs`: `rmcp` streamable HTTP on `axum`, 127.0.0.1, random port, bearer token. Integration test: with the token → OK; without it → 401.
- `perm_broker.rs`: oneshot map, 5-minute auto-deny, deny on cancel and on quit. Tests: allow, deny, timeout, cancel.
- If the MCP server can't start, refuse to run prompts.

### 2.4 UI
- `ipc.ts`
- `mood.ts`: pure `next()`, tested with vitest. It pauses physics.
- `bubble.ts`: input, streaming reply with light markdown, Ask (Allow/Deny), minimize, and the "…" dot.
- `activity.ts`: the label above the pet.
- `pet_window` resizes toward the free side. Focus is taken only when typing and returned on close.
- Global hotkey ⌘⇧Space.

### 2.5 Conversation basics
- `persona.md` (§12) with Shiba personality, passed via `--append-system-prompt-file`.
- Workspace `~/BuddyWorkspace`.
- `conversations.rs`: `ConversationStore` (record, touch, trim to 20, remove, corrupt-file safe) and `title_from`. Tests first.
- Slash parser (`/new`, `/clear`, `/reset`, `/resume`), recognized only as the whole message.
- **New conversation** and **Recent ▸** in the menu bar and the bubble.
- "No conversation found" → remove the entry and offer **Send as a new conversation**.

### 2.6 Done alert
- `notify.rs`: rules for over 30 s, minimized, and approval while minimized. The rules are tested first.
- Use `tauri-plugin-notification`; clicking a notification reopens the reply.

**Exit criteria:**
- Manual checklist §16 items 3, 4, 5, 11, 12, 13 and 17 pass in `tauri dev`.
- No `claude` process is left after ⌘Q (`pgrep claude`).

---

## Phase 3 (M3): Smart 🧠
**Goal:** modes, models, plan approval, standing on windows, and all helpers.

### 3.1 Modes, models and effort (§7)
- `modebar.ts`: chips (click to cycle), Auto warning on first use, 🗺️/⚡ badge, one-off `/plan`.
- Menus for Mode, Model and Effort. The model actually used is shown in the reply footer.
- "Model not available" → offer **Switch to Default model**.

### 3.2 Plan approval (§10.4)
- Plan bubble with **Approve: ask me before each change**, **Approve: auto-accept edits** and **Keep planning** (with feedback).
- Allow, or deny then auto-continue, depending on the spike result.

### 3.3 Standing on windows (§6.6)
- Spike: CGWindowList bounds without the Screen Recording permission.
- `platform/macos/window_list.rs` with the layer, alpha, own-PID and size filters.
- `physics/world.rs`: visible-ledge subtraction (tests first, including overlaps).
- Engine: carried when the window moves, falls when it's removed, sit-dangle, jump to a higher ledge.
- If the window list is empty or denied, turn **Stand on windows** off automatically.

### 3.4 Drop files (§8.1)
- `dropzone.ts`: catch and happy poses, chips with ✕, struck-through chips for missing paths.
- `attachments.rs`:
  - Attached-items text, 20-item cap, per-conversation `--add-dir`
  - the protected-folder warning, shown once
- Tests first.

### 3.5 Quick actions (§8.4)
- `quick_actions.rs`:
  - parse and validate, with defaults on error
  - file watcher (`notify` crate)
  - placeholders `{{clipboard}}` (2,000-character inline rule, otherwise an inbox file), `{{screen}}`, `{{workspace}}`
  - `~` expansion
- Right-click NSMenu (§8.4 layout) and **Edit quick actions…**

### 3.6 Look at my screen (§8.5)
- `screen.rs`: hide the pet, capture with `xcap`, show the pet, downscale to 1920 px.
- Preview bubble with **Send** and **Cancel**. The file is saved to the inbox only on Send.
- 24-hour inbox cleanup at startup and every hour.
- Explain the Screen Recording permission when it's denied.

### 3.7 Compact, context meter, branch (§8.6)
- `compact::summary_prompt` and `carry_prompt`. `CompactStrategy` is a constant set from the spike result.
- `pending_summary` flow, with the "Compacted: …" entry in Recent.
- `context::percent` and `should_hint`. Footer 🧠 ~N%; the **Compact / Not now** hint appears once per conversation.
- **Branch:** one-shot `--fork-session`. Hidden if the spike failed.
- `build_args` tests for `--fork-session` and `--tools ""`.

### 3.8 Settings and menu bar
- Full `settings.rs` keys (§9.1).
- Complete menu bar list (§9.1), including **Change workspace folder…**, **Stand on windows ✓** and **Notifications ✓**.

**Exit criteria:** manual checklist §16 items 2 (windows), 6–10 and 14–16 pass in `tauri dev`.

---

## Phase 4 (M4): Polish ✨
- `npm run tauri build` → ad-hoc signed `Desktop Buddy.app` in `~/Applications`.
- Run the **full §16 manual checklist from a Finder launch**, starting with item 1: the login-shell PATH finds `claude`.
- Edge cases:
  - multiple displays (arrangement, unplugging)
  - Spaces and full screen
  - Stage Manager
  - side Dock walls
- Permission-flow polish: the Screen Recording re-prompt after a rebuild, the Files and Folders denial text, notification denial.
- Performance: measure idle CPU in Activity Monitor (target: under 1–2% idle). Lower the tick rate while the pet sleeps; skip `set_position` when the pet hasn't moved.
- Coverage report at 80% or more on the pure modules, plus a final code review and security review.

---

## Critical files (new)
- **Rust:**
  - `src-tauri/src/platform/{mod.rs, fake/, macos/{screens,mouse,app,env,process,window_list}.rs}`
  - `src-tauri/src/physics/{engine,world,behavior,drag}.rs`
  - `claude_runner.rs`, `stream_parser.rs`, `activity.rs`, `perm_server.rs`, `perm_broker.rs`
  - `conversations.rs`, `attachments.rs`, `quick_actions.rs`, `screen.rs`, `notify.rs`, `settings.rs`, `menubar.rs`, `pet_window.rs`
- **Web:** `src/{sprite,mood,bubble,modebar,activity,dropzone,ipc}.ts`
- **Data:** `characters/shiba/`, `behaviors.json`, `persona.md`, `docs/spikes.md`

## Verification
- **Every phase:** `cargo test`, `cargo clippy -- -D warnings`, `npx vitest run --coverage`, `cargo llvm-cov`; then `npm run tauri dev` and the phase's manual checklist items.
- **Phase 2 onward:** `pgrep claude` after quitting; inspect `conversations.json` and `settings.json` under `~/Library/Application Support/com.jeffry.desktopbuddy/`.
- **Phase 4:** repeat everything from the built `.app` launched from Finder.
