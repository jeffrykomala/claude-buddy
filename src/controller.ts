/**
 * The UI's brain (design §10): turns clicks, typing and Rust events into
 * mood changes and bubble updates. It owns the bubble, the activity label
 * and the "…" dot, and talks to Rust only through the `Ipc` it is given, so
 * the whole flow can be tested without a window.
 */

import { ActivityLabel, ReplyDot } from "./activity";
import { Bubble, type BubbleHandlers, type BubbleSize } from "./bubble";
import {
  errorMessage,
  type ApprovalRequest,
  type Ipc,
  type RunEvent,
} from "./ipc";
import {
  NONE,
  isRunning,
  next,
  pausesPhysics,
  poseFor,
  type Mood,
  type MoodEvent,
} from "./mood";

/** How long a minimized pet stays happy or sad before it moves again. */
export const SETTLE_MS = 6000;

/** The messages Rust handles itself instead of sending to Claude. */
const SLASH_COMMAND = /^\s*\/(new|clear|reset|resume)\s*$/i;

export interface ControllerDeps {
  readonly doc: Document;
  readonly ipc: Ipc;
  /** The pet's pose while a mood is acting (null: use the physics pose). */
  readonly setPoseOverride: (pose: string | null) => void;
  /** Runs `fn` after `ms`; returns a function that cancels it. */
  readonly schedule: (fn: () => void, ms: number) => () => void;
  readonly onError: (message: string) => void;
}

export class Controller {
  readonly bubble: Bubble;
  readonly label: ActivityLabel;
  readonly dot: ReplyDot;

  private mood: Mood = NONE;
  private minimized = false;
  private approval: ApprovalRequest | null = null;
  private lastMessage = "";
  private appliedPose: string | null = null;
  private appliedPaused = false;
  private cancelSettle: (() => void) | null = null;

  constructor(private readonly deps: ControllerDeps) {
    this.bubble = new Bubble(deps.doc, this.bubbleHandlers());
    this.label = new ActivityLabel(deps.doc);
    this.dot = new ReplyDot(deps.doc, () => this.restore());
  }

  // ------------------------------------------------------- pet and menu

  /** The pet was clicked: open, or close, the bubble. */
  onPetClick(): void {
    if (this.bubble.isOpen) {
      this.close();
    } else if (this.minimized) {
      this.restore();
    } else {
      this.openInput();
    }
  }

  /** The hotkey or the menu's Talk. */
  openInput(): void {
    if (this.minimized) {
      this.restore();
      return;
    }
    this.cancelPendingSettle();
    this.dispatch({ type: "open" });
    this.bubble.show();
    this.fire(this.deps.ipc.takeFocus());
    this.bubble.focusInput();
  }

  /** Close the bubble; if Claude is working this only minimizes it. */
  close(): void {
    if (isRunning(this.mood)) {
      this.minimize();
      return;
    }
    this.hideBubble();
    this.minimized = false;
    this.fire(this.deps.ipc.setMinimized(false));
    this.dispatch({ type: "close" });
    this.dispatch({ type: "settled" });
    this.updateDot();
  }

  minimize(): void {
    this.hideBubble();
    this.minimized = true;
    this.fire(this.deps.ipc.setMinimized(true));
    this.updateDot();
  }

  /** The "…" dot was clicked, or a notification brought the app forward. */
  restore(): void {
    if (!this.minimized) {
      return;
    }
    this.cancelPendingSettle();
    this.minimized = false;
    this.fire(this.deps.ipc.setMinimized(false));
    this.bubble.show();
    this.updateDot();
  }

  /** The app came forward (e.g. a notification was clicked). */
  onReopen(): void {
    if (this.minimized && (this.bubble.hasReply() || isRunning(this.mood))) {
      this.restore();
    }
  }

  // ---------------------------------------------------------- messages

  private send(text: string): void {
    this.lastMessage = text;
    if (SLASH_COMMAND.test(text)) {
      // Rust answers these with a "new conversation" or "picker" event.
      this.fire(this.deps.ipc.sendMessage(text));
      return;
    }
    this.cancelPendingSettle();
    this.dispatch({ type: "sent" });
    this.bubble.setBusy(true);
    this.bubble.startReply();
    this.deps.ipc.sendMessage(text).catch((error: unknown) => {
      this.finishRun(errorMessage(error), true, false, null);
    });
  }

  /** Events from the run (`buddy://event`). */
  onRunEvent(event: RunEvent): void {
    switch (event.type) {
      case "text":
        this.dispatch({ type: "text" });
        this.bubble.appendReply(event.text);
        return;
      case "activity":
        this.dispatch({
          type: "activity",
          pose: event.pose,
          icon: event.icon,
          label: event.label,
        });
        return;
      case "retrying":
        this.bubble.setStatus("retrying…");
        return;
      case "finished":
        this.finishRun(
          event.text,
          event.is_error,
          event.cancelled,
          event.model,
        );
        return;
      case "lost":
        this.onLost();
        return;
      case "started":
      case "tool_done":
        return;
    }
  }

  private finishRun(
    text: string,
    isError: boolean,
    cancelled: boolean,
    model: string | null,
  ): void {
    this.approval = null;
    this.bubble.setBusy(false);
    this.bubble.clearApproval();
    if (cancelled) {
      this.bubble.setStatus("Stopped.");
    } else {
      this.bubble.finishReply(text, isError, model);
    }
    this.dispatch({ type: "finished", isError, cancelled });
    if (this.minimized && !cancelled) {
      this.cancelPendingSettle();
      this.cancelSettle = this.deps.schedule(() => {
        this.cancelSettle = null;
        this.dispatch({ type: "settled" });
      }, SETTLE_MS);
    }
    this.updateDot();
  }

  private onLost(): void {
    this.approval = null;
    this.bubble.setBusy(false);
    this.dispatch({ type: "lost" });
    this.bubble.showLost();
    if (this.minimized) {
      this.restore();
    }
  }

  // --------------------------------------------------------- approvals

  onApproval(request: ApprovalRequest): void {
    this.approval = request;
    this.dispatch({ type: "approval", plan: request.kind === "plan" });
    this.bubble.showApproval(request);
    this.updateDot();
  }

  private answer(id: string, allow: boolean): void {
    this.fire(this.deps.ipc.answerApproval(id, allow));
    this.approval = null;
    this.bubble.clearApproval();
    this.dispatch({ type: "answered" });
    this.updateDot();
  }

  // ----------------------------------------------------- conversations

  /** A new conversation began (header button, menu, or `/new`). */
  onNewConversation(): void {
    this.cancelPendingSettle();
    this.approval = null;
    this.dispatch({ type: "reset" });
    this.bubble.setBusy(false);
    this.bubble.reset();
    this.hideBubble();
    this.minimized = false;
    this.fire(this.deps.ipc.setMinimized(false));
    this.updateDot();
  }

  /** `/resume` was typed: show the recent conversations. */
  onPicker(): void {
    this.deps.ipc
      .listRecent()
      .then((entries) => {
        if (!this.bubble.isOpen) {
          this.openInput();
        }
        this.bubble.showPicker(entries);
      })
      .catch((error: unknown) => this.deps.onError(errorMessage(error)));
  }

  /** "Resumed: …" and similar one-line notices. */
  onNotice(message: string): void {
    if (!this.bubble.isOpen) {
      this.openInput();
    }
    this.bubble.showNotice(message);
  }

  private pick(id: string): void {
    this.bubble.reset();
    this.deps.ipc.resume(id).catch((error: unknown) => {
      this.bubble.showNotice(errorMessage(error));
    });
  }

  private sendAsNew(): void {
    this.bubble.reset();
    if (this.lastMessage !== "") {
      this.send(this.lastMessage);
    }
  }

  // ------------------------------------------------------------ plumbing

  private bubbleHandlers(): BubbleHandlers {
    return {
      onSend: (text) => this.send(text),
      onStop: () => this.fire(this.deps.ipc.cancelRun()),
      onMinimize: () => this.minimize(),
      onClose: () => this.close(),
      onNew: () => this.fire(this.deps.ipc.startNewConversation()),
      onAnswer: (id, allow) => this.answer(id, allow),
      onPick: (id) => this.pick(id),
      onSendAsNew: () => this.sendAsNew(),
      onSizeChange: (size) => this.resizeBubble(size),
    };
  }

  private resizeBubble(size: BubbleSize): void {
    this.fire(this.deps.ipc.setBubble(size.width, size.height));
  }

  /** Hides the bubble and gives the window and the keyboard back. */
  private hideBubble(): void {
    this.bubble.hide();
    this.fire(this.deps.ipc.setBubble(0, 0));
    this.fire(this.deps.ipc.releaseFocus());
  }

  private dispatch(event: MoodEvent): void {
    this.mood = next(this.mood, event);
    this.applyMood();
  }

  private applyMood(): void {
    const pose = poseFor(this.mood);
    if (pose !== this.appliedPose) {
      this.appliedPose = pose;
      this.deps.setPoseOverride(pose);
    }
    const paused = pausesPhysics(this.mood);
    if (paused !== this.appliedPaused) {
      this.appliedPaused = paused;
      this.fire(this.deps.ipc.setPaused(paused));
    }
    if (this.mood.kind === "working") {
      this.label.show(this.mood.icon, this.mood.label);
    } else {
      this.label.hide();
    }
    this.updateDot();
  }

  private updateDot(): void {
    const waiting = isRunning(this.mood) || this.bubble.hasReply();
    if (this.minimized && waiting) {
      this.dot.show(this.approval !== null);
    } else {
      this.dot.hide();
    }
  }

  private cancelPendingSettle(): void {
    this.cancelSettle?.();
    this.cancelSettle = null;
  }

  private fire(promise: Promise<unknown>): void {
    promise.catch((error: unknown) => this.deps.onError(errorMessage(error)));
  }
}
