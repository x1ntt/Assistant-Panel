<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from "vue";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { pluginRegistry } from "@/plugins/registry";
import { Button } from "@/components/ui/button";
import { X } from "lucide-vue-next";
import SettingsView from "./SettingsView.vue";
import LogView from "./LogView.vue";

const contentId = ref<string | null>(null);
let pollTimer: ReturnType<typeof setInterval> | null = null;

onMounted(async () => {
  await listen<string>("floating:set-content", (event) => {
    contentId.value = event.payload;
  });

  await listen<boolean>("theme:change", (event) => {
    applyTheme(event.payload);
  });

  // ESC key closes floating window
  document.addEventListener("keydown", onKeydown);

  await syncTheme();
  pollTimer = setInterval(syncTheme, 500);
});

onUnmounted(() => {
  if (pollTimer) clearInterval(pollTimer);
  document.removeEventListener("keydown", onKeydown);
});

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") {
    close();
  }
}

function applyTheme(isDark: boolean) {
  if (isDark) {
    document.documentElement.classList.add("dark");
  } else {
    document.documentElement.classList.remove("dark");
  }
}

async function syncTheme() {
  try {
    const theme = await invoke<string>("read_theme");
    applyTheme(theme === "dark");
  } catch (_) {}
}

const currentPlugin = computed(() =>
  contentId.value ? pluginRegistry.find((p) => p.id === contentId.value) : null
);

const title = computed(() => {
  if (contentId.value === "settings") return "设置";
  if (contentId.value === "logs") return "日志";
  return currentPlugin.value?.name ?? "";
});

async function close() {
  contentId.value = null;
  try {
    await invoke("hide_floating");
  } catch (_) {}
  try {
    await invoke("focus_main");
  } catch (_) {}
}
</script>

<template>
  <div class="h-full flex flex-col">
    <div class="flex items-center justify-between px-2 py-1 border-b border-border bg-card shrink-0" data-tauri-drag-region>
      <span class="text-xs font-medium text-muted-foreground" data-tauri-drag-region>{{ title }}</span>
      <Button variant="ghost" size="icon" class="h-5 w-5" @click="close">
        <X class="h-3 w-3" />
      </Button>
    </div>
    <div class="flex-1 overflow-auto">
      <component
        v-if="currentPlugin?.floatingView"
        :is="currentPlugin.floatingView"
      />
      <SettingsView v-else-if="contentId === 'settings'" />
      <LogView v-else-if="contentId === 'logs'" />
      <div v-else class="p-4 text-sm text-muted-foreground">不可用</div>
    </div>
  </div>
</template>
