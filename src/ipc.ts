/**
 * Everything the webview says to and hears from Rust (design §9.2): the
 * command wrappers and the shape of each event. Kept in one place so the
 * rest of the UI never spells a command or event name.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ActivityPose } from "./mood";

export const EVENTS = {
  pose: "pet://pose",
  click: "pet://click",
  layout: "pet://layout",
  character: "pet://character",
  run: "buddy://event",
  approval: "buddy://approval",
  newConversation: "buddy://new",
  talk: "buddy://talk",
  picker: "buddy://picker",
  notice: "buddy://notice",
  reopen: "buddy://reopen",
} as const;

/** What happens while Claude works (`ui_event.rs`). */
export type RunEvent =
  | { readonly type: "started"; readonly id: string; readonly model: string }
  | { readonly type: "text"; readonly text: string }
  | {
      readonly type: "activity";
      readonly pose: ActivityPose;
      readonly icon: string;
      readonly label: string;
    }
  | { readonly type: "tool_done"; readonly ok: boolean }
  | { readonly type: "retrying"; readonly attempt: number }
  | {
      readonly type: "finished";
      readonly text: string;
      readonly is_error: boolean;
      readonly cancelled: boolean;
      readonly model: string | null;
    }
  | { readonly type: "lost" };

/** Claude is waiting for the user's OK (`perm_broker.rs`). */
export interface ApprovalRequest {
  readonly id: string;
  readonly tool_name: string;
  readonly kind: "action" | "plan";
  readonly headline: string;
  readonly detail: string | null;
}

/** Where the pet and bubble go inside the window (`pet_window.rs`). */
export interface LayoutEvent {
  readonly side: "above" | "below";
  readonly petX: number;
  readonly petY: number;
  readonly width: number;
  readonly height: number;
}

export interface RecentEntry {
  readonly id: string;
  readonly title: string;
  readonly when: string;
}

/** The commands Rust offers, as one object so tests can fake it. */
export interface Ipc {
  sendMessage(text: string): Promise<void>;
  cancelRun(): Promise<void>;
  answerApproval(id: string, allow: boolean): Promise<void>;
  setMinimized(minimized: boolean): Promise<void>;
  startNewConversation(): Promise<void>;
  resume(id: string): Promise<string>;
  listRecent(): Promise<RecentEntry[]>;
  setBubble(width: number, height: number): Promise<LayoutEvent>;
  setPaused(paused: boolean): Promise<void>;
  takeFocus(): Promise<void>;
  releaseFocus(): Promise<void>;
  grab(offsetX: number, offsetY: number): Promise<void>;
}

export const ipc: Ipc = {
  sendMessage: (text) => invoke("send_message", { text }),
  cancelRun: () => invoke("cancel_run"),
  answerApproval: (id, allow) => invoke("answer_approval", { id, allow }),
  setMinimized: (minimized) => invoke("set_minimized", { minimized }),
  startNewConversation: () => invoke("start_new_conversation"),
  resume: (id) => invoke("resume", { id }),
  listRecent: () => invoke("list_recent"),
  setBubble: (width, height) => invoke("set_bubble", { width, height }),
  setPaused: (paused) => invoke("set_paused", { paused }),
  takeFocus: () => invoke("take_focus"),
  releaseFocus: () => invoke("release_focus"),
  grab: (offsetX, offsetY) => invoke("grab", { offsetX, offsetY }),
};

/** Calls `handler` with each event's payload until the returned function runs. */
export function subscribe<T>(
  name: string,
  handler: (payload: T) => void,
): Promise<UnlistenFn> {
  return listen<T>(name, ({ payload }) => handler(payload));
}

/** A readable message from whatever a failed command rejected with. */
export function errorMessage(error: unknown): string {
  if (typeof error === "string") {
    return error;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return "Something went wrong.";
}
