// Character definitions (design §9.2): parsing character.json, choosing which
// pose to show when a character lacks one, and picking the frame to draw.
// Everything here is pure; sprite.ts does the actual drawing.

import { SHIMEJI_POSES } from "./shimeji";

export type Facing = "left" | "right";

export interface PoseAnimation {
  readonly row: number;
  readonly frames: number;
  readonly fps: number;
  readonly loop: boolean;
}

interface CharacterBase {
  readonly name: string;
  readonly license: string;
  /** pose name -> pose it should borrow when the character lacks it. */
  readonly aliases: Readonly<Record<string, string>>;
  readonly scale: number;
}

export interface SheetCharacter extends CharacterBase {
  readonly format: "sheet";
  readonly image: string;
  readonly frameWidth: number;
  readonly frameHeight: number;
  readonly poses: Readonly<Record<string, PoseAnimation>>;
}

export interface ShimejiCharacter extends CharacterBase {
  readonly format: "shimeji";
}

export type CharacterDef = SheetCharacter | ShimejiCharacter;

/** The part of a source image to draw, and how much to scale it. */
export interface FramePlan {
  readonly src: string;
  /** null = draw the whole image (Shimeji frames). */
  readonly crop: {
    readonly sx: number;
    readonly sy: number;
    readonly sw: number;
    readonly sh: number;
  } | null;
  readonly scale: number;
}

export class CharacterError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "CharacterError";
  }
}

const DEFAULT_SHEET_IMAGE = "sheet.png";
const MAX_ALIAS_HOPS = 8;
const FALLBACK_POSE = "stand";
const THINK_POSE = "think";
/** Poses that fall back to "think" before "stand" (design §8.2). */
const ACTIVITY_POSES: ReadonlySet<string> = new Set([
  "read",
  "write",
  "run",
  "browse",
]);

// ---------------------------------------------------------------- parsing

type Obj = Record<string, unknown>;

function isObject(value: unknown): value is Obj {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function requireString(obj: Obj, key: string): string {
  const value = obj[key];
  if (typeof value !== "string" || value.length === 0) {
    throw new CharacterError(`"${key}" must be a non-empty string`);
  }
  return value;
}

function requirePositive(value: unknown, label: string): number {
  if (typeof value !== "number" || !Number.isFinite(value) || value <= 0) {
    throw new CharacterError(`${label} must be a positive number`);
  }
  return value;
}

function parseScale(obj: Obj): number {
  return obj.scale === undefined ? 1 : requirePositive(obj.scale, '"scale"');
}

function parseAliases(obj: Obj): Record<string, string> {
  if (obj.aliases === undefined) {
    return {};
  }
  if (!isObject(obj.aliases)) {
    throw new CharacterError('"aliases" must be an object');
  }
  const aliases: Record<string, string> = {};
  for (const [pose, target] of Object.entries(obj.aliases)) {
    if (typeof target !== "string") {
      throw new CharacterError(`alias "${pose}" must point to a pose name`);
    }
    aliases[pose] = target;
  }
  return aliases;
}

function parsePose(name: string, raw: unknown): PoseAnimation {
  if (!isObject(raw)) {
    throw new CharacterError(`pose "${name}" must be an object`);
  }
  const row = raw.row;
  if (typeof row !== "number" || !Number.isInteger(row) || row < 0) {
    throw new CharacterError(`pose "${name}": row must be a whole number >= 0`);
  }
  const frames = requirePositive(raw.frames, `pose "${name}": frames`);
  const fps = requirePositive(raw.fps, `pose "${name}": fps`);
  return { row, frames, fps, loop: raw.loop !== false };
}

function parsePoses(obj: Obj): Record<string, PoseAnimation> {
  if (!isObject(obj.poses) || Object.keys(obj.poses).length === 0) {
    throw new CharacterError('"poses" must define at least one pose');
  }
  const poses: Record<string, PoseAnimation> = {};
  for (const [name, raw] of Object.entries(obj.poses)) {
    poses[name] = parsePose(name, raw);
  }
  return poses;
}

/** Validates untrusted character.json content (design: never trust files). */
export function parseCharacter(raw: unknown): CharacterDef {
  if (!isObject(raw)) {
    throw new CharacterError("character.json must be an object");
  }
  const base = {
    name: requireString(raw, "name"),
    license: requireString(raw, "license"),
    aliases: parseAliases(raw),
    scale: parseScale(raw),
  };

  if (raw.format === "shimeji") {
    return { ...base, format: "shimeji" };
  }
  if (raw.format === "sheet") {
    return {
      ...base,
      format: "sheet",
      image:
        typeof raw.image === "string" && raw.image.length > 0
          ? raw.image
          : DEFAULT_SHEET_IMAGE,
      frameWidth: requirePositive(raw.frameWidth, '"frameWidth"'),
      frameHeight: requirePositive(raw.frameHeight, '"frameHeight"'),
      poses: parsePoses(raw),
    };
  }
  throw new CharacterError(`unknown format: ${String(raw.format)}`);
}

// ------------------------------------------------------- pose resolution

function hasPose(character: CharacterDef, pose: string): boolean {
  return character.format === "sheet"
    ? pose in character.poses
    : pose in SHIMEJI_POSES;
}

function followAliases(character: CharacterDef, pose: string): string | null {
  let current = pose;
  for (let hop = 0; hop < MAX_ALIAS_HOPS; hop += 1) {
    if (hasPose(character, current)) {
      return current;
    }
    const next = character.aliases[current];
    if (next === undefined) {
      return null;
    }
    current = next;
  }
  return null;
}

/**
 * The pose this character will actually show for `pose`: the pose itself,
 * else its alias, else "think" (activities only), else "stand". Null when the
 * character cannot even stand, and the placeholder takes over.
 */
export function resolvePose(
  character: CharacterDef,
  pose: string,
): string | null {
  const direct = followAliases(character, pose);
  if (direct !== null) {
    return direct;
  }
  if (ACTIVITY_POSES.has(pose) && hasPose(character, THINK_POSE)) {
    return THINK_POSE;
  }
  return hasPose(character, FALLBACK_POSE) ? FALLBACK_POSE : null;
}

// --------------------------------------------------------- frame choosing

/** Which frame of a `count`-frame animation is showing after `elapsedMs`. */
export function frameIndex(
  count: number,
  fps: number,
  loop: boolean,
  elapsedMs: number,
): number {
  if (count <= 1 || elapsedMs <= 0) {
    return 0;
  }
  const step = Math.floor((elapsedMs * fps) / 1000);
  return loop ? step % count : Math.min(step, count - 1);
}

/** What to draw for `pose` `elapsedMs` after it started, or null for none. */
export function planFrame(
  character: CharacterDef,
  basePath: string,
  pose: string,
  elapsedMs: number,
): FramePlan | null {
  const resolved = resolvePose(character, pose);
  if (resolved === null) {
    return null;
  }

  if (character.format === "sheet") {
    const animation = character.poses[resolved];
    const index = frameIndex(
      animation.frames,
      animation.fps,
      animation.loop,
      elapsedMs,
    );
    return {
      src: `${basePath}/${character.image}`,
      crop: {
        sx: index * character.frameWidth,
        sy: animation.row * character.frameHeight,
        sw: character.frameWidth,
        sh: character.frameHeight,
      },
      scale: character.scale,
    };
  }

  const animation = SHIMEJI_POSES[resolved];
  const index = frameIndex(
    animation.frames.length,
    animation.fps,
    true,
    elapsedMs,
  );
  return {
    src: `${basePath}/img/shime${animation.frames[index]}.png`,
    crop: null,
    scale: character.scale,
  };
}
