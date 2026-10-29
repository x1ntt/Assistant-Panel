import { computed, onMounted, onUnmounted, ref, shallowRef } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Snapshot } from "../types";

// Only a freshness clock, never a frontend sampler or a backend lifecycle owner.
const STALE_AFTER_MS = 10_000;

export function useNetworkTraffic() {
  const snapshot = shallowRef<Snapshot | null>(null);
  const loading = ref(true);
  const error = ref("");
  const operationError = ref("");
  const pending = ref(false);
  const now = ref(Date.now());
  const rows = computed(() => snapshot.value?.interfaces ?? []);
  const stale = computed(() => !!snapshot.value?.enabled && (
    !!error.value || !!snapshot.value.error || snapshot.value.updatedAt <= 0 ||
    now.value - snapshot.value.updatedAt > STALE_AFTER_MS
  ));
  const busy = computed(() => loading.value || pending.value || !snapshot.value);

  let disposed = false;
  let unlisten: UnlistenFn | null = null;
  let clock: ReturnType<typeof setInterval> | undefined;
  let connecting: Promise<void> | null = null;
  let mutations: Promise<unknown> = Promise.resolve();

  function applySnapshot(next: Snapshot) {
    if (disposed || (snapshot.value && next.revision < snapshot.value.revision)) return;
    snapshot.value = next;
    now.value = Date.now();
    error.value = "";
  }

  async function connectAndRefresh() {
    loading.value = true;
    error.value = "";
    try {
      // Subscribe before reading so a racing event cannot be overwritten by an older read.
      if (!unlisten) {
        const stop = await listen<Snapshot>("network-traffic:stats", event => {
          applySnapshot(event.payload);
        });
        if (disposed) { stop(); return; }
        unlisten = stop;
      }
      if (disposed) return;
      applySnapshot(await invoke<Snapshot>("get_network_traffic_snapshot"));
    } catch (e) {
      if (!disposed) error.value = String(e);
    } finally {
      if (!disposed) loading.value = false;
    }
  }

  async function retry() {
    if (disposed || pending.value) return;
    if (!connecting) {
      connecting = connectAndRefresh().finally(() => { connecting = null; });
    }
    await connecting;
  }

  async function mutate(command: string, args: Record<string, unknown>): Promise<boolean> {
    if (disposed || loading.value || !snapshot.value) return false;
    // Delayed single clicks on different rows may mature during another
    // request. Serialize them rather than silently dropping a busy mutation.
    const operation = mutations.then(async () => {
      if (disposed) return false;
      pending.value = true;
      operationError.value = "";
      try {
        const result = await invoke<Snapshot>(command, args);
        applySnapshot(result);
        return !disposed;
      } catch (e) {
        if (!disposed) operationError.value = String(e);
        return false;
      } finally {
        if (!disposed) pending.value = false;
      }
    });
    mutations = operation;
    return operation;
  }

  onMounted(() => {
    clock = setInterval(() => { now.value = Date.now(); }, 1000);
    void retry();
  });
  onUnmounted(() => {
    disposed = true;
    if (clock !== undefined) clearInterval(clock);
    unlisten?.();
    unlisten = null;
  });

  return {
    snapshot, rows, loading, error, operationError, pending, busy, stale,
    retry,
    pinInterface: (id: string) => mutate("pin_network_traffic_interface", { id }),
    unpinInterface: (id: string) => mutate("pin_network_traffic_interface", { id, pinned: false }),
    setHighlight: (id: string, highlighted: boolean) =>
      mutate("set_network_traffic_highlight", { id, highlighted }),
  };
}
