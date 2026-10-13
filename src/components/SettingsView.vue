<script setup lang="ts">
import { ref, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import {
  disable as disableAutostart,
  enable as enableAutostart,
  isEnabled as isAutostartEnabled,
} from "@tauri-apps/plugin-autostart";
import { LazyStore } from "@tauri-apps/plugin-store";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Separator } from "@/components/ui/separator";
import { useTauriCommand } from "@/composables/useTauriCommand";
import HotkeyInput from "@/components/HotkeyInput.vue";
import { pluginRegistry } from "@/plugins/registry";
import { toast } from "vue-sonner";
import type { Snapshot } from "@/plugins/network-traffic/types";

interface Settings {
  hotkey: string;
  width: number;
  always_on_top: boolean;
  auto_start: boolean;
}

const { execute: getSettings } = useTauriCommand<{}, Settings>("get_settings");
const { execute: setSetting } = useTauriCommand<{ key: string; value: unknown }, void>("set_setting");

const localSettings = ref<Settings>({
  hotkey: "Control+Shift+Backquote",
  width: 420,
  always_on_top: true,
  auto_start: false,
});

const currentTheme = ref("light");

// Plugin enable/disable state (stored per-host in settings.json).
const DISABLED_KEY = "plugins.disabled";
const pluginStore = new LazyStore("settings.json");
const disabledPlugins = ref<string[]>([]);
const networkTrafficPending = ref(false);
// Plugins whose backend work only fully stops after a restart.
const RESTART_HINT_IDS = new Set(["api-dashboard", "process-manager", "window-pin"]);

function isPluginEnabled(id: string): boolean {
  return !disabledPlugins.value.includes(id);
}

async function togglePlugin(id: string, enabled: boolean) {
  if (id === "network-traffic") {
    await toggleNetworkTraffic(enabled);
    return;
  }
  const set = new Set(disabledPlugins.value);
  if (enabled) set.delete(id);
  else set.add(id);
  disabledPlugins.value = Array.from(set);
  await pluginStore.set(DISABLED_KEY, disabledPlugins.value);
  await pluginStore.save();
  // Refresh the main panel list immediately (UI). Backend stops on next launch.
  await emit("plugins:changed");
}

async function toggleNetworkTraffic(enabled: boolean) {
  if (networkTrafficPending.value) return;
  networkTrafficPending.value = true;
  const previousSetting = isPluginEnabled("network-traffic");
  let previousRuntime: boolean | undefined;
  let attempted = false;
  function updateSetting(value: boolean) {
    const set = new Set(disabledPlugins.value);
    if (value) set.delete("network-traffic");
    else set.add("network-traffic");
    disabledPlugins.value = Array.from(set);
  }
  try {
    const currentRuntime = await invoke<Snapshot>("get_network_traffic_snapshot");
    // A failed Store load may leave an empty shared cache. Do not save it over
    // the original settings, including through the rollback path below.
    if (currentRuntime.configError) throw new Error(currentRuntime.configError);
    previousRuntime = currentRuntime.enabled;
    attempted = true;
    const result = await invoke<Snapshot>("set_network_traffic_enabled", { enabled });
    if (result.enabled !== enabled) throw new Error("后端未确认监控状态");
    updateSetting(result.enabled);
    await pluginStore.set(DISABLED_KEY, disabledPlugins.value);
    await pluginStore.save();
    await emit("plugins:changed");
  } catch (e) {
    const rollbackErrors: string[] = [];
    if (attempted && previousRuntime !== undefined) {
      try {
        const restored = await invoke<Snapshot>("set_network_traffic_enabled", { enabled: previousRuntime });
        if (restored.enabled !== previousRuntime) throw new Error("后端未确认恢复状态");
      } catch (restoreError) { rollbackErrors.push("运行状态恢复失败：" + String(restoreError)); }
    }
    updateSetting(previousSetting);
    if (attempted) {
      try {
        await pluginStore.set(DISABLED_KEY, disabledPlugins.value);
        await pluginStore.save();
      } catch (restoreError) { rollbackErrors.push("设置恢复失败：" + String(restoreError)); }
      try { await emit("plugins:changed"); }
      catch (restoreError) { rollbackErrors.push("面板刷新失败：" + String(restoreError)); }
    }
    toast.error("网络流量设置失败", {
      description: [String(e), ...rollbackErrors].join("；"),
      position: "top-center",
    });
  } finally {
    networkTrafficPending.value = false;
  }
}

onMounted(async () => {
  const result = await getSettings();
  if (result) {
    localSettings.value = { ...result };
  }
  // The operating system is the source of truth for autostart registration.
  try {
    localSettings.value.auto_start = await isAutostartEnabled();
  } catch (_) {}
  try {
    const theme = await invoke<string>("read_theme");
    currentTheme.value = theme;
  } catch (_) {}
  try {
    const disabled = await pluginStore.get<string[]>(DISABLED_KEY);
    if (Array.isArray(disabled)) disabledPlugins.value = disabled;
  } catch (_) {}
});

async function onAlwaysOnTopChange(enabled: boolean) {
  const previous = localSettings.value.always_on_top;
  localSettings.value.always_on_top = enabled;
  try {
    await invoke("set_app_always_on_top", { enabled });
  } catch (_) {
    localSettings.value.always_on_top = previous;
  }
}

async function onAutoStartChange(enabled: boolean) {
  const previous = localSettings.value.auto_start;
  localSettings.value.auto_start = enabled;
  try {
    if (enabled) await enableAutostart();
    else await disableAutostart();
    // Confirm the system registration before committing the displayed value.
    const actual = await isAutostartEnabled();
    localSettings.value.auto_start = actual;
    await setSetting({ key: "panel.autoStart", value: actual });
  } catch (_) {
    localSettings.value.auto_start = previous;
  }
}

async function onWidthChange() {
  const w = Math.max(200, Math.min(2000, Number(localSettings.value.width) || 420));
  localSettings.value.width = w;
  await setSetting({ key: "panel.width", value: w });
  // Apply immediately so the panel resizes and stays docked to the right edge.
  try {
    await invoke("apply_panel_width", { width: w });
  } catch (_) {}
}

async function onPanelHotkeyChange(val: string) {
  const previous = localSettings.value.hotkey;
  if (val === previous) return;
  try {
    await invoke("update_hotkey", { hotkeyType: "panel", newHotkey: val });
  } catch (e) {
    toast.error("召唤快捷键设置失败", {
      description: String(e),
      position: "top-center",
    });
    return;
  }
  localSettings.value.hotkey = val;
  await setSetting({ key: "panel.hotkey", value: val });
}

async function setTheme(theme: string) {
  currentTheme.value = theme;
  const isDark = theme === "dark";
  try { await invoke("write_theme", { isDark }); } catch (_) {}
  if (isDark) {
    document.documentElement.classList.add("dark");
  } else {
    document.documentElement.classList.remove("dark");
  }
  try { await invoke("broadcast_theme", { isDark }); } catch (_) {}
}
</script>

<template>
  <div class="p-2 space-y-2">
    <Card>
      <CardHeader class="py-1 px-2">
        <CardTitle class="text-xs">面板</CardTitle>
      </CardHeader>
      <CardContent class="space-y-2 px-2 pb-2">
        <div class="space-y-1">
          <Label class="text-[10px]">召唤快捷键</Label>
          <HotkeyInput :model-value="localSettings.hotkey" @update:model-value="onPanelHotkeyChange" />
        </div>
        <div class="space-y-1">
          <Label class="text-[10px]">面板宽度 (px)</Label>
          <Input v-model.number="localSettings.width" type="number" class="h-7 text-xs"
            @change="onWidthChange" />
        </div>
        <Separator />
        <div class="flex items-center justify-between">
          <Label class="text-[10px]">始终置顶</Label>
          <Switch :model-value="localSettings.always_on_top" @update:model-value="onAlwaysOnTopChange" />
        </div>
        <div class="flex items-center justify-between">
          <Label class="text-[10px]">开机自启</Label>
          <Switch :model-value="localSettings.auto_start" @update:model-value="onAutoStartChange" />
        </div>
      </CardContent>
    </Card>

    <Card>
      <CardHeader class="py-1 px-2">
        <CardTitle class="text-xs">主题</CardTitle>
      </CardHeader>
      <CardContent class="px-2 pb-2">
        <div class="flex gap-1">
          <button v-for="t in [{id: 'light', label: '浅色'}, {id: 'dark', label: '深色'}]" :key="t.id"
            class="px-2 py-1 text-[10px] rounded-sm border transition-colors"
            :class="currentTheme === t.id ? 'bg-primary text-primary-foreground' : 'bg-background hover:bg-accent'"
            @click="setTheme(t.id)">
            {{ t.label }}
          </button>
        </div>
      </CardContent>
    </Card>

    <Card>
      <CardHeader class="py-1 px-2">
        <CardTitle class="text-xs">插件</CardTitle>
      </CardHeader>
      <CardContent class="space-y-2 px-2 pb-2">
        <div v-for="p in pluginRegistry" :key="p.id" class="flex items-center justify-between gap-2">
          <div class="min-w-0">
            <Label class="text-[10px]">{{ p.name }}</Label>
            <p v-if="RESTART_HINT_IDS.has(p.id)" class="text-[9px] text-muted-foreground">
              后端功能重启后完全停止
            </p>
          </div>
          <Switch
            :model-value="isPluginEnabled(p.id)"
            :disabled="p.id === 'network-traffic' && networkTrafficPending"
            @update:model-value="togglePlugin(p.id, $event)"
          />
        </div>
      </CardContent>
    </Card>
  </div>
</template>
