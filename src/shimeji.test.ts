import { describe, expect, test } from "vitest";
import { SHIMEJI_IMAGE_COUNT, SHIMEJI_POSES } from "./shimeji";

const PHYSICS_POSES = [
  "stand",
  "walk",
  "run",
  "sit",
  "sit_dangle",
  "sleep",
  "fall",
  "land",
  "splat",
  "climb",
  "wall_hang",
  "ceiling_crawl",
  "ceiling_hang",
  "jump",
  "dragged",
  "thrown",
];

describe("SHIMEJI_POSES", () => {
  test.each(PHYSICS_POSES)("defines the physics pose %s", (pose) => {
    expect(SHIMEJI_POSES[pose]?.frames.length).toBeGreaterThan(0);
  });

  test("only references images that exist in a standard Shimeji set", () => {
    const used = Object.values(SHIMEJI_POSES).flatMap((pose) => pose.frames);

    for (const image of used) {
      expect(image).toBeGreaterThanOrEqual(1);
      expect(image).toBeLessThanOrEqual(SHIMEJI_IMAGE_COUNT);
    }
  });

  test("every pose has a positive fps", () => {
    for (const pose of Object.values(SHIMEJI_POSES)) {
      expect(pose.fps).toBeGreaterThan(0);
    }
  });
});
