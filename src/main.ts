import { invoke } from "@tauri-apps/api/core";
import { parseCharacter, type CharacterDef, type Facing } from "./character";
import { Controller } from "./controller";
import {
  EVENTS,
  ipc,
  subscribe,
  type ApprovalRequest,
  type LayoutEvent,
  type RunEvent,
} from "./ipc";
import { applyLayout } from "./stage";
import { Sprite, type ImageLike } from "./sprite";

/** Matches the window size in tauri.conf.json and physics::constants. */
const PET_SIZE_PT = 128;
/** Used when Rust can't be asked (e.g. the page opened in a plain browser). */
const FALLBACK_CHARACTER = "shiba";

interface PoseEvent {
  pose: string;
  facing: Facing;
}

function loadImage(url: string): Promise<ImageLike> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error(`could not load ${url}`));
    image.src = url;
  });
}

/** A missing or broken character is not fatal: the placeholder shows. */
async function loadCharacter(basePath: string): Promise<CharacterDef | null> {
  try {
    const response = await fetch(`${basePath}/character.json`);
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}`);
    }
    return parseCharacter(await response.json());
  } catch (error: unknown) {
    const reason = error instanceof Error ? error.message : String(error);
    console.warn(`Using the placeholder pet: ${reason}`);
    return null;
  }
}

async function savedCharacterId(): Promise<string> {
  try {
    return await invoke<string>("current_character");
  } catch {
    return FALLBACK_CHARACTER;
  }
}

async function createSprite(
  canvas: HTMLCanvasElement,
  characterId: string,
): Promise<Sprite> {
  const basePath = `characters/${characterId}`;
  return new Sprite({
    canvas,
    size: PET_SIZE_PT,
    devicePixelRatio: window.devicePixelRatio,
    character: await loadCharacter(basePath),
    basePath,
    loadImage,
  });
}

function requireElement<T extends HTMLElement>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (element === null) {
    throw new Error(`${selector} is missing from index.html`);
  }
  return element;
}

async function start(): Promise<void> {
  const canvas = requireElement<HTMLCanvasElement>("#pet");
  const stage = requireElement<HTMLElement>("#stage");
  let sprite = await createSprite(canvas, await savedCharacterId());
  let latest: PoseEvent = { pose: "stand", facing: "left" };
  // A mood (listening, working, happy...) overrides the physics pose.
  let override: string | null = null;
  const showPose = (): void =>
    sprite.setState(override ?? latest.pose, latest.facing, performance.now());

  const controller = new Controller({
    doc: document,
    ipc,
    setPoseOverride: (pose) => {
      override = pose;
      showPose();
    },
    schedule: (fn, ms) => {
      const timer = window.setTimeout(fn, ms);
      return () => window.clearTimeout(timer);
    },
    onError: (message) => console.error(message),
  });
  stage.append(
    controller.bubble.element,
    controller.label.element,
    controller.dot.element,
  );

  canvas.addEventListener("pointerdown", (event) => {
    const rect = canvas.getBoundingClientRect();
    void ipc.grab(event.clientX - rect.left, event.clientY - rect.top);
  });
  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      controller.close();
    }
  });

  // Rust is the source of truth for the pose and which way the pet faces.
  await subscribe<PoseEvent>(EVENTS.pose, (payload) => {
    latest = payload;
    showPose();
  });
  await subscribe<LayoutEvent>(EVENTS.layout, (layout) =>
    applyLayout(
      {
        stage,
        pet: canvas,
        bubble: controller.bubble.element,
        label: controller.label.element,
        dot: controller.dot.element,
      },
      layout,
      PET_SIZE_PT,
    ),
  );

  // Swap the sprite in place, keeping the pose the pet is currently in.
  await subscribe<string>(EVENTS.character, async (characterId) => {
    const next = await createSprite(canvas, characterId);
    sprite = next;
    showPose();
  });

  await subscribe<void>(EVENTS.click, () => controller.onPetClick());
  await subscribe<void>(EVENTS.talk, () => controller.openInput());
  await subscribe<RunEvent>(EVENTS.run, (event) =>
    controller.onRunEvent(event),
  );
  await subscribe<ApprovalRequest>(EVENTS.approval, (request) =>
    controller.onApproval(request),
  );
  await subscribe<void>(EVENTS.newConversation, () =>
    controller.onNewConversation(),
  );
  await subscribe<void>(EVENTS.picker, () => controller.onPicker());
  await subscribe<string>(EVENTS.notice, (message) =>
    controller.onNotice(message),
  );
  await subscribe<void>(EVENTS.reopen, () => controller.onReopen());

  const frame = (nowMs: number): void => {
    sprite.render(nowMs);
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
}

window.addEventListener("DOMContentLoaded", () => {
  start().catch((error: unknown) => {
    console.error("Desktop Buddy failed to start", error);
  });
});
