import { describe, expect, it } from "vitest";
import {
  NONE,
  isRunning,
  next,
  pausesPhysics,
  poseFor,
  type Mood,
  type MoodEvent,
} from "./mood";

const WORKING: Mood = {
  kind: "working",
  pose: "read",
  icon: "📖",
  label: "reading main.rs",
};
const ACTIVITY: MoodEvent = {
  type: "activity",
  pose: "write",
  icon: "✍️",
  label: "writing notes.md",
};
const FINISHED_OK: MoodEvent = {
  type: "finished",
  isError: false,
  cancelled: false,
};

describe("mood.next", () => {
  it("opening the bubble starts listening", () => {
    expect(next(NONE, { type: "open" })).toEqual({ kind: "listening" });
  });

  it("opening the bubble after a happy or sad moment listens again", () => {
    expect(next({ kind: "happy" }, { type: "open" }).kind).toBe("listening");
    expect(next({ kind: "sad" }, { type: "open" }).kind).toBe("listening");
  });

  it("opening the bubble does not disturb a run that is going", () => {
    expect(next(WORKING, { type: "open" })).toBe(WORKING);
    expect(next({ kind: "asking" }, { type: "open" }).kind).toBe("asking");
  });

  it("closing while only listening goes back to none", () => {
    expect(next({ kind: "listening" }, { type: "close" })).toEqual(NONE);
  });

  it("closing does not end a run's mood", () => {
    expect(next(WORKING, { type: "close" })).toBe(WORKING);
  });

  it("sending a message makes the pet think", () => {
    expect(next({ kind: "listening" }, { type: "sent" })).toEqual({
      kind: "thinking",
    });
  });

  it("a tool call switches to working with its activity", () => {
    expect(next({ kind: "thinking" }, ACTIVITY)).toEqual({
      kind: "working",
      pose: "write",
      icon: "✍️",
      label: "writing notes.md",
    });
  });

  it("a new tool call replaces the previous activity", () => {
    const mood = next(WORKING, ACTIVITY);

    expect(mood.kind === "working" && mood.label).toBe("writing notes.md");
  });

  it("a tool call after the run is over is ignored", () => {
    expect(next(NONE, ACTIVITY)).toEqual(NONE);
    expect(next({ kind: "happy" }, ACTIVITY).kind).toBe("happy");
    expect(next({ kind: "sad" }, ACTIVITY).kind).toBe("sad");
  });

  it("streaming text makes the pet talk", () => {
    expect(next({ kind: "thinking" }, { type: "text" }).kind).toBe("talking");
    expect(next(WORKING, { type: "text" }).kind).toBe("talking");
  });

  it("text outside a run is ignored", () => {
    expect(next(NONE, { type: "text" })).toEqual(NONE);
  });

  it("an approval makes the pet ask", () => {
    expect(next(WORKING, { type: "approval", plan: false }).kind).toBe(
      "asking",
    );
  });

  it("a plan approval makes the pet plan", () => {
    expect(next(WORKING, { type: "approval", plan: true }).kind).toBe(
      "planning",
    );
  });

  it("an approval outside a run is ignored", () => {
    expect(next(NONE, { type: "approval", plan: false })).toEqual(NONE);
  });

  it("answering an approval goes back to thinking", () => {
    expect(next({ kind: "asking" }, { type: "answered" }).kind).toBe(
      "thinking",
    );
    expect(next({ kind: "planning" }, { type: "answered" }).kind).toBe(
      "thinking",
    );
  });

  it("answering when nothing was asked changes nothing", () => {
    expect(next(WORKING, { type: "answered" })).toBe(WORKING);
  });

  it("a finished run makes the pet happy", () => {
    expect(next({ kind: "talking" }, FINISHED_OK).kind).toBe("happy");
  });

  it("a failed run makes the pet sad", () => {
    expect(
      next(
        { kind: "thinking" },
        { type: "finished", isError: true, cancelled: false },
      ).kind,
    ).toBe("sad");
  });

  it("a cancelled run just ends the mood", () => {
    expect(
      next(WORKING, { type: "finished", isError: true, cancelled: true }),
    ).toEqual(NONE);
  });

  it("a stray finish when nothing is running changes nothing", () => {
    expect(next(NONE, FINISHED_OK)).toEqual(NONE);
    expect(next({ kind: "listening" }, FINISHED_OK).kind).toBe("listening");
  });

  it("a lost conversation goes back to listening", () => {
    expect(next({ kind: "thinking" }, { type: "lost" }).kind).toBe(
      "listening",
    );
  });

  it("settling ends happy and sad only", () => {
    expect(next({ kind: "happy" }, { type: "settled" })).toEqual(NONE);
    expect(next({ kind: "sad" }, { type: "settled" })).toEqual(NONE);
    expect(next(WORKING, { type: "settled" })).toBe(WORKING);
  });

  it("a reset always returns to none", () => {
    expect(next(WORKING, { type: "reset" })).toEqual(NONE);
    expect(next({ kind: "asking" }, { type: "reset" })).toEqual(NONE);
  });

  it("never mutates the mood it is given", () => {
    const before: Mood = Object.freeze({ kind: "thinking" });

    expect(() => next(before, ACTIVITY)).not.toThrow();
    expect(before).toEqual({ kind: "thinking" });
  });
});

describe("isRunning", () => {
  it("is true while a run is going", () => {
    for (const mood of [
      { kind: "thinking" },
      WORKING,
      { kind: "talking" },
      { kind: "asking" },
      { kind: "planning" },
    ] as Mood[]) {
      expect(isRunning(mood)).toBe(true);
    }
  });

  it("is false otherwise", () => {
    for (const mood of [
      NONE,
      { kind: "listening" },
      { kind: "happy" },
      { kind: "sad" },
    ] as Mood[]) {
      expect(isRunning(mood)).toBe(false);
    }
  });
});

describe("pausesPhysics", () => {
  it("only none lets the pet move", () => {
    expect(pausesPhysics(NONE)).toBe(false);
    expect(pausesPhysics({ kind: "listening" })).toBe(true);
    expect(pausesPhysics(WORKING)).toBe(true);
    expect(pausesPhysics({ kind: "happy" })).toBe(true);
  });
});

describe("poseFor", () => {
  it("leaves the physics pose alone when idle", () => {
    expect(poseFor(NONE)).toBeNull();
  });

  it("maps each mood to its pose", () => {
    expect(poseFor({ kind: "listening" })).toBe("listen");
    expect(poseFor({ kind: "thinking" })).toBe("think");
    expect(poseFor(WORKING)).toBe("read");
    expect(poseFor({ kind: "talking" })).toBe("talk");
    expect(poseFor({ kind: "asking" })).toBe("ask");
    expect(poseFor({ kind: "planning" })).toBe("ask");
    expect(poseFor({ kind: "happy" })).toBe("happy");
    expect(poseFor({ kind: "sad" })).toBe("sad");
  });
});
