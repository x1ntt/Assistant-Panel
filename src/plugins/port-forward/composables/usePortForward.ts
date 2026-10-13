import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { LazyStore } from "@tauri-apps/plugin-store";
import { formatBytes, formatSpeed } from "@/lib/network-format";

export interface ForwardRule {
  id: string;
  listenAddr: string;
  listenPort: number;
  targetAddr: string;
  targetPort: number;
  protocol: string;
  totalBytesIn: number;
  totalBytesOut: number;
  speedIn: number;
  speedOut: number;
  connectionCount: number;
  connections: ConnectionInfo[];
  running: boolean;
  expanded?: boolean;
  editing?: boolean;
  paused?: boolean;
}

export interface ConnectionInfo {
  clientAddr: string;
  bytesIn: number;
  bytesOut: number;
  speedIn: number;
  speedOut: number;
}

export interface InterfaceInfo {
  name: string;
  ip: string;
}

interface SavedRule {
  listenAddr: string;
  listenPort: number;
  targetAddr: string;
  targetPort: number;
  protocol: string;
  paused?: boolean;
}

const STORE_KEY = "port-forward.rules";

export function usePortForward() {
  const rules = ref<ForwardRule[]>([]);
  const interfaces = ref<InterfaceInfo[]>([]);
  let unlisten: UnlistenFn | null = null;
  const store = new LazyStore("settings.json");

  async function loadInterfaces() {
    try {
      interfaces.value = await invoke<InterfaceInfo[]>("get_interfaces");
    } catch (e) {
      console.error("获取接口失败:", e);
    }
  }

  async function saveRules() {
    const saved: SavedRule[] = rules.value.map(r => ({
      listenAddr: r.listenAddr, listenPort: r.listenPort,
      targetAddr: r.targetAddr, targetPort: r.targetPort,
      protocol: r.protocol, paused: r.paused,
    }));
    await store.set(STORE_KEY, saved);
    await store.save();
  }

  function ruleKey(r: { listenAddr: string; listenPort: number; targetAddr: string; targetPort: number; protocol: string }): string {
    return `${r.listenAddr}:${r.listenPort}->${r.targetAddr}:${r.targetPort}/${r.protocol}`;
  }

  async function restoreRules() {
    try {
      // First, pull rules that already exist in the Rust backend. On an F5 /
      // webview reload the backend process keeps running (ports still bound),
      // so we must reuse those live rules instead of re-adding them (which
      // would fail with "port in use" and make the entries disappear).
      let existing: ForwardRule[] = [];
      try {
        existing = await invoke<ForwardRule[]>("get_forward_rules");
      } catch (_) {}
      const existingByKey = new Map<string, ForwardRule>();
      for (const e of existing) existingByKey.set(ruleKey(e), e);

      const saved = await store.get<SavedRule[]>(STORE_KEY);
      if (saved && saved.length > 0) {
        for (const r of saved) {
          if (r.paused) {
            // Paused: add to list without starting
            rules.value.push({
              id: Math.random().toString(36).slice(2, 10),
              listenAddr: r.listenAddr, listenPort: r.listenPort,
              targetAddr: r.targetAddr, targetPort: r.targetPort,
              protocol: r.protocol,
              totalBytesIn: 0, totalBytesOut: 0,
              speedIn: 0, speedOut: 0,
              connectionCount: 0, connections: [],
              running: false, expanded: false, editing: false, paused: true,
            });
            continue;
          }

          const live = existingByKey.get(ruleKey(r));
          if (live) {
            // Backend already runs this rule (reload case) — reuse its snapshot.
            rules.value.push({ ...live, expanded: false, editing: false, paused: false });
            existingByKey.delete(ruleKey(r));
          } else {
            // Not running yet (cold start) — create it.
            try {
              const snapshot = await invoke<ForwardRule>("add_forward_rule", { rule: r });
              rules.value.push({ ...snapshot, expanded: false, editing: false, paused: false });
            } catch (e) {
              console.error("恢复规则失败:", r, e);
            }
          }
        }
      }

      // Any backend rule not present in the saved list (e.g. added then reloaded
      // before persistence) — surface it too, so nothing silently disappears.
      for (const live of existingByKey.values()) {
        rules.value.push({ ...live, expanded: false, editing: false, paused: false });
      }
    } catch (_) {}
  }

  async function addRule(rule: SavedRule) {
    const snapshot = await invoke<ForwardRule>("add_forward_rule", { rule });
    rules.value.push({ ...snapshot, expanded: false, editing: false, paused: false });
    await saveRules();
  }

  async function removeRule(id: string) {
    const rule = rules.value.find(r => r.id === id);
    if (rule && !rule.paused) {
      try { await invoke("remove_forward_rule", { id }); } catch (_) {}
    }
    rules.value = rules.value.filter((r) => r.id !== id);
    await saveRules();
  }

  async function pauseRule(id: string) {
    const rule = rules.value.find(r => r.id === id);
    if (!rule) return;
    try {
      await invoke("pause_forward", { id });
    } catch (_) {}
    rule.paused = true;
    rule.running = false;
    rule.speedIn = 0;
    rule.speedOut = 0;
    rule.connectionCount = 0;
    rule.connections = [];
    await saveRules();
  }

  async function resumeRule(id: string) {
    const rule = rules.value.find(r => r.id === id);
    if (!rule) return;
    try {
      const snapshot = await invoke<ForwardRule>("resume_forward", {
        rule: {
          listenAddr: rule.listenAddr, listenPort: rule.listenPort,
          targetAddr: rule.targetAddr, targetPort: rule.targetPort,
          protocol: rule.protocol,
        },
      });
      // Replace old entry with new one (new ID from Rust)
      const idx = rules.value.findIndex(r => r.id === id);
      if (idx !== -1) {
        rules.value[idx] = { ...snapshot, expanded: rule.expanded, editing: false, paused: false };
      }
    } catch (e) {
      console.error("恢复规则失败:", e);
    }
    await saveRules();
  }

  async function disconnect(ruleId: string, clientAddr: string) {
    try {
      await invoke("disconnect_rule_connection", { ruleId, clientAddr });
      const rule = rules.value.find((r) => r.id === ruleId);
      if (rule) {
        rule.connections = rule.connections.filter(c => c.clientAddr !== clientAddr);
        rule.connectionCount = rule.connections.length;
      }
    } catch (e) {
      console.error("断开连接失败:", e);
    }
  }

  function toggleExpand(id: string) {
    const rule = rules.value.find((r) => r.id === id);
    if (rule) rule.expanded = !rule.expanded;
  }

  function toggleEdit(id: string) {
    const rule = rules.value.find((r) => r.id === id);
    if (rule) rule.editing = !rule.editing;
  }

  onMounted(async () => {
    await loadInterfaces();
    await restoreRules();

    unlisten = await listen<ForwardRule>("port-forward:stats", (event) => {
      const snapshot = event.payload;
      const idx = rules.value.findIndex((r) => r.id === snapshot.id);
      if (idx !== -1) {
        const { expanded, editing, paused } = rules.value[idx];
        rules.value[idx] = { ...snapshot, expanded, editing, paused };
      }
    });
  });

  onUnmounted(() => {
    unlisten?.();
  });

  return {
    rules, interfaces,
    addRule, removeRule, disconnect,
    pauseRule, resumeRule,
    toggleExpand, toggleEdit,
    formatBytes, formatSpeed,
    loadInterfaces,
  };
}
