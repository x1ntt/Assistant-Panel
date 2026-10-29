<script setup lang="ts">
import { inject, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { LazyStore } from "@tauri-apps/plugin-store";
import { Switch } from "@/components/ui/switch";
import HotkeyInput from "@/components/HotkeyInput.vue";
import { CardHeaderKey } from "@/composables/cardHeader";
import { toast } from "vue-sonner";

const enabled = ref(true);
const hotkey = ref("Control+Alt+Backquote");
const store = new LazyStore("settings.json");
inject(CardHeaderKey, null)?.register({});

async function toggleEnabled(value: boolean) {
  enabled.value = value;
  await store.set("windowPin.enabled", value);
  await store.save();
  try { await invoke("set_pin_enabled", { enabled: value }); } catch (_) {}
}

async function onHotkeyChange(value: string) {
  if (value === hotkey.value) return;
  try {
    await invoke("update_hotkey", { hotkeyType: "windowPin", newHotkey: value });
  } catch (error) {
    toast.error("置顶快捷键设置失败", { description: String(error), position: "top-center" });
    return;
  }
  hotkey.value = value;
  await store.set("windowPin.hotkey", value);
  await store.save();
}

onMounted(async () => {
  try {
    const saved = await store.get<boolean>("windowPin.enabled");
    if (saved !== null && saved !== undefined) enabled.value = saved;
  } catch (_) {}
  try {
    const saved = await store.get<string>("windowPin.hotkey");
    if (saved) hotkey.value = saved;
  } catch (_) {}
  try { await invoke("set_pin_enabled", { enabled: enabled.value }); } catch (_) {}
});
</script>

<template>
  <div class="flex w-full min-w-0 items-center gap-2 px-1 py-0.5">
    <label class="flex shrink-0 items-center gap-1">
      <span class="text-[9px] text-muted-foreground">启用</span>
      <Switch :model-value="enabled" aria-label="启用窗口置顶" @update:model-value="toggleEnabled"
        class="scale-75" />
    </label>
    <span class="shrink-0 text-[9px] text-muted-foreground">快捷键</span>
    <HotkeyInput :model-value="hotkey" aria-label="设置窗口置顶快捷键"
      class="min-w-0 flex-1" @update:model-value="onHotkeyChange" />
  </div>
</template>
