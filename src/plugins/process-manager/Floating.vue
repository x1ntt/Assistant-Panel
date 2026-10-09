<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted, nextTick } from "vue";
import { toast } from "vue-sonner";
import { listen, emit, type UnlistenFn } from "@tauri-apps/api/event";
import { Button } from "@/components/ui/button";
import { Activity, Copy, Trash2, ArrowDownToLine } from "lucide-vue-next";
import {
  useProcessManager,
  selectedOutputId,
} from "./composables/useProcessManager";

const {
  processes, outputs, loadProcesses, loadOutput, clearOutput, getOutput,
} = useProcessManager();

const selectedId = ref<string | null>(selectedOutputId.value);
const scrollRef = ref<HTMLElement | null>(null);
const autoScroll = ref(true);
let unlistenShow: UnlistenFn | null = null;

const current = computed(() =>
  processes.value.find((p) => p.id === selectedId.value)
);

const lines = computed(() => (selectedId.value ? getOutput(selectedId.value) : []));

async function selectProcess(id: string) {
  selectedId.value = id;
  await loadOutput(id);
  scrollToBottom();
}

function scrollToBottom() {
  nextTick(() => {
    const el = scrollRef.value;
    if (el && autoScroll.value) el.scrollTop = el.scrollHeight;
  });
}

function onScroll() {
  const el = scrollRef.value;
  if (!el) return;
  autoScroll.value = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
}

async function copyAll() {
  const text = lines.value.map((l) => l.line).join("\n");
  try {
    await navigator.clipboard.writeText(text);
    toast.success("已复制全部输出", { position: "top-center" });
  } catch (e) {
    toast.error("复制失败", { description: String(e), position: "top-center" });
  }
}

async function onClear() {
  if (selectedId.value) await clearOutput(selectedId.value);
}

watch(
  () => (selectedId.value ? outputs.value[selectedId.value]?.length : 0),
  () => scrollToBottom()
);

onMounted(async () => {
  await loadProcesses();

  // Listen for cross-window selection changes (main window -> this window).
  unlistenShow = await listen<string>("process-manager:show-output", async (e) => {
    if (e.payload) await selectProcess(e.payload);
  });

  // Ask the main window which process should be shown (covers the case where
  // this view mounts after the initial emit was sent).
  await emit("process-manager:request-output");

  if (!selectedId.value && processes.value.length > 0) {
    selectedId.value = processes.value[0].id;
  }
  if (selectedId.value) {
    await loadOutput(selectedId.value);
    scrollToBottom();
  }
});

watch(selectedOutputId, async (id) => {
  if (id && id !== selectedId.value) {
    await selectProcess(id);
  }
});

onUnmounted(() => {
  unlistenShow?.();
  selectedOutputId.value = null;
});
</script>

<template>
  <div class="h-full flex flex-col p-2 gap-2">
    <div class="flex items-center gap-1.5 shrink-0">
      <Activity class="h-4 w-4" />
      <h2 class="text-sm font-semibold">进程输出</h2>
    </div>

    <!-- Current command (above the output) -->
    <div
      v-if="current"
      class="shrink-0 font-mono text-[11px] bg-muted/50 rounded-sm px-2 py-1 break-all"
      :title="current.command"
    >
      <span class="text-muted-foreground">$ </span>{{ current.command }}
    </div>
    <div v-else class="shrink-0 text-xs text-muted-foreground">暂无进程</div>

    <!-- Output (theme-aware: light = white/black, dark = dark/light) -->
    <div
      ref="scrollRef"
      class="flex-1 overflow-auto font-mono text-[11px] leading-[1.3] bg-background text-foreground border border-border rounded-sm px-2 py-1.5 whitespace-pre-wrap break-all"
      @scroll="onScroll"
    >
      <template v-if="lines.length > 0">
        <div
          v-for="(l, i) in lines"
          :key="i"
          :class="l.stream === 'stderr' ? 'text-destructive' : ''"
        >{{ l.line || " " }}</div>
      </template>
      <div v-else class="text-muted-foreground">暂无输出</div>
    </div>

    <!-- Footer: line count + actions -->
    <div v-if="current" class="flex items-center justify-between shrink-0 border-t border-border pt-1.5">
      <span class="text-[10px] text-muted-foreground tabular-nums">
        {{ lines.length }} 行{{ lines.length >= 1000 ? " (上限1000)" : "" }}
      </span>
      <div class="flex items-center gap-1">
        <Button variant="ghost" size="sm" class="h-6 text-[10px] px-1.5" @click="autoScroll = true; scrollToBottom()">
          <ArrowDownToLine class="h-3 w-3 mr-0.5" />底部
        </Button>
        <Button variant="ghost" size="sm" class="h-6 text-[10px] px-1.5" @click="copyAll">
          <Copy class="h-3 w-3 mr-0.5" />复制全部
        </Button>
        <Button variant="ghost" size="sm" class="h-6 text-[10px] px-1.5 text-destructive" @click="onClear">
          <Trash2 class="h-3 w-3 mr-0.5" />清空
        </Button>
      </div>
    </div>
  </div>
</template>
