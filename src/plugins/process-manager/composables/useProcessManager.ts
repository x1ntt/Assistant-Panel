import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface OutputLine {
  id: string;
  stream: string; // "stdout" | "stderr"
  line: string;
  time: string;
}

export interface ManagedProcess {
  id: string;
  command: string;
  cmd: string;
  args: string[];
  workingDir: string;
  autoStart: boolean;
  autoRestart: boolean;
  pid: number | null;
  status: string; // "running" | "stopped" | "crashed"
  startedAt: string | null;
  // UI state
  editing?: boolean;
  expanded?: boolean;
}

const MAX_LINES = 1000;

// Module-level shared selection: which process's output the floating window
// should display. Set by the compact card, read by the floating view.
export const selectedOutputId = ref<string | null>(null);

export function useProcessManager() {
  const processes = ref<ManagedProcess[]>([]);
  const loadError = ref<string | null>(null);
  const loading = ref(true);
  const ready = ref(false);
  const busy = ref(false);
  const mutationError = ref("");
  const saveError = ref("");
  const pendingSnapshots = new Map<string, ManagedProcess>();
  let fetching = false;
  let statusRevision = 0;
  const processRevisions = new Map<string, number>();
  // Per-process output lines, keyed by process id.
  const outputs = ref<Record<string, OutputLine[]>>({});
  let unlisten: UnlistenFn | null = null;
  let unlistenOutput: UnlistenFn | null = null;

  function appendOutput(line: OutputLine) {
    const arr = outputs.value[line.id] ?? [];
    arr.push(line);
    if (arr.length > MAX_LINES) arr.splice(0, arr.length - MAX_LINES);
    outputs.value = { ...outputs.value, [line.id]: arr };
  }

  async function loadOutput(id: string) {
    try {
      const lines = await invoke<OutputLine[]>("get_process_output", { id });
      outputs.value = { ...outputs.value, [id]: lines };
    } catch (e) {
      console.error("加载进程输出失败:", e);
    }
  }

  async function clearOutput(id: string) {
    try {
      await invoke("clear_process_output", { id });
    } catch (_) {}
    outputs.value = { ...outputs.value, [id]: [] };
  }

  function getOutput(id: string): OutputLine[] {
    return outputs.value[id] ?? [];
  }

  async function loadProcesses() {
    if (fetching) return;
    fetching = true;
    loading.value = true;
    pendingSnapshots.clear();
    try {
      const list = await invoke<ManagedProcess[]>("list_processes");
      processes.value = list.map((p) => ({
        ...p,
        editing: false,
        expanded: false,
      }));
      for (const snapshot of pendingSnapshots.values()) upsertProcess(snapshot);
      loadError.value = null;
      ready.value = true;
      try {
        await invoke("report_process_view", { visibleCount: processes.value.length });
      } catch (error) {
        console.error("记录进程视图日志失败:", error);
      }
    } catch (e) {
      const detail = e instanceof Error ? e.message : String(e);
      loadError.value = `加载进程列表失败：${detail}`;
      ready.value = false;
      console.error(loadError.value);
    } finally {
      loading.value = false;
      fetching = false;
    }
  }

  function upsertProcess(snapshot: ManagedProcess) {
    const index = processes.value.findIndex(process => process.id === snapshot.id);
    if (index === -1) {
      processes.value.push({ ...snapshot, editing: false, expanded: false });
    } else {
      const { editing, expanded } = processes.value[index];
      processes.value[index] = { ...snapshot, editing, expanded };
    }
  }

  function applyResponse(snapshot: ManagedProcess, revision: number) {
    // A fast exit or another window's stop can arrive before the command
    // response. Keep that newer event instead of forcing status to running.
    if ((processRevisions.get(snapshot.id) ?? 0) <= revision) upsertProcess(snapshot);
  }

  async function mutate(action: () => Promise<void>): Promise<boolean> {
    if (!ready.value || loading.value || busy.value || saveError.value) return false;
    busy.value = true;
    mutationError.value = "";
    try {
      await action();
      return true;
    } catch (error) {
      const detail = String(error);
      if (detail.includes("进程配置未保存")) saveError.value = detail;
      else mutationError.value = detail;
      await loadProcesses();
      return false;
    } finally {
      busy.value = false;
    }
  }

  async function retrySave() {
    if (!ready.value || loading.value || busy.value) return;
    busy.value = true;
    try {
      await invoke("save_process_definitions");
      saveError.value = "";
    } catch (error) {
      saveError.value = String(error);
    } finally {
      busy.value = false;
    }
  }

  async function addProcess(
    command: string,
    workingDir: string,
    autoStart: boolean,
    autoRestart: boolean
  ) {
    return mutate(async () => {
      const revision = statusRevision;
      const entry = await invoke<ManagedProcess>("add_process", {
        command,
        workingDir,
        autoStart,
        autoRestart,
      });
      applyResponse(entry, revision);
    });
  }

  async function startProc(id: string) {
    return mutate(async () => {
      const revision = statusRevision;
      const entry = await invoke<ManagedProcess>("start_process_cmd", { id });
      applyResponse(entry, revision);
    });
  }

  async function stopProc(id: string) {
    return mutate(async () => {
      const revision = statusRevision;
      const entry = await invoke<ManagedProcess>("stop_process", { id });
      applyResponse(entry, revision);
    });
  }

  async function removeProc(id: string) {
    return mutate(async () => {
      await invoke("remove_process", { id });
      processes.value = processes.value.filter((p) => p.id !== id);
    });
  }

  async function updateProc(
    id: string,
    command: string,
    workingDir: string,
    autoStart: boolean,
    autoRestart: boolean
  ) {
    return mutate(async () => {
      const revision = statusRevision;
      const entry = await invoke<ManagedProcess>("update_process", {
        id,
        command,
        workingDir,
        autoStart,
        autoRestart,
      });
      applyResponse(entry, revision);
      const process = processes.value.find(process => process.id === id);
      if (process) process.editing = false;
    });
  }

  function toggleEdit(id: string) {
    const p = processes.value.find((p) => p.id === id);
    if (p) p.editing = !p.editing;
  }

  function toggleExpand(id: string) {
    const p = processes.value.find((p) => p.id === id);
    if (p) p.expanded = !p.expanded;
  }

  onMounted(async () => {
    try {
      unlisten = await listen<ManagedProcess>("process-manager:status", (event) => {
        const snapshot = event.payload;
        processRevisions.set(snapshot.id, ++statusRevision);
        if (fetching) pendingSnapshots.set(snapshot.id, snapshot);
        upsertProcess(snapshot);
      });
      unlistenOutput = await listen<OutputLine>("process-manager:output", (event) => {
        appendOutput(event.payload);
      });
    } catch (error) {
      console.error("订阅进程事件失败:", error);
    }
    await loadProcesses();
  });

  onUnmounted(() => {
    unlisten?.();
    unlistenOutput?.();
  });

  return {
    processes,
    loadError,
    loading, ready, busy, mutationError, saveError, retrySave,
    outputs,
    loadProcesses,
    addProcess,
    startProc,
    stopProc,
    removeProc,
    updateProc,
    toggleEdit,
    toggleExpand,
    getOutput,
    loadOutput,
    clearOutput,
  };
}
