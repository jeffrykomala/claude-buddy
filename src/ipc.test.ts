import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
const listen = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen }));

import { errorMessage, ipc, subscribe } from "./ipc";

beforeEach(() => {
  invoke.mockReset();
  invoke.mockResolvedValue(undefined);
  listen.mockReset();
});

describe("ipc commands", () => {
  it("sends a message", async () => {
    await ipc.sendMessage("hello");

    expect(invoke).toHaveBeenCalledWith("send_message", { text: "hello" });
  });

  it("answers an approval", async () => {
    await ipc.answerApproval("abc", false);

    expect(invoke).toHaveBeenCalledWith("answer_approval", {
      id: "abc",
      allow: false,
    });
  });

  it("passes the bubble size and the pause flag", async () => {
    await ipc.setBubble(340, 150);
    await ipc.setPaused(true);

    expect(invoke).toHaveBeenCalledWith("set_bubble", {
      width: 340,
      height: 150,
    });
    expect(invoke).toHaveBeenCalledWith("set_paused", { paused: true });
  });

  it("starts a grab with the press point", async () => {
    await ipc.grab(10, 20);

    expect(invoke).toHaveBeenCalledWith("grab", { offsetX: 10, offsetY: 20 });
  });

  it("calls the argument-free commands by name", async () => {
    await ipc.cancelRun();
    await ipc.startNewConversation();
    await ipc.listRecent();
    await ipc.takeFocus();
    await ipc.releaseFocus();

    expect(invoke.mock.calls.map((call) => call[0])).toEqual([
      "cancel_run",
      "start_new_conversation",
      "list_recent",
      "take_focus",
      "release_focus",
    ]);
  });

  it("resumes a conversation and tells Rust about minimizing", async () => {
    invoke.mockResolvedValueOnce("Old chat");

    const title = await ipc.resume("id-1");
    await ipc.setMinimized(true);

    expect(title).toBe("Old chat");
    expect(invoke).toHaveBeenCalledWith("resume", { id: "id-1" });
    expect(invoke).toHaveBeenCalledWith("set_minimized", { minimized: true });
  });
});

describe("subscribe", () => {
  it("hands the handler just the payload", async () => {
    const unlisten = vi.fn();
    listen.mockImplementation(
      (_name: string, callback: (event: { payload: unknown }) => void) => {
        callback({ payload: { n: 1 } });
        return Promise.resolve(unlisten);
      },
    );
    const handler = vi.fn();

    const stop = await subscribe("buddy://event", handler);

    expect(listen).toHaveBeenCalledWith("buddy://event", expect.any(Function));
    expect(handler).toHaveBeenCalledWith({ n: 1 });
    expect(stop).toBe(unlisten);
  });
});

describe("errorMessage", () => {
  it("reads strings, errors and anything else", () => {
    expect(errorMessage("plain")).toBe("plain");
    expect(errorMessage(new Error("boom"))).toBe("boom");
    expect(errorMessage({ weird: true })).toBe("Something went wrong.");
  });
});
