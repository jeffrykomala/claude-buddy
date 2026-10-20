import { describe, expect, test, vi } from "vitest";
import { drawPlaceholder, type PlaceholderContext } from "./placeholder";

function fakeContext() {
  return {
    beginPath: vi.fn(),
    ellipse: vi.fn(),
    arc: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    closePath: vi.fn(),
    fill: vi.fn(),
    fillStyle: "",
  } satisfies PlaceholderContext;
}

describe("drawPlaceholder", () => {
  test("draws body, ears and eyes inside the box", () => {
    const ctx = fakeContext();

    drawPlaceholder(ctx, 128, "stand", 0);

    expect(ctx.ellipse).toHaveBeenCalled();
    expect(ctx.arc).toHaveBeenCalledTimes(2); // two eyes
    expect(ctx.lineTo).toHaveBeenCalledTimes(4); // two triangular ears
    expect(ctx.fill.mock.calls.length).toBeGreaterThanOrEqual(5);
  });

  test("holds still when standing", () => {
    const first = fakeContext();
    const later = fakeContext();

    drawPlaceholder(first, 128, "stand", 0);
    drawPlaceholder(later, 128, "stand", 40);

    expect(later.ellipse.mock.calls).toEqual(first.ellipse.mock.calls);
  });

  test("bobs while moving", () => {
    const first = fakeContext();
    const later = fakeContext();

    drawPlaceholder(first, 128, "walk", 0);
    drawPlaceholder(later, 128, "walk", 40);

    expect(later.ellipse.mock.calls[0][1]).not.toBe(
      first.ellipse.mock.calls[0][1],
    );
  });
});
