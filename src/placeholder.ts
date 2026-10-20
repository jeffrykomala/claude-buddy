// Built-in stand-in drawn when a character is missing, broken, or its art has
// not loaded yet (design §13: "Character missing or broken -> built-in simple
// shape"). A little Shiba-coloured blob so the pet is never invisible.

export type PlaceholderContext = Pick<
  CanvasRenderingContext2D,
  | "beginPath"
  | "ellipse"
  | "arc"
  | "moveTo"
  | "lineTo"
  | "closePath"
  | "fill"
  | "fillStyle"
>;

const COAT = "#d98b3a";
const CREAM = "#f6e7cf";
const EYE = "#2b1d10";

const BOB_AMPLITUDE_PX = 2;
const BOB_PERIOD_MS = 160;
/** Poses where the blob bobs as if moving. */
const MOVING_POSES: ReadonlySet<string> = new Set([
  "walk",
  "run",
  "climb",
  "ceiling_crawl",
  "dragged",
  "thrown",
]);

function bobOffset(pose: string, nowMs: number): number {
  return MOVING_POSES.has(pose)
    ? Math.sin((nowMs / BOB_PERIOD_MS) * Math.PI) * BOB_AMPLITUDE_PX
    : 0;
}

function ear(ctx: PlaceholderContext, x: number, base: number, size: number) {
  ctx.beginPath();
  ctx.moveTo(x - size * 0.09, base);
  ctx.lineTo(x, base - size * 0.17);
  ctx.lineTo(x + size * 0.09, base);
  ctx.closePath();
  ctx.fill();
}

/** Draws into a `size` x `size` box, facing left (the sprite convention). */
export function drawPlaceholder(
  ctx: PlaceholderContext,
  size: number,
  pose: string,
  nowMs: number,
): void {
  const cx = size / 2;
  const cy = size * 0.62 + bobOffset(pose, nowMs);

  ctx.fillStyle = COAT;
  ear(ctx, cx - size * 0.16, cy - size * 0.2, size);
  ear(ctx, cx + size * 0.16, cy - size * 0.2, size);
  ctx.beginPath();
  ctx.ellipse(cx, cy, size * 0.3, size * 0.27, 0, 0, Math.PI * 2);
  ctx.fill();

  ctx.fillStyle = CREAM;
  ctx.beginPath();
  ctx.ellipse(cx, cy + size * 0.08, size * 0.18, size * 0.13, 0, 0, Math.PI * 2);
  ctx.fill();

  ctx.fillStyle = EYE;
  for (const dx of [-0.1, 0.1]) {
    ctx.beginPath();
    ctx.arc(cx + size * dx, cy - size * 0.04, size * 0.025, 0, Math.PI * 2);
    ctx.fill();
  }
}
