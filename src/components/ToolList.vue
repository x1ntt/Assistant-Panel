<script setup lang="ts">
import { ref } from "vue";
import { usePluginOrder } from "@/composables/usePluginOrder";
import ToolListItem from "./ToolListItem.vue";

const { orderedPlugins, movePlugin } = usePluginOrder();

const dragIndex = ref<number | null>(null);
const overIndex = ref<number | null>(null);
const isDragging = ref(false);
const startY = ref(0);
const currentY = ref(0);
const dragEl = ref<HTMLElement | null>(null);
const listRef = ref<HTMLElement | null>(null);

function onPointerDown(index: number, e: PointerEvent) {
  // Only left mouse button
  if (e.button !== 0) return;
  dragIndex.value = index;
  startY.value = e.clientY;
  currentY.value = e.clientY;
  isDragging.value = false;

  const target = e.currentTarget as HTMLElement;
  dragEl.value = target.closest("[data-card]") as HTMLElement;

  document.addEventListener("pointermove", onPointerMove);
  document.addEventListener("pointerup", onPointerUp);
}

function onPointerMove(e: PointerEvent) {
  if (dragIndex.value === null) return;

  const dy = Math.abs(e.clientY - startY.value);
  if (dy > 5) {
    isDragging.value = true;
  }

  if (!isDragging.value) return;

  currentY.value = e.clientY;

  // Find which card we're over
  if (!listRef.value) return;
  const cards = listRef.value.querySelectorAll("[data-card]");
  for (let i = 0; i < cards.length; i++) {
    if (i === dragIndex.value) continue;
    const rect = cards[i].getBoundingClientRect();
    if (e.clientY >= rect.top && e.clientY <= rect.bottom) {
      overIndex.value = i;
      break;
    }
  }
}

function onPointerUp(_e: PointerEvent) {
  document.removeEventListener("pointermove", onPointerMove);
  document.removeEventListener("pointerup", onPointerUp);

  if (isDragging.value && dragIndex.value !== null && overIndex.value !== null && dragIndex.value !== overIndex.value) {
    movePlugin(dragIndex.value, overIndex.value);
  }

  dragIndex.value = null;
  overIndex.value = null;
  isDragging.value = false;
  dragEl.value = null;
}
</script>

<template>
  <div ref="listRef" class="h-full overflow-y-auto p-2 space-y-1.5">
    <div
      v-for="(plugin, index) in orderedPlugins"
      :key="plugin.id"
      data-card
      class="transition-all duration-100"
      :class="[
        dragIndex === index && isDragging ? 'opacity-30 scale-[0.97]' : '',
        overIndex === index && isDragging && dragIndex !== index ? 'border-t-2 border-primary' : '',
      ]"
    >
      <ToolListItem
        :plugin="plugin"
        @pointer-down="onPointerDown(index, $event)"
      />
    </div>
  </div>
</template>
