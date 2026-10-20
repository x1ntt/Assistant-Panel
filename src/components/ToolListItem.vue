<script setup lang="ts">
import { usePanelStore } from "@/stores/panel";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Maximize2, GripVertical, Plus } from "lucide-vue-next";
import type { PanelPlugin } from "@/plugins/types";
import * as icons from "lucide-vue-next";
import { computed, provide, ref } from "vue";
import { CardHeaderKey, type CardHeaderOptions } from "@/composables/cardHeader";

const props = defineProps<{
  plugin: PanelPlugin;
}>();

const emit = defineEmits<{
  pointerDown: [e: PointerEvent];
}>();

const panel = usePanelStore();

const iconComponent = computed(() => {
  return (icons as Record<string, unknown>)[props.plugin.icon] ?? icons.Circle;
});

// Card header integration: the compact view reports its count badge and
// optional add-action, which we render in this shared title bar.
// Store a getter to avoid ref-in-ref unwrap typing issues.
const countGetter = ref<(() => number) | null>(null);
const cardOnAdd = ref<(() => void) | null>(null);
const displayCount = computed(() => (countGetter.value ? countGetter.value() : null));

provide(CardHeaderKey, {
  register(opts: CardHeaderOptions) {
    countGetter.value = opts.count ? () => opts.count!.value : null;
    cardOnAdd.value = opts.onAdd ?? null;
  },
});
</script>

<template>
  <div class="border border-border rounded-sm overflow-hidden">
    <div
      class="flex items-center justify-between px-2 py-1 border-b border-border bg-muted/30 cursor-grab active:cursor-grabbing select-none"
      @pointerdown="emit('pointerDown', $event)"
    >
      <div class="flex items-center gap-1.5 min-w-0">
        <GripVertical class="h-3 w-3 text-muted-foreground/50 shrink-0" />
        <component :is="iconComponent" class="h-3 w-3 text-muted-foreground shrink-0" />
        <span class="text-[10px] text-muted-foreground truncate">{{ plugin.name }}</span>
        <Badge
          v-if="displayCount !== null"
          variant="outline"
          class="text-[8px] px-1 py-0 h-3.5 shrink-0"
        >{{ displayCount }}</Badge>
      </div>
      <div class="flex items-center gap-0.5 shrink-0">
        <Button
          v-if="cardOnAdd"
          :title="'添加' + plugin.name"
          :aria-label="'添加' + plugin.name"
          variant="ghost"
          size="icon"
          class="h-5 w-5"
          @click.stop="cardOnAdd()"
          @pointerdown.stop
        >
          <Plus class="h-3 w-3" />
        </Button>
        <Button
          v-if="plugin.floatingView && plugin.showFloatingButton !== false"
          variant="ghost"
          size="icon"
          class="h-5 w-5"
          @click.stop="panel.openFloatingPanel(plugin.id)"
          @pointerdown.stop
        >
          <Maximize2 class="h-3 w-3" />
        </Button>
      </div>
    </div>
    <div class="px-2 py-1">
      <component :is="plugin.compactView" />
    </div>
  </div>
</template>
