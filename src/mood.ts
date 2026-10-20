/**
 * The buddy's mood (design §9.2): what the pet is doing while a conversation
 * happens. `next` is a pure function, so every transition is unit-tested.
 * Any mood other than "none" pauses the pet's physics.
 */

/** The poses Claude's tools are acted out with (design §8.2). */
export type ActivityPose = "read" | "write" | "run" | "browse" | "think";

export type Mood =
  | { readonly kind: "none" }
  | { readonly kind: "listening" }
  | { readonly kind: "thinking" }
  | {
      readonly kind: "working";
      readonly pose: ActivityPose;
      readonly icon: string;
      readonly label: string;
    }
  | { readonly kind: "talking" }
  | { readonly kind: "asking" }
  | { readonly kind: "planning" }
  | { readonly kind: "happy" }
  | { readonly kind: "sad" };

export type MoodEvent =
  /** The input bubble opened. */
  | { readonly type: "open" }
  /** The bubble closed with no run going. */
  | { readonly type: "close" }
  /** A message was sent. */
  | { readonly type: "sent" }
  | {
      readonly type: "activity";
      readonly pose: ActivityPose;
      readonly icon: string;
      readonly label: string;
    }
  /** Part of the reply arrived. */
  | { readonly type: "text" }
  /** Claude needs an answer; `plan` is true for a plan to approve. */
  | { readonly type: "approval"; readonly plan: boolean }
  /** The approval was answered. */
  | { readonly type: "answered" }
  | {
      readonly type: "finished";
      readonly isError: boolean;
      readonly cancelled: boolean;
    }
  /** The saved conversation is gone: the bubble offers to start a new one. */
  | { readonly type: "lost" }
  /** The happy or sad moment is over. */
  | { readonly type: "settled" }
  /** A new conversation began. */
  | { readonly type: "reset" };

export const NONE: Mood = { kind: "none" };

/** Moods in which a run is going. */
const RUNNING: ReadonlySet<Mood["kind"]> = new Set([
  "thinking",
  "working",
  "talking",
  "asking",
  "planning",
]);

export function isRunning(mood: Mood): boolean {
  return RUNNING.has(mood.kind);
}

export function next(mood: Mood, event: MoodEvent): Mood {
  switch (event.type) {
    case "open":
      return mood.kind === "none" || mood.kind === "happy" || mood.kind === "sad"
        ? { kind: "listening" }
        : mood;
    case "close":
      return mood.kind === "listening" ? NONE : mood;
    case "sent":
      return { kind: "thinking" };
    case "activity":
      return isRunning(mood)
        ? {
            kind: "working",
            pose: event.pose,
            icon: event.icon,
            label: event.label,
          }
        : mood;
    case "text":
      return isRunning(mood) ? { kind: "talking" } : mood;
    case "approval":
      return isRunning(mood)
        ? { kind: event.plan ? "planning" : "asking" }
        : mood;
    case "answered":
      return mood.kind === "asking" || mood.kind === "planning"
        ? { kind: "thinking" }
        : mood;
    case "finished":
      return finish(mood, event.isError, event.cancelled);
    case "lost":
      return { kind: "listening" };
    case "settled":
      return mood.kind === "happy" || mood.kind === "sad" ? NONE : mood;
    case "reset":
      return NONE;
  }
}

function finish(mood: Mood, isError: boolean, cancelled: boolean): Mood {
  if (cancelled) {
    return NONE;
  }
  if (!isRunning(mood)) {
    return mood;
  }
  return isError ? { kind: "sad" } : { kind: "happy" };
}

/** Anything but "none" stands the pet still. */
export function pausesPhysics(mood: Mood): boolean {
  return mood.kind !== "none";
}

/** The pose that overrides the physics pose, or null to use the physics one. */
export function poseFor(mood: Mood): string | null {
  switch (mood.kind) {
    case "none":
      return null;
    case "listening":
      return "listen";
    case "thinking":
      return "think";
    case "working":
      return mood.pose;
    case "talking":
      return "talk";
    case "asking":
    case "planning":
      return "ask";
    case "happy":
      return "happy";
    case "sad":
      return "sad";
  }
}
