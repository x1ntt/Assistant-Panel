<script setup lang="ts">
import { usePanelStore } from "@/stores/panel";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { onMounted, onUnmounted } from "vue";
import PanelHeader from "./PanelHeader.vue";
import ToolList from "./ToolList.vue";
import BottomBar from "./BottomBar.vue";
import FloatingContent from "./FloatingContent.vue";
import CloseButton from "./CloseButton.vue";
import AlertWindow from "./AlertWindow.vue";

const panel = usePanelStore();
// Resolve before the first render: auxiliary webviews must never mount main plugins.
const label = getCurrentWebview().label;
const windowType: "main" | "floating" | "close-btn" | "alert" =
  label === "floating" || label === "close-btn" || label === "alert" ? label : "main";

function onMainKeydown(e: KeyboardEvent) {
  // Block webview refresh (F5 / Ctrl+R / Ctrl+Shift+R). This is a desktop app,
  // not a web page — a reload wipes in-memory UI state and desyncs from the
  // still-running Rust backend.
  if (
    e.key === "F5" ||
    ((e.ctrlKey || e.metaKey) && (e.key === "r" || e.key === "R"))
  ) {
    e.preventDefault();
    return;
  }
  if (e.key === "Escape") {
    panel.collapse();
  }
}

onMounted(async () => {
  if (windowType === "main") {
    await panel.init();
    await listen("hotkey:toggle", () => {
      panel.toggle();
    });
    document.addEventListener("keydown", onMainKeydown);
  }
});

onUnmounted(() => {
  document.removeEventListener("keydown", onMainKeydown);
});
</script>

<template>
  <!-- Close button window -->
  <div v-if="windowType === 'close-btn'" class="w-full h-full">
    <CloseButton />
  </div>

  <!-- Alert window -->
  <div v-else-if="windowType === 'alert'" class="w-full h-full">
    <AlertWindow />
  </div>

  <!-- Floating window -->
  <div
    v-else-if="windowType === 'floating'"
    class="h-screen w-full flex flex-col bg-background text-foreground overflow-hidden border-l border-border"
  >
    <FloatingContent />
  </div>

  <!-- Main window -->
  <div
    v-else
    class="h-screen w-full flex flex-col bg-background text-foreground overflow-hidden border-l border-border"
  >
    <PanelHeader />
    <div class="flex-1 overflow-auto">
      <ToolList />
    </div>
    <BottomBar />
  </div>
</template>
