import { describe, expect, it } from "vitest";
import type { LayoutEvent } from "./ipc";
import { applyLayout, bubbleSpace, type StageElements } from "./stage";

const PET = 128;

function elements(): StageElements {
  const make = (): HTMLElement => document.createElement("div");
  return {
    stage: make(),
    pet: make(),
    bubble: make(),
    label: make(),
    dot: make(),
  };
}

const ABOVE: LayoutEvent = {
  side: "above",
  petX: 106,
  petY: 260,
  width: 340,
  height: 388,
};
const BELOW: LayoutEvent = {
  side: "below",
  petX: 106,
  petY: 0,
  width: 340,
  height: 388,
};
const PET_ONLY: LayoutEvent = {
  side: "above",
  petX: 0,
  petY: 0,
  width: 128,
  height: 128,
};

describe("bubbleSpace", () => {
  it("is the room above the pet when the bubble is above", () => {
    expect(bubbleSpace(ABOVE, PET)).toBe(260);
  });

  it("is the room below the pet when the bubble is below", () => {
    expect(bubbleSpace(BELOW, PET)).toBe(260);
  });

  it("is zero when the window is just the pet", () => {
    expect(bubbleSpace(PET_ONLY, PET)).toBe(0);
  });

  it("is never negative", () => {
    expect(bubbleSpace({ ...ABOVE, petY: -5 }, PET)).toBe(0);
  });
});

describe("applyLayout", () => {
  it("sizes the stage to the window", () => {
    const els = elements();

    applyLayout(els, ABOVE, PET);

    expect(els.stage.style.width).toBe("340px");
    expect(els.stage.style.height).toBe("388px");
  });

  it("puts the pet where Rust says", () => {
    const els = elements();

    applyLayout(els, ABOVE, PET);

    expect(els.pet.style.left).toBe("106px");
    expect(els.pet.style.top).toBe("260px");
  });

  it("puts the bubble above the pet", () => {
    const els = elements();

    applyLayout(els, ABOVE, PET);

    expect(els.bubble.style.top).toBe("0px");
    expect(els.bubble.style.height).toBe("260px");
    expect(els.bubble.dataset["side"]).toBe("above");
  });

  it("puts the bubble below the pet", () => {
    const els = elements();

    applyLayout(els, BELOW, PET);

    expect(els.bubble.style.top).toBe("128px");
    expect(els.bubble.style.height).toBe("260px");
    expect(els.bubble.dataset["side"]).toBe("below");
  });

  it("lays the label across the top of the pet and the dot in its corner", () => {
    const els = elements();

    applyLayout(els, ABOVE, PET);

    expect(els.label.style.left).toBe("106px");
    expect(els.label.style.width).toBe("128px");
    expect(els.label.style.top).toBe("262px");
    expect(els.dot.style.left).toBe("206px");
    expect(els.dot.style.top).toBe("264px");
  });
});
