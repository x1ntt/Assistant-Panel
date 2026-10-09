<script setup lang="ts">
import { ref, onMounted, onUnmounted, nextTick, watch } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, X } from "lucide-vue-next";

interface AlertPayload {
  metalId: string;
  metalName: string;
  currentValue: number;
  unit: string;
  rule: string;
  time: string;
}

const alert = ref<AlertPayload | null>(null);
const cardRef = ref<HTMLElement | null>(null);
let unlisten: UnlistenFn | null = null;
let pollTimer: ReturnType<typeof setInterval> | null = null;

// Measure the actual rendered card height and ask the backend to resize the
// window to fit (clamped in Rust). Includes the 6px outer padding on both sides.
async function fitWindow() {
  await nextTick();
  const el = cardRef.value;
  if (!el) return;
  const h = el.getBoundingClientRect().height + 12; // p-1.5 (=6px) top+bottom
  try {
    await invoke("resize_alert", { height: h });
  } catch (_) {}
}

onMounted(async () => {
  unlisten = await listen<AlertPayload>("gold:alert", async (event) => {
    alert.value = event.payload;
    await fitWindow();
  });

  await listen<boolean>("theme:change", (event) => applyTheme(event.payload));
  await syncTheme();
  pollTimer = setInterval(syncTheme, 500);
  await fitWindow();
});

// Re-fit whenever content changes.
watch(alert, () => fitWindow());

onUnmounted(() => {
  unlisten?.();
  if (pollTimer) clearInterval(pollTimer);
});

function applyTheme(isDark: boolean) {
  document.documentElement.classList.toggle("dark", isDark);
}

async function syncTheme() {
  try {
    const theme = await invoke<string>("read_theme");
    applyTheme(theme === "dark");
  } catch (_) {}
}

async function close() {
  try {
    await invoke("hide_alert");
  } catch (_) {}
}
</script>

<template>
  <div class="w-full p-1.5 bg-transparent">
    <div
      ref="cardRef"
      class="w-full rounded-md border-2 border-destructive bg-background shadow-xl flex flex-col overflow-hidden"
    >
      <!-- Header -->
      <div class="flex items-center justify-between px-3 py-2 bg-destructive/10 border-b border-destructive/30">
        <div class="flex items-center gap-1.5">
          <AlertTriangle class="h-4 w-4 text-destructive" />
          <span class="text-sm font-semibold text-destructive">金价告警</span>
        </div>
        <button
          class="w-6 h-6 rounded-sm hover:bg-accent flex items-center justify-center"
          @click="close"
        >
          <X class="h-4 w-4 text-foreground" />
        </button>
      </div>

      <!-- Body -->
      <div v-if="alert" class="flex flex-col px-4 py-3 gap-2">
        <div class="text-base font-semibold">{{ alert.metalName }}</div>
        <div class="flex items-baseline gap-1.5">
          <span class="text-2xl font-bold text-destructive tabular-nums">
            {{ alert.currentValue.toLocaleString() }}
          </span>
          <span class="text-xs text-muted-foreground">{{ alert.unit }}</span>
        </div>
        <div class="text-xs text-muted-foreground">
          触发规则：卖出价 <span class="font-medium text-foreground">{{ alert.rule }}</span>
        </div>
        <div class="text-[10px] text-muted-foreground">{{ alert.time }}</div>
      </div>
      <div v-else class="px-4 py-6 text-center text-xs text-muted-foreground">
        等待告警...
      </div>

      <!-- Footer -->
      <div class="px-3 py-2 border-t border-border">
        <button
          class="w-full h-7 rounded-sm bg-destructive text-destructive-foreground text-xs font-medium hover:opacity-90"
          @click="close"
        >
          知道了
        </button>
      </div>
    </div>
  </div>
</template>
