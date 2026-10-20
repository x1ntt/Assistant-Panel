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
  error?: string;
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
  const loading = ref(true);
  const ready = ref(false);
  const busy = ref(false);
  const loadError = ref("");
  const saveError = ref("");
  const operationError = ref("");
  let unlisten: UnlistenFn | null = null;
  let restoring = false;
  const store = new LazyStore("settings.json");

  async function loadInterfaces() {
    try {
      interfaces.value = await invoke<InterfaceInfo[]>("get_interfaces");
    } catch (e) {
      console.error("获取接口失败:", e);
    }
  }

  async function saveRules(): Promise<boolean> {
    try {
      await invoke<SavedRule[]>("get_saved_forward_rules");
      const saved: SavedRule[] = rules.value.map(rule => ({
        listenAddr: rule.listenAddr, listenPort: rule.listenPort,
        targetAddr: rule.targetAddr, targetPort: rule.targetPort,
        protocol: rule.protocol, paused: rule.paused,
      }));
      await store.set(STORE_KEY, saved);
      await store.save();
      saveError.value = "";
      return true;
    } catch (error) {
      saveError.value = `规则未保存，重启可能丢失本次更改: ${String(error)}`;
      return false;
    }
  }

  function ruleKey(r: { listenAddr: string; listenPort: number; targetAddr: string; targetPort: number; protocol: string }): string {
    return `${r.listenAddr}:${r.listenPort}->${r.targetAddr}:${r.targetPort}/${r.protocol}`;
  }

  function inactiveRule(saved: SavedRule): ForwardRule {
    return {
      ...saved, id: crypto.randomUUID(), paused: saved.paused ?? false,
      totalBytesIn: 0, totalBytesOut: 0, speedIn: 0, speedOut: 0,
      connectionCount: 0, connections: [], running: false,
      expanded: false, editing: false,
    };
  }

  async function restoreRules() {
    if (busy.value || ready.value || restoring) return;
    restoring = true;
    loading.value = true;
    loadError.value = "";
    try {
      const saved = await invoke<SavedRule[]>("get_saved_forward_rules");
      const existing = await invoke<ForwardRule[]>("get_forward_rules");
      const existingByKey = new Map<string, ForwardRule>();
      for (const snapshot of existing) existingByKey.set(ruleKey(snapshot), snapshot);
      rules.value = saved.map(inactiveRule);
      for (let index = 0; index < rules.value.length; index++) {
        const rule = rules.value[index];
        const live = existingByKey.get(ruleKey(rule));
        if (live) {
          rules.value[index] = { ...live, paused: false, expanded: false, editing: false };
          existingByKey.delete(ruleKey(rule));
        } else if (!rule.paused) {
          try {
            const snapshot = await invoke<ForwardRule>("add_forward_rule", { rule: saved[index] });
            rules.value[index] = { ...snapshot, paused: false, expanded: false, editing: false };
          } catch (error) {
            rule.error = String(error);
          }
        }
      }
      for (const live of existingByKey.values()) {
        rules.value.push({ ...live, expanded: false, editing: false, paused: false });
      }
      if (existingByKey.size > 0) saveError.value = "检测到未保存的运行规则，请重试保存";
      ready.value = true;
    } catch (error) {
      loadError.value = `规则加载失败，已禁止修改配置: ${String(error)}`;
    } finally {
      loading.value = false;
      restoring = false;
    }
  }

  async function mutate(action: () => Promise<void>): Promise<boolean> {
    if (!ready.value || loading.value || busy.value || saveError.value) return false;
    busy.value = true;
    operationError.value = "";
    try {
      await action();
      await saveRules();
      return true;
    } catch (error) {
      operationError.value = String(error);
      return false;
    } finally {
      busy.value = false;
    }
  }

  async function retrySave() {
    if (!ready.value || loading.value || busy.value) return;
    busy.value = true;
    try {
      await saveRules();
    } finally {
      busy.value = false;
    }
  }

  async function addRule(rule: SavedRule) {
    return mutate(async () => {
      const snapshot = await invoke<ForwardRule>("add_forward_rule", { rule });
      rules.value.push({ ...snapshot, expanded: false, editing: false, paused: false });
    });
  }

  async function removeRule(id: string) {
    return mutate(async () => {
      const rule = rules.value.find(rule => rule.id === id);
      if (rule?.running) await invoke("remove_forward_rule", { id });
      rules.value = rules.value.filter(rule => rule.id !== id);
    });
  }

  async function pauseRule(id: string) {
    return mutate(async () => {
      const rule = rules.value.find(rule => rule.id === id);
      if (!rule) return;
      if (rule.running) await invoke("pause_forward", { id });
      rule.paused = true;
      rule.running = false;
      rule.speedIn = 0;
      rule.speedOut = 0;
      rule.connectionCount = 0;
      rule.connections = [];
      rule.error = undefined;
    });
  }

  async function resumeRule(id: string) {
    return mutate(async () => {
      const rule = rules.value.find(rule => rule.id === id);
      if (!rule || rule.running) return;
      try {
        const snapshot = await invoke<ForwardRule>("resume_forward", {
          rule: {
            listenAddr: rule.listenAddr, listenPort: rule.listenPort,
            targetAddr: rule.targetAddr, targetPort: rule.targetPort,
            protocol: rule.protocol,
          },
        });
        const index = rules.value.findIndex(rule => rule.id === id);
        if (index !== -1) {
          rules.value[index] = { ...snapshot, expanded: rule.expanded, editing: false, paused: false };
        }
      } catch (error) {
        rule.error = String(error);
        throw error;
      }
    });
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
    void loadInterfaces();
    try {
      unlisten = await listen<ForwardRule>("port-forward:stats", (event) => {
        const snapshot = event.payload;
        const index = rules.value.findIndex(rule => rule.id === snapshot.id);
        if (index !== -1) {
          const { expanded, editing, paused, running } = rules.value[index];
          if (!running) return;
          Object.assign(rules.value[index], snapshot, { expanded, editing, paused });
        }
      });
    } catch (error) {
      console.error("订阅转发统计失败:", error);
    }
    await restoreRules();
  });

  onUnmounted(() => {
    unlisten?.();
  });

  return {
    rules, interfaces,
    loading, ready, busy, loadError, saveError, operationError,
    restoreRules, retrySave,
    addRule, removeRule, disconnect,
    pauseRule, resumeRule,
    toggleExpand, toggleEdit,
    formatBytes, formatSpeed,
    loadInterfaces,
  };
}
