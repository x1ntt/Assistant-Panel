<script setup lang="ts">
import { ref, onMounted, computed } from "vue";
import { toast } from "vue-sonner";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Globe, Check } from "lucide-vue-next";
import { useGold, METAL_META, type Thresholds } from "./composables/useGold";

const { metals, nodeName, stale, thresholds, loadThresholds, saveThresholds } = useGold();

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
        <h2 class="text-base font-semibold">金价监控</h2>
      </div>
      <Badge
        variant="outline"
        class="text-xs"
        :class="stale ? 'text-yellow-500 border-yellow-500/50' : ''"
      >
        {{ statusText }}
      </Badge>
    </div>

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
