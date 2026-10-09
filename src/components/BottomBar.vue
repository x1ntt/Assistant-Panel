<script setup lang="ts">
import { ref } from "vue";
import { usePanelStore } from "@/stores/panel";
import { useTheme } from "@/composables/useTheme";
import { Button } from "@/components/ui/button";
import { Settings, ScrollText, Moon, Sun, LogOut, RotateCcw, Power } from "lucide-vue-next";
import { invoke } from "@tauri-apps/api/core";

const panel = usePanelStore();
const { isDark, toggleTheme } = useTheme();

const showExitConfirm = ref(false);
let hideTimer: ReturnType<typeof setTimeout> | null = null;

function onExitClick() {
  showExitConfirm.value = !showExitConfirm.value;
  if (hideTimer) clearTimeout(hideTimer);
  if (showExitConfirm.value) {
    hideTimer = setTimeout(() => { showExitConfirm.value = false; }, 4000);
  }
}

function doExit() {
  invoke("exit_app");
}

function doRestart() {
  invoke("restart_app");
}
</script>

<template>
  <div class="relative flex items-center justify-around px-2 py-1 border-t border-border bg-card">
    <Button variant="ghost" size="sm" class="flex items-center gap-1 text-xs h-7"
      @click="panel.openFloatingPanel('settings')">
      <Settings class="h-3.5 w-3.5" />
      <span>设置</span>
    </Button>
    <Button variant="ghost" size="sm" class="flex items-center gap-1 text-xs h-7"
      @click="panel.openFloatingPanel('logs')">
      <ScrollText class="h-3.5 w-3.5" />
      <span>日志</span>
    </Button>
    <Button variant="ghost" size="icon" class="h-7 w-7" @click="toggleTheme()">
      <Moon v-if="!isDark" class="h-3.5 w-3.5" />
      <Sun v-else class="h-3.5 w-3.5" />
    </Button>
    <Button variant="ghost" size="icon" class="h-7 w-7" :class="showExitConfirm ? 'text-destructive' : ''"
      @click="onExitClick()">
      <LogOut class="h-3.5 w-3.5" />
    </Button>

    <!-- Exit / restart popup -->
    <Transition
      enter-active-class="transition-opacity duration-150"
      leave-active-class="transition-opacity duration-150"
      enter-from-class="opacity-0"
      leave-to-class="opacity-0"
    >
      <div v-if="showExitConfirm"
        class="absolute bottom-full right-0 mb-1 p-1 bg-background border border-border rounded-sm shadow-md flex items-center gap-1">
        <Button variant="ghost" size="sm" class="h-6 text-[10px] px-1.5 gap-1"
          @click="doRestart()">
          <RotateCcw class="h-3 w-3" />重启
        </Button>
        <Button variant="ghost" size="sm" class="h-6 text-[10px] px-1.5 gap-1 text-destructive"
          @click="doExit()">
          <Power class="h-3 w-3" />退出
        </Button>
      </div>
    </Transition>
  </div>
</template>
