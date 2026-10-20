import { describe, expect, test, vi } from "vitest";
import { parseCharacter, type CharacterDef } from "./character";
import { Sprite, type ImageLike, type SpriteContext } from "./sprite";

const SIZE = 128;

const catSheet = parseCharacter({
  format: "sheet",
  name: "Cat",
  license: "CC0",
  frameWidth: 32,
  frameHeight: 32,
  scale: 3,
  poses: {
    stand: { row: 0, frames: 4, fps: 10, loop: true },
    walk: { row: 1, frames: 6, fps: 10, loop: true },
  },
});

function fakeContext() {
  return {
    clearRect: vi.fn(),
    save: vi.fn(),
    restore: vi.fn(),
    translate: vi.fn(),
    scale: vi.fn(),
    setTransform: vi.fn(),
    drawImage: vi.fn(),
    beginPath: vi.fn(),
    ellipse: vi.fn(),
    arc: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    closePath: vi.fn(),
    fill: vi.fn(),
    fillStyle: "",
    imageSmoothingEnabled: true,
  } satisfies SpriteContext;
}

function fakeCanvas(ctx: SpriteContext | null) {
  return { width: 0, height: 0, getContext: () => ctx };
}

const sheetImage: ImageLike = { width: 192, height: 96 };
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

interface SetupOptions {
  character?: CharacterDef | null;
  loadImage?: (url: string) => Promise<ImageLike>;
  dpr?: number;
}

function setup(overrides: SetupOptions = {}) {
  const ctx = fakeContext();
  const canvas = fakeCanvas(ctx);
  const loadImage =
    overrides.loadImage ?? vi.fn(() => Promise.resolve(sheetImage));
  const sprite = new Sprite({
    canvas,
    size: SIZE,
    devicePixelRatio: overrides.dpr ?? 2,
    character: overrides.character === undefined ? catSheet : overrides.character,
    basePath: "characters/cat",
    loadImage,
  });
  return { ctx, canvas, loadImage, sprite };
}

/** Renders once so the image starts loading, then waits for it to be ready. */
async function renderUntilLoaded(sprite: Sprite, ctx: ReturnType<typeof fakeContext>) {
  sprite.render(0);
  await flush();
  ctx.drawImage.mockClear();
}

describe("Sprite", () => {
  test("sizes the canvas for the device pixel ratio and turns smoothing off", () => {
    const { ctx, canvas } = setup({ dpr: 2 });

    expect(canvas.width).toBe(256);
    expect(canvas.height).toBe(256);
    expect(ctx.setTransform).toHaveBeenCalledWith(2, 0, 0, 2, 0, 0);
    expect(ctx.imageSmoothingEnabled).toBe(false);
  });

  test("throws when the canvas has no 2d context", () => {
    expect(
      () =>
        new Sprite({
          canvas: fakeCanvas(null),
          size: SIZE,
          devicePixelRatio: 1,
          character: null,
          basePath: "x",
          loadImage: () => Promise.reject(new Error("unused")),
        }),
    ).toThrow("2d canvas context unavailable");
  });

  test("draws the placeholder while the art is still loading", () => {
    const { ctx, sprite } = setup({ loadImage: () => new Promise(() => {}) });

    sprite.render(0);

    expect(ctx.clearRect).toHaveBeenCalledWith(0, 0, SIZE, SIZE);
    expect(ctx.drawImage).not.toHaveBeenCalled();
    expect(ctx.ellipse).toHaveBeenCalled();
  });

  test("draws the cropped frame, bottom-centred, once loaded", async () => {
    const { ctx, sprite } = setup();
    await renderUntilLoaded(sprite, ctx);

    sprite.render(0);

    // 32px frame scaled x3 = 96pt, centred in 128pt, feet on the bottom edge.
    expect(ctx.drawImage).toHaveBeenCalledWith(
      sheetImage,
      0, 0, 32, 32,
      16, 32, 96, 96,
    );
  });

  test("advances through the frames as time passes", async () => {
    const { ctx, sprite } = setup();
    await renderUntilLoaded(sprite, ctx);

    sprite.render(250); // 10 fps -> frame 2

    expect(ctx.drawImage.mock.calls[0].slice(1, 5)).toEqual([64, 0, 32, 32]);
  });

  test("restarts the animation when the pose changes", async () => {
    const { ctx, sprite } = setup();
    await renderUntilLoaded(sprite, ctx);

    sprite.setState("walk", "left", 1000);
    sprite.render(1000);

    // Frame 0 of the walk row, not 1s of progress into it.
    expect(ctx.drawImage.mock.calls[0].slice(1, 5)).toEqual([0, 32, 32, 32]);
  });

  test("keeps the animation clock when only the facing changes", async () => {
    const { ctx, sprite } = setup();
    await renderUntilLoaded(sprite, ctx);

    sprite.setState("stand", "right", 250);
    sprite.render(250);

    expect(ctx.drawImage.mock.calls[0].slice(1, 5)).toEqual([64, 0, 32, 32]);
  });

  test("mirrors the sprite when facing right", () => {
    const { ctx, sprite } = setup();
    sprite.setState("stand", "right", 0);

    sprite.render(0);

    expect(ctx.translate).toHaveBeenCalledWith(SIZE, 0);
    expect(ctx.scale).toHaveBeenCalledWith(-1, 1);
    expect(ctx.save).toHaveBeenCalledTimes(1);
    expect(ctx.restore).toHaveBeenCalledTimes(1);
  });

  test("does not mirror when facing left", () => {
    const { ctx, sprite } = setup();

    sprite.render(0);

    expect(ctx.translate).not.toHaveBeenCalled();
    expect(ctx.scale).not.toHaveBeenCalled();
  });

  test("falls back to the placeholder and does not retry when the image fails", async () => {
    const loadImage = vi.fn(() => Promise.reject(new Error("404")));
    const { ctx, sprite } = setup({ loadImage });
    sprite.render(0);
    await flush();
    ctx.ellipse.mockClear();

    sprite.render(100);
    sprite.render(200);

    expect(ctx.drawImage).not.toHaveBeenCalled();
    expect(ctx.ellipse).toHaveBeenCalled();
    expect(loadImage).toHaveBeenCalledTimes(1);
  });

  test("loads each image only once", async () => {
    const { loadImage, sprite } = setup();

    sprite.render(0);
    sprite.render(16);
    await flush();
    sprite.render(32);

    expect(loadImage).toHaveBeenCalledTimes(1);
    expect(loadImage).toHaveBeenCalledWith("characters/cat/sheet.png");
  });

  test("uses the placeholder when there is no character", () => {
    const { ctx, loadImage, sprite } = setup({ character: null });

    sprite.render(0);

    expect(ctx.ellipse).toHaveBeenCalled();
    expect(loadImage).not.toHaveBeenCalled();
  });

  test("uses the placeholder when the character cannot show the pose", () => {
    const noStand = parseCharacter({
      format: "sheet",
      name: "Odd",
      license: "x",
      frameWidth: 8,
      frameHeight: 8,
      poses: { walk: { row: 0, frames: 2, fps: 4, loop: true } },
    });
    const { ctx, sprite } = setup({ character: noStand });

    sprite.render(0); // initial pose is "stand", which Odd lacks

    expect(ctx.drawImage).not.toHaveBeenCalled();
    expect(ctx.ellipse).toHaveBeenCalled();
  });

  test("draws whole Shimeji images at their natural size", async () => {
    const shimeji = parseCharacter({ format: "shimeji", name: "S", license: "x" });
    const image: ImageLike = { width: 128, height: 128 };
    const { ctx, sprite } = setup({
      character: shimeji,
      loadImage: () => Promise.resolve(image),
    });
    await renderUntilLoaded(sprite, ctx);

    sprite.render(0);

    expect(ctx.drawImage).toHaveBeenCalledWith(image, 0, 0, 128, 128);
  });
});
