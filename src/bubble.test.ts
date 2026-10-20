import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  Bubble,
  COMPACT_SIZE,
  TALL_SIZE,
  type BubbleHandlers,
} from "./bubble";
import type { ApprovalRequest } from "./ipc";

function makeHandlers(): { [K in keyof BubbleHandlers]: ReturnType<typeof vi.fn> } {
  return {
    onSend: vi.fn(),
    onStop: vi.fn(),
    onMinimize: vi.fn(),
    onClose: vi.fn(),
    onNew: vi.fn(),
    onAnswer: vi.fn(),
    onPick: vi.fn(),
    onSendAsNew: vi.fn(),
    onSizeChange: vi.fn(),
  };
}

let handlers: ReturnType<typeof makeHandlers>;
let bubble: Bubble;

beforeEach(() => {
  handlers = makeHandlers();
  bubble = new Bubble(document, handlers as unknown as BubbleHandlers);
  document.body.replaceChildren(bubble.element);
});

function input(): HTMLTextAreaElement {
  return bubble.element.querySelector("textarea") as HTMLTextAreaElement;
}

function key(
  target: HTMLElement,
  key: string,
  init: KeyboardEventInit = {},
): KeyboardEvent {
  const event = new KeyboardEvent("keydown", {
    key,
    bubbles: true,
    cancelable: true,
    ...init,
  });
  target.dispatchEvent(event);
  return event;
}

function click(selector: string): void {
  (bubble.element.querySelector(selector) as HTMLElement).click();
}

const APPROVAL: ApprovalRequest = {
  id: "req-1",
  tool_name: "Bash",
  kind: "action",
  headline: "💻 running npm install",
  detail: "npm install --save left-pad",
};

describe("opening and closing", () => {
  it("starts hidden and compact", () => {
    expect(bubble.element.hidden).toBe(true);
    expect(bubble.isOpen).toBe(false);
    expect(bubble.size()).toEqual(COMPACT_SIZE);
  });

  it("shows itself and reports its size", () => {
    bubble.show();

    expect(bubble.element.hidden).toBe(false);
    expect(bubble.isOpen).toBe(true);
    expect(handlers.onSizeChange).toHaveBeenCalledWith(COMPACT_SIZE);
  });

  it("hides again", () => {
    bubble.show();

    bubble.hide();

    expect(bubble.element.hidden).toBe(true);
    expect(bubble.isOpen).toBe(false);
  });

  it("focuses the text box", () => {
    bubble.show();

    bubble.focusInput();

    expect(document.activeElement).toBe(input());
  });
});

describe("the text box", () => {
  it("Return sends the trimmed text and clears the box", () => {
    input().value = "  hello  ";

    const event = key(input(), "Enter");

    expect(handlers.onSend).toHaveBeenCalledWith("hello");
    expect(input().value).toBe("");
    expect(event.defaultPrevented).toBe(true);
  });

  it("Shift+Return adds a new line instead of sending", () => {
    input().value = "hello";

    const event = key(input(), "Enter", { shiftKey: true });

    expect(handlers.onSend).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });

  it("does not send while an input method is composing", () => {
    input().value = "にほん";

    key(input(), "Enter", { isComposing: true });

    expect(handlers.onSend).not.toHaveBeenCalled();
  });

  it("does not send an empty message", () => {
    input().value = "   ";

    key(input(), "Enter");
    click(".bubble-send");

    expect(handlers.onSend).not.toHaveBeenCalled();
  });

  it("the Send button sends", () => {
    input().value = "hi";

    click(".bubble-send");

    expect(handlers.onSend).toHaveBeenCalledWith("hi");
  });

  it("Escape closes", () => {
    key(input(), "Escape");

    expect(handlers.onClose).toHaveBeenCalledOnce();
  });

  it("other keys do nothing", () => {
    input().value = "x";

    key(input(), "a");

    expect(handlers.onSend).not.toHaveBeenCalled();
    expect(handlers.onClose).not.toHaveBeenCalled();
  });

  it("is locked while busy and the button becomes Stop", () => {
    bubble.setBusy(true);
    input().disabled = false;
    input().value = "typed anyway";

    key(input(), "Enter");
    click(".bubble-send");

    expect(handlers.onSend).not.toHaveBeenCalled();
    expect(handlers.onStop).toHaveBeenCalledOnce();
    expect(bubble.element.querySelector(".bubble-send")?.textContent).toBe(
      "Stop",
    );
  });

  it("unlocks again", () => {
    bubble.setBusy(true);

    bubble.setBusy(false);

    expect(input().disabled).toBe(false);
    expect(bubble.element.querySelector(".bubble-send")?.textContent).toBe(
      "Send",
    );
  });
});

describe("header buttons", () => {
  it("New, Minimize and Close call back", () => {
    const buttons = bubble.element.querySelectorAll<HTMLElement>(".header-button");

    buttons[0]?.click();
    buttons[1]?.click();
    buttons[2]?.click();

    expect(handlers.onNew).toHaveBeenCalledOnce();
    expect(handlers.onMinimize).toHaveBeenCalledOnce();
    expect(handlers.onClose).toHaveBeenCalledOnce();
  });
});

describe("the reply", () => {
  it("starting a reply shows thinking and grows the bubble", () => {
    bubble.show();
    handlers.onSizeChange.mockClear();

    bubble.startReply();

    expect(bubble.element.querySelector(".bubble-status")?.textContent).toBe(
      "thinking…",
    );
    expect(bubble.size()).toEqual(TALL_SIZE);
    expect(handlers.onSizeChange).toHaveBeenCalledWith(TALL_SIZE);
  });

  it("streams text as light markdown", () => {
    bubble.startReply();

    bubble.appendReply("Hello **there**");
    bubble.appendReply(", `code`");

    const reply = bubble.element.querySelector(".bubble-reply");
    expect(reply?.innerHTML).toBe(
      "<p>Hello <strong>there</strong>, <code>code</code></p>",
    );
    expect(bubble.element.querySelector(".bubble-status")).toHaveProperty(
      "hidden",
      true,
    );
    expect(bubble.hasReply()).toBe(true);
  });

  it("never lets reply HTML through", () => {
    bubble.startReply();

    bubble.appendReply("<img src=x onerror=alert(1)>");

    expect(bubble.element.querySelector("img")).toBeNull();
  });

  it("finishing keeps the streamed text and shows the model", () => {
    bubble.startReply();
    bubble.appendReply("Streamed");

    bubble.finishReply("Streamed", false, "claude-opus");

    expect(bubble.element.querySelector(".bubble-reply")?.textContent).toBe(
      "Streamed",
    );
    expect(bubble.element.querySelector(".bubble-footer")?.textContent).toBe(
      "claude-opus",
    );
  });

  it("finishing with no streamed text shows the final text", () => {
    bubble.startReply();

    bubble.finishReply("All at once", false, null);

    expect(bubble.element.querySelector(".bubble-reply")?.textContent).toBe(
      "All at once",
    );
    expect(bubble.element.querySelector(".bubble-footer")).toHaveProperty(
      "hidden",
      true,
    );
  });

  it("an error is shown as text, after what was already streamed", () => {
    bubble.startReply();
    bubble.appendReply("Part");

    bubble.finishReply("<b>Boom</b>", true, null);

    const error = bubble.element.querySelector(".bubble-error");
    expect(error?.textContent).toBe("<b>Boom</b>");
    expect(error?.querySelector("b")).toBeNull();
    expect(bubble.element.querySelector(".bubble-reply")?.textContent).toContain(
      "Part",
    );
  });

  it("starting a new reply clears the old one", () => {
    bubble.startReply();
    bubble.appendReply("Old");
    bubble.finishReply("Old", false, "m");

    bubble.startReply();

    expect(bubble.element.querySelector(".bubble-reply")?.innerHTML).toBe("");
    expect(bubble.element.querySelector(".bubble-footer")).toHaveProperty(
      "hidden",
      true,
    );
  });

  it("can show a status such as retrying", () => {
    bubble.setStatus("retrying…");

    expect(bubble.element.querySelector(".bubble-status")?.textContent).toBe(
      "retrying…",
    );
    bubble.setStatus(null);
    expect(bubble.element.querySelector(".bubble-status")).toHaveProperty(
      "hidden",
      true,
    );
  });

  it("has no reply before anything is said", () => {
    expect(bubble.hasReply()).toBe(false);
  });
});

describe("approvals", () => {
  it("shows what Claude wants to do with Allow and Deny", () => {
    bubble.showApproval(APPROVAL);

    const box = bubble.element.querySelector(".bubble-approval");
    expect(box?.textContent).toContain("running npm install");
    expect(box?.querySelector("pre")?.textContent).toBe(
      "npm install --save left-pad",
    );
    expect(bubble.size()).toEqual(TALL_SIZE);
  });

  it("Allow and Deny answer with the request id", () => {
    bubble.showApproval(APPROVAL);

    click(".allow");
    click(".deny");

    expect(handlers.onAnswer).toHaveBeenNthCalledWith(1, "req-1", true);
    expect(handlers.onAnswer).toHaveBeenNthCalledWith(2, "req-1", false);
  });

  it("works without detail text", () => {
    bubble.showApproval({ ...APPROVAL, detail: null });

    expect(bubble.element.querySelector(".approval-detail")).toBeNull();
  });

  it("never renders the command as HTML", () => {
    bubble.showApproval({ ...APPROVAL, detail: "<img src=x onerror=a()>" });

    expect(bubble.element.querySelector("img")).toBeNull();
  });

  it("clears once answered", () => {
    bubble.showApproval(APPROVAL);

    bubble.clearApproval();

    expect(bubble.element.querySelector(".bubble-approval")).toBeNull();
  });

  it("clearing an approval leaves other extras alone", () => {
    bubble.showLost();

    bubble.clearApproval();

    expect(bubble.element.querySelector(".bubble-lost")).not.toBeNull();
  });
});

describe("the recent picker", () => {
  it("lists conversations and picks one", () => {
    bubble.showPicker([
      { id: "a", title: "First chat", when: "2 h ago" },
      { id: "b", title: "Second <b>chat</b>", when: "just now" },
    ]);

    const rows = bubble.element.querySelectorAll<HTMLElement>(".picker-row");
    rows[1]?.click();

    expect(rows).toHaveLength(2);
    expect(rows[0]?.textContent).toContain("First chat");
    expect(rows[1]?.querySelector("b")).toBeNull();
    expect(handlers.onPick).toHaveBeenCalledWith("b");
  });

  it("says so when there is nothing to resume", () => {
    bubble.showPicker([]);

    expect(bubble.element.querySelector(".picker-empty")).not.toBeNull();
  });
});

describe("a forgotten conversation", () => {
  it("offers to send the message as a new conversation", () => {
    bubble.showLost();

    click(".send-new");

    expect(bubble.element.textContent).toContain("forgotten");
    expect(handlers.onSendAsNew).toHaveBeenCalledOnce();
  });
});

describe("notices and reset", () => {
  it("shows a short notice without growing the bubble", () => {
    bubble.showNotice("Resumed: Old chat");

    expect(bubble.element.querySelector(".bubble-notice")?.textContent).toBe(
      "Resumed: Old chat",
    );
    expect(bubble.size()).toEqual(COMPACT_SIZE);
  });

  it("a new conversation forgets everything", () => {
    bubble.show();
    bubble.startReply();
    bubble.appendReply("Something");
    bubble.showNotice("note");
    bubble.showApproval(APPROVAL);

    bubble.reset();

    expect(bubble.hasReply()).toBe(false);
    expect(bubble.element.querySelector(".bubble-approval")).toBeNull();
    expect(bubble.element.querySelector(".bubble-notice")).toHaveProperty(
      "hidden",
      true,
    );
    expect(bubble.size()).toEqual(COMPACT_SIZE);
  });

  it("only reports size changes to Rust while open", () => {
    bubble.startReply();

    expect(handlers.onSizeChange).not.toHaveBeenCalled();
  });
});
