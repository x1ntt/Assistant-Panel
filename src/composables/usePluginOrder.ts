import { ref, onMounted } from "vue";
import { LazyStore } from "@tauri-apps/plugin-store";
import { listen } from "@tauri-apps/api/event";
import { pluginRegistry } from "@/plugins/registry";
import type { PanelPlugin } from "@/plugins/types";

const ORDER_KEY = "plugins.order";
const DISABLED_KEY = "plugins.disabled";

const orderedPlugins = ref<PanelPlugin[]>([]);
const store = new LazyStore("settings.json");
let listenerReady = false;

async function getDisabled(): Promise<string[]> {
  try {
    const disabled = await store.get<string[]>(DISABLED_KEY);
    return Array.isArray(disabled) ? disabled : [];
  } catch {
    return [];
  }
}

async function loadOrder() {
  const disabled = await getDisabled();
  const isActive = (p: PanelPlugin) => p.enabled && !disabled.includes(p.id);
  try {
    const savedOrder = await store.get<string[]>(ORDER_KEY);
    if (savedOrder && savedOrder.length > 0) {
      const active = pluginRegistry.filter(isActive);
      const sorted: PanelPlugin[] = [];
      // First: plugins in saved order
      for (const id of savedOrder) {
        const p = active.find((e) => e.id === id);
        if (p) sorted.push(p);
      }
      // Then: new plugins not in saved order
      for (const p of active) {
        if (!sorted.find((s) => s.id === p.id)) {
          sorted.push(p);
        }
      }
      orderedPlugins.value = sorted;
    } else {
      orderedPlugins.value = pluginRegistry
        .filter(isActive)
        .sort((a, b) => a.order - b.order);
    }
  } catch {
    orderedPlugins.value = pluginRegistry
      .filter(isActive)
      .sort((a, b) => a.order - b.order);
  }
}

async function saveOrder() {
  const ids = orderedPlugins.value.map((p) => p.id);
  await store.set(ORDER_KEY, ids);
  await store.save();
}

function movePlugin(fromIndex: number, toIndex: number) {
  const item = orderedPlugins.value.splice(fromIndex, 1)[0];
  orderedPlugins.value.splice(toIndex, 0, item);
  saveOrder();
}

export function usePluginOrder() {
  onMounted(async () => {
    await loadOrder();
    // Reload the list when the enabled/disabled set changes (from settings).
    if (!listenerReady) {
      listenerReady = true;
      await listen("plugins:changed", () => {
        loadOrder();
      });
    }
  });

  return { orderedPlugins, movePlugin };
}
