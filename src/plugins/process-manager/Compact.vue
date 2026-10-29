<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, inject } from "vue";
import { Button } from "@/components/ui/button";
import { TooltipProvider } from "@/components/ui/tooltip";
import CellHint from "./CellHint.vue";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Switch } from "@/components/ui/switch";
import {
  Trash2, Pencil, Check, X, Play, Square,
  ChevronDown, ChevronRight,
} from "lucide-vue-next";
import { useProcessManager, type ManagedProcess, selectedOutputId } from "./composables/useProcessManager";
import { usePanelStore } from "@/stores/panel";
import { CardHeaderKey } from "@/composables/cardHeader";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";

const panel = usePanelStore();

const {
  processes, addProcess, startProc, stopProc,
  removeProc, updateProc, toggleEdit, toggleExpand,
  getOutput, loadOutput, loadProcesses, loadError,
  loading, ready, busy, mutationError, saveError, retrySave,
} = useProcessManager();
const canModify = computed(() => ready.value && !loading.value && !busy.value && !saveError.value);

// Push count + add action into the shared card title bar.
const count = computed(() => processes.value.length);
const cardHeader = inject(CardHeaderKey, null);
cardHeader?.register({ count, onAdd: () => openAddRow() });

// Open the floating window showing the full output for a given process.
// selectedOutputId lives per-webview, so we ALSO emit a cross-window event
// carrying the process id (main and floating are separate webviews and do not
// share JS module state).
const lastOutputId = ref<string | null>(null);
async function openOutputFloating(id: string) {
  lastOutputId.value = id;
  selectedOutputId.value = id;
  await emit("process-manager:show-output", id);
  await panel.ensureFloatingPanel("process-manager");
}

// The floating view asks which process to show when it (re)mounts.
let unlistenReq: UnlistenFn | null = null;
onMounted(async () => {
  unlistenReq = await listen("process-manager:request-output", async () => {
    if (lastOutputId.value) {
      await emit("process-manager:show-output", lastOutputId.value);
    }
  });
});
onUnmounted(() => {
  unlistenReq?.();
});

// Expand a row and lazily load its buffered output.
async function onToggleExpand(p: ManagedProcess) {
  const willExpand = !p.expanded;
  toggleExpand(p.id);
  if (willExpand) await loadOutput(p.id);
}

// Last N output lines shown inline in the compact card.
function tailOutput(id: string, n = 10) {
  const arr = getOutput(id);
  return arr.slice(Math.max(0, arr.length - n));
}

// Ticking clock used to compute live uptime from startedAt.
const now = ref(Date.now());
let clock: ReturnType<typeof setInterval> | null = null;
onMounted(() => {
  clock = setInterval(() => (now.value = Date.now()), 1000);
});
onUnmounted(() => {
  if (clock) clearInterval(clock);
});

// Derive the process (executable) name from cmd / command.
function processName(p: ManagedProcess): string {
  const raw = (p.cmd || p.command || "").trim();
  if (!raw) return "-";
  // Strip path and quotes, keep the executable's base name.
  const first = raw.replace(/^["']|["']$/g, "").split(/[\\/]/).pop() || raw;
  return first;
}

// Human-readable uptime since startedAt (only meaningful while running).
function uptime(p: ManagedProcess): string {
  if (p.status !== "running" || !p.startedAt) return "-";
  const start = new Date(p.startedAt.replace(/-/g, "/")).getTime();
  if (Number.isNaN(start)) return "-";
  let sec = Math.max(0, Math.floor((now.value - start) / 1000));
  const d = Math.floor(sec / 86400); sec -= d * 86400;
  const h = Math.floor(sec / 3600); sec -= h * 3600;
  const m = Math.floor(sec / 60); sec -= m * 60;
  const parts: string[] = [];
  if (d) parts.push(`${d}天`);
  if (h || d) parts.push(`${h}时`);
  if (m || h || d) parts.push(`${m}分`);
  parts.push(`${sec}秒`);
  return parts.join("");
}

// Add row
const showAddRow = ref(false);
const newProc = ref({
  command: "", workingDir: "", autoStart: false, autoRestart: false,
});
const addError = ref("");

function openAddRow() {
  if (!canModify.value) return;
  showAddRow.value = true;
  newProc.value = { command: "", workingDir: "", autoStart: false, autoRestart: false };
  addError.value = "";
}

function cancelAdd() {
  showAddRow.value = false;
  addError.value = "";
}

async function confirmAdd() {
  addError.value = "";
  if (!newProc.value.command.trim()) {
    addError.value = "请填写命令";
    return;
  }
  try {
    const added = await addProcess(
      newProc.value.command,
      newProc.value.workingDir,
      newProc.value.autoStart,
      newProc.value.autoRestart,
    );
    if (added || saveError.value) showAddRow.value = false;
  } catch (e) {
    addError.value = String(e);
  }
}

// Edit
const editData = ref({
  command: "", workingDir: "", autoStart: false, autoRestart: false,
});

function startEdit(p: any) {
  editData.value = {
    command: p.command,
    workingDir: p.workingDir,
    autoStart: p.autoStart,
    autoRestart: p.autoRestart,
  };
  toggleEdit(p.id);
}

async function confirmEdit(p: any) {
  try {
    await updateProc(
      p.id,
      editData.value.command,
      editData.value.workingDir,
      editData.value.autoStart,
      editData.value.autoRestart,
    );
  } catch (_) {}
}

function cancelEdit(p: any) {
  toggleEdit(p.id);
}

function statusVariant(status: string) {
  return status === "running"
    ? "default"
    : status === "crashed"
      ? "destructive"
      : "secondary";
}

function statusLabel(status: string) {
  return status === "running"
    ? "运行中"
    : status === "crashed"
      ? "已崩溃"
      : "已停止";
}
</script>

<template>
  <TooltipProvider :delay-duration="350">
  <div class="w-full overflow-hidden">
    <div v-if="loading" class="py-1 text-[9px] text-muted-foreground" role="status">正在加载进程定义…</div>
    <div v-if="loadError" class="py-1 text-[9px] text-destructive" role="alert">
      {{ loadError }}
      <Button variant="ghost" class="h-5 px-1 text-[9px]" :disabled="loading" @click="loadProcesses">重试加载</Button>
    </div>
    <div v-if="saveError" class="py-1 text-[9px] text-destructive" role="alert">
      {{ saveError }}
      <Button variant="ghost" class="h-5 px-1 text-[9px]" :disabled="busy" @click="retrySave">重试保存</Button>
    </div>
    <div v-if="mutationError" class="py-1 text-[9px] text-destructive" role="alert">{{ mutationError }}</div>
    <fieldset :disabled="!canModify" class="min-w-0 border-0 p-0 m-0">
    <div class="w-full min-w-0">
      <table class="w-full table-fixed text-[9px] border-collapse">
        <colgroup>
          <col />
          <col />
          <col class="w-9" />
          <col class="w-9" />
          <col class="w-16" />
        </colgroup>
        <thead>
          <tr class="border-b border-border text-muted-foreground">
            <CellHint text="命令"><th scope="col" class="truncate text-left px-1 py-0.5 font-medium">命令</th></CellHint>
            <CellHint text="工作目录"><th scope="col" class="truncate text-left px-1 py-0.5 font-medium">工作目录</th></CellHint>
            <CellHint text="状态"><th scope="col" class="truncate text-center px-1 py-0.5 font-medium">状态</th></CellHint>
            <CellHint text="开机启动"><th scope="col" class="truncate text-center px-1 py-0.5 font-medium">开机启动</th></CellHint>
            <CellHint text="操作"><th scope="col" class="sticky right-0 z-10 bg-background text-center px-1 py-0.5 font-medium">操作</th></CellHint>
          </tr>
        </thead>
        <tbody>
          <template v-for="p in processes" :key="p.id">
            <!-- Normal row -->
            <tr v-if="!p.editing" class="border-b border-border/50 hover:bg-accent/30">
              <CellHint :text="p.command">
              <td class="overflow-hidden px-1 py-0.5 font-mono">
                <button
                  class="flex w-full min-w-0 items-center gap-0.5 overflow-hidden hover:text-primary"
                  :aria-label="(p.expanded ? '收起' : '展开') + '进程：' + p.command"
                  @click="onToggleExpand(p)"
                >
                  <ChevronDown v-if="p.expanded" class="h-2.5 w-2.5 shrink-0" />
                  <ChevronRight v-else class="h-2.5 w-2.5 shrink-0" />
                  <span class="min-w-0 truncate">{{ p.command }}</span>
                </button>
              </td>
              </CellHint>
              <CellHint :text="p.workingDir || '.'">
                <td class="px-1 py-0.5 text-muted-foreground truncate">{{ p.workingDir || "." }}</td>
              </CellHint>
              <CellHint :text="statusLabel(p.status)">
              <td class="px-1 py-0.5 text-center">
                <Badge :variant="statusVariant(p.status)" class="text-[8px] px-0.5 py-0 h-3">
                  {{ statusLabel(p.status) }}
                </Badge>
              </td>
              </CellHint>
              <CellHint :text="'开机启动：' + (p.autoStart ? '是' : '否')">
              <td class="px-1 py-0.5 text-center">
                <span class="text-[8px]" :class="p.autoStart ? 'text-primary' : 'text-muted-foreground'">
                  {{ p.autoStart ? "是" : "否" }}
                </span>
              </td>
              </CellHint>
              <CellHint :text="(p.status === 'running' ? '停止进程' : '启动进程') + ' · 编辑进程 · 删除进程'">
              <td class="sticky right-0 z-10 bg-background px-1 py-0.5 text-center">
                <div class="flex flex-nowrap items-center justify-center gap-0.5">
                  <Button v-if="p.status === 'running'" variant="ghost" size="icon"
                    class="h-4 w-4 shrink-0 text-destructive" title="停止进程" aria-label="停止进程" @click="stopProc(p.id)">
                    <Square class="h-2.5 w-2.5" />
                  </Button>
                  <Button v-else variant="ghost" size="icon"
                    class="h-4 w-4 shrink-0 text-primary" title="启动进程" aria-label="启动进程" @click="startProc(p.id)">
                    <Play class="h-2.5 w-2.5" />
                  </Button>
                  <Button variant="ghost" size="icon" class="h-4 w-4 shrink-0" title="编辑进程" aria-label="编辑进程" @click="startEdit(p)">
                    <Pencil class="h-2.5 w-2.5" />
                  </Button>
                  <Button variant="ghost" size="icon" class="h-4 w-4 shrink-0 text-destructive" title="删除进程" aria-label="删除进程"
                    @click="removeProc(p.id)">
                    <Trash2 class="h-2.5 w-2.5" />
                  </Button>
                </div>
              </td>
              </CellHint>
            </tr>

            <!-- Expanded detail row -->
            <tr v-if="!p.editing && p.expanded" class="border-b border-border/50 bg-muted/20">
              <td colspan="5" class="px-2 py-1">
                <div class="grid grid-cols-2 gap-x-3 gap-y-0.5 text-[9px]">
                  <div class="flex justify-between gap-2">
                    <span class="text-muted-foreground">PID</span>
                    <span class="font-mono">{{ p.pid ?? "-" }}</span>
                  </div>
                  <div class="flex justify-between gap-2">
                    <span class="text-muted-foreground">进程名</span>
                    <span class="font-mono truncate">{{ processName(p) }}</span>
                  </div>
                  <div class="flex justify-between gap-2">
                    <span class="text-muted-foreground">运行时间</span>
                    <span class="font-mono tabular-nums">{{ uptime(p) }}</span>
                  </div>
                  <div class="flex justify-between gap-2">
                    <span class="text-muted-foreground">启动于</span>
                    <span class="font-mono truncate">{{ p.startedAt ?? "-" }}</span>
                  </div>
                  <div class="flex justify-between gap-2">
                    <span class="text-muted-foreground">自动重启</span>
                    <span>{{ p.autoRestart ? "是" : "否" }}</span>
                  </div>
                  <div class="flex justify-between gap-2">
                    <span class="text-muted-foreground">参数</span>
                    <span class="font-mono truncate" :title="p.args.join(' ')">
                      {{ p.args.length ? p.args.join(" ") : "-" }}
                    </span>
                  </div>
                  <div class="col-span-2 flex justify-between gap-2">
                    <span class="text-muted-foreground shrink-0">工作目录</span>
                    <span class="font-mono truncate" :title="p.workingDir || '.'">{{ p.workingDir || "." }}</span>
                  </div>
                </div>

                <!-- Inline output: last 10 lines (white bg, black text, no scrollbar) -->
                <div class="mt-1 border-t border-border/50 pt-1">
                  <div class="flex items-center justify-between mb-0.5">
                    <span class="text-[8px] text-muted-foreground">输出 (最新10行)</span>
                    <button
                      class="text-[8px] text-muted-foreground hover:text-primary"
                      @click="openOutputFloating(p.id)"
                    >
                      查看全部 ↗
                    </button>
                  </div>
                  <div
                    v-if="tailOutput(p.id).length > 0"
                    class="font-mono text-[9px] leading-[1.35] bg-background text-foreground border border-border rounded-sm px-1.5 py-1 overflow-hidden whitespace-pre-wrap break-all"
                  >
                    <div
                      v-for="(l, i) in tailOutput(p.id)"
                      :key="i"
                      :class="l.stream === 'stderr' ? 'text-destructive' : ''"
                    >{{ l.line || " " }}</div>
                  </div>
                  <div v-else class="text-[8px] text-muted-foreground px-1 py-0.5">暂无输出</div>
                </div>
              </td>
            </tr>

            <!-- Edit row -->
            <tr v-if="p.editing" class="border-b border-primary/30 bg-primary/5">
              <td colspan="4" class="overflow-hidden px-0.5 py-0.5">
                <div class="flex flex-col gap-1">
                  <Input v-model="editData.command" class="h-5 text-[9px] px-1"
                    placeholder="命令 (如: python node.py)" />
                  <div class="flex gap-1 items-center">
                    <Input v-model="editData.workingDir" class="h-5 text-[9px] px-1 flex-1"
                      placeholder="工作目录" />
                    <div class="flex items-center gap-0.5">
                      <span class="text-[8px] text-muted-foreground">自启</span>
                      <Switch :model-value="editData.autoStart"
                        @update:model-value="editData.autoStart = $event" class="scale-60 origin-left" />
                    </div>
                    <div class="flex items-center gap-0.5">
                      <span class="text-[8px] text-muted-foreground">重启</span>
                      <Switch :model-value="editData.autoRestart"
                        @update:model-value="editData.autoRestart = $event" class="scale-60 origin-left" />
                    </div>
                  </div>
                </div>
              </td>
              <td class="sticky right-0 z-10 bg-background px-0.5 py-0.5 text-center">
                <div class="flex items-center justify-center gap-0.5">
                  <Button variant="ghost" size="icon" class="h-4 w-4 text-primary"
                    title="保存修改" aria-label="保存修改" @click="confirmEdit(p)">
                    <Check class="h-2.5 w-2.5" />
                  </Button>
                  <Button variant="ghost" size="icon" class="h-4 w-4"
                    title="取消修改" aria-label="取消修改" @click="cancelEdit(p)">
                    <X class="h-2.5 w-2.5" />
                  </Button>
                </div>
              </td>
            </tr>
          </template>

          <!-- Add row -->
          <tr v-if="showAddRow" class="border-b border-primary/30 bg-primary/5">
            <td colspan="4" class="overflow-hidden px-0.5 py-0.5">
              <div class="flex flex-col gap-1">
                <Input v-model="newProc.command" class="h-5 text-[9px] px-1"
                  placeholder="命令 (如: python node.py)" />
                <div class="flex gap-1 items-center">
                  <Input v-model="newProc.workingDir" class="h-5 text-[9px] px-1 flex-1"
                    placeholder="工作目录 (可选)" />
                  <div class="flex items-center gap-0.5">
                    <span class="text-[8px] text-muted-foreground">自启</span>
                    <Switch :model-value="newProc.autoStart"
                      @update:model-value="newProc.autoStart = $event" class="scale-60 origin-left" />
                  </div>
                  <div class="flex items-center gap-0.5">
                    <span class="text-[8px] text-muted-foreground">重启</span>
                    <Switch :model-value="newProc.autoRestart"
                      @update:model-value="newProc.autoRestart = $event" class="scale-60 origin-left" />
                  </div>
                </div>
              </div>
            </td>
            <td class="sticky right-0 z-10 bg-background px-0.5 py-0.5 text-center">
              <div class="flex items-center justify-center gap-0.5">
                <Button variant="ghost" size="icon" class="h-4 w-4 shrink-0 text-primary" title="添加进程" aria-label="添加进程" @click="confirmAdd()">
                  <Check class="h-2.5 w-2.5" />
                </Button>
                <Button variant="ghost" size="icon" class="h-4 w-4 shrink-0" title="取消添加" aria-label="取消添加" @click="cancelAdd()">
                  <X class="h-2.5 w-2.5" />
                </Button>
              </div>
            </td>
          </tr>

          <tr v-if="addError" class="bg-destructive/10">
            <td colspan="5" class="px-1 py-0.5 text-destructive text-[8px] text-center">{{ addError }}</td>
          </tr>
          <tr v-if="ready && processes.length === 0 && !showAddRow">
            <td colspan="5" class="px-1 py-2 text-center text-muted-foreground text-[9px]">
              暂无进程，点击 + 添加
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    </fieldset>
  </div>
  </TooltipProvider>
</template>
