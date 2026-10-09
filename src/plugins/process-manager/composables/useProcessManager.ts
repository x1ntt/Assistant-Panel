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
    try {
      const list = await invoke<ManagedProcess[]>("list_processes");
      processes.value = list.map((p) => ({
        ...p,
        editing: false,
        expanded: false,
      }));
      loadError.value = null;
    } catch (e) {
      const detail = e instanceof Error ? e.message : String(e);
      loadError.value = `加载进程列表失败：${detail}`;
      console.error(loadError.value);
    }
  }

  async function addProcess(
    command: string,
    workingDir: string,
    autoStart: boolean,
    autoRestart: boolean
  ) {
    const entry = await invoke<ManagedProcess>("add_process", {
      command,
      workingDir,
      autoStart,
      autoRestart,
    });
    processes.value.push({ ...entry, editing: false });
  }

  async function startProc(id: string) {
    await invoke("start_process_cmd", { id });
    const p = processes.value.find((p) => p.id === id);
    if (p) {
      p.status = "running";
    }
  }

  async function stopProc(id: string) {
    await invoke("stop_process", { id });
    const p = processes.value.find((p) => p.id === id);
    if (p) {
      p.status = "stopped";
      p.pid = null;
    }
  }

  async function removeProc(id: string) {
    try {
      await invoke("remove_process", { id });
    } catch (_) {}
    processes.value = processes.value.filter((p) => p.id !== id);
  }

  async function updateProc(
    id: string,
    command: string,
    workingDir: string,
    autoStart: boolean,
    autoRestart: boolean
  ) {
    const entry = await invoke<ManagedProcess>("update_process", {
      id,
      command,
      workingDir,
      autoStart,
      autoRestart,
    });
    const idx = processes.value.findIndex((p) => p.id === id);
    if (idx !== -1) {
      processes.value[idx] = { ...entry, editing: false };
    }
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
    // The backend finishes restoring managed processes while the desktop
    // window is being created. Retry transient IPC failures so a blank card
    // cannot become a permanent state after startup.
    for (let attempt = 0; attempt < 3; attempt += 1) {
      await loadProcesses();
      if (!loadError.value) break;
      await new Promise<void>((resolve) => window.setTimeout(resolve, 300 * (attempt + 1)));
    }

    unlisten = await listen<ManagedProcess>("process-manager:status", (event) => {
      const snapshot = event.payload;
      const idx = processes.value.findIndex((p) => p.id === snapshot.id);
      if (idx !== -1) {
        const { editing, expanded } = processes.value[idx];
        processes.value[idx] = { ...snapshot, editing, expanded };
      }
    });

    unlistenOutput = await listen<OutputLine>("process-manager:output", (event) => {
      appendOutput(event.payload);
    });
  });

  onUnmounted(() => {
    unlisten?.();
    unlistenOutput?.();
  });

  return {
    processes,
    loadError,
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
