<script setup lang="ts">
import { ref, onMounted, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "vue-sonner";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Globe, Check, RefreshCw } from "lucide-vue-next";
import { useGold, METAL_META, type Thresholds, type GoldConnection } from "./composables/useGold";

const { metals, nodeName, stale, lastError, updatedAtText, thresholds, loadThresholds, saveThresholds } = useGold();
const endpoint = ref("");
const apiKey = ref("");
const connectionReady = ref(false);
const connectionSaving = ref(false);
const connectionError = ref<string | null>(null);
const connectionExpanded = ref(true);
const refreshing = ref(false);

async function loadConnection() {
  connectionError.value = null;
  try {
    const connection = await invoke<GoldConnection>("get_gold_connection");
    endpoint.value = connection.endpoint;
    apiKey.value = connection.apiKey;
    connectionExpanded.value = !connection.apiKey;
    connectionReady.value = true;
  } catch (error) {
    connectionError.value = String(error);
    connectionExpanded.value = true;
  }
}

async function saveConnection() {
  if (!connectionReady.value || connectionSaving.value) return;
  connectionSaving.value = true;
  connectionError.value = null;
  try {
    const connection = await invoke<GoldConnection>("save_gold_connection", {
      endpoint: endpoint.value, apiKey: apiKey.value,
    });
    endpoint.value = connection.endpoint;
    apiKey.value = connection.apiKey;
    toast.success("连接设置已保存", {
      description: connection.apiKey ? "正在从 PersonalApiV2 获取金价" : "Key 已清空，已停止请求",
      position: "top-center",
    });
  } catch (error) {
    connectionError.value = String(error);
    connectionExpanded.value = true;
  } finally {
    connectionSaving.value = false;
  }
}

function onConnectionToggle(event: Event) {
  connectionExpanded.value = (event.target as HTMLDetailsElement).open;
}

async function refresh() {
  refreshing.value = true;
  try {
    await invoke("refresh_gold_data");
  } catch (error) {
    toast.error("刷新失败", { description: String(error), position: "top-center" });
  } finally {
    refreshing.value = false;
  }
}

// Local editable copy of thresholds (string-backed for the inputs).
interface EditRow {
  id: string;
  name: string;
  max: string | number;
  min: string | number;
}

const rows = ref<EditRow[]>(
  METAL_META.map((m) => ({ id: m.id, name: m.name, max: "", min: "" }))
);
const saved = ref(false);

onMounted(async () => {
  await loadConnection();
  await loadThresholds();
  const t = thresholds.value;
  rows.value = rows.value.map((r) => ({
    ...r,
    max: t[r.id]?.max != null ? String(t[r.id].max) : "",
    min: t[r.id]?.min != null ? String(t[r.id].min) : "",
  }));
});

function currentPrice(id: string): string {
  const m = metals.value.find((x) => x.id === id);
  return m?.sellPrice != null ? m.sellPrice.toLocaleString() : "--";
}

// Parse an input value (may be string or number from the number input) into
// a finite number, or null when empty/invalid.
function parseBound(v: string | number): number | null {
  if (v === null || v === undefined) return null;
  const s = String(v).trim();
  if (s === "") return null;
  const n = Number(s);
  return Number.isFinite(n) ? n : null;
}

async function save() {
  const next: Thresholds = {};
  for (const r of rows.value) {
    next[r.id] = {
      max: parseBound(r.max),
      min: parseBound(r.min),
    };
  }
  thresholds.value = next;
  try {
    await saveThresholds();
    saved.value = true;
    toast.success("阈值已保存", {
      description: "下次轮询将按新阈值判断",
      position: "top-center",
    });
    setTimeout(() => (saved.value = false), 1500);
  } catch (e) {
    console.error("[api-dashboard] save failed", e);
    toast.error("保存失败", { description: String(e), position: "top-center" });
  }
}

const statusText = computed(() => {
  if (stale.value) return "数据异常";
  return nodeName.value ? `节点 ${nodeName.value}` : "等待数据";
});
</script>

<template>
  <div class="p-3 space-y-3">
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-2">
        <Globe class="h-5 w-5" />
        <h2 class="text-base font-semibold">金价监控 · V2</h2>
      </div>
      <Badge
        variant="outline"
        class="text-xs"
        :class="stale ? 'text-yellow-500 border-yellow-500/50' : ''"
      >
        {{ statusText }}
      </Badge>
    </div>

    <div class="flex items-center justify-between gap-2">
      <p class="text-xs text-muted-foreground tabular-nums" title="节点采集时间，按本地时区显示">
        数据更新：{{ updatedAtText }}
      </p>
      <Button variant="ghost" size="sm" class="h-7 px-2 text-xs" :disabled="refreshing" @click="refresh">
        <RefreshCw class="h-3 w-3 mr-1" />刷新
      </Button>
    </div>
    <p v-if="lastError" class="text-xs text-yellow-600 dark:text-yellow-500 break-words">{{ lastError }}</p>

    <details class="rounded-md border p-3" :open="connectionExpanded" @toggle="onConnectionToggle">
      <summary class="cursor-pointer text-sm font-medium">PersonalApiV2 连接设置</summary>
      <form class="mt-3 space-y-2" @submit.prevent="saveConnection">
        <label class="block space-y-1">
          <span class="text-xs text-muted-foreground">接口地址</span>
          <Input v-model="endpoint" type="url" required :disabled="!connectionReady || connectionSaving" class="h-8 text-xs"
            placeholder="https://xint.top/index.php/action/personal-api-v2" />
        </label>
        <label class="block space-y-1">
          <span class="text-xs text-muted-foreground">用户 Key</span>
          <Input v-model="apiKey" type="password" autocomplete="off" :disabled="!connectionReady || connectionSaving" class="h-8 text-xs"
            placeholder="在 PersonalApiV2 后台新增用户后获取" />
        </label>
        <p class="text-[11px] text-muted-foreground">仅订阅 gold，每 30 秒刷新。Key 使用 X-API-Key 请求头发送。留空保存可停止请求。</p>
        <p v-if="connectionError" class="text-xs text-destructive break-words">{{ connectionError }}</p>
        <Button v-if="!connectionReady" type="button" size="sm" variant="outline" @click="loadConnection">重试读取设置</Button>
        <Button type="submit" size="sm" class="w-full" :disabled="!connectionReady || connectionSaving">
          {{ connectionSaving ? "保存中…" : "保存连接设置" }}
        </Button>
      </form>
    </details>

    <p class="text-xs text-muted-foreground">
      设定卖出价的上/下限。越界时右侧弹出提醒（仅手动关闭）。留空表示不限。
    </p>

    <Card v-for="row in rows" :key="row.id">
      <CardHeader class="py-2 px-3">
        <CardTitle class="text-sm flex items-center justify-between">
          <span>{{ row.name }}</span>
          <span class="text-xs text-muted-foreground tabular-nums">
            当前 {{ currentPrice(row.id) }}
          </span>
        </CardTitle>
      </CardHeader>
      <CardContent class="px-3 pb-3 space-y-2">
        <div class="grid grid-cols-[3rem_1fr] items-center gap-2">
          <span class="text-xs text-muted-foreground">上限 &gt;</span>
          <Input
            v-model="row.max"
            type="number"
            placeholder="不限"
            class="h-8 text-sm"
          />
        </div>
        <div class="grid grid-cols-[3rem_1fr] items-center gap-2">
          <span class="text-xs text-muted-foreground">下限 &lt;</span>
          <Input
            v-model="row.min"
            type="number"
            placeholder="不限"
            class="h-8 text-sm"
          />
        </div>
      </CardContent>
    </Card>

    <Button size="sm" class="w-full h-9 text-sm" @click="save">
      <Check v-if="saved" class="h-4 w-4 mr-1" />
      {{ saved ? "已保存" : "保存阈值" }}
    </Button>
  </div>
</template>
