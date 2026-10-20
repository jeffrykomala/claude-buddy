# Desktop Buddy: Design Spec

**Date:** 2026-10-19
**Status:** Draft v4.2, awaiting review
**Platform:** **macOS first**: macOS 13 Ventura or later, Apple Silicon and Intel. **Windows-ready architecture** (a Windows port later means adding one platform folder; see §9.0).

**Changes in v4.2:**
- New **§8.6 Conversations**, like Claude Code's session commands:
  - **New conversation** (`/new`, `/clear`, `/reset`, buttons and menus)
  - **Recent conversations** (resume any of the last 20)
  - **Compact** (`/compact [focus]`)
  - **Context meter** with a "my head is getting full" hint
  - **Branch conversation**
- New spikes (§4, §16) to confirm what works headless: `/compact`, usage fields, `--fork-session`.

**Changes in v4.1:**
- All OS-specific code goes behind small traits in `platform/`. v1 implements only `platform/macos/`.
- Tauri is kept **because** it keeps a later Windows port cheap.

**Changes in v4:**
- **macOS only.** All Windows-specific design is removed.
- Mac-specific additions:
  - menu bar icon
  - login-shell PATH so the buddy can find `claude`, `git`, `npm`, etc.
  - point-based coordinates
  - process-group cancel with SIGINT
  - Screen Recording permission notes for dev builds
- Development must happen **on a Mac** (§14).

**Changes in v3:**
- Helper features: drop files, activity poses, "Done!" alert, quick actions, look at my screen.
- The prompt is sent through stdin.
- The build is split into milestones.

**Changes in v2:** Shimeji-style physics; modes and models; Shimeji-ee image sets.

---

## 1. Summary

Desktop Buddy is a small animated character that lives on your Mac's desktop, like **Shimeji**. Gravity pulls it down and it lands on the Dock's edge or **on top of your open windows**. It walks along window edges, climbs the sides of the screen, crawls along the underside of the menu bar and lets go. You can **grab it with the mouse and throw it**.

Click it (or press a hotkey) and a speech bubble opens where you can type a request. The request goes to **Claude Code**, which runs on your Mac using your existing Claude subscription. Claude's answer streams back into the character's speech bubble. Other ways to give it work:
- **drop files** from Finder on it
- **right-click** it for one-click quick actions
- ask it to **look at your screen**

While it works, the pet **acts out what it's doing**: reading, writing, running commands or browsing. It **jumps and waves** when it's done.

Because Claude Code is behind it, the buddy can do real work. You choose how much freedom it has with **modes** (Ask first, Accept edits, Plan, Auto). The default is **Ask first**: anything that changes your Mac needs your **Allow / Deny** click.

---

## 2. Goals and non-goals

### Goals (v1)
- **Shimeji-style movement:**
  - gravity and falling
  - landing on the floor (just above the Dock) and **on the top edges of open windows**
  - walking, running, sitting, sitting with legs dangling off a ledge, and sleeping
  - **climbing** the screen's left and right edges, and **crawling along the ceiling** (just under the menu bar)
  - being **grabbed and thrown** with the mouse, then bouncing and landing
  - being **carried along** when the window it stands on moves, and **falling** when that window is minimized or closed
- Chat with Claude Code from a speech bubble, with streamed replies.
- **Modes**, switchable before any message: **Ask first** (default), **Accept edits**, **Plan**, **Auto**.
- **Model picker**: Default, `opusplan`, `opus`, `sonnet`, `haiku`, `fable` (whatever your plan allows).
- **Plan approval bubble**: approve, approve with auto-accepted edits, or keep planning with feedback.
- **Helper features** (§8): drop files on the pet, activity poses and labels, "Done!" alert, quick-action menu, look at my screen.
- **Conversation management** (§8.6): memory across prompts, **new conversation**, **recent conversations** (resume), **compact**, a **context meter**, and **branch**.
- A personality defined in an editable `persona.md`.
- Swappable character art: **sprite sheets** or **Shimeji-ee image folders**, with no code changes.
- A **menu bar icon** with all settings. No Dock icon.

### Non-goals (v1)
- **Windows or Linux builds in v1.** The architecture is Windows-ready (§9.0), but only macOS is implemented and tested.
- **Claude controlling the pet** (MCP tools such as `emote`, `walk_to`, `set_reminder`). Future.
- **Hide during full-screen** apps. Future.
- **Start on login.** Future.
- Distributing the app to other people: notarization, Developer ID signing, the App Store. v1 is built and run on your own Mac.
- Voice input/output.
- Climbing the **sides** of other apps' windows; pushing or throwing other apps' windows.
- Chasing the mouse cursor, multiple pets, or pets that multiply.
- Auto-update.
- A long-lived Claude process (v1 starts one `claude` process per prompt).
- `bypassPermissions` mode. **It is never offered.**

---

## 3. Key decisions

| Decision | Choice | Why |
|---|---|---|
| Purpose | Companion **and** productivity helper | User's choice |
| Platform | **macOS first, Windows-ready** | User's choice: build for the Mac now, keep a Windows port possible |
| Default safety | **Ask first** for anything that changes the Mac | One misunderstood prompt shouldn't break anything |
| Other modes | Accept edits, Plan, Auto, switchable before any message | User wants control over modes, like in Claude Code |
| App framework | **Tauri v2 (Rust)** | Light on RAM for an always-running app; user is an experienced developer. **Keeps the UI, physics, Claude runner, approvals and helpers shared**, so a Windows port only needs a new `platform/windows/` (native Swift would mean a full rewrite for Windows) |
| OS-specific code | Behind **traits in `platform/`**; v1 implements `platform/macos/` only | Nothing outside `platform/` calls OS APIs, so porting is contained |
| Claude integration | **Rust spawns the `claude` CLI** (`claude -p`), one process per message, **prompt sent through stdin** | Uses the existing Claude login; mode and model can change per message; no argument-length or quoting issues |
| Approvals | `--permission-prompt-tool` → a small **MCP server inside the app** | The documented way to answer permission prompts in non-interactive mode |
| Physics | **Rust**: a pure `step()` function on a fixed 60 Hz tick, in **points** (logical pixels) | Testable without a screen. macOS window APIs already use points, so there's no DPI math. |
| Window detection | `CGWindowListCopyWindowInfo` (bounds only) | Lets the pet stand on other windows without the Screen Recording permission |
| Screen info | `NSScreen` `frame` / `visibleFrame` | `visibleFrame` already excludes the menu bar and the Dock |
| Large context (clipboard, screenshots) | Saved as files in `~/BuddyWorkspace/.buddy/inbox/`, and the prompt points to them | Claude reads them with its normal tools |
| Screenshots | **Always previewed and confirmed** before sending | Privacy |
| Conversations | Sessions are Claude Code session ids kept in `conversations.json`; **new** = forget the id, **recent** = `--resume`, **compact** = native `/compact` if the spike allows, otherwise a buddy-made summary carried into a new session | Works the same under `claude -p` without relying on terminal-only commands |
| Art | Sprite sheets **or** Shimeji-ee image folders | Thousands of existing Shimeji characters; easy to swap |
| UI tech | TypeScript + Vite, **no framework**, canvas for sprites | The UI is small; a framework adds nothing |

---

## 4. Verified facts this design relies on

Checked against the official Claude Code docs (code.claude.com/docs). The CLI behavior is the same on macOS. Login was checked on the user's Windows PC (Claude Pro account); **the Mac must be logged in too** (`claude auth status`).

1. **`claude -p` uses the subscription login** (`claude auth login`). **Do not use `--bare`**, because bare mode ignores the subscription login and requires an API key.
2. **Streaming:** `--output-format stream-json --verbose --include-partial-messages` writes one JSON object per line:
   - `system` / `init`: includes `session_id` and the **model** actually used
   - `stream_event`: text deltas (`event.delta.type == "text_delta"`)
   - `assistant` / `user`: full messages, including `tool_use` (tool **name and input**) and `tool_result` blocks
   - `system` / `api_retry`: retry notices
   - `result`: final text, `is_error`, cost and session metadata (always the last line)
3. **Memory:** `--resume <session_id>` continues a conversation. Every message can use **different** `--permission-mode`, `--model` and `--add-dir` flags.
4. **Approvals:** `--permission-prompt-tool mcp__<server>__<tool>` together with `--mcp-config <json>` sends every permission prompt to that MCP tool.
   - The tool receives: `{ "tool_name": string, "input": object }`
   - The tool returns a **JSON-stringified** text result:
     - Allow: `{"behavior":"allow","updatedInput": <input>}`
     - Deny: `{"behavior":"deny","message":"<reason>"}`
5. **Permission modes** (`--permission-mode`):

| Mode | What runs without asking |
|---|---|
| `default` ("Manual") | Reads only |
| `acceptEdits` | Reads, file edits, and common file commands (`mkdir`, `touch`, `mv`, `cp`) |
| `plan` | Reads, plus classifier-approved commands when auto mode is available. **No edits** until the plan is approved |
| `auto` | Most actions; a classifier model reviews each one instead of you. Available on all plans with supported models |

   A `-p` run without the flag can start in `auto`, so the buddy **always passes the flag explicitly**.
6. **`opusplan`** is a model alias that uses **Opus in plan mode** and **Sonnet for execution**. It is set with `--model opusplan`. Other aliases: `default`, `sonnet`, `opus`, `haiku`, `fable`, `best`.
7. **Effort:** `--effort low|medium|high|xhigh|max` applies to that run only.
8. **Persona:** `--append-system-prompt-file <file>` adds to Claude Code's default prompt. It is snapshotted per session, so edits apply to **new** conversations.
9. **Extra folders:** `--add-dir <path>` gives Claude access to folders outside the working directory. Reads there work; edits still follow the permission mode.
10. **Piped stdin** is accepted by `claude -p` (capped at 10 MB).
11. **Stopping a run:** SIGINT ends the current turn cleanly. SIGTERM exits with code 143 and leaves the turn unfinished.

**To verify in the first spike** (on the Mac):
- **Plan mode:**
  - In plan mode under `-p`, does finishing a plan arrive at the permission tool as `tool_name: "ExitPlanMode"` with `input.plan`?
  - After it's allowed, which mode does the rest of the run use? (§10.4)
- **Prompt via stdin:** does `claude -p` with **no prompt argument** use stdin as the whole prompt? If not, use the fallback: write the prompt to `.buddy/inbox/prompt-<id>.md` and pass `"Read and follow the request in <path>"`.
- **Image reading:** does Claude Code's Read tool view a PNG inside the workspace without a permission prompt in `default` mode?
- **Window bounds:** does `CGWindowListCopyWindowInfo` return `kCGWindowBounds` for other apps' windows **without** the Screen Recording permission on the installed macOS version?
- **Native compact:** does `claude -p --resume <id> "/compact"` (and `"/compact <focus>"`) actually compact, and what does the stream/`result` report? The docs list only some built-in commands (`/model`, `/effort`, `/config`, `/rename`, …) as available with `-p`; `/compact` and `/clear` are not listed. The answer picks the compact path in §8.6.
- **Context usage:** which fields of the `result` message (and the last `assistant` message) give the tokens in context, and where does the context-window size come from (200k vs `[1m]` models)? Does the stream report an automatic compaction? If usage can't be read, the context meter is hidden and nothing else changes.
- **Fork:** does `claude -p --resume <id> --fork-session` return a new `session_id` and leave the original resumable?
- **No-tools summary run:** does `claude -p --resume <id> --tools ""` run a turn that answers in text only (needed for the buddy-made compact)?

---

## 5. Architecture

```
┌──────────────────────────────── Tauri app (one process) ─────────────────────────────────┐
│                                                                                           │
│  WebView (TypeScript)                 Rust core                                           │
│  ────────────────────                 ─────────                                           │
│  sprite.ts   draws frames             physics/     engine (pure step), behaviors, drag    │
│  mood.ts     conversation moods       platform/    traits + macos/ impl (windows later)   │
│  bubble.ts   input/reply/ask/plan     pet_window   applies position @60 Hz, sizes window  │
│  modebar.ts  mode + model chips       claude_runner, stream_parser, activity              │
│  activity.ts "📖 reading foo.rs"      perm_server (MCP), perm_broker                      │
│  dropzone.ts Finder drops             attachments  inbox files, clipboard, screenshots    │
│  ipc.ts      talks to Rust            quick_actions, notify, settings, menubar, hotkey    │
│                                                                                           │
└───────────────▲──────────────────────────────────────────────────┬───────────────────────┘
                │ events: pose, activity, chat text, asks, done     │ one process per message
                │ commands: send, answer, set mode/model, grab…     ▼
                │      stdin: <prompt text>  ──►  claude -p --resume <session_id>
                │                --permission-mode <default|acceptEdits|plan|auto>
                │                --model <alias>  [--effort <level>]  [--add-dir <dir>…]
                │                --output-format stream-json --verbose --include-partial-messages
                │                --mcp-config '{"mcpServers":{"buddy":{http, url, token}}}'
                │                --permission-prompt-tool mcp__buddy__approve
                │                --append-system-prompt-file persona.md
                │                (env PATH = your login shell's PATH)
                │                                                   │
                └──── Allow / Deny / Approve plan ◄── perm_broker ◄─┘  MCP call: approve(tool_name, input)
```

**Who owns what:**
- **Rust physics owns *where* the pet is**: position, velocity, surface, and movement pose.
- **TypeScript mood owns *how it feels* during a conversation**: listening, thinking, talking, an activity, asking, happy, sad.
- The animation shown is the **mood/activity pose** while a conversation is active, otherwise the **physics pose**.
- During a conversation the physics engine is **paused**. The pet holds still wherever it is so the bubble stays put.

---

## 6. Physics & movement (Shimeji-style)

### 6.1 Coordinates
- All physics uses **points in a top-left-origin global space**: (0,0) is the top-left of the main display, and y grows downward. `CGWindowList` already uses this space.
- `NSScreen` frames use a bottom-left origin, so `platform/macos/screens.rs` flips them once: `y_top = main_screen_height − (y + height)`.
- Window moves use Tauri `LogicalPosition` (points). **There is no DPI math anywhere**; Retina scaling is handled by macOS.

### 6.2 The world
Rebuilt every **250 ms** and treated as a list of surfaces:

| Surface | Comes from | Pet can… |
|---|---|---|
| **Floor** | bottom of each screen's `visibleFrame` (just above the Dock, or the screen bottom if the Dock is hidden or on the side) | walk, sit, sleep |
| **Walls** | left/right edges of each `visibleFrame` (a side Dock moves its wall inward) | climb up/down, hang |
| **Ceiling** | top of each `visibleFrame` (just under the menu bar and notch) | crawl, let go |
| **Ledges** | top edges of other apps' windows, **only the visible parts** | walk, sit, sit with legs dangling at the end |

**Visible parts only:** if window B is in front of window A and covers part of A's top edge, that part isn't a ledge. This is pure geometry (subtract nearer windows' rectangles from each top edge), and it is unit-tested.

### 6.3 The engine: `physics/engine.rs`
- `step(state, world, input, dt) -> (state, events)` is a **pure function**: no OS calls, fully unit-tested.
- `state`: position, velocity, facing, current surface, pose, behavior timer.
- Runs on a fixed **60 Hz** tick. After each step, `pet_window` moves the real window.
- **Gravity** about 1000 pt/s², terminal velocity about 900 pt/s (values in points).
- **Landing:** a slow fall plays **land**; a fast fall plays a **splat** (bounce with damping 0.3, then lie flat briefly).
- **Hitting a wall mid-air:** the pet **clings and climbs** about half the time, otherwise it bounces off.
- **Standing on a ledge:** window **moved** → the pet is carried along by the same offset; window **minimized/closed/covered/moved to another Space** → the pet **falls**.

### 6.4 Behaviors: `physics/behavior.rs`
Weighted random choices, as in Shimeji's `behaviors.xml`, tunable in `behaviors.json`:

| On | Possible behaviors |
|---|---|
| Floor / ledge | stand, walk, run, sit, **sit-dangle** (at a ledge end), sleep, **jump to a nearby higher ledge**, walk to a wall and **climb** |
| Wall | climb up, climb down, hang still, **jump off**, reach the top and move onto the ceiling |
| Ceiling | crawl left/right, hang, **let go** (fall) |
| In the air | fall |

### 6.5 Grab and throw: `physics/drag.rs`
- **Mouse down on the pet** → `invoke("grab", offset)`. Rust switches to **dragged** (dangling pose) and moves the window with the **global cursor** every tick (`CGEvent` location, `platform/macos/mouse.rs`).
- **Release detection happens in Rust**, by polling `CGEventSourceButtonState(CombinedSessionState, left)`. A fast flick that leaves the small window is still caught. **No Accessibility permission is needed.**
- **Throw velocity** = the average cursor velocity over the last 100 ms, capped at about 1500 pt/s.
- **Click vs. drag:** if the cursor moved less than 4 pt and was released within 250 ms, it's a **left click**, which opens the chat bubble.
- **Right click** (or Control-click) opens the quick-action menu (§8.4).

### 6.6 Window detection: `platform/macos/window_list.rs`
- `CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements, kCGNullWindowID)` returns windows **front to back**, on the current Space only.
- **Keep** a window only if:
  - `kCGWindowLayer == 0` (normal app windows, not menus, the Dock or overlays)
  - `kCGWindowAlpha > 0`
  - the owner PID isn't the buddy's own
  - it's at least 100×50 pt
- Only `kCGWindowBounds` is read. Titles are **never** read, so **no Screen Recording permission** is needed for this (verified in the spike).
- **Spaces & full screen:** only the current Space's windows are listed, so switching Spaces simply changes the ledges. The pet window is visible on all Spaces (§11).

### 6.7 Multiple displays
The world combines every display's `visibleFrame`, using the arrangement from System Settings → Displays. The pet can walk off one display's floor and fall onto another's if they're arranged that way. Gaps between displays count as walls.

---

## 7. Modes & models

### 7.1 Modes
The mode applies to the **next message you send**. The conversation continues either way.

| Buddy name | `--permission-mode` | What the buddy asks you about |
|---|---|---|
| 🛡️ **Ask first** (default) | `default` | Every edit, command, or web action |
| ✏️ **Accept edits** | `acceptEdits` | Commands and web actions; file edits just happen |
| 🗺️ **Plan** | `plan` | Nothing changes until you approve the plan (§10.4) |
| ⚡ **Auto** | `auto` | Rarely: a safety classifier reviews actions instead of you. Shown with a warning the first time you pick it |

- `bypassPermissions` / `--dangerously-skip-permissions` is **never offered**.
- **How to switch:**
  - the **mode chip** under the input box: click to cycle, like Shift+Tab
  - the menu bar → **Mode**
  - the right-click menu → **Mode**
- The pet wears a tiny badge: 🗺️ in Plan, ⚡ in Auto.
- **One-off plan:** start a message with `/plan`.

### 7.2 Models
- The **model chip** sits next to the mode chip. Options: **Default**, **opusplan**, **opus**, **sonnet**, **haiku**, **fable**. It is passed as `--model <alias>`.
- Which models work depends on your Claude plan. The reply footer shows the **model actually used** (from `system/init`).
- **`opusplan` + Plan mode** is recommended for bigger tasks: Opus plans, and Sonnet carries out the plan after you approve.
- **Effort** (menu bar → Effort): Default / low / medium / high / xhigh / max.

All choices are saved and remembered across restarts.

---

## 8. Helper features

### 8.1 Drop files on the pet
- Drag one or more files or folders from **Finder** (or the Desktop) onto the pet. Tauri's window `drag-drop` events report the paths.
- **While hovering:** the pet plays a **catch** pose with arms out. On drop it plays **happy**.
- **On drop:** physics pauses and the **input bubble** opens with the items shown as **chips** (📄 `report.pdf`, 📁 `Photos`). Each chip has an ✕ to remove it. Type what you want. Pressing Enter with no text sends "Take a look at these and tell me what they are."
- **What's sent to Claude:**
  - the prompt text, followed by an **Attached items** list of absolute paths
  - `--add-dir` for each item's folder (for a dropped folder, the folder itself)
  - **edits still follow the current mode** (Ask first asks)
- The added folders are remembered **for the current conversation** and passed again on every following message. They are cleared on New conversation.
- **Limits:** at most **20 items** per drop. Folders aren't read upfront; Claude explores them as needed.
- **macOS privacy:** files in `~/Desktop`, `~/Documents` and `~/Downloads` are protected by macOS. The first time `claude` reads from them, macOS may ask for permission ("… would like to access files in your Downloads folder"). The prompt names whichever app started the process, which is the buddy (or your terminal during `tauri dev`). The pet warns about this the first time. If access is denied, Claude's read fails and the error is shown.

### 8.2 The pet shows what it's doing (`activity.rs` / `activity.ts`)
While Claude works, each `tool_use` in the stream switches the pet's pose and a small label above its head:

| Tool(s) | Pose | Label example |
|---|---|---|
| `Read`, `Glob`, `Grep` | **read** | 📖 reading `main.rs` |
| `Write`, `Edit`, `NotebookEdit` | **write** | ✍️ writing `notes.md` |
| `Bash` | **run** | 💻 running `npm test` |
| `WebFetch`, `WebSearch` | **browse** | 🌐 searching "tauri drag drop" |
| `Agent` / `Task` (subagents) | **think** | 🧠 thinking hard… |
| anything else (`mcp__*`, etc.) | **think** | 🔧 using a tool |

- Labels are cut to 40 characters, and paths are shortened to the file name.
- Characters without these poses fall back through `aliases`, then `think`, then `stand`.

### 8.3 "Done!" alert
- When a run finishes, the pet plays **happy + wave** (or **sad** on error).
- **Minimizing the bubble:** you can close the bubble while Claude is still working. The pet stays put, keeps its activity pose and label, and shows a small **"…" dot** you can click to reopen the reply.
- **macOS notification** (`tauri-plugin-notification`, Notification Center) when **either**:
  - the run took **longer than 30 seconds**, or
  - the bubble was **minimized** when it finished.

  The notification reads "✅ Done: <first line of the reply>" (or "⚠️ Something went wrong"). Clicking it reopens the reply.
- **Approval requests also notify** if the bubble is minimized: "🙋 I need your OK to run `npm install`". The pet also bounces.
- macOS asks for notification permission on first use. If it's denied, notifications are silently skipped; the wave and the "…" dot still work.
- Notifications can be turned off in settings (`notifications: false`).

### 8.4 Quick-action menu
- **Right-click** (or Control-click) the pet to open a native context menu (Tauri v2 `Menu` popup, an NSMenu):

```
⚡ Summarize my clipboard
🧩 Explain the code in my clipboard
🧹 Tidy my Downloads folder
👀 What's on my screen?
──────────────
💬 Talk               (same as left-click)
🆕 New conversation   🕘 Recent ▸
🗜️ Compact conversation   🌿 Branch
🛡️ Mode ▸   🤖 Model ▸
✏️ Edit quick actions…
```

- Actions come from **`quick-actions.json`** in `~/Library/Application Support/<bundle-id>/`. **Edit quick actions…** opens it in your default editor, and changes are picked up automatically (file watcher).

```json
[
  { "label": "Summarize my clipboard",  "icon": "⚡",
    "prompt": "Summarize this in 3 bullet points:\n\n{{clipboard}}" },
  { "label": "Explain the code in my clipboard", "icon": "🧩",
    "prompt": "Explain what this code does, simply:\n\n{{clipboard}}" },
  { "label": "Tidy my Downloads folder", "icon": "🧹",
    "prompt": "Look at my Downloads folder and propose how to organize it into subfolders.",
    "mode": "plan", "addDirs": ["~/Downloads"] },
  { "label": "What's on my screen?", "icon": "👀",
    "prompt": "Describe what's on my screen and point out anything that looks wrong.\n\n{{screen}}" }
]
```

- **Fields:**
  - `label`, `icon`, `prompt` (required)
  - `mode` and `model` (optional, apply to **this message only**)
  - `addDirs` (optional, `~` allowed)
  - `ask` (optional, `true`): opens the input bubble prefilled with the prompt so you can edit it before sending
- **Placeholders:**
  - `{{clipboard}}`: the current clipboard text (`tauri-plugin-clipboard-manager`). Up to 2,000 characters go inline. Longer text is saved to `.buddy/inbox/clipboard-<id>.txt` and the placeholder becomes "(see the file at <path>)". An empty clipboard → the pet says "Your clipboard is empty!" and nothing is sent.
  - `{{screen}}`: runs the screenshot flow (§8.5) first; it is replaced with the screenshot's path.
  - `{{workspace}}`: the workspace folder path.
- A broken `quick-actions.json` → the menu shows "⚠️ quick-actions.json has an error" with **Edit**, and the built-in defaults are used.

### 8.5 Look at my screen
- **Ways to start:**
  - the **📷 button** in the input bubble
  - the quick action "What's on my screen?"
  - an optional hotkey (`screen_hotkey`, unset by default)
- **Capture:**
  1. The pet window **hides** for one frame so it isn't in the picture.
  2. The **display the pet is on** is captured (`xcap` crate, ScreenCaptureKit on macOS 13+).
  3. The pet reappears.
- **Always confirmed:** the bubble shows a **thumbnail preview** and "This picture will be sent to Claude." with **Send** and **Cancel**. Nothing leaves the Mac without a click, even from a quick action.
- **On Send:** the PNG is saved to `.buddy/inbox/screen-<id>.png` (downscaled so the longest side is at most 1920 px; Retina captures are large). The prompt includes the path, and Claude opens it with its Read tool.
- **Cleanup:** inbox files older than **24 hours** are deleted when the app starts and every hour.
- **Screen Recording permission:**
  - The first capture triggers the macOS prompt (System Settings → Privacy & Security → Screen & System Audio Recording). If it's denied, the pet explains how to enable it.
  - **Dev builds:** during `npm run tauri dev`, the permission is granted to the **terminal app** you launched from. Built `.app` bundles that are only ad-hoc signed may **ask again after each rebuild**. This is a known macOS nuisance for unsigned personal builds and doesn't happen once the app stops changing.

### 8.6 Conversations: new, recent, compact, branch

The buddy starts one `claude -p` process per message and keeps the conversation going by passing the Claude Code **session id** with `--resume`. Every conversation is therefore just an id, which makes these Claude Code-style commands cheap.

**New conversation** (`/new`)
- **Ways to start:**
  - menu bar → **New conversation**
  - right-click menu → **New conversation**
  - the **🆕** button in the bubble header
  - typing `/new`, `/clear` or `/reset` as the whole message
- **What it does:**
  1. If a run is active, it is stopped first (SIGINT, as in §9.1 `cancel()`).
  2. `last_session_id`, `session_add_dirs` and `pending_summary` are cleared.
  3. The reply bubble closes and the pet waves.
- The old conversation is **not deleted**: it stays in **Recent** (below).
- The next message starts a fresh Claude Code session with no memory of the old one.

**Recent conversations** (`/resume`)
- `conversations.json` in the app's config folder keeps the last **20** conversations, newest first:

```json
[
  { "id": "550e8400-e29b-41d4-a716-446655440000",
    "title": "Summarize the PDF in Downloads",
    "last_used": "2026-10-19T14:02:11Z",
    "model": "claude-opus-5-5",
    "add_dirs": ["/Users/me/Downloads"] }
]
```

- An entry is added on `SessionStarted`. The `title` is the first prompt with whitespace collapsed and cut to **40 characters**. `last_used` and `model` update on every `Finished`. The 21st oldest entry is dropped from the list (the transcript itself stays on disk until Claude Code's own cleanup).
- **Recent ▸** (menu bar and right-click) lists titles with relative times ("2 h ago"). Picking one sets `last_session_id`, restores its `add_dirs` (Claude Code does not restore `--add-dir` on resume, so the buddy passes them again), clears `pending_summary`, and shows "Resumed: <title>" in the bubble.
- The buddy does not replay the old messages (the transcript file format is internal to Claude Code). Ask "where were we?" and Claude answers from its own history.
- Typing `/resume` as the whole message opens the Recent list as a small picker in the bubble.

**Compact** (`/compact [focus]`)
- **Ways to start:**
  - typing `/compact` or `/compact <what to keep>`
  - menu bar and right-click → **Compact conversation**
  - the **Compact** button on the context hint (below)
- Disabled while a run is active, and when there is no conversation yet.
- The pet shows 🗜️ **compacting…** (think pose) and the bubble is replaced by a spinner; the result is "🗜️ Conversation compacted."
- **Path A, native** (used only if the §4 spike shows it works): `claude -p --resume <id> "/compact <focus>"`. The session id stays the same.
- **Path B, buddy-made** (the fallback):
  1. One extra turn on the **same session** with `--tools ""` (text only, nothing can change): "Summarize our conversation so far in at most 300 words. Keep decisions, file paths, names, open tasks and anything I asked you to remember. <focus if given>. Reply with the summary only." The full reply is captured (not shown as a normal answer).
  2. The summary is stored as `pending_summary` (and saved to `.buddy/inbox/compact-<id>.md` for reference).
  3. `last_session_id` is cleared; `session_add_dirs` is **kept**.
  4. On the **next** message, the prompt becomes "Context from our earlier conversation (summary):\n<summary>\n\n<your message>" in a **new** session. `pending_summary` is cleared when that session starts. This costs no extra turn.
  5. In Recent, the old conversation keeps its title; the new one is titled "Compacted: <old title>".
- Which path is used is a build-time choice (`CompactStrategy::Native | Summarize`) set after the spike, not detected at run time.
- If the summary turn fails or comes back empty, **nothing changes**: the old conversation is still active and the pet says "Couldn't compact. Your conversation is unchanged."
- **Automatic compaction** still happens inside Claude Code when the context nears its limit; the buddy doesn't need to do anything. If the stream reports it (spike), the label shows "🗜️ tidying memory…".

**Context meter**
- The reply footer shows a small ring/bar **🧠 62%**: tokens currently in context ÷ the model's context window. It is an estimate and is shown with a "~".
- Source: the usage fields of the last turn (exact fields decided by the spike). If they can't be read, the meter is hidden and nothing else changes.
- At or above `context_hint_percent` (default **80**) the pet asks "My head is getting full. Compact?" with **Compact** and **Not now**. It asks **once per conversation**; **Compact** or **New conversation** resets that.

**Branch conversation**
- Menu → **Branch conversation** arms a one-shot flag: the **next** message runs with `--resume <id> --fork-session`. Claude Code copies the history into a new session id, which becomes `last_session_id`; the original stays in Recent and can be resumed later.
- Use it to try a different approach without losing the current thread.
- Dependent on the §4 fork spike; if it fails under `-p`, the menu item is hidden.

---

## 9. Components

### 9.0 Platform layer (Windows-ready)

**Only `platform/` touches OS APIs.** Everything else (physics, Claude runner, approvals, helpers, UI) is shared code.

```rust
// platform/mod.rs
pub trait Screens     { fn work_areas(&self) -> Vec<Display>; }            // floor/walls/ceiling (top-left points)
pub trait WindowList  { fn scan(&self) -> Vec<WindowRect>; }               // front-to-back, others' windows only
pub trait Mouse       { fn cursor(&self) -> Point; fn buttons(&self) -> Buttons; }
pub trait ShellEnv    { fn path(&self) -> String; }                         // PATH for child processes
pub trait ProcessCtl  { fn spawn_group(&self, cmd: Command) -> Child;       // own process group / job
                        fn interrupt(&self, c: &Child); fn kill(&self, c: &Child); }
pub trait PetWindowOs { fn setup(&self, w: &WebviewWindow); }              // no Dock icon, all Spaces, level…

pub fn current() -> Platform { /* #[cfg(target_os = "macos")] → macos::new() */ }
```

- `#[cfg(target_os = "macos")] mod macos;` is the only implementation in v1.
- **A Windows port later** = add `platform/windows/` implementing the same traits (`EnumWindows` + DWM bounds, `GetAsyncKeyState`, Job Objects + CTRL_BREAK, `SetWindowLong` tool window, …) plus the Windows rows in `tauri.conf.json`. No changes elsewhere.
- Tests use a **fake platform** (fixed displays, scripted windows and cursor), which is also how the physics and drag tests run without a screen.

### 9.1 Rust (`src-tauri/src/`)

**`platform/macos/`: the macOS implementation of §9.0**
- `env.rs`: **login-shell PATH.** Apps opened from Finder start with a minimal PATH (`/usr/bin:/bin:/usr/sbin:/sbin`), so `claude`, `git`, `node` and Homebrew tools wouldn't be found.
  - At startup it runs `$SHELL -ilc 'printf %s "$PATH"'` (5 s timeout) and uses the result for every `claude` process.
  - Fallback: append `/opt/homebrew/bin:/usr/local/bin:~/.local/bin`.
- `screens.rs`: `NSScreen` frames and visibleFrames → the top-left-origin rectangles of §6.1 (`objc2-app-kit`). Refreshed when displays change.
- `window_list.rs`: §6.6 (`objc2-core-graphics` / `core-graphics`).
- `mouse.rs`: global cursor location and left/right button state (`CGEvent`, `CGEventSource`).
- `app.rs`:
  - `ActivationPolicy::Accessory` (no Dock icon, no app menu)
  - pet window collection behavior `canJoinAllSpaces | stationary | fullScreenAuxiliary`
  - floating window level

**`claude_runner.rs`: runs Claude Code**
- `build_args(session_id?, mode, model?, effort?, add_dirs, mcp_config, persona_path) -> Vec<String>` is a **pure function** with unit tests.
  - Always includes `--permission-mode <mode>` and `--permission-prompt-tool mcp__buddy__approve`.
  - Never includes `--bare`, `bypassPermissions` or `--dangerously-skip-permissions`.
- **The prompt is written to stdin**, which is then closed. The prompt-file fallback is used if the spike says so.
- Finds `claude` using the login-shell PATH (`which` crate), then `~/.local/bin/claude`, `/opt/homebrew/bin/claude`, `/usr/local/bin/claude`.
- Started with the working directory set to the **workspace folder**, `PATH` from the `ShellEnv` trait (`platform/macos/env.rs`), and **its own process group** via the `ProcessCtl` trait (`process_group(0)` on macOS).
- Only one run at a time.
- **`cancel()`** sends **SIGINT** to the process group, so the turn ends cleanly. After 3 s it sends SIGTERM, and after 3 more seconds SIGKILL. This also stops any `Bash` commands Claude started.
- **Preflight on start:** `claude auth status`.

**`stream_parser.rs`: JSON lines → app events**
- `parse_line(&str) -> Option<BuddyEvent>`:
  - `SessionStarted { id, model }`
  - `TextDelta { text }`
  - `ToolStarted { name, input }`
  - `ToolFinished { ok }`
  - `Retry { attempt }`
  - `Finished { text, is_error, cost_usd, context_tokens: Option<u64> }` (`context_tokens` read from the usage fields; `None` when absent)
- Unknown or malformed lines return `None`.

**`activity.rs`:** `describe(tool_name, input) -> Activity { pose, icon, label }`, a pure function for §8.2. It is shared with `perm_broker`.

**`perm_server.rs`:** streamable-HTTP MCP server (`rmcp` on `axum`), **127.0.0.1 only**, **random port**, **random bearer token** per launch (sent via `--mcp-config` headers; requests without it get `401`). One tool: `approve(tool_name, input)`.

**`perm_broker.rs`**
- Pending requests: `request_id → oneshot sender`. Summaries come from `activity::describe`. `ExitPlanMode` → the plan text.
- **Auto-denies** after **5 min**, when a run is cancelled, or when the app quits.
- Triggers an approval notification when the bubble is minimized.

**`attachments.rs`:** the inbox (`~/BuddyWorkspace/.buddy/inbox/`) with clipboard and screenshot files and 24 h cleanup; the Attached-items text; the `--add-dir` list (20-item cap); per-conversation folders.

**`screen.rs`:** hide pet → capture the current display (`xcap`) → show pet → downscale → PNG bytes for the preview. It saves to the inbox only after **Send**.

**`quick_actions.rs`:** loads and validates `quick-actions.json` (with defaults), watches it for changes, fills in placeholders, and builds the right-click menu.

**`notify.rs`:** the notification rules of §8.3, through `tauri-plugin-notification`.

**`conversations.rs`** (§8.6)
- `ConversationStore`: load/save `conversations.json` (a missing or corrupt file → empty list; an unwritable path → the buddy keeps working without history).
  - `record_started(id, first_prompt, model, add_dirs)`, `touch(id, model)`, `list() -> Vec<Conversation>` (newest first, max 20), `remove(id)`.
  - `title_from(prompt) -> String`: whitespace collapsed, cut to 40 characters.
- `compact::summary_prompt(focus: Option<&str>) -> String` and `compact::carry_prompt(summary: &str, user_message: &str) -> String` (pure functions).
- `context::percent(tokens: u64, window: u64) -> u8` (clamped to 0–100) and `context::should_hint(percent, threshold, already_hinted) -> bool`.
- `CompactStrategy::{Native, Summarize}`: chosen by a constant after the spike.

**`physics/`**: `engine.rs`, `world.rs`, `behavior.rs`, `drag.rs` (see §6). No OS calls; `pet_window` feeds them data from the `platform` traits.

**`pet_window.rs`**
- The 60 Hz tick: world refresh every 250 ms → physics step → `set_position(LogicalPosition)`. Emits `pose` events.
- Resizes the window when a bubble opens, toward the side with room: above normally, below on the ceiling, beside on a wall.
- Handles hiding for screenshots.

**`settings.rs`**: JSON in `~/Library/Application Support/<bundle-id>/settings.json`:

| Key | Default |
|---|---|
| `workspace_dir` | `~/BuddyWorkspace` (created if missing) |
| `hotkey` | `Cmd+Shift+Space` |
| `screen_hotkey` | none |
| `character` | the first one in `characters/` |
| `mode` | `default` (Ask first) |
| `model` | none (Claude Code default) |
| `effort` | none |
| `last_session_id` | none |
| `session_add_dirs` | `[]` |
| `pending_summary` | none (set by a buddy-made compact; cleared when the new session starts) |
| `context_hint_percent` | `80` |
| `wander` | `true` |
| `stand_on_windows` | `true` |
| `notifications` | `true` |
| `auto_mode_warned` | `false` |
| `protected_folder_warned` | `false` |

**`menubar.rs` + hotkeys**
- A **menu bar icon** (Tauri tray icon = `NSStatusItem`, a template image so it suits light and dark menu bars) with:
  - **Talk**
  - **New conversation**
  - **Recent ▸** (last 20 conversations)
  - **Compact conversation**
  - **Branch conversation**
  - **Mode ▸**
  - **Model ▸**
  - **Effort ▸**
  - **Character ▸**
  - **Pause wandering**
  - **Stand on windows** ✓
  - **Notifications** ✓
  - **Edit quick actions…**
  - **Change workspace folder…**
  - **Quit Desktop Buddy** (⌘Q)
- Plugins: `global-shortcut`, `dialog`, `notification`, `clipboard-manager`, `opener`.

### 9.2 Web UI (`src/`)

**`sprite.ts`: draws the character**
- Every pose name maps to frames:
  - **Physics poses:** `stand, walk, run, sit, sit_dangle, sleep, fall, land, splat, climb, wall_hang, ceiling_crawl, ceiling_hang, jump, dragged, thrown`
  - **Moods:** `listen, think, talk, ask, happy, wave, sad, catch`
  - **Activities:** `read, write, run, browse`
- Missing poses fall back through `aliases`, then `think` (for activities), then `stand`. Sprites face **left** (the Shimeji convention), and facing right mirrors them.
- Drawn at `devicePixelRatio` with `image-rendering: pixelated`, so pixel art stays crisp on Retina.

**Format A: sprite sheet** (`characters/<name>/character.json` + `sheet.png`)
```json
{
  "format": "sheet",
  "name": "Pixel Cat",
  "license": "CC0, from <source URL>",
  "frameWidth": 32, "frameHeight": 32, "scale": 3,
  "poses": {
    "stand": { "row": 0, "frames": 4, "fps": 6,  "loop": true },
    "walk":  { "row": 1, "frames": 6, "fps": 10, "loop": true },
    "fall":  { "row": 2, "frames": 2, "fps": 8,  "loop": true },
    "climb": { "row": 3, "frames": 4, "fps": 8,  "loop": true }
  },
  "aliases": { "run": "walk", "think": "sit", "talk": "stand", "read": "sit" }
}
```

**Format B: Shimeji-ee image folder** (`characters/<name>/img/shime1.png … shime46.png`)
```json
{ "format": "shimeji", "name": "My Shimeji", "license": "<from the set's readme>" }
```
- The pose → image mapping comes from Shimeji-ee's standard `actions.xml` and is built into the app.
- ⚠️ The exact mapping must be read from the Shimeji-ee source at implementation time.
- ⚠️ Many Shimeji sets are fan art. They're fine for personal use, but check each set's license before sharing.

**`mood.ts`:** `next(mood, event) -> mood` is a **pure function** with unit tests. Moods: `none`, `listening`, `thinking`, `working(activity)`, `talking`, `asking`, `planning`, `happy`, `sad`. Anything other than `none` pauses physics.

**`bubble.ts`: the speech bubble**
- **Input:** a text box (Return sends, Shift+Return adds a newline, Esc closes), attachment chips, a **📷** button, the mode and model chips.
- **Reply:** streaming text, light markdown, scrolling, **Open full**, a footer with the model used, and **minimize**.
- **Ask:** the summary, then **Allow** and **Deny**.
- **Plan:** see §10.4.
- **Screenshot preview:** a thumbnail, then **Send** and **Cancel**.

**`activity.ts`:** the label above the pet's head and the "…" dot. **`dropzone.ts`:** catch pose and chips. **`modebar.ts`:** the chips and the Auto warning. **`ipc.ts`:** wrappers around `invoke`/`listen`.

---

## 10. Data flows

### 10.1 One message
1. Click the pet, press **⌘⇧Space**, drop files, or pick a quick action. The mood becomes **listening** and physics pauses.
2. Send → `claude_runner` writes the prompt to stdin and starts `claude -p` with the mode, model, effort, `--add-dir` list, `--resume`, and the login-shell PATH.
3. `SessionStarted` → the session id and model are saved.
4. `ToolStarted` → **working(activity)**: the pose and label change.
5. `TextDelta` → **talking**, and text streams into the bubble.
6. `Finished` → **happy + wave** (or **sad**), plus a notification if the §8.3 rules apply. When the bubble closes, the mood returns to **none** and physics resumes.

### 10.2 An action that needs approval
1. Claude Code calls MCP `approve(tool_name, input)`.
2. The mood becomes **asking**, and the bubble shows the summary with **Allow / Deny**. If the bubble is minimized, you get a notification and the pet bounces.
3. **Allow** → `{"behavior":"allow","updatedInput": input}`. **Deny** → `{"behavior":"deny","message":"The user denied this action."}`.

### 10.3 Grab and throw
Mouse down → grab → the pet follows the cursor → release → velocity → it flies, hits a wall or the ceiling, and lands or splats → wandering resumes.

### 10.4 Plan mode
1. Send a message in **Plan** mode (or start it with `/plan`). Claude explores but changes nothing.
2. When the plan is ready → `approve("ExitPlanMode", { plan })`.
3. The mood becomes **planning**, and a bigger bubble shows the plan with three buttons:
   - **✅ Approve: ask me before each change**
   - **⚡ Approve: auto-accept edits**
   - **✏️ Keep planning**: a text box, sent as `{"behavior":"deny","message":"Keep planning. User feedback: …"}`
4. On approve, the buddy's **mode** setting switches to the choice.
5. **The rest of the current run (decided by the spike):**
   - **If** allowing `ExitPlanMode` continues in Manual (`default`) → answer **allow**.
   - **Otherwise** → answer **deny** with `"Plan approved. Stop here; execution starts in the next message."`, then **automatically send** "Go ahead with the approved plan." with the chosen `--permission-mode`.

### 10.5 Drop files
Drop from Finder → catch/happy → input bubble with chips → send → prompt plus **Attached items**, with `--add-dir` for their folders (remembered for this conversation) → normal message flow.

### 10.6 Quick action
Right-click → pick an action → fill placeholders: `{{clipboard}}` (inline or a file), `{{screen}}` (preview, then Send) → if `ask: true`, an editable bubble; otherwise send right away, with the action's one-message `mode`/`model`/`addDirs`.

### 10.7 Look at my screen
📷 → hide the pet → capture the display → show the pet → preview bubble → **Send** saves the PNG to the inbox and adds its path to the prompt / **Cancel** discards it.

### 10.8 New and recent conversations
- **New:** stop any active run → clear `last_session_id`, `session_add_dirs`, `pending_summary` → close the bubble → wave. The next message starts a new session; `SessionStarted` adds it to `conversations.json`.
- **Recent:** pick an entry → `last_session_id = entry.id`, `session_add_dirs = entry.add_dirs`, `pending_summary` cleared → "Resumed: <title>". The next message runs `claude -p --resume <id> …` with those folders as `--add-dir`.

### 10.9 Compact
1. Trigger (`/compact [focus]`, menu, or the context hint) → refuse if a run is active or there is no conversation.
2. **Native strategy:** run `claude -p --resume <id> "/compact <focus>"` (session id unchanged) → on success show "🗜️ Conversation compacted."
3. **Summarize strategy:**
   1. Run the summary turn: `claude -p --resume <id> --tools "" --permission-mode default` with `compact::summary_prompt(focus)`.
   2. On success, store `pending_summary`, clear `last_session_id`, keep `session_add_dirs`, show "🗜️ Conversation compacted."
   3. On the next message, send `compact::carry_prompt(summary, message)` in a new session; on `SessionStarted` clear `pending_summary` and add the "Compacted: …" entry.
4. On any failure, change nothing and show "Couldn't compact. Your conversation is unchanged."

### 10.10 Context hint
After each `Finished`: `percent = context::percent(context_tokens, window)`; if `context::should_hint(percent, context_hint_percent, hinted)` → the pet shows the **Compact / Not now** bubble and `hinted = true` for this conversation.

---

## 11. Window behavior (macOS)

- One **borderless, transparent** window, with no shadow and not resizable, configured in `tauri.conf.json`:
  - `macOSPrivateApi: true` (required for transparent windows)
  - `decorations: false`, `transparent: true`, `shadow: false`, `alwaysOnTop: true`, `visibleOnAllWorkspaces: true`
- **The window is exactly the size of the character**, so there are no invisible areas blocking clicks on other apps. It's also the **drop target** for Finder files.
- **No Dock icon and no app menu bar** (`ActivationPolicy::Accessory`). The buddy lives in the **menu bar icon** and the pet itself. ⌘Q works from the menu bar menu.
- **Spaces & full screen:** the pet follows you to every Space and sits above full-screen apps (`canJoinAllSpaces | fullScreenAuxiliary`). Hiding it during full screen is a future feature.
- **Focus:**
  - Clicking the pet must **not** steal focus from your current app until a bubble needs typing.
  - The input bubble then focuses the buddy window, and closing it returns focus to the app you were using.
- **Mission Control / Stage Manager:** the pet stays visible (`stationary`). Stage Manager's side thumbnails aren't layer-0 windows, so they're never treated as ledges.
- Movement means moving the window with `LogicalPosition`, 60 times a second. When a bubble opens, the window grows toward the free side and shrinks back afterwards.

---

## 12. Personality: `persona.md`

Appended with `--append-system-prompt-file`. It contains:
- The buddy's **name** and **personality** (cheerful, curious, a little playful).
- **Display rules:** replies show in a small speech bubble, so lead with the answer, keep it short, and give details only if asked.
- **Work rules:**
  - you're on **macOS**: use macOS commands and paths (`open`, `pbcopy`, `mdfind`, `~/Library/...`)
  - work inside the workspace folder by default
  - say clearly what you're about to change
  - files listed under **Attached items** are things the user dropped on you
  - files in `.buddy/inbox/` are clipboard text or screenshots the user chose to share

Edits apply to the next **new conversation**.

---

## 13. Error handling

| Situation | What the buddy does |
|---|---|
| `claude` not found (even with the login-shell PATH) | **sad**, bubble shows how to install Claude Code and where it looked |
| Login-shell PATH lookup times out or fails | uses the fallback PATH (§9.1) and logs a warning |
| Not logged in | **sad**, bubble says to run `claude auth login` in Terminal |
| Chosen model not available on your plan | **sad**, shows the error, offers "Switch to Default model" |
| Auto mode not available | Claude Code falls back to Manual; the buddy shows "Auto isn't available, using Ask first" |
| Claude exits with an error / `result.is_error` | **sad**, bubble shows the error text |
| `api_retry` event | the label shows "retrying…" |
| MCP server can't start | Retries on a new port. If it still fails, **refuses to run prompts**. It never falls back to running without approvals. |
| Approval or plan not answered in 5 min | auto-deny, bubble says so |
| App quits or run cancelled with an approval pending | auto-deny, and the process group is stopped (SIGINT → SIGTERM → SIGKILL) |
| Window list unavailable or empty because of a permission change | **Stand on windows** turns off automatically; floor, walls and ceiling still work |
| A display is unplugged or rearranged | the world is rebuilt; if the pet is off every display, it is placed at the top-center of the main display and falls |
| Character missing or broken | built-in simple shape |
| More than 20 items dropped | the first 20 are used; the pet says "I can only carry 20 at a time!" |
| A dropped path disappears before sending | its chip is shown struck through and it is left out |
| macOS denies access to Desktop/Documents/Downloads | Claude's read error is shown, plus how to allow it in System Settings → Privacy & Security → Files and Folders |
| Clipboard empty or not text | the pet says so; nothing is sent |
| `quick-actions.json` invalid | menu shows "⚠️ quick-actions.json has an error" with **Edit**; defaults are used |
| Screen Recording denied | **sad**, explains how to enable it in System Settings; nothing is sent |
| Notifications denied | silently skipped; the wave and the "…" dot still work |
| Inbox folder can't be written | **sad**, shows the path and error; the action is cancelled |
| Resuming a conversation Claude Code no longer has (error text "No conversation found with session ID", e.g. after its 30-day cleanup) | the entry is removed from Recent, `last_session_id` is cleared, and the bubble says "I've forgotten that conversation" with **Send as a new conversation** |
| Compact fails or the summary is empty | nothing changes; "Couldn't compact. Your conversation is unchanged." |
| Compact or Branch asked while a run is active, or with no conversation | the menu item is greyed out; typing `/compact` shows "Nothing to compact yet." or "Wait for me to finish first." |
| `conversations.json` corrupt or unwritable | treated as an empty list; chatting keeps working, Recent is empty |
| Context usage can't be read | the context meter and hint are hidden; everything else works |
| `--fork-session` doesn't work with `-p` (spike) | **Branch conversation** is hidden |

---

## 14. Project setup

**⚠️ Development must happen on a Mac.** Tauri can't build or run a macOS app from Windows. The code is written, built and tested on the Mac, ideally with Claude Code running there.

- **Location (on the Mac):** `~/projects/desktop-buddy` (new git repo).
- **Scaffold:** `npm create tauri-app@latest`, template **vanilla-ts**, Tauri **v2**.
- **Bundle id:** `com.<you>.desktopbuddy`.
- **Rust crates:** `rmcp` (server, streamable HTTP), `axum`, `tokio`, `serde`, `serde_json`, `which`, `uuid`, `rand`, `xcap`, `image`, `notify` (file watcher), `nix` (process groups and signals), `objc2`, `objc2-app-kit`, `objc2-foundation`, `objc2-core-graphics` (or `core-graphics`).
- **Tauri plugins:** `global-shortcut`, `dialog`, `notification`, `clipboard-manager`, `opener`.
- **Build:**
  - `npm run tauri dev` while developing
  - `npm run tauri build` → `Desktop Buddy.app`, ad-hoc signed, kept in `~/Applications`
  - target: the Mac's own architecture (`aarch64-apple-darwin` on Apple Silicon); a universal build is optional
- **Prerequisites on the Mac:**

| Tool | How to check / install |
|---|---|
| macOS 13 Ventura or later | Apple menu → About This Mac |
| Xcode Command Line Tools | `xcode-select --install` |
| Rust toolchain | `curl https://sh.rustup.rs -sSf \| sh`, then check `cargo --version` |
| Node.js 20+ | `node --version` (install from nodejs.org or `brew install node`) |
| Claude Code, logged in | `claude --version` and `claude auth status` (install, then `claude auth login`) |
| Git | comes with the Xcode Command Line Tools |

---

## 15. Future ideas (not in v1)

- **Claude controls the pet:** MCP tools `emote(...)`, `walk_to(...)`, `set_reminder(time, text)` on the existing buddy MCP server.
- **Hide during full-screen** apps.
- **Start on login** (`SMAppService` / Login Items), auto-update.
- Developer ID signing and notarization, to share the app with friends.
- Climb the sides of other windows; chase the mouse cursor; multiple pets.
- Keep one `claude` process alive for faster replies.
- "Always allow this" rules per tool or command.
- Voice in/out (macOS speech APIs).
- React to events (new email, finished build, calendar reminders).
- **Windows port:** add `platform/windows/` implementing the §9.0 traits (`EnumWindows` + `DWMWA_EXTENDED_FRAME_BOUNDS`, `GetAsyncKeyState`, Job Objects, tool-window style, taskbar work area) and the Windows bundle config.

---

## 16. Testing

**Automated**
- `cargo test`:
  - **physics engine:**
    - falls and lands on the floor
    - lands on a ledge
    - ledge removed → falls
    - window moved → carried along
    - fast landing → splat
    - thrown into a wall → clings or bounces
    - climbs a wall onto the ceiling
    - let go from the ceiling → falls
  - **world geometry:** visible ledge parts with overlapping windows; multiple displays; side Dock walls
  - **coordinate flip:** NSScreen bottom-left → top-left, including a secondary display above or left of the main one
  - **drag:** click vs. drag vs. right-click; throw velocity
  - **stream_parser** with recorded real output
  - **activity::describe:** every row of §8.2, plus path shortening and 40-character cutting
  - **build_args:** mode/model/effort/add-dir flags; the permission tool is always present; never `--bare` / `bypassPermissions` / `--dangerously-skip-permissions`
  - **env:** login-shell PATH parsing; fallback PATH
  - **cancel:** SIGINT → SIGTERM → SIGKILL escalation timing (with a fake child process)
  - **attachments:** Attached-items text, the 20-item cap, per-conversation folders, the 2,000-character clipboard rule, 24 h cleanup
  - **quick_actions:** parse, validation fallback, placeholder filling, `~` expansion
  - **notify rules:** over 30 s, minimized, approval while minimized, disabled
  - **perm_broker:** allow, deny, timeout, cancel, ExitPlanMode summary
  - **conversations:**
    - add, touch, and the 20-item trim (newest first)
    - `title_from`: whitespace collapsed, cut to 40 characters
    - corrupt or missing file → empty list
    - an unwritable path doesn't panic
    - `remove` on "No conversation found"
  - **compact:**
    - `summary_prompt` with and without a focus
    - `carry_prompt` format
    - `pending_summary` is prepended once and then cleared
    - failure leaves state unchanged
    - `session_add_dirs` kept across a buddy-made compact
  - **context:** `percent` clamps 0–100; `should_hint` fires at the threshold, only once per conversation
  - **slash commands:** `/new`, `/clear`, `/reset`, `/compact [focus]`, `/resume` are recognized only as the whole message start, not mid-sentence
  - **build_args:** `--fork-session` only when the branch flag is armed; `--tools ""` only for the summary turn
- **MCP integration test:** `approve` with the token works; without it → 401.
- `vitest`: `mood.ts` transitions; character parsing (sheet + shimeji, aliases, activity fallbacks).

**Spikes first (on the Mac):**
1. Plan mode / `ExitPlanMode` (§10.4).
2. Prompt via stdin.
3. Reading a workspace PNG with no permission prompt.
4. `CGWindowList` bounds without the Screen Recording permission.
5. Native `/compact` under `claude -p --resume` (§8.6): decides `CompactStrategy`.
6. Context usage fields and the context-window size (§8.6): decides whether the meter ships.
7. `--fork-session` under `-p` (§8.6): decides whether Branch ships.
8. `--tools ""` with `--resume` for the summary turn (§8.6).

**Manual end-to-end (macOS, `npm run tauri dev`, then again with the built `.app` opened from Finder)**
1. **Launch from Finder:** the `.app` finds `claude` (the login-shell PATH works); no Dock icon; the menu bar icon appears.
2. **Physics:**
   - the pet falls to just above the Dock
   - it stands on a Safari window, is carried when the window moves, and falls when it's minimized (⌘M)
   - throw it → it bounces or climbs
   - it crawls under the menu bar and lets go
   - switch Spaces → the ledges update
   - try a second display if available
3. **Ask first:** "What files are in my workspace?" → no prompt. "Create hello.txt" → **Allow** → the file exists; with **Deny** → no file.
4. **Activity:** "Read README and run `ls`" → 📖 then 💻.
5. **Done alert:** minimize during a long task → the "…" dot → a Notification Center alert → clicking it reopens the reply. An approval while minimized → notification and bounce.
6. **Drop files:** drop a PDF from Downloads → the macOS folder-access prompt the first time → chips → "summarize" works. A follow-up message still reaches it. New conversation clears it.
7. **Quick actions:** copy text → "Summarize my clipboard". Edit the JSON → the menu updates. Break it → warning plus defaults.
8. **Screen:** 📷 → the Screen Recording prompt the first time → the pet isn't in the shot → **Cancel** sends nothing; **Send** gets a description.
9. **Plan + opusplan:** the footer shows Opus → plan bubble → *Keep planning* → *Approve: ask me before each change* → each change is asked about.
10. **Accept edits / Auto:** behave as described in §7.1; the Auto warning appears the first time.
11. **Focus:** clicking the pet doesn't take focus from your app; closing the bubble gives focus back.
12. Quit with ⌘Q while an approval is pending → denied, and no `claude` process is left (`pgrep claude`).
13. **New conversation:** tell it a secret word, choose **New conversation** (also try `/new`), ask "what was the secret word?" → it doesn't know. **Recent ▸** → pick the earlier conversation → ask again → it knows.
14. **Compact:** hold a longer conversation with a few specific facts, run `/compact keep the file names` → "Conversation compacted" → ask about one of the facts → still known. Check the meter dropped (if shown). Repeat from the menu.
15. **Context hint:** push a conversation past 80% (paste long text) → the **Compact / Not now** bubble appears once; **Not now** doesn't ask again in that conversation.
16. **Branch:** **Branch conversation** → send a message → ask about the old thread (known) → **Recent** shows both; the original is unchanged.
17. **Expired conversation:** delete a conversation's transcript under `~/.claude/projects/…` → resume it from Recent → "I've forgotten that conversation" and **Send as a new conversation** works.

---

## 17. Milestones

Each milestone ends with something you can run and enjoy on your Mac.

| Milestone | What you get | Includes |
|---|---|---|
| **M1: Alive** 🐾 | A Shimeji-style pet on your Mac | scaffold, `platform/` traits + fake platform for tests + `platform/macos/` (screens, mouse, app setup), physics engine, floor/walls/ceiling, behaviors, grab & throw, sprite sheets + Shimeji folders, basic menu bar icon |
| **M2: Talks** 💬 | Chat with Claude, safely | spikes, login-shell PATH, claude runner (stdin, process group), stream parser, MCP approvals (Ask first), bubbles, memory, persona, **activity poses (§8.2)**, **Done alert (§8.3)**, **new conversation and recent conversations (§8.6)** |
| **M3: Smart** 🧠 | Modes and helpers | modes/models/effort, plan bubble, **standing on windows (§6.6)**, **drop files (§8.1)**, **quick actions (§8.4)**, **look at my screen (§8.5)**, **compact, context meter and branch (§8.6)**, full settings and menu bar |
| **M4: Polish** ✨ | Daily-driver quality | built `.app` in `~/Applications`, the full manual checklist from Finder launch, multi-display and Spaces edge cases, permission-flow polish, performance (CPU while idle) |

---

## 18. Next steps

1. **You review this spec.** Ask for any changes.
2. Get the Mac ready (§14 prerequisites) and copy this spec into the new project as `docs/specs/2026-10-19-desktop-buddy-design.md`.
3. Write the step-by-step implementation plan, one milestone at a time, starting with **M1**.
4. Pick a character (a free sprite sheet or a Shimeji set).
5. Build, test, and meet your buddy.
