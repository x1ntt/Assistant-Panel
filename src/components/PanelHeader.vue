<script setup lang="ts">
import { usePanelStore } from "@/stores/panel";
import { ChevronRight } from "lucide-vue-next";
import { Button } from "@/components/ui/button";

const panel = usePanelStore();
</script>

<template>
  <div
    class="flex items-center justify-end px-2 py-1 border-b border-border bg-card"
    data-tauri-drag-region
  >
    <Button
      variant="ghost"
      size="icon"
      class="h-6 w-6 shrink-0"
      @click="panel.collapse()"
    >
      <ChevronRight class="h-3.5 w-3.5" />
    </Button>
  </div>
</template>
