/// <reference types="node" />
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, test } from "vitest";
import { parseCharacter, resolvePose } from "./character";

// The generated Shiba must be a valid character whose sheet really contains
// every cell its character.json points at. Regenerate with
// `npm run art:shiba` if this fails after editing scripts/shiba/.

const SHIBA_DIR = resolve(__dirname, "..", "public", "characters", "shiba");
const PNG_SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

function readJson(): unknown {
  return JSON.parse(readFileSync(resolve(SHIBA_DIR, "character.json"), "utf8"));
}

function readPngSize(): { width: number; height: number } {
  const bytes = readFileSync(resolve(SHIBA_DIR, "sheet.png"));
  expect([...bytes.subarray(0, 8)]).toEqual(PNG_SIGNATURE);
  return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) };
}

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

describe("generated Shiba character", () => {
  test("is a valid sheet character with a license", () => {
    const character = parseCharacter(readJson());

    expect(character.format).toBe("sheet");
    expect(character.license.length).toBeGreaterThan(0);
  });

  test.each(PHYSICS_POSES)("draws the %s pose itself", (pose) => {
    const character = parseCharacter(readJson());

    expect(resolvePose(character, pose)).toBe(pose);
  });

  test("the sheet is large enough for every pose's row and frames", () => {
    const character = parseCharacter(readJson());
    if (character.format !== "sheet") {
      throw new Error("expected a sheet character");
    }
    const { width, height } = readPngSize();

    for (const pose of Object.values(character.poses)) {
      expect((pose.row + 1) * character.frameHeight).toBeLessThanOrEqual(height);
      expect(pose.frames * character.frameWidth).toBeLessThanOrEqual(width);
    }
  });

  test("activity and mood poses resolve to something drawn", () => {
    const character = parseCharacter(readJson());

    for (const pose of ["listen", "ask", "catch", "read", "write", "browse", "happy", "sad", "wave", "talk", "think"]) {
      expect(resolvePose(character, pose)).not.toBeNull();
    }
  });
});
