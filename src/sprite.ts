// Draws the character on a canvas (design §9.2): picks the frame for the
// current pose, mirrors it when facing right, keeps pixel art crisp on Retina,
// and falls back to the built-in placeholder while art is loading or broken.

import { planFrame, type CharacterDef, type Facing } from "./character";
import { drawPlaceholder, type PlaceholderContext } from "./placeholder";

export interface ImageLike {
  readonly width: number;
  readonly height: number;
}

export type SpriteContext = PlaceholderContext &
  Pick<
    CanvasRenderingContext2D,
    | "clearRect"
    | "save"
    | "restore"
    | "translate"
    | "scale"
    | "setTransform"
    | "drawImage"
    | "imageSmoothingEnabled"
  >;

export interface SpriteCanvas {
  width: number;
  height: number;
  getContext(type: "2d"): SpriteContext | null;
}

export interface SpriteOptions {
  canvas: SpriteCanvas;
  /** Side of the square the pet is drawn in, in CSS pixels / points. */
  size: number;
  devicePixelRatio: number;
  /** null = always use the placeholder. */
  character: CharacterDef | null;
  /** Folder holding the character's files, e.g. "characters/shiba". */
  basePath: string;
  loadImage: (url: string) => Promise<ImageLike>;
}

type ImageState =
  | { readonly status: "loading" }
  | { readonly status: "failed" }
  | { readonly status: "ready"; readonly image: ImageLike };

const INITIAL_POSE = "stand";

export class Sprite {
  private readonly ctx: SpriteContext;
  private readonly images = new Map<string, ImageState>();
  private pose = INITIAL_POSE;
  private facing: Facing = "left";
  private poseStartedAt = 0;

  constructor(private readonly options: SpriteOptions) {
    const { canvas, size, devicePixelRatio } = options;
    const ctx = canvas.getContext("2d");
    if (ctx === null) {
      throw new Error("2d canvas context unavailable");
    }
    this.ctx = ctx;
    canvas.width = Math.round(size * devicePixelRatio);
    canvas.height = Math.round(size * devicePixelRatio);
    // Resizing a canvas resets its state, so configure it after sizing.
    ctx.setTransform(devicePixelRatio, 0, 0, devicePixelRatio, 0, 0);
    ctx.imageSmoothingEnabled = false;
  }

  /** Tells the sprite what to show; restarts the animation on a new pose. */
  setState(pose: string, facing: Facing, nowMs: number): void {
    if (pose !== this.pose) {
      this.pose = pose;
      this.poseStartedAt = nowMs;
    }
    this.facing = facing;
  }

  render(nowMs: number): void {
    const { ctx } = this;
    const { size } = this.options;
    ctx.clearRect(0, 0, size, size);
    ctx.save();
    if (this.facing === "right") {
      ctx.translate(size, 0);
      ctx.scale(-1, 1);
    }
    if (!this.drawCharacterFrame(nowMs)) {
      drawPlaceholder(ctx, size, this.pose, nowMs);
    }
    ctx.restore();
  }

  /** Returns false when the placeholder should be drawn instead. */
  private drawCharacterFrame(nowMs: number): boolean {
    const { character, basePath, size } = this.options;
    if (character === null) {
      return false;
    }
    const plan = planFrame(
      character,
      basePath,
      this.pose,
      nowMs - this.poseStartedAt,
    );
    if (plan === null) {
      return false;
    }
    const state = this.imageFor(plan.src);
    if (state.status !== "ready") {
      return false;
    }

    const { crop } = plan;
    const image = state.image as CanvasImageSource & ImageLike;
    const sw = crop?.sw ?? image.width;
    const sh = crop?.sh ?? image.height;
    const dw = Math.round(sw * plan.scale);
    const dh = Math.round(sh * plan.scale);
    // Feet on the bottom edge, centred: matches the Shimeji anchor and keeps
    // the sprite standing on whatever surface the window is sitting on.
    const dx = Math.round((size - dw) / 2);
    const dy = size - dh;
    if (crop === null) {
      this.ctx.drawImage(image, dx, dy, dw, dh);
    } else {
      this.ctx.drawImage(
        image,
        crop.sx,
        crop.sy,
        crop.sw,
        crop.sh,
        dx,
        dy,
        dw,
        dh,
      );
    }
    return true;
  }

  private imageFor(src: string): ImageState {
    const known = this.images.get(src);
    if (known !== undefined) {
      return known;
    }
    const loading: ImageState = { status: "loading" };
    this.images.set(src, loading);
    this.options.loadImage(src).then(
      (image) => this.images.set(src, { status: "ready", image }),
      () => this.images.set(src, { status: "failed" }),
    );
    return loading;
  }
}
