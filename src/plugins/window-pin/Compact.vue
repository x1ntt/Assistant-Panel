<script setup lang="ts">
import { ref, computed, inject, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { LazyStore } from "@tauri-apps/plugin-store";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { Unplug } from "lucide-vue-next";
import HotkeyInput from "@/components/HotkeyInput.vue";
import { CardHeaderKey } from "@/composables/cardHeader";
import { formatHotkey } from "@/composables/useHotkeyRecorder";
import { toast } from "vue-sonner";

interface PinnedWindow {
  hwnd: number;
  title: string;
  pinnedAt: string;
}

const enabled = ref(true);
const pinnedWindows = ref<PinnedWindow[]>([]);
const hotkey = ref("Control+Alt+Backquote");
const store = new LazyStore("settings.json");
let unlisten: UnlistenFn | null = null;

// Push the pinned-window count into the shared card title bar (no add action).
const count = computed(() => pinnedWindows.value.length);
const cardHeader = inject(CardHeaderKey, null);
cardHeader?.register({ count });

async function loadState() {
  try {
    const savedEnabled = await store.get<boolean>("windowPin.enabled");
    if (savedEnabled !== null && savedEnabled !== undefined) enabled.value = savedEnabled;
  } catch (_) {}
  try {
    const savedHotkey = await store.get<string>("windowPin.hotkey");
    if (savedHotkey) hotkey.value = savedHotkey;
  } catch (_) {}
  await refreshList();
}

async function refreshList() {
  try {
    pinnedWindows.value = await invoke<PinnedWindow[]>("list_pinned");
  } catch (_) {}
}

async function toggleEnabled(val: boolean) {
  enabled.value = val;
  await store.set("windowPin.enabled", val);
  await store.save();
  try {
    await invoke("set_pin_enabled", { enabled: val });
  } catch (_) {}
}

async function onHotkeyChange(val: string) {
  const previous = hotkey.value;
  if (val === previous) return;
  try {
    await invoke("update_hotkey", { hotkeyType: "windowPin", newHotkey: val });
  } catch (e) {
    toast.error("置顶快捷键设置失败", {
      description: String(e),
      position: "top-center",
    });
    return;
  }
  hotkey.value = val;
  await store.set("windowPin.hotkey", val);
  await store.save();
}

async function unpinWindow(hwnd: number) {
  try {
    await invoke("unpin_window", { hwnd });
    await refreshList();
  } catch (_) {}
}

async function unpinAll() {
  try {
    await invoke("unpin_all");
    pinnedWindows.value = [];
  } catch (_) {}
}

onMounted(async () => {
  await loadState();
  // Sync enabled state to Rust side
  try {
    await invoke("set_pin_enabled", { enabled: enabled.value });
  } catch (_) {}
  unlisten = await listen("window-pin:changed", () => {
    refreshList();
  });
});

onUnmounted(() => {
  unlisten?.();
});
</script>

<template>
  <div class="w-full space-y-1">
    <!-- Enable toggle -->
    <div class="flex items-center justify-between px-1.5 py-0.5">
      <span class="text-[10px] text-muted-foreground font-medium">启用</span>
      <Switch :model-value="enabled" @update:model-value="toggleEnabled" class="scale-75 origin-right" />
    </div>

    <template v-if="enabled">
      <!-- Pinned windows list -->
      <div v-if="pinnedWindows.length > 0" class="space-y-0.5">
        <div v-for="w in pinnedWindows" :key="w.hwnd"
          class="flex items-center justify-between px-1.5 py-0.5 bg-muted/30 rounded-sm">
          <span class="text-[9px] truncate flex-1">{{ w.title }}</span>
          <Button variant="ghost" size="icon" class="h-4 w-4 shrink-0 text-destructive"
            @click="unpinWindow(w.hwnd)">
            <Unplug class="h-2.5 w-2.5" />
          </Button>
        </div>
      </div>
      <div v-else class="px-1.5 py-0.5">
        <span class="text-[9px] text-muted-foreground">按 {{ formatHotkey(hotkey) }} 置顶当前窗口</span>
      </div>

      <!-- Hotkey setting -->
      <div class="flex items-center gap-1 px-1.5 py-0.5">
        <span class="text-[9px] text-muted-foreground whitespace-nowrap">快捷键</span>
        <HotkeyInput :model-value="hotkey" @update:model-value="onHotkeyChange" class="flex-1" />
      </div>

      <!-- Unpin all -->
      <div v-if="pinnedWindows.length > 0" class="px-1.5 pb-0.5">
        <Button variant="outline" size="sm" class="h-5 text-[9px] w-full" @click="unpinAll">
          全部取消置顶
        </Button>
      </div>
    </template>
  </div>
</template>
