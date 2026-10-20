import { describe, expect, test } from "vitest";
import {
  CharacterError,
  frameIndex,
  parseCharacter,
  planFrame,
  resolvePose,
  type CharacterDef,
} from "./character";

const sheetJson = {
  format: "sheet",
  name: "Pixel Cat",
  license: "CC0",
  frameWidth: 32,
  frameHeight: 32,
  scale: 3,
  poses: {
    stand: { row: 0, frames: 4, fps: 6, loop: true },
    walk: { row: 1, frames: 6, fps: 10, loop: true },
    land: { row: 2, frames: 3, fps: 10, loop: false },
  },
  aliases: { run: "walk", read: "sit" },
};

function sheet(overrides: Record<string, unknown> = {}): CharacterDef {
  return parseCharacter({ ...sheetJson, ...overrides });
}

describe("parseCharacter", () => {
  test("parses a sprite sheet definition", () => {
    const character = sheet();

    expect(character.format).toBe("sheet");
    expect(character.name).toBe("Pixel Cat");
    expect(character.license).toBe("CC0");
    expect(character.aliases).toEqual({ run: "walk", read: "sit" });
  });

  test("parses a Shimeji folder definition", () => {
    const character = parseCharacter({
      format: "shimeji",
      name: "My Shimeji",
      license: "from the readme",
    });

    expect(character.format).toBe("shimeji");
    expect(character.aliases).toEqual({});
  });

  test("defaults scale to 1 and aliases to empty", () => {
    const { scale: _scale, aliases: _aliases, ...rest } = sheetJson;
    const character = parseCharacter(rest);

    expect(character.scale).toBe(1);
    expect(character.aliases).toEqual({});
  });

  test.each([
    ["not an object", "nope"],
    ["null", null],
    ["unknown format", { ...sheetJson, format: "gif" }],
    ["missing name", { ...sheetJson, name: undefined }],
    ["missing license", { ...sheetJson, license: undefined }],
    ["zero frame width", { ...sheetJson, frameWidth: 0 }],
    ["negative scale", { ...sheetJson, scale: -1 }],
    ["no poses", { ...sheetJson, poses: {} }],
    [
      "pose with zero frames",
      { ...sheetJson, poses: { stand: { row: 0, frames: 0, fps: 6, loop: true } } },
    ],
    [
      "pose with a negative row",
      { ...sheetJson, poses: { stand: { row: -1, frames: 2, fps: 6, loop: true } } },
    ],
    [
      "pose with non-positive fps",
      { ...sheetJson, poses: { stand: { row: 0, frames: 2, fps: 0, loop: true } } },
    ],
    ["alias that is not a string", { ...sheetJson, aliases: { run: 3 } }],
  ])("rejects %s", (_label, raw) => {
    expect(() => parseCharacter(raw)).toThrow(CharacterError);
  });
});

describe("resolvePose", () => {
  test("returns the pose itself when the character has it", () => {
    expect(resolvePose(sheet(), "walk")).toBe("walk");
  });

  test("follows an alias to a pose the character has", () => {
    expect(resolvePose(sheet(), "run")).toBe("walk");
  });

  test("follows alias chains", () => {
    const character = sheet({ aliases: { run: "dash", dash: "walk" } });

    expect(resolvePose(character, "run")).toBe("walk");
  });

  test("survives alias cycles by falling back to stand", () => {
    const character = sheet({ aliases: { run: "dash", dash: "run" } });

    expect(resolvePose(character, "run")).toBe("stand");
  });

  test("falls back to think for a missing activity pose", () => {
    const character = sheet({
      poses: {
        ...sheetJson.poses,
        think: { row: 3, frames: 2, fps: 4, loop: true },
      },
      aliases: {},
    });

    expect(resolvePose(character, "browse")).toBe("think");
  });

  test("falls back to stand for a missing physics pose", () => {
    expect(resolvePose(sheet(), "ceiling_crawl")).toBe("stand");
  });

  test("falls back to stand for an activity when think is also missing", () => {
    expect(resolvePose(sheet(), "write")).toBe("stand");
  });

  test("returns null when the character has not even a stand pose", () => {
    const character = sheet({
      poses: { walk: sheetJson.poses.walk },
      aliases: {},
    });

    expect(resolvePose(character, "fall")).toBeNull();
  });

  test("a Shimeji character covers the physics poses out of the box", () => {
    const character = parseCharacter({
      format: "shimeji",
      name: "S",
      license: "x",
    });

    expect(resolvePose(character, "climb")).toBe("climb");
    expect(resolvePose(character, "ceiling_crawl")).toBe("ceiling_crawl");
  });
});

describe("frameIndex", () => {
  test("advances at the given fps", () => {
    expect(frameIndex(4, 10, true, 0)).toBe(0);
    expect(frameIndex(4, 10, true, 99)).toBe(0);
    expect(frameIndex(4, 10, true, 100)).toBe(1);
    expect(frameIndex(4, 10, true, 250)).toBe(2);
  });

  test("wraps around when looping", () => {
    expect(frameIndex(4, 10, true, 400)).toBe(0);
    expect(frameIndex(4, 10, true, 550)).toBe(1);
  });

  test("holds the last frame when not looping", () => {
    expect(frameIndex(3, 10, false, 5000)).toBe(2);
  });

  test("treats negative time as the first frame", () => {
    expect(frameIndex(4, 10, true, -50)).toBe(0);
  });

  test("a single frame is always frame 0", () => {
    expect(frameIndex(1, 10, true, 12345)).toBe(0);
  });
});

describe("planFrame", () => {
  test("picks the right cell of a sprite sheet", () => {
    const plan = planFrame(sheet(), "characters/cat", "walk", 250);

    expect(plan).toEqual({
      src: "characters/cat/sheet.png",
      crop: { sx: 64, sy: 32, sw: 32, sh: 32 },
      scale: 3,
    });
  });

  test("uses the resolved pose when asked for a missing one", () => {
    const plan = planFrame(sheet(), "characters/cat", "run", 0);

    expect(plan?.crop).toEqual({ sx: 0, sy: 32, sw: 32, sh: 32 });
  });

  test("honours a custom sheet image name", () => {
    const plan = planFrame(sheet({ image: "cat.png" }), "c", "stand", 0);

    expect(plan?.src).toBe("c/cat.png");
  });

  test("returns null when no pose can be resolved", () => {
    const character = sheet({ poses: { walk: sheetJson.poses.walk }, aliases: {} });

    expect(planFrame(character, "c", "fall", 0)).toBeNull();
  });

  test("picks the right Shimeji image for a walk cycle", () => {
    const character = parseCharacter({ format: "shimeji", name: "S", license: "x" });

    // Shimeji-ee walks through shime1, shime2, shime1, shime3.
    const srcs = [0, 1, 2, 3].map(
      (step) => planFrame(character, "characters/s", "walk", step * 125)?.src,
    );

    expect(srcs).toEqual([
      "characters/s/img/shime1.png",
      "characters/s/img/shime2.png",
      "characters/s/img/shime1.png",
      "characters/s/img/shime3.png",
    ]);
  });

  test("Shimeji frames are not cropped", () => {
    const character = parseCharacter({ format: "shimeji", name: "S", license: "x" });

    expect(planFrame(character, "c", "stand", 0)?.crop).toBeNull();
  });

  test("Shimeji honours a custom scale", () => {
    const character = parseCharacter({
      format: "shimeji",
      name: "S",
      license: "x",
      scale: 0.5,
    });

    expect(planFrame(character, "c", "stand", 0)?.scale).toBe(0.5);
  });
});
