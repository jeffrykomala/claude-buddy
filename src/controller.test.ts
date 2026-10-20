import { beforeEach, describe, expect, it, vi } from "vitest";
import { Controller, SETTLE_MS } from "./controller";
import type { ApprovalRequest, Ipc, LayoutEvent } from "./ipc";

type FakeIpc = { [K in keyof Ipc]: ReturnType<typeof vi.fn> };

const LAYOUT: LayoutEvent = {
  side: "above",
  petX: 0,
  petY: 0,
  width: 128,
  height: 128,
};

function makeIpc(): FakeIpc {
  return {
    sendMessage: vi.fn().mockResolvedValue(undefined),
    cancelRun: vi.fn().mockResolvedValue(undefined),
    answerApproval: vi.fn().mockResolvedValue(undefined),
    setMinimized: vi.fn().mockResolvedValue(undefined),
    startNewConversation: vi.fn().mockResolvedValue(undefined),
    resume: vi.fn().mockResolvedValue("Old chat"),
    listRecent: vi.fn().mockResolvedValue([]),
    setBubble: vi.fn().mockResolvedValue(LAYOUT),
    setPaused: vi.fn().mockResolvedValue(undefined),
    takeFocus: vi.fn().mockResolvedValue(undefined),
    releaseFocus: vi.fn().mockResolvedValue(undefined),
    grab: vi.fn().mockResolvedValue(undefined),
  };
}

let ipc: FakeIpc;
let poses: (string | null)[];
let scheduled: { fn: () => void; ms: number; cancelled: boolean }[];
let errors: string[];
let controller: Controller;

beforeEach(() => {
  ipc = makeIpc();
  poses = [];
  scheduled = [];
  errors = [];
  controller = new Controller({
    doc: document,
    ipc: ipc as unknown as Ipc,
    setPoseOverride: (pose) => poses.push(pose),
    schedule: (fn, ms) => {
      const entry = { fn, ms, cancelled: false };
      scheduled.push(entry);
      return () => {
        entry.cancelled = true;
      };
    },
    onError: (message) => errors.push(message),
  });
  document.body.replaceChildren(
    controller.bubble.element,
    controller.label.element,
    controller.dot.element,
  );
});

const lastPose = (): string | null | undefined => poses[poses.length - 1];

function typeAndSend(text: string): void {
  const box = controller.bubble.element.querySelector(
    "textarea",
  ) as HTMLTextAreaElement;
  box.value = text;
  box.dispatchEvent(
    new KeyboardEvent("keydown", { key: "Enter", cancelable: true }),
  );
}

function replyText(): string {
  return controller.bubble.element.querySelector(".bubble-reply")?.textContent ?? "";
}

const action = (selector: string): void =>
  (controller.bubble.element.querySelector(selector) as HTMLElement).click();

const APPROVAL: ApprovalRequest = {
  id: "req-1",
  tool_name: "Bash",
  kind: "action",
  headline: "💻 running ls",
  detail: "ls -la",
};

async function flush(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

describe("opening the bubble", () => {
  it("a click opens it, listens and stands the pet still", () => {
    controller.onPetClick();

    expect(controller.bubble.isOpen).toBe(true);
    expect(lastPose()).toBe("listen");
    expect(ipc.setPaused).toHaveBeenLastCalledWith(true);
    expect(ipc.takeFocus).toHaveBeenCalledOnce();
    expect(ipc.setBubble).toHaveBeenCalledWith(340, 150);
  });

  it("the hotkey opens it too", () => {
    controller.openInput();

    expect(controller.bubble.isOpen).toBe(true);
  });

  it("a second click closes it, gives focus back and lets the pet move", () => {
    controller.onPetClick();

    controller.onPetClick();

    expect(controller.bubble.isOpen).toBe(false);
    expect(ipc.setBubble).toHaveBeenLastCalledWith(0, 0);
    expect(ipc.releaseFocus).toHaveBeenCalled();
    expect(lastPose()).toBeNull();
    expect(ipc.setPaused).toHaveBeenLastCalledWith(false);
  });
});

describe("sending a message", () => {
  beforeEach(() => controller.openInput());

  it("sends it, locks the box and thinks", () => {
    typeAndSend("hello");

    expect(ipc.sendMessage).toHaveBeenCalledWith("hello");
    expect(lastPose()).toBe("think");
    expect(
      controller.bubble.element.querySelector(".bubble-send")?.textContent,
    ).toBe("Stop");
  });

  it("acts out each tool Claude uses", () => {
    typeAndSend("hello");

    controller.onRunEvent({
      type: "activity",
      pose: "read",
      icon: "📖",
      label: "reading main.rs",
    });

    expect(lastPose()).toBe("read");
    expect(controller.label.element.textContent).toBe("📖 reading main.rs");
    expect(controller.label.element.hidden).toBe(false);
  });

  it("streams the reply and talks", () => {
    typeAndSend("hello");

    controller.onRunEvent({ type: "text", text: "Woof" });

    expect(replyText()).toBe("Woof");
    expect(lastPose()).toBe("talk");
    expect(controller.label.element.hidden).toBe(true);
  });

  it("is happy when it finishes and unlocks the box", () => {
    typeAndSend("hello");
    controller.onRunEvent({ type: "text", text: "Done" });

    controller.onRunEvent({
      type: "finished",
      text: "Done",
      is_error: false,
      cancelled: false,
      model: "claude-opus",
    });

    expect(lastPose()).toBe("happy");
    expect(
      controller.bubble.element.querySelector(".bubble-send")?.textContent,
    ).toBe("Send");
    expect(
      controller.bubble.element.querySelector(".bubble-footer")?.textContent,
    ).toBe("claude-opus");
  });

  it("is sad when the run fails, and shows why", () => {
    typeAndSend("hello");

    controller.onRunEvent({
      type: "finished",
      text: "Not logged in",
      is_error: true,
      cancelled: false,
      model: null,
    });

    expect(lastPose()).toBe("sad");
    expect(
      controller.bubble.element.querySelector(".bubble-error")?.textContent,
    ).toBe("Not logged in");
  });

  it("a cancelled run says it stopped and does not celebrate", () => {
    typeAndSend("hello");

    controller.onRunEvent({
      type: "finished",
      text: "Cancelled.",
      is_error: true,
      cancelled: true,
      model: null,
    });

    expect(lastPose()).toBeNull();
    expect(
      controller.bubble.element.querySelector(".bubble-status")?.textContent,
    ).toBe("Stopped.");
  });

  it("Stop cancels the run", () => {
    typeAndSend("hello");

    action(".bubble-send");

    expect(ipc.cancelRun).toHaveBeenCalledOnce();
  });

  it("shows retrying", () => {
    typeAndSend("hello");

    controller.onRunEvent({ type: "retrying", attempt: 1 });

    expect(
      controller.bubble.element.querySelector(".bubble-status")?.textContent,
    ).toBe("retrying…");
  });

  it("ignores the events it has no use for", () => {
    typeAndSend("hello");

    controller.onRunEvent({ type: "started", id: "s", model: "m" });
    controller.onRunEvent({ type: "tool_done", ok: true });

    expect(lastPose()).toBe("think");
  });

  it("shows the problem if the message could not be sent", async () => {
    ipc.sendMessage.mockRejectedValueOnce("I'm still working on your last message.");

    typeAndSend("hello");
    await flush();

    expect(
      controller.bubble.element.querySelector(".bubble-error")?.textContent,
    ).toBe("I'm still working on your last message.");
    expect(lastPose()).toBe("sad");
  });

  it("does not go busy for a slash command Rust handles itself", () => {
    typeAndSend("/new");

    expect(ipc.sendMessage).toHaveBeenCalledWith("/new");
    expect(
      controller.bubble.element.querySelector(".bubble-send")?.textContent,
    ).toBe("Send");
    expect(lastPose()).toBe("listen");
  });
});

describe("minimizing while Claude works", () => {
  beforeEach(() => {
    controller.openInput();
    typeAndSend("hello");
    controller.onRunEvent({
      type: "activity",
      pose: "write",
      icon: "✍️",
      label: "writing notes.md",
    });
  });

  it("closing only minimizes, keeping the pose, label and a dot", () => {
    controller.close();

    expect(controller.bubble.isOpen).toBe(false);
    expect(ipc.setMinimized).toHaveBeenLastCalledWith(true);
    expect(ipc.setBubble).toHaveBeenLastCalledWith(0, 0);
    expect(lastPose()).toBe("write");
    expect(controller.label.element.hidden).toBe(false);
    expect(controller.dot.element.hidden).toBe(false);
  });

  it("the header button minimizes too", () => {
    action(".header-button:nth-of-type(2)");

    expect(ipc.setMinimized).toHaveBeenLastCalledWith(true);
  });

  it("clicking the dot reopens the bubble", () => {
    controller.minimize();

    controller.dot.element.click();

    expect(controller.bubble.isOpen).toBe(true);
    expect(controller.dot.element.hidden).toBe(true);
    expect(ipc.setMinimized).toHaveBeenLastCalledWith(false);
  });

  it("clicking the pet reopens a minimized bubble", () => {
    controller.minimize();

    controller.onPetClick();

    expect(controller.bubble.isOpen).toBe(true);
  });

  it("the hotkey reopens it as well", () => {
    controller.minimize();

    controller.openInput();

    expect(controller.bubble.isOpen).toBe(true);
  });

  it("finishing while minimized keeps the dot and settles after a while", () => {
    controller.minimize();

    controller.onRunEvent({
      type: "finished",
      text: "All done",
      is_error: false,
      cancelled: false,
      model: "m",
    });

    expect(lastPose()).toBe("happy");
    expect(controller.dot.element.hidden).toBe(false);
    expect(scheduled).toHaveLength(1);
    expect(scheduled[0]?.ms).toBe(SETTLE_MS);

    scheduled[0]?.fn();

    expect(lastPose()).toBeNull();
    expect(ipc.setPaused).toHaveBeenLastCalledWith(false);
    expect(controller.dot.element.hidden).toBe(false);
  });

  it("reopening cancels the pending settle", () => {
    controller.minimize();
    controller.onRunEvent({
      type: "finished",
      text: "x",
      is_error: false,
      cancelled: false,
      model: null,
    });

    controller.restore();

    expect(scheduled[0]?.cancelled).toBe(true);
  });

  it("a notification bringing the app forward reopens the reply", () => {
    controller.minimize();

    controller.onReopen();

    expect(controller.bubble.isOpen).toBe(true);
  });

  it("bringing the app forward with nothing minimized does nothing", () => {
    controller.restore();
    controller.onReopen();

    expect(controller.bubble.isOpen).toBe(true);
  });
});

describe("approvals", () => {
  beforeEach(() => {
    controller.openInput();
    typeAndSend("hello");
  });

  it("asks, with the pose to match", () => {
    controller.onApproval(APPROVAL);

    expect(lastPose()).toBe("ask");
    expect(
      controller.bubble.element.querySelector(".bubble-approval")?.textContent,
    ).toContain("running ls");
  });

  it("Allow answers yes and goes back to thinking", () => {
    controller.onApproval(APPROVAL);

    action(".allow");

    expect(ipc.answerApproval).toHaveBeenCalledWith("req-1", true);
    expect(lastPose()).toBe("think");
    expect(controller.bubble.element.querySelector(".bubble-approval")).toBeNull();
  });

  it("Deny answers no", () => {
    controller.onApproval(APPROVAL);

    action(".deny");

    expect(ipc.answerApproval).toHaveBeenCalledWith("req-1", false);
  });

  it("a plan also makes the pet ask", () => {
    controller.onApproval({ ...APPROVAL, kind: "plan" });

    expect(lastPose()).toBe("ask");
  });

  it("a minimized pet's dot asks for attention", () => {
    controller.minimize();

    controller.onApproval(APPROVAL);

    expect(controller.dot.element.hidden).toBe(false);
    expect(controller.dot.element.textContent).toBe("!");
  });
});

describe("conversations", () => {
  it("the header button asks Rust for a new conversation", () => {
    controller.openInput();

    action(".header-button");

    expect(ipc.startNewConversation).toHaveBeenCalledOnce();
  });

  it("a new conversation clears the reply, closes the bubble and frees the pet", () => {
    controller.openInput();
    typeAndSend("hello");
    controller.onRunEvent({ type: "text", text: "Hi" });
    controller.minimize();

    controller.onNewConversation();

    expect(controller.bubble.isOpen).toBe(false);
    expect(controller.bubble.hasReply()).toBe(false);
    expect(lastPose()).toBeNull();
    expect(controller.dot.element.hidden).toBe(true);
    expect(ipc.setMinimized).toHaveBeenLastCalledWith(false);
  });

  it("a notice opens the bubble and shows the line", () => {
    controller.onNotice("Resumed: Old chat");

    expect(controller.bubble.isOpen).toBe(true);
    expect(
      controller.bubble.element.querySelector(".bubble-notice")?.textContent,
    ).toBe("Resumed: Old chat");
  });

  it("the picker lists recent conversations and resumes the one picked", async () => {
    ipc.listRecent.mockResolvedValueOnce([
      { id: "a", title: "First", when: "1 h ago" },
    ]);

    controller.onPicker();
    await flush();
    action(".picker-row");

    expect(controller.bubble.isOpen).toBe(true);
    expect(ipc.resume).toHaveBeenCalledWith("a");
  });

  it("shows why a resume failed", async () => {
    ipc.resume.mockRejectedValueOnce("I can't find that conversation any more.");
    ipc.listRecent.mockResolvedValueOnce([{ id: "a", title: "First", when: "now" }]);
    controller.onPicker();
    await flush();

    action(".picker-row");
    await flush();

    expect(
      controller.bubble.element.querySelector(".bubble-notice")?.textContent,
    ).toBe("I can't find that conversation any more.");
  });

  it("reports a picker that could not load", async () => {
    ipc.listRecent.mockRejectedValueOnce("nope");

    controller.onPicker();
    await flush();

    expect(errors).toEqual(["nope"]);
  });

  it("a forgotten conversation offers to resend as a new one", () => {
    controller.openInput();
    typeAndSend("where were we?");
    ipc.sendMessage.mockClear();

    controller.onRunEvent({ type: "lost" });
    action(".send-new");

    expect(ipc.sendMessage).toHaveBeenCalledWith("where were we?");
  });

  it("a forgotten conversation reopens a minimized bubble to say so", () => {
    controller.openInput();
    typeAndSend("hello");
    controller.minimize();

    controller.onRunEvent({ type: "lost" });

    expect(controller.bubble.isOpen).toBe(true);
    expect(controller.bubble.element.textContent).toContain("forgotten");
  });

  it("sending as new does nothing when there was no message", () => {
    controller.onRunEvent({ type: "lost" });

    action(".send-new");

    expect(ipc.sendMessage).not.toHaveBeenCalled();
  });
});

describe("errors from Rust", () => {
  it("are reported, not thrown", async () => {
    ipc.takeFocus.mockRejectedValueOnce(new Error("no focus"));

    controller.openInput();
    await flush();

    expect(errors).toEqual(["no focus"]);
  });
});
