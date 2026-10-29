import { computed, ref, onMounted, onUnmounted } from "vue";
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
  stale: boolean;
  nodeName: string | null;
  metals: MetalInfo[];
  collectedAt: number | null;
  expiresAt: number | null;
  error: string | null;
}

export interface GoldConnection {
  endpoint: string;
  apiKey: string;
  clientId: string;
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
  const collectedAt = ref<number | null>(null);
  const expiresAt = ref<number | null>(null);
  const snapshotStale = ref(false);
  const snapshotError = ref<string | null>(null);
  const now = ref(Date.now());
  const expired = computed(() => expiresAt.value !== null && now.value / 1000 > expiresAt.value);
  const stale = computed(() => snapshotStale.value || expired.value);
  const lastError = computed(() => snapshotError.value ?? (expired.value ? "金价数据已过期，等待新的采集回报" : null));
  const updatedAtText = computed(() => {
    if (collectedAt.value === null) return metals.value.length ? "时间未知" : "暂无数据";
    const date = new Date(collectedAt.value * 1000);
    if (!Number.isFinite(date.getTime())) return "时间未知";
    return date.toLocaleString("zh-CN", {
      year: "numeric", month: "2-digit", day: "2-digit",
      hour: "2-digit", minute: "2-digit", second: "2-digit", hourCycle: "h23",
    });
  });
  const thresholds = ref<Thresholds>({});
  let unlisten: UnlistenFn | null = null;
  let clock: ReturnType<typeof setInterval> | null = null;
  let disposed = false;
  let eventRevision = 0;

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
    // The backend retains the last values on network failures and clears them
    // when changing Key/server, so all windows apply the same complete state.
    metals.value = snap.metals;
    nodeName.value = snap.nodeName;
    collectedAt.value = snap.collectedAt;
    expiresAt.value = snap.expiresAt;
    snapshotStale.value = !snap.ok || snap.stale;
    snapshotError.value = snap.error;
    now.value = Date.now();
  }

  onMounted(async () => {
    clock = setInterval(() => { now.value = Date.now(); }, 1000);
    try {
      // Subscribe first; never overwrite a newer event with an in-flight cache read.
      unlisten = await listen<GoldSnapshot>("gold:data", (event) => {
        if (disposed) return;
        eventRevision++;
        applySnapshot(event.payload);
      });
      if (disposed) { unlisten(); return; }
      const revision = eventRevision;
      const snap = await invoke<GoldSnapshot | null>("get_gold_snapshot");
      if (!disposed && revision === eventRevision && snap) applySnapshot(snap);
    } catch (_) {
      if (!disposed) {
        snapshotStale.value = true;
        snapshotError.value = "无法读取金价监控状态";
      }
    }
    if (!disposed) await loadThresholds();
  });

  onUnmounted(() => {
    disposed = true;
    if (clock) clearInterval(clock);
    unlisten?.();
  });

  return {
    metals,
    nodeName,
    collectedAt,
    updatedAtText,
    stale,
    lastError,
    thresholds,
    getThreshold,
    setThreshold,
    saveThresholds,
    loadThresholds,
  };
}
