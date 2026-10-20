/**
 * Places the pet, the bubble, the activity label and the "…" dot inside the
 * window from the layout Rust sends (design §11): the window grows toward
 * the side with room, so the bubble sits above or below the pet.
 */

import type { LayoutEvent } from "./ipc";

/** The dot sits in the pet's top-right corner. */
const DOT_INSET_PT = 28;
const DOT_TOP_PT = 4;
const LABEL_TOP_PT = 2;

export interface StageElements {
  readonly stage: HTMLElement;
  readonly pet: HTMLElement;
  readonly bubble: HTMLElement;
  readonly label: HTMLElement;
  readonly dot: HTMLElement;
}

function px(value: number): string {
  return `${value}px`;
}

/** The free space for the bubble on the side it is on, never negative. */
export function bubbleSpace(layout: LayoutEvent, petSize: number): number {
  const space =
    layout.side === "above"
      ? layout.petY
      : layout.height - (layout.petY + petSize);
  return Math.max(0, space);
}

export function applyLayout(
  els: StageElements,
  layout: LayoutEvent,
  petSize: number,
): void {
  const { stage, pet, bubble, label, dot } = els;
  stage.style.width = px(layout.width);
  stage.style.height = px(layout.height);

  pet.style.left = px(layout.petX);
  pet.style.top = px(layout.petY);

  const space = bubbleSpace(layout, petSize);
  bubble.style.left = "0px";
  bubble.style.width = px(layout.width);
  bubble.style.height = px(space);
  bubble.style.top =
    layout.side === "above" ? "0px" : px(layout.petY + petSize);
  bubble.dataset["side"] = layout.side;

  label.style.left = px(layout.petX);
  label.style.width = px(petSize);
  label.style.top = px(layout.petY + LABEL_TOP_PT);

  dot.style.left = px(layout.petX + petSize - DOT_INSET_PT);
  dot.style.top = px(layout.petY + DOT_TOP_PT);
}
