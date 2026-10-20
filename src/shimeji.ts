// Pose -> image mapping for Shimeji-ee image folders (design §9.2, format B).
//
// Read from Shimeji-ee's stock conf/actions.xml: each <Action> there lists the
// shimeN.png frames it plays. We keep only the actions that correspond to our
// pose names; anything a set does not draw falls back through the usual
// alias -> think -> stand chain (character.ts).

export interface ShimejiPose {
  /** shimeN.png numbers, played in order. */
  readonly frames: readonly number[];
  readonly fps: number;
}

/** A standard Shimeji-ee set ships shime1.png .. shime46.png. */
export const SHIMEJI_IMAGE_COUNT = 46;

export const SHIMEJI_POSES: Readonly<Record<string, ShimejiPose>> = {
  stand: { frames: [1], fps: 1 },
  walk: { frames: [1, 2, 1, 3], fps: 8 },
  run: { frames: [1, 2, 1, 3], fps: 12 },
  sit: { frames: [11], fps: 1 },
  sit_dangle: { frames: [31, 32, 31, 33], fps: 4 },
  sleep: { frames: [21], fps: 1 }, // "Sprawl"
  fall: { frames: [4], fps: 1 },
  land: { frames: [18], fps: 1 }, // first frame of "Bouncing"
  splat: { frames: [19, 20], fps: 4 },
  climb: { frames: [14, 12, 13], fps: 6 }, // "ClimbWall"
  wall_hang: { frames: [13], fps: 1 }, // "GrabWall"
  ceiling_crawl: { frames: [25, 23, 24], fps: 6 }, // "ClimbCeiling"
  ceiling_hang: { frames: [23], fps: 1 }, // "GrabCeiling"
  jump: { frames: [22], fps: 1 },
  dragged: { frames: [9, 7, 8, 10], fps: 6 }, // "Pinched"
  thrown: { frames: [4], fps: 1 }, // the falling frame, as Shimeji does
  // Looking up reads as "listening" and "thinking" well enough until a set
  // supplies better art.
  listen: { frames: [26], fps: 1 },
  think: { frames: [26], fps: 1 },
};
