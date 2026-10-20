// The Shiba, drawn from simple shapes (side view, facing left, as every sprite
// in this app does). Each pose builder returns frames: 32x32 grids of palette
// keys that canvas.mjs turns into the sprite sheet. The feet rest on row 30.

import {
  FRAME,
  createGrid,
  ellipse,
  flipV,
  outline,
  polygon,
  put,
  rect,
  rotateCW,
  stroke,
} from "./canvas.mjs";

const GROUND = 30;
const TAU = Math.PI * 2;

const flipH = (grid) => grid.map((row) => row.slice().reverse());

// ------------------------------------------------------------------ parts

const earShapes = {
  up: [
    { outer: [[-3, -3.5], [-2.5, -10], [1.5, -4.5]], inner: [[-2, -4.5], [-2, -8], [0, -5]] },
    { outer: [[1, -5], [4.5, -10], [6, -2.5]], inner: [[2.5, -5], [4.5, -8], [5, -4]] },
  ],
  back: [
    { outer: [[-1, -4], [4.5, -8], [3, -2.5]], inner: [] },
    { outer: [[2.5, -4], [8.5, -7], [6, -1.5]], inner: [] },
  ],
  down: [
    { outer: [[-4, -2], [-7.5, 3], [-1.5, -4.5]], inner: [] },
    { outer: [[2, -4.5], [7.5, 1], [5, -4]], inner: [] },
  ],
};

function drawEars(grid, cx, cy, kind) {
  for (const { outer, inner } of earShapes[kind]) {
    polygon(grid, outer.map(([x, y]) => [cx + x, cy + y]), "C");
    if (inner.length > 0) {
      polygon(grid, inner.map(([x, y]) => [cx + x, cy + y]), "S");
    }
  }
}

function drawEye(grid, cx, cy, kind) {
  if (kind === "closed") {
    rect(grid, cx - 3, cy, 2, 1, "D");
  } else if (kind === "happy") {
    put(grid, cx - 3, cy, "D");
    put(grid, cx - 2, cy - 1, "D");
    put(grid, cx - 1, cy - 1, "D");
    put(grid, cx, cy, "D");
  } else if (kind === "x") {
    for (const [dx, dy] of [[-3, -1], [-1, -1], [-2, 0], [-3, 1], [-1, 1]]) {
      put(grid, cx + dx, cy + dy, "D");
    }
  } else if (kind === "sad") {
    rect(grid, cx - 3, cy, 2, 2, "D");
    put(grid, cx - 3, cy - 2, "D");
    put(grid, cx - 2, cy - 1, "D");
    put(grid, cx - 2, cy + 3, "B");
  } else {
    rect(grid, cx - 3, cy - 1, 2, 2, "D");
  }
  if (kind !== "x") {
    put(grid, cx - 3, cy - 3, "W"); // the Shiba's cream "eyebrow" dot
  }
}

function drawMouth(grid, cx, cy, kind) {
  if (kind === "open") {
    rect(grid, cx - 6, cy + 4, 3, 2, "D");
    rect(grid, cx - 5, cy + 5, 2, 1, "T");
  } else if (kind === "pant") {
    rect(grid, cx - 6, cy + 4, 3, 1, "D");
    rect(grid, cx - 5, cy + 5, 2, 3, "T");
  } else {
    rect(grid, cx - 5, cy + 4, 2, 1, "D");
  }
}

function drawHead(grid, { cx, cy, eye = "open", mouth = "closed", ears = "up" }) {
  drawEars(grid, cx, cy, ears);
  ellipse(grid, cx, cy, 5.5, 5, "C");
  ellipse(grid, cx, cy + 3.5, 4, 2.3, "W");
  ellipse(grid, cx - 4, cy + 2.5, 3.2, 2.4, "W");
  rect(grid, cx - 7, cy + 1, 2, 2, "D"); // nose
  drawEye(grid, cx, cy, eye);
  drawMouth(grid, cx, cy, mouth);
}

function drawTail(grid, x, y) {
  rect(grid, x - 4, y + 3, 4, 5, "C");
  ellipse(grid, x, y, 3.8, 3.8, "C");
  ellipse(grid, x - 1, y + 1.5, 1.8, 1.8, "W");
}

function drawLeg(grid, hipX, hipY, { dx = 0, lift = 0 }, key) {
  const footX = hipX + dx;
  const footY = GROUND - 1 - lift;
  stroke(grid, hipX, hipY, footX, footY - 1, 3, key);
  rect(grid, Math.round(footX - 1.5), footY - 1, 3, 2, "W");
}

/** Light-blue dots with a dark ring, drawn after the outline pass. */
function bubbleDot(grid, x, y) {
  rect(grid, x - 1, y - 1, 4, 4, "O");
  rect(grid, x, y, 2, 2, "B");
}

function zzz(grid, x, y) {
  for (const [dx, dy] of [[0, 0], [1, 0], [2, 0], [1, 1], [0, 2], [1, 2], [2, 2]]) {
    put(grid, x + dx, y + dy, "B");
  }
}

// ---------------------------------------------------------------- bodies

const FEET_DOWN = { fn: {}, ff: {}, bn: {}, bf: {} };

function standDog({
  dy = 0,
  headDy = 0,
  tailDx = 0,
  legs = FEET_DOWN,
  eye,
  mouth,
  ears,
} = {}) {
  const grid = createGrid();
  drawLeg(grid, 19, 22 + dy, legs.bf ?? {}, "S");
  drawLeg(grid, 14, 22 + dy, legs.ff ?? {}, "S");
  drawTail(grid, 26 + tailDx, 13 + dy);
  ellipse(grid, 17, 19 + dy, 9, 5.5, "C");
  ellipse(grid, 16, 21.5 + dy, 7, 2.6, "W");
  drawLeg(grid, 23, 22 + dy, legs.bn ?? {}, "C");
  drawLeg(grid, 11, 22 + dy, legs.fn ?? {}, "C");
  ellipse(grid, 10.5, 21 + dy, 3, 3.5, "W");
  drawHead(grid, { cx: 8, cy: 13 + dy + headDy, eye, mouth, ears });
  return outline(grid);
}

function sitDog({
  tailDx = 0,
  headDy = 0,
  swing = 0,
  pawUp = null,
  eye,
  mouth,
  ears,
  dots = 0,
} = {}) {
  const grid = createGrid();
  rect(grid, 13, 23, 3, 7, "S"); // far front leg
  drawTail(grid, 27 + tailDx, 21);
  ellipse(grid, 20.5, 25, 6.5, 5, "C");
  rect(grid, 14, 28, 7, 2, "W"); // hind foot pointing forward
  ellipse(grid, 15, 19.5, 5.5, 8.5, "C");
  ellipse(grid, 12, 22, 3, 5, "W");
  if (pawUp === null) {
    stroke(grid, 11, 23, 11 + swing, 28, 3, "C");
    rect(grid, Math.round(11 + swing - 1.5), 28, 3, 2, "W");
  } else {
    stroke(grid, 10, 20, pawUp.x, pawUp.y, 3, "C");
    rect(grid, pawUp.x - 1, pawUp.y - 1, 3, 2, "W");
  }
  drawHead(grid, { cx: 9, cy: 11 + headDy, eye, mouth, ears });
  const framed = outline(grid);
  for (let n = 0; n < dots; n += 1) {
    bubbleDot(framed, 18 + n * 4, 8 - n * 2);
  }
  return framed;
}

function sleepDog({ breathe = 0, withZ = 0 } = {}) {
  const grid = createGrid();
  drawTail(grid, 27, 21);
  ellipse(grid, 18, 25.5, 10, 4.5 + breathe, "C");
  ellipse(grid, 15, 28, 6, 2, "W");
  polygon(grid, [[5, 22], [6, 17.5], [10, 21]], "C");
  polygon(grid, [[9, 21], [12, 17.5], [13.5, 22]], "C");
  ellipse(grid, 8, 25, 5.5, 4.2, "C");
  ellipse(grid, 4.5, 27, 3.2, 2.2, "W");
  rect(grid, 1, 25, 2, 2, "D");
  rect(grid, 5, 23, 2, 1, "D"); // closed eye
  rect(grid, 2, 28, 7, 2, "W"); // paws under the chin
  const framed = outline(grid);
  if (withZ > 0) {
    zzz(framed, 20, 9);
  }
  if (withZ > 1) {
    zzz(framed, 25, 4);
  }
  return framed;
}

function flatDog({ star = 0, tongue = 2 } = {}) {
  const grid = createGrid();
  stroke(grid, 22, 27, 29, 29, 2, "S");
  stroke(grid, 12, 27, 16, 29, 2, "S");
  drawTail(grid, 28, 21);
  ellipse(grid, 18, 26.5, 10, 3.6, "C");
  ellipse(grid, 8, 26, 5.5, 4, "C");
  polygon(grid, [[4, 23], [0.5, 20], [7, 22.5]], "C");
  polygon(grid, [[9, 22], [12.5, 19.5], [13.5, 23]], "C");
  ellipse(grid, 4.5, 27.5, 3.2, 2.2, "W");
  rect(grid, 1, 26, 2, 2, "D");
  drawEye(grid, 8, 25, "x");
  rect(grid, 2, 29, 2, tongue, "T");
  stroke(grid, 6, 28, 3, 29, 2, "W");
  const framed = outline(grid);
  if (star > 0) {
    const angle = (star / 3) * TAU;
    bubbleDot(framed, Math.round(9 + 5 * Math.cos(angle)), Math.round(15 + 3 * Math.sin(angle)));
  }
  return framed;
}

function hangDog({ swing = 0 } = {}) {
  const grid = createGrid();
  drawLeg(grid, 19, 24, { dx: swing + 1 }, "S");
  drawLeg(grid, 13, 24, { dx: -swing - 1 }, "S");
  stroke(grid, 17, 25, 18 + swing, 29, 3, "C");
  ellipse(grid, 16, 21, 5.5, 7, "C");
  ellipse(grid, 14, 23, 3, 4.5, "W");
  drawLeg(grid, 20, 25, { dx: swing }, "C");
  drawLeg(grid, 12, 25, { dx: -swing }, "C");
  drawHead(grid, { cx: 13, cy: 11, eye: "open", mouth: "open", ears: "down" });
  return outline(grid);
}

// ------------------------------------------------------------------ gaits

function gait(phase, { amp, lift: liftAmp, gallop = false }) {
  const foot = (offset) => ({
    dx: Math.round(amp * Math.sin(TAU * (phase + offset))),
    lift: Math.max(0, Math.round(-liftAmp * Math.cos(TAU * (phase + offset)))),
  });
  return gallop
    ? { fn: foot(0), ff: foot(0.06), bn: foot(0.45), bf: foot(0.5) }
    : { fn: foot(0), bf: foot(0), ff: foot(0.5), bn: foot(0.5) };
}

const bobFor = (phase) => (Math.abs(Math.sin(TAU * phase)) > 0.6 ? 1 : 0);

const walkFrame = (phase, options = {}) =>
  standDog({ legs: gait(phase, { amp: 4, lift: 2 }), dy: bobFor(phase), ...options });

const frames = (count, build) => Array.from({ length: count }, (_, i) => build(i));
const tailWag = [0, 1, 0, -1];

// ------------------------------------------------------------------ poses

/** Every pose of the sheet, in row order. */
export function buildPoses() {
  const climbing = (i) => flipH(rotateCW(walkFrame(i / 4, { dy: 0 })));
  const standing = (i) => standDog({ tailDx: tailWag[i], eye: i === 3 ? "closed" : "open" });
  const falling = (i) =>
    standDog({
      dy: -3,
      headDy: -1,
      mouth: "open",
      tailDx: i,
      legs: i === 0
        ? { fn: { dx: -4 }, ff: { dx: -2 }, bn: { dx: 4 }, bf: { dx: 2 } }
        : { fn: { dx: -2, lift: 1 }, ff: { dx: -5 }, bn: { dx: 2 }, bf: { dx: 5, lift: 1 } },
    });

  return [
    { name: "stand", fps: 3, loop: true, frames: frames(4, standing) },
    { name: "walk", fps: 10, loop: true, frames: frames(6, (i) => walkFrame(i / 6)) },
    {
      name: "run",
      fps: 14,
      loop: true,
      frames: frames(6, (i) =>
        standDog({
          legs: gait(i / 6, { amp: 6, lift: 3, gallop: true }),
          dy: bobFor(i / 6),
          mouth: "pant",
          ears: "back",
          tailDx: i % 2,
        }),
      ),
    },
    { name: "sit", fps: 3, loop: true, frames: frames(2, (i) => sitDog({ tailDx: i })) },
    {
      name: "sit_dangle",
      fps: 4,
      loop: true,
      frames: frames(4, (i) => sitDog({ swing: [-3, -1, 1, -1][i], tailDx: i % 2, mouth: "open" })),
    },
    {
      name: "sleep",
      fps: 1.5,
      loop: true,
      frames: frames(2, (i) => sleepDog({ breathe: i, withZ: i + 1 })),
    },
    { name: "fall", fps: 8, loop: true, frames: frames(2, falling) },
    {
      name: "land",
      fps: 12,
      loop: false,
      frames: frames(3, (i) => standDog({ dy: 3 - i, eye: i === 0 ? "closed" : "open", ears: i === 0 ? "back" : "up" })),
    },
    { name: "splat", fps: 4, loop: false, frames: frames(3, (i) => flatDog({ star: i, tongue: i + 1 })) },
    { name: "climb", fps: 6, loop: true, frames: frames(4, climbing) },
    {
      name: "wall_hang",
      fps: 2,
      loop: true,
      frames: frames(2, (i) => flipH(rotateCW(standDog({ tailDx: i, mouth: "closed" })))),
    },
    {
      name: "ceiling_crawl",
      fps: 6,
      loop: true,
      frames: frames(4, (i) => flipV(walkFrame(i / 4, { dy: 0 }))),
    },
    { name: "ceiling_hang", fps: 2, loop: true, frames: frames(2, (i) => flipV(standDog({ tailDx: i }))) },
    {
      name: "jump",
      fps: 1,
      loop: true,
      frames: [
        standDog({
          dy: -3,
          mouth: "open",
          ears: "back",
          legs: { fn: { dx: 2, lift: 4 }, ff: { dx: 4, lift: 5 }, bn: { dx: 6, lift: 3 }, bf: { dx: 8, lift: 4 } },
        }),
      ],
    },
    { name: "dragged", fps: 6, loop: true, frames: frames(4, (i) => hangDog({ swing: [0, 1, 0, -1][i] })) },
    {
      name: "thrown",
      fps: 12,
      loop: true,
      frames: frames(4, (i) => {
        let grid = falling(0);
        for (let turn = 0; turn < i; turn += 1) {
          grid = rotateCW(grid);
        }
        return grid;
      }),
    },
    {
      name: "think",
      fps: 2,
      loop: true,
      frames: frames(3, (i) => sitDog({ dots: i + 1, headDy: 0, tailDx: i % 2 })),
    },
    {
      name: "talk",
      fps: 6,
      loop: true,
      frames: frames(2, (i) => standDog({ mouth: i === 0 ? "open" : "closed", headDy: -i, tailDx: i })),
    },
    {
      name: "happy",
      fps: 8,
      loop: true,
      frames: frames(4, (i) =>
        standDog({
          dy: i % 2 === 0 ? 0 : -2,
          eye: "happy",
          mouth: "pant",
          tailDx: tailWag[i],
          legs: i % 2 === 0 ? FEET_DOWN : { fn: { lift: 3 }, ff: { lift: 3 }, bn: { lift: 3 }, bf: { lift: 3 } },
        }),
      ),
    },
    {
      name: "wave",
      fps: 4,
      loop: true,
      frames: frames(4, (i) =>
        sitDog({ pawUp: { x: i % 2 === 0 ? 5 : 8, y: 14 }, eye: "happy", mouth: "open", tailDx: i % 2 }),
      ),
    },
    {
      name: "sad",
      fps: 1.5,
      loop: true,
      frames: frames(2, (i) => sitDog({ eye: "sad", ears: "down", headDy: 1, tailDx: -i })),
    },
  ];
}

export const FRAME_SIZE = FRAME;
