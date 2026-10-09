import { defineStore } from "pinia";
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";

export const usePanelStore = defineStore("panel", () => {
  const isCollapsed = ref(false);
  const floatingPanelContent = ref<string | null>(null);

  async function init() {
    await listen("floating:closed", () => {
      floatingPanelContent.value = null;
    });
  }

  async function toggle() {
    // Always accept the toggle — the Rust animation is interruptible, so a
    // mid-slide second press reverses from the current x instead of being
    // dropped.
    if (isCollapsed.value) {
      await expand();
    } else {
      await collapse();
    }
  }

  async function collapse() {
    isCollapsed.value = true;
    floatingPanelContent.value = null;
    try {
      await invoke("slide_out_main");
    } catch (_) {}
  }

  async function expand() {
    isCollapsed.value = false;
    try {
      await invoke("slide_in_main");
    } catch (_) {}
  }

  function close() {
    collapse();
  }

  async function openFloatingPanel(contentId: string) {
    if (floatingPanelContent.value === contentId) {
      await closeFloatingPanel();
      return;
    }
    floatingPanelContent.value = contentId;
    await emit("floating:set-content", contentId);
    try {
      await invoke("show_floating");
    } catch (_) {}
  }

  /** Open (or keep open) the floating panel for a content id — never toggles closed. */
  async function ensureFloatingPanel(contentId: string) {
    floatingPanelContent.value = contentId;
    await emit("floating:set-content", contentId);
    try {
      await invoke("show_floating");
    } catch (_) {}
  }

  async function closeFloatingPanel() {
    floatingPanelContent.value = null;
    try {
      await invoke("hide_floating");
    } catch (_) {}
    try {
      await invoke("focus_main");
    } catch (_) {}
  }

  return {
    isCollapsed,
    floatingPanelContent,
    init,
    toggle,
    collapse,
    expand,
    close,
    openFloatingPanel,
    ensureFloatingPanel,
    closeFloatingPanel,
  };
});
