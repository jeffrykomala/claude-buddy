import { describe, expect, it, vi } from "vitest";
import { ActivityLabel, ReplyDot } from "./activity";

describe("ActivityLabel", () => {
  it("starts hidden", () => {
    expect(new ActivityLabel(document).element.hidden).toBe(true);
  });

  it("shows the icon and label", () => {
    const label = new ActivityLabel(document);

    label.show("📖", "reading main.rs");

    expect(label.element.hidden).toBe(false);
    expect(label.element.textContent).toBe("📖 reading main.rs");
  });

  it("never renders its text as HTML", () => {
    const label = new ActivityLabel(document);

    label.show("💻", "running <img src=x onerror=a()>");

    expect(label.element.querySelector("img")).toBeNull();
  });

  it("hides and clears", () => {
    const label = new ActivityLabel(document);
    label.show("📖", "x");

    label.hide();

    expect(label.element.hidden).toBe(true);
    expect(label.element.textContent).toBe("");
  });
});

describe("ReplyDot", () => {
  it("starts hidden and shows an ellipsis", () => {
    const dot = new ReplyDot(document, () => {});

    expect(dot.element.hidden).toBe(true);
    dot.show(false);
    expect(dot.element.hidden).toBe(false);
    expect(dot.element.textContent).toBe("…");
  });

  it("looks different when an answer is needed", () => {
    const dot = new ReplyDot(document, () => {});

    dot.show(true);

    expect(dot.element.textContent).toBe("!");
    expect(dot.element.classList.contains("attention")).toBe(true);
    dot.show(false);
    expect(dot.element.classList.contains("attention")).toBe(false);
  });

  it("calls back when clicked, without the click reaching the pet", () => {
    const onClick = vi.fn();
    const dot = new ReplyDot(document, onClick);
    const outer = vi.fn();
    document.body.addEventListener("click", outer);
    document.body.append(dot.element);

    dot.element.click();

    expect(onClick).toHaveBeenCalledOnce();
    expect(outer).not.toHaveBeenCalled();
    document.body.removeEventListener("click", outer);
    dot.element.remove();
  });

  it("hides again", () => {
    const dot = new ReplyDot(document, () => {});
    dot.show(false);

    dot.hide();

    expect(dot.element.hidden).toBe(true);
  });
});
