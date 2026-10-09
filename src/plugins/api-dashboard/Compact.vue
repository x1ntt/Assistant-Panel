<script setup lang="ts">
import { computed } from "vue";
import { Badge } from "@/components/ui/badge";
import { Globe } from "lucide-vue-next";
import { useGold, METAL_META } from "./composables/useGold";

const { metals, stale, getThreshold } = useGold();

// Merge live data with the fixed metal order so rows are stable.
const rows = computed(() =>
  METAL_META.map((meta) => {
    const m = metals.value.find((x) => x.id === meta.id);
    return {
      id: meta.id,
      name: meta.name,
      sellPrice: m?.sellPrice ?? null,
      highPrice: m?.highPrice ?? null,
      lowPrice: m?.lowPrice ?? null,
      unit: m?.unit ?? "",
    };
  })
);

function fmt(v: number | null): string {
  if (v === null || v === undefined) return "--";
  return v.toLocaleString(undefined, { maximumFractionDigits: 2 });
}

// True when a metal's sell price is outside its configured threshold.
function breached(id: string, price: number | null): boolean {
  if (price === null) return false;
  const t = getThreshold(id);
  if (t.max !== null && t.max !== undefined && price > t.max) return true;
  if (t.min !== null && t.min !== undefined && price < t.min) return true;
  return false;
}

const hasData = computed(() => metals.value.length > 0);
</script>

<template>
  <div class="w-full space-y-1">
    <!-- Header -->
    <div class="flex items-center justify-between px-1.5 py-0.5">
      <div class="flex items-center gap-1.5">
        <Globe class="h-3.5 w-3.5 text-muted-foreground" />
        <span class="text-xs text-muted-foreground font-medium">金价</span>
      </div>
      <Badge
        variant="outline"
        class="text-[10px] px-1 py-0 h-4"
        :class="stale ? 'text-yellow-500 border-yellow-500/50' : ''"
      >
        {{ stale ? "数据异常" : hasData ? "在线" : "等待" }}
      </Badge>
    </div>

    <!-- Metal rows -->
    <div class="space-y-0.5">
      <div
        v-for="row in rows"
        :key="row.id"
        class="grid grid-cols-[5rem_1fr_auto] items-center gap-2 px-1.5 py-1 bg-muted/30 rounded-sm"
      >
        <span class="text-xs truncate">{{ row.name }}</span>
        <span
          class="text-sm font-semibold tabular-nums text-right"
          :class="[
            stale ? 'text-yellow-500/80' : breached(row.id, row.sellPrice) ? 'text-destructive' : '',
          ]"
        >
          {{ fmt(row.sellPrice) }}
        </span>
        <span class="text-[10px] text-muted-foreground tabular-nums whitespace-nowrap">
          H {{ fmt(row.highPrice) }} / L {{ fmt(row.lowPrice) }}
        </span>
      </div>
    </div>
  </div>
</template>
