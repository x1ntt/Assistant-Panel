<script setup lang="ts">
import { onMounted, onUnmounted } from "vue";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { ChevronRight } from "lucide-vue-next";

let pollTimer: ReturnType<typeof setInterval> | null = null;

onMounted(async () => {
  await listen<boolean>("theme:change", (event) => {
    applyTheme(event.payload);
  });
  await syncTheme();
  pollTimer = setInterval(syncTheme, 500);
  document.addEventListener("keydown", onKeydown);
});

onUnmounted(() => {
  if (pollTimer) clearInterval(pollTimer);
  document.removeEventListener("keydown", onKeydown);
});

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") {
    closeFloating();
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

async function closeFloating() {
  try {
    await invoke("hide_floating");
  } catch (_) {}
}
</script>

<template>
  <div class="w-8 h-8 flex items-center justify-center bg-transparent">
    <button
      class="w-7 h-7 rounded-sm bg-background border border-border hover:bg-accent flex items-center justify-center"
      @click="closeFloating"
    >
      <ChevronRight class="h-3.5 w-3.5 text-foreground" />
    </button>
  </div>
</template>
