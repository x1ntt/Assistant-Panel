<script setup lang="ts">
import { computed, inject, onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { ChevronDown, ChevronUp, Highlighter, RotateCcw } from "lucide-vue-next";
import { Button } from "@/components/ui/button";
import { CardHeaderKey } from "@/composables/cardHeader";
import { formatBytes, formatSpeed } from "@/lib/network-format";
import { useNetworkTraffic } from "./composables/useNetworkTraffic";
import PinButton from "./PinButton.vue";
import type { InterfaceMeta, TrafficRow } from "./types";

const {
  snapshot, rows, loading, error, operationError, pending, busy, stale,
  retry, pinInterface, unpinInterface, setHighlight,
} = useNetworkTraffic();
const doubleClickInterval = ref(500);
onMounted(async () => {
  try {
    const interval = await invoke<number>("get_network_traffic_double_click_interval");
    if (Number.isFinite(interval) && interval > 0 && interval <= 5000) doubleClickInterval.value = interval;
  } catch (_) {}
});
const expanded = ref(false);
const showFilters = ref(false);
const filterCount = computed(() => rows.value.filter(row => row.isFilter).length);
const displayRows = computed(() => showFilters.value ? rows.value : rows.value.filter(row => !row.isFilter));
const collapsedRows = computed(() => {
  const pinned = displayRows.value.filter(row => row.pinned);
  return pinned.length ? pinned : displayRows.value.slice(0, 1);
});
const hiddenCount = computed(() => displayRows.value.length - collapsedRows.value.length);
const visibleRows = computed(() => expanded.value ? displayRows.value : collapsedRows.value);
const count = computed(() => displayRows.value.length);
const cardHeader = inject(CardHeaderKey, null);
cardHeader?.register({ count });
onUnmounted(() => cardHeader?.register({}));

function interfaceTitle(iface: InterfaceMeta) {
  return [iface.name, iface.description, iface.status, iface.id].filter(Boolean).join(" · ");
}
function unavailable(row: TrafficRow) {
  return !!row.error || row.speedIn === null || row.speedOut === null;
}
function speed(row: TrafficRow, value: number | null) {
  return !snapshot.value?.enabled || stale.value || row.error || value === null ? "—" : formatSpeed(value);
}
async function retryAll() {
  operationError.value = "";
  await retry();
}
</script>

<template>
  <div class="w-full overflow-hidden text-[9px]" :aria-busy="loading || pending">
    <div class="flex flex-wrap items-center justify-between gap-1 px-1 py-0.5">
      <span class="text-[8px] text-muted-foreground">
        筛选后 {{ displayRows.length }} / 全部 {{ rows.length }}
        <span v-if="!showFilters && filterCount"> · 已隐藏 {{ filterCount }} 个过滤接口</span>
      </span>
      <Button variant="ghost" size="xs" class="h-5 text-[9px]" :aria-pressed="showFilters"
        :aria-label="showFilters ? '隐藏过滤接口' : '显示过滤接口'"
        :disabled="!filterCount" @click="showFilters = !showFilters">
        {{ showFilters ? '隐藏过滤接口' : '显示过滤接口' }} ({{ filterCount }})
      </Button>
    </div>
    <p v-if="snapshot?.warning" class="px-1 py-0.5 text-muted-foreground break-words" role="status">
      配置清理警告（不影响采样）：{{ snapshot.warning }}
    </p>
    <div v-if="loading || error || snapshot?.error || stale || (snapshot && !snapshot.enabled) || pending || operationError"
      class="flex items-center justify-between gap-1 px-1 py-0.5" aria-live="polite">
      <div class="min-w-0 break-words" :class="error || snapshot?.error || operationError ? 'text-destructive' : 'text-muted-foreground'">
        <div v-if="loading">加载中…</div>
        <div v-if="error">获取网络流量失败：{{ error }}</div>
        <div v-if="snapshot?.error">采样失败：{{ snapshot.error }}</div>
        <div v-if="operationError">操作失败：{{ operationError }}；可再次操作或重试</div>
        <div v-if="snapshot && !snapshot.enabled">监控已停用，请在设置中启用</div>
        <div v-else-if="stale" :title="snapshot?.updatedAt ? new Date(snapshot.updatedAt).toLocaleString() : '尚未采样'">
          数据已过期，累计流量为上次快照
        </div>
        <div v-if="pending">操作中…</div>
      </div>
      <Button v-if="error || snapshot?.error || stale || operationError" variant="ghost" size="icon"
        class="h-4 w-4 shrink-0" title="重试" aria-label="重试"
        :disabled="loading || pending" @click="retryAll">
        <RotateCcw class="h-2.5 w-2.5" />
      </Button>
    </div>
    <table class="w-full table-fixed text-[9px] border-collapse">
      <caption class="sr-only">网络接口速度与流量；↑发送，↓接收；流量自监控开始累计</caption>
      <thead>
        <tr class="border-b border-border text-muted-foreground">
          <th scope="col" class="text-left px-1 py-0.5 font-medium">接口</th>
          <th scope="col" class="w-20 text-center px-1 py-0.5 font-medium">速度</th>
          <th scope="col" class="w-20 text-center px-1 py-0.5 font-medium">流量</th>
          <th scope="col" class="w-12 text-center px-1 py-0.5 font-medium">操作</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="row in visibleRows" :key="row.id" class="border-b border-border/50"
          :class="row.highlighted
            ? row.pinned ? 'bg-primary/15 hover:bg-primary/20' : 'bg-primary/10 hover:bg-primary/10'
            : row.pinned ? 'bg-foreground/5 hover:bg-foreground/10' : 'hover:bg-accent/30'">
          <td class="px-1 py-0.5">
            <div class="flex items-center gap-1 min-w-0">
              <span class="w-0.5 self-stretch shrink-0" :class="row.highlighted ? 'bg-primary' : 'bg-transparent'" aria-hidden="true"></span>
              <div class="min-w-0" :title="interfaceTitle(row)">
                <div class="truncate">{{ row.name || row.id }}</div>
                <div class="truncate text-[8px] text-muted-foreground">{{ row.description || row.status }}</div>
                <div v-if="unavailable(row)" class="text-[8px] text-muted-foreground">不可用 · {{ row.status }}</div>
                <div v-if="row.error" class="text-[8px] text-destructive break-words">{{ row.error }}</div>
              </div>
            </div>
          </td>
          <td class="px-1 py-0.5 text-center tabular-nums whitespace-nowrap leading-tight"
            :class="stale || unavailable(row) || !snapshot?.enabled ? 'text-muted-foreground' : ''">
            <div :title="'发送速度：' + speed(row, row.speedOut)">↑{{ speed(row, row.speedOut) }}</div>
            <div :title="'接收速度：' + speed(row, row.speedIn)">↓{{ speed(row, row.speedIn) }}</div>
          </td>
          <td class="px-1 py-0.5 text-center tabular-nums whitespace-nowrap leading-tight"
            :class="stale || unavailable(row) || !snapshot?.enabled ? 'text-muted-foreground' : ''">
            <div :title="'累计发送：' + row.totalBytesOut + ' B'">↑{{ formatBytes(row.totalBytesOut) }}</div>
            <div :title="'累计接收：' + row.totalBytesIn + ' B'">↓{{ formatBytes(row.totalBytesIn) }}</div>
          </td>
          <td class="px-1 py-0.5 text-center">
            <div class="flex items-center justify-center gap-0.5">
              <Button variant="ghost" size="icon" class="h-4 w-4" :class="row.highlighted ? 'text-primary' : ''"
                :title="row.highlighted ? '取消高亮' : '高亮'" :aria-label="row.highlighted ? '取消高亮' : '高亮'"
                :aria-pressed="row.highlighted" :disabled="busy" @click="setHighlight(row.id, !row.highlighted)">
                <Highlighter class="h-2.5 w-2.5" />
              </Button>
              <PinButton :pinned="row.pinned" :disabled="busy" :double-click-interval="doubleClickInterval"
                @pin="pinInterface(row.id)" @unpin="unpinInterface(row.id)" />
            </div>
          </td>
        </tr>
        <tr v-if="!loading && !error && !snapshot?.error && snapshot && !displayRows.length">
          <td colspan="4" class="px-1 py-2 text-center text-muted-foreground">
            {{ rows.length ? '过滤接口已隐藏，点击“显示过滤接口”查看' : '暂无可用接口，等待系统发现' }}
          </td>
        </tr>
      </tbody>
    </table>
    <div v-if="hiddenCount > 0 || expanded" class="flex justify-center py-0.5">
      <Button variant="ghost" size="xs" class="h-5 text-[9px]" :aria-expanded="expanded"
        :aria-label="expanded ? '收起接口列表' : '展开全部接口'" @click="expanded = !expanded">
        <ChevronUp v-if="expanded" class="size-3" />
        <ChevronDown v-else class="size-3" />
        {{ expanded ? '收起' : `展开其余 ${hiddenCount} 条` }}
      </Button>
    </div>
    <p class="px-1 py-0.5 text-[8px] text-muted-foreground">↑发送 · ↓接收 · 流量自监控开始累计</p>
  </div>
</template>
