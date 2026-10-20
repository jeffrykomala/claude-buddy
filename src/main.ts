import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

// Temporary wiring so the pet can be dragged and thrown (M1 1.3) before
// sprite.ts (1.4) takes over rendering and input.

window.addEventListener("DOMContentLoaded", () => {
  const pet = document.querySelector<HTMLDivElement>("#pet");
  pet?.addEventListener("pointerdown", (event) => {
    const rect = pet.getBoundingClientRect();
    void invoke("grab", {
      offsetX: event.clientX - rect.left,
      offsetY: event.clientY - rect.top,
    });
  });

  // Rust is the source of truth for the current pose; once sprite.ts lands
  // this drives which frames are drawn instead of a CSS class.
  void listen<string>("pet://pose", (event) => {
    if (pet) {
      pet.dataset.pose = event.payload;
    }
  });
});
