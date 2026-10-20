# Desktop Buddy: implementation checklist

Tracks progress against `desktop-buddy-implementation-plan.md`. Check items
off as each task is implemented, tested and committed.

---

## Phase 1 (M1): Alive 🐾

### 1.0 Setup
- [x] Install Rust (`rustup`), `cargo-llvm-cov`, Xcode CLT check
- [x] `npm create tauri-app@latest` (vanilla-ts, Tauri v2) into the repo root
- [x] Move the spec to `docs/specs/2026-10-19-desktop-buddy-design.md`
- [x] Add `.gitignore`, `vitest`, `cargo fmt`/`clippy` scripts
- [x] Bundle id `com.jeffry.desktopbuddy`

### 1.1 Platform layer (§9.0)
- [x] `platform/mod.rs`: traits (`Screens`, `Mouse`, `PetWindowOs`; `WindowList`,
      `ShellEnv`, `ProcessCtl` declared for later phases) + `Display`/`Rect`/
      `Point`/`Buttons` types + `current()`
- [x] `platform/fake/`: scripted displays, cursor and buttons for tests
- [x] `platform/macos/screens.rs`: `NSScreen` frame/visibleFrame → top-left
      points; `flip_rect(rect, main_height)` tested first (secondary displays
      above/left/both)
- [x] `platform/macos/mouse.rs`: `CGEvent` location + `CGEventSourceButtonState`
- [x] `platform/macos/app.rs`: `ActivationPolicy::Accessory`, collection
      behavior (`canJoinAllSpaces | stationary | fullScreenAuxiliary`),
      floating window level
- [x] `tauri.conf.json`: `macOSPrivateApi`, transparent, no decorations/shadow,
      `alwaysOnTop`, `visibleOnAllWorkspaces`

### 1.2 Physics (pure, test first; §6.2-6.5)
- [x] `physics/world.rs`: floor/walls/ceiling from displays; gaps between
      displays count as walls *(ledges deferred to Phase 3)*
- [x] `physics/engine.rs`: `step(state, world, input, dt) -> (State, Vec<Event>)`
  - [x] gravity 1000 pt/s², terminal velocity 900 pt/s
  - [x] land vs. splat (bounce damping 0.3)
  - [x] wall cling or bounce, seeded RNG for test determinism
  - [x] engine tests from §16 (minus the ledge cases)
- [x] `physics/behavior.rs`: weighted behaviors loaded from `behaviors.json`
      (floor, wall and ceiling sets)
- [x] `physics/drag.rs`
  - [x] grab and follow the cursor
  - [x] release detection by polling
  - [x] throw velocity = average over the last 100 ms, capped at 1500
  - [x] click vs. drag (4 pt / 250 ms)
  - [x] right click (`is_right_click`; menu itself is Phase 3)

### 1.3 Pet window
- [x] `pet_window.rs`: 60 Hz tick, world refresh every 250 ms (main-thread
      `NSScreen` read), `step`, `set_position(LogicalPosition)`, emits `pose`
- [x] Commands `grab` and `click`
- [x] Handle display changes: respawn at top-center of the main display when
      off every display

### 1.4 Sprites (§9.2)
- [ ] `src/sprite.ts`
  - [ ] Format A (sheet) and Format B (Shimeji folder) loaders
  - [ ] pose fallback: alias → `think` → `stand`
  - [ ] mirror when facing right, `devicePixelRatio`, pixelated rendering
  - [ ] built-in placeholder shape
- [ ] `vitest`: character parsing, aliases, fallbacks
- [ ] **Shiba task:** find a CC0/CC-BY Shiba Inu sprite sheet or Shimeji set;
      record license in `character.json`; map poses into `characters/shiba/`
- [ ] **Shimeji mapping:** `shime1…46` → pose table from `actions.xml`

### 1.5 Basic menu bar
- [ ] `menubar.rs`: tray icon with **Pause wandering**, **Character ▸**,
      **Quit** (⌘Q)
- [ ] Minimal `settings.rs` (load/save JSON, defaults from §9.1)

### Phase 1 exit criteria
- [x] `cargo test` passes (55 tests, ~80-100% coverage on physics modules)
- [ ] `vitest` passes *(no frontend tests yet — added with sprite.ts in 1.4)*
- [x] `cargo clippy -- -D warnings` clean (only expected dead-code warnings for
      traits/helpers not wired up until later phases)
- [x] `npm run tauri dev`: pet falls and lands near the bottom of the screen
      (spot-checked); full checklist (wandering, climbing, crawling under the
      menu bar, throwing) needs a real sprite to observe easily — recheck
      after 1.4
- [ ] No Dock icon, and the menu bar icon works *(menu bar icon is 1.5)*

---

## Phase 2 (M2): Talks 💬

### 2.0 Spikes
- [ ] 1. ExitPlanMode reaches the permission tool; which mode continues after
- [ ] 2. Prompt via stdin
- [ ] 3. Reading a workspace PNG without a prompt
- [ ] 4. `/compact` under `-p --resume`
- [ ] 5. Usage fields + context-window size
- [ ] 6. `--fork-session` under `-p`
- [ ] 7. `--tools ""` with `--resume`
- [ ] Record real `stream-json` fixtures under `src-tauri/tests/fixtures/`
- [ ] `docs/spikes.md` written up

### 2.1 Process and environment
- [ ] `platform/macos/env.rs`: login-shell PATH (5s timeout + fallback), tested
- [ ] `platform/macos/process.rs`: process group, SIGINT→SIGTERM→SIGKILL
      escalation, tested with a fake child

### 2.2 Claude runner
- [ ] `claude_runner.rs`: pure `build_args(...)` + tests (permission tool
      always present; `--bare`/`bypassPermissions`/`--dangerously-skip-permissions`
      never appear)
- [ ] locate `claude`, stdin prompt (or file fallback per spike)
- [ ] one run at a time, `cancel()`, preflight `claude auth status`
- [ ] `stream_parser.rs`: `parse_line`, tested against fixtures
- [ ] `activity.rs`: `describe(tool, input)` tested against §8.2 + path
      shortening + 40-char cutting

### 2.3 Approvals
- [ ] `perm_server.rs`: `rmcp` streamable HTTP on `axum`, 127.0.0.1, random
      port, bearer token; integration test (token → OK, no token → 401)
- [ ] `perm_broker.rs`: oneshot map, 5-min auto-deny, deny on cancel/quit;
      tested (allow, deny, timeout, cancel)
- [ ] Refuse to run prompts if the MCP server can't start

### 2.4 UI
- [ ] `ipc.ts`
- [ ] `mood.ts`: pure `next()`, vitest-tested; pauses physics
- [ ] `bubble.ts`: input, streaming reply, Ask (Allow/Deny), minimize, "…" dot
- [ ] `activity.ts`: label above the pet
- [ ] `pet_window` resizes toward the free side; focus only when typing
- [ ] Global hotkey ⌘⇧Space

### 2.5 Conversation basics
- [ ] `persona.md` via `--append-system-prompt-file`
- [ ] Workspace `~/BuddyWorkspace`
- [ ] `conversations.rs`: `ConversationStore` + `title_from`, tested first
- [ ] Slash parser (`/new`, `/clear`, `/reset`, `/resume`)
- [ ] **New conversation** / **Recent ▸** in menu bar and bubble
- [ ] "No conversation found" → remove + **Send as a new conversation**

### 2.6 Done alert
- [ ] `notify.rs`: rules for >30s, minimized, approval while minimized (tested
      first)
- [ ] `tauri-plugin-notification`; clicking reopens the reply

### Phase 2 exit criteria
- [ ] Manual checklist §16 items 3, 4, 5, 11, 12, 13, 17 pass in `tauri dev`
- [ ] No `claude` process left after ⌘Q (`pgrep claude`)

---

## Phase 3 (M3): Smart 🧠

### 3.1 Modes, models and effort (§7)
- [ ] `modebar.ts`: chips, Auto warning, 🗺️/⚡ badge, one-off `/plan`
- [ ] Menus for Mode, Model, Effort; reply footer shows model used
- [ ] "Model not available" → **Switch to Default model**

### 3.2 Plan approval (§10.4)
- [ ] Plan bubble: Approve (ask first) / Approve (auto-accept) / Keep planning
- [ ] Allow-or-deny-then-continue per spike result

### 3.3 Standing on windows (§6.6)
- [ ] Spike: `CGWindowList` bounds without Screen Recording permission
- [ ] `platform/macos/window_list.rs` (layer, alpha, own-PID, size filters)
- [ ] `physics/world.rs`: visible-ledge subtraction, tested (incl. overlaps)
- [ ] Engine: carried on window move, falls when removed, sit-dangle, jump to
      a higher ledge
- [ ] Auto-disable **Stand on windows** if the window list is empty/denied

### 3.4 Drop files (§8.1)
- [ ] `dropzone.ts`: catch/happy poses, chips with ✕, struck-through for
      missing paths
- [ ] `attachments.rs`: attached-items text, 20-item cap, per-conversation
      `--add-dir`, protected-folder warning (tested first)

### 3.5 Quick actions (§8.4)
- [ ] `quick_actions.rs`: parse/validate with defaults, file watcher,
      placeholders (`{{clipboard}}`, `{{screen}}`, `{{workspace}}`), `~` expansion
- [ ] Right-click NSMenu + **Edit quick actions…**

### 3.6 Look at my screen (§8.5)
- [ ] `screen.rs`: hide pet → capture (`xcap`) → show pet → downscale
- [ ] Preview bubble (Send/Cancel); inbox write only on Send
- [ ] 24h inbox cleanup; Screen Recording permission explainer

### 3.7 Compact, context meter, branch (§8.6)
- [ ] `compact::summary_prompt` / `carry_prompt`; `CompactStrategy` from spike
- [ ] `pending_summary` flow + "Compacted: …" Recent entry
- [ ] `context::percent` / `should_hint`; footer 🧠 ~N%; hint once per conversation
- [ ] Branch: one-shot `--fork-session` (hidden if spike failed)
- [ ] `build_args` tests for `--fork-session` / `--tools ""`

### 3.8 Settings and menu bar
- [ ] Full `settings.rs` keys (§9.1)
- [ ] Complete menu bar list (§9.1)

### Phase 3 exit criteria
- [ ] Manual checklist §16 items 2 (windows), 6-10, 14-16 pass in `tauri dev`

---

## Phase 4 (M4): Polish ✨
- [ ] `npm run tauri build` → signed `Desktop Buddy.app` in `~/Applications`
- [ ] Full §16 manual checklist from a Finder launch (item 1: login-shell PATH)
- [ ] Edge cases: multiple displays, Spaces/full screen, Stage Manager, side
      Dock walls
- [ ] Permission-flow polish (Screen Recording re-prompt, Files/Folders
      denial text, notification denial)
- [ ] Performance: idle CPU under 1-2%; lower tick rate while asleep; skip
      `set_position` when unmoved
- [ ] Coverage ≥80% on pure modules + final code review + security review

---

## Reviews (per working rules)
- [ ] Phase 1 code review (Rust/TypeScript)
- [ ] Phase 2 code review + security review (MCP server, process spawning)
- [ ] Phase 3 code review
- [ ] Phase 4 code review + security review
