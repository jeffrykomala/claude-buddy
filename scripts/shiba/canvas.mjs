// Tiny pixel-art toolkit for generate-shiba.mjs: a square grid of palette keys,
// drawing primitives, an automatic outline pass, rotate/flip, and a PNG writer
// (no dependencies, only node:zlib). Primitives mutate the grid they are given;
// outline() and the transforms return new grids.

import { deflateSync } from "node:zlib";

export const FRAME = 32;

/** Palette key -> RGBA. null cells are transparent. */
export const PALETTE = {
  O: [0x3a, 0x22, 0x14, 255], // outline
  C: [0xe3, 0x8a, 0x2d, 255], // coat
  S: [0xb8, 0x62, 0x1a, 255], // shade (far legs, inner ears)
  W: [0xf8, 0xea, 0xd2, 255], // cream (urajiro)
  D: [0x1c, 0x12, 0x0a, 255], // eyes, nose, mouth
  T: [0xe8, 0x6a, 0x7a, 255], // tongue
  B: [0x8f, 0xc7, 0xf0, 255], // thought dots, tears
};

export function createGrid() {
  return Array.from({ length: FRAME }, () => Array(FRAME).fill(null));
}

export function put(grid, x, y, key) {
  const ix = Math.round(x);
  const iy = Math.round(y);
  if (ix >= 0 && ix < FRAME && iy >= 0 && iy < FRAME) {
    grid[iy][ix] = key;
  }
}

export function rect(grid, x, y, w, h, key) {
  for (let yy = 0; yy < h; yy += 1) {
    for (let xx = 0; xx < w; xx += 1) {
      put(grid, x + xx, y + yy, key);
    }
  }
}

/** Fills pixels whose centre lies inside the ellipse (continuous coords). */
export function ellipse(grid, cx, cy, rx, ry, key) {
  for (let y = Math.floor(cy - ry); y <= Math.ceil(cy + ry); y += 1) {
    for (let x = Math.floor(cx - rx); x <= Math.ceil(cx + rx); x += 1) {
      const dx = (x + 0.5 - cx) / rx;
      const dy = (y + 0.5 - cy) / ry;
      if (dx * dx + dy * dy <= 1) {
        put(grid, x, y, key);
      }
    }
  }
}

function insidePolygon(points, px, py) {
  let inside = false;
  for (let i = 0, j = points.length - 1; i < points.length; j = i, i += 1) {
    const [xi, yi] = points[i];
    const [xj, yj] = points[j];
    if (yi > py !== yj > py && px < ((xj - xi) * (py - yi)) / (yj - yi) + xi) {
      inside = !inside;
    }
  }
  return inside;
}

export function polygon(grid, points, key) {
  const xs = points.map((p) => p[0]);
  const ys = points.map((p) => p[1]);
  for (let y = Math.floor(Math.min(...ys)); y <= Math.ceil(Math.max(...ys)); y += 1) {
    for (let x = Math.floor(Math.min(...xs)); x <= Math.ceil(Math.max(...xs)); x += 1) {
      if (insidePolygon(points, x + 0.5, y + 0.5)) {
        put(grid, x, y, key);
      }
    }
  }
}

/** A thick line, drawn as a trail of `width` x `width` squares. */
export function stroke(grid, x0, y0, x1, y1, width, key) {
  const steps = Math.max(1, Math.ceil(Math.hypot(x1 - x0, y1 - y0) * 2));
  for (let s = 0; s <= steps; s += 1) {
    const x = x0 + ((x1 - x0) * s) / steps;
    const y = y0 + ((y1 - y0) * s) / steps;
    rect(grid, Math.round(x - width / 2), Math.round(y - width / 2), width, width, key);
  }
}

/** Dark outline around the whole silhouette (4-neighbourhood). */
export function outline(grid, key = "O") {
  const out = grid.map((row) => row.slice());
  for (let y = 0; y < FRAME; y += 1) {
    for (let x = 0; x < FRAME; x += 1) {
      if (grid[y][x] !== null) {
        continue;
      }
      const touches =
        (y > 0 && grid[y - 1][x] !== null) ||
        (y < FRAME - 1 && grid[y + 1][x] !== null) ||
        (x > 0 && grid[y][x - 1] !== null) ||
        (x < FRAME - 1 && grid[y][x + 1] !== null);
      if (touches) {
        out[y][x] = key;
      }
    }
  }
  return out;
}

export function rotateCW(grid) {
  const out = createGrid();
  for (let y = 0; y < FRAME; y += 1) {
    for (let x = 0; x < FRAME; x += 1) {
      out[x][FRAME - 1 - y] = grid[y][x];
    }
  }
  return out;
}

export function flipV(grid) {
  return grid.map((_, y) => grid[FRAME - 1 - y].slice());
}

// ------------------------------------------------------------------ PNG

const CRC_TABLE = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k += 1) {
    c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  }
  return c >>> 0;
});

function crc32(buffer) {
  let c = 0xffffffff;
  for (const byte of buffer) {
    c = CRC_TABLE[(c ^ byte) & 0xff] ^ (c >>> 8);
  }
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([length, body, crc]);
}

/** Encodes straight RGBA bytes (width * height * 4) as a PNG file. */
export function encodePng(width, height, rgba) {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header[8] = 8; // bit depth
  header[9] = 6; // RGBA
  const stride = width * 4;
  const raw = Buffer.alloc((stride + 1) * height);
  for (let y = 0; y < height; y += 1) {
    raw[y * (stride + 1)] = 0; // filter: none
    rgba.copy(raw, y * (stride + 1) + 1, y * stride, (y + 1) * stride);
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", header),
    chunk("IDAT", deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

/** Lays frames out in rows (one row per pose) and returns RGBA bytes. */
export function sheetToRgba(rows, columns) {
  const width = columns * FRAME;
  const height = rows.length * FRAME;
  const rgba = Buffer.alloc(width * height * 4);
  rows.forEach((frames, row) => {
    frames.forEach((grid, column) => {
      for (let y = 0; y < FRAME; y += 1) {
        for (let x = 0; x < FRAME; x += 1) {
          const key = grid[y][x];
          if (key === null) {
            continue;
          }
          const offset = ((row * FRAME + y) * width + column * FRAME + x) * 4;
          Buffer.from(PALETTE[key]).copy(rgba, offset);
        }
      }
    });
  });
  return { width, height, rgba };
}

/** Nearest-neighbour upscale for eyeballing the sheet. */
export function upscale({ width, height, rgba }, factor) {
  const out = Buffer.alloc(width * factor * height * factor * 4);
  for (let y = 0; y < height * factor; y += 1) {
    for (let x = 0; x < width * factor; x += 1) {
      const from = (Math.floor(y / factor) * width + Math.floor(x / factor)) * 4;
      rgba.copy(out, (y * width * factor + x) * 4, from, from + 4);
    }
  }
  return { width: width * factor, height: height * factor, rgba: out };
}
