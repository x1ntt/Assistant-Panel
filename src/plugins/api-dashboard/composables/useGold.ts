import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { LazyStore } from "@tauri-apps/plugin-store";

export interface MetalInfo {
  id: string;
  name: string;
  sellPrice: number | null;
  highPrice: number | null;
  lowPrice: number | null;
  unit: string;
}

export interface GoldSnapshot {
  ok: boolean;
  nodeName: string | null;
  metals: MetalInfo[];
  error: string | null;
}

export interface Threshold {
  max: number | null;
  min: number | null;
}

export type Thresholds = Record<string, Threshold>;

const STORE_KEY = "api-dashboard.thresholds";

// The two metals we track, in display order.
export const METAL_META = [
  { id: "gold9999", name: "黄金 9999" },
  { id: "londonGold", name: "伦敦金(现货)" },
] as const;

export function useGold() {
  const metals = ref<MetalInfo[]>([]);
  const nodeName = ref<string | null>(null);
  // stale = last fetch failed / returned bad data -> show values in pale yellow, keep old values.
  const stale = ref(false);
  const lastError = ref<string | null>(null);
  const thresholds = ref<Thresholds>({});
  let unlisten: UnlistenFn | null = null;

  const store = new LazyStore("settings.json");

  async function loadThresholds() {
    try {
      const saved = await store.get<Thresholds>(STORE_KEY);
      if (saved) thresholds.value = saved;
    } catch (_) {}
  }

  async function saveThresholds() {
    await store.set(STORE_KEY, thresholds.value);
    await store.save();
  }

  function getThreshold(id: string): Threshold {
    return thresholds.value[id] ?? { max: null, min: null };
  }

  function setThreshold(id: string, t: Threshold) {
    thresholds.value = { ...thresholds.value, [id]: t };
  }

  function applySnapshot(snap: GoldSnapshot) {
    if (snap.ok && snap.metals.length > 0) {
      metals.value = snap.metals;
      nodeName.value = snap.nodeName;
      stale.value = false;
      lastError.value = null;
    } else {
      // Keep previous values; just flag stale.
      stale.value = true;
      lastError.value = snap.error;
    }
  }

  onMounted(async () => {
    await loadThresholds();

    // Immediately pull the last cached snapshot so a freshly-opened window
    // (e.g. floating panel) shows data without waiting for the next poll.
    try {
      const snap = await invoke<GoldSnapshot | null>("get_gold_snapshot");
      if (snap) applySnapshot(snap);
    } catch (_) {}

    unlisten = await listen<GoldSnapshot>("gold:data", (event) => {
      applySnapshot(event.payload);
    });
  });

  onUnmounted(() => {
    unlisten?.();
  });

  return {
    metals,
    nodeName,
    stale,
    lastError,
    thresholds,
    getThreshold,
    setThreshold,
    saveThresholds,
    loadThresholds,
  };
}
