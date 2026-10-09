<script setup lang="ts">
import { ref, onMounted } from "vue";
import { useTauriCommand } from "@/composables/useTauriCommand";
import { Badge } from "@/components/ui/badge";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ScrollArea } from "@/components/ui/scroll-area";

interface LogEntry {
  timestamp: string;
  level: string;
  source: string;
  message: string;
}

const { execute: getSources, data: sources } = useTauriCommand<{}, string[]>("list_log_sources");
const { execute: getLogs, loading } = useTauriCommand<{ source: string | null }, LogEntry[]>("get_logs");

const activeSource = ref("system");
const logEntries = ref<LogEntry[]>([]);

async function loadLogs(source: string) {
  activeSource.value = source;
  const result = await getLogs({ source });
  if (result) {
    logEntries.value = result;
  }
}

onMounted(async () => {
  await getSources();
  await loadLogs("system");
});

function getLevelVariant(level: string): "default" | "destructive" | "outline" | "secondary" {
  switch (level) {
    case "ERROR": return "destructive";
    case "WARN": return "secondary";
    case "INFO": return "default";
    default: return "outline";
  }
}
</script>

<template>
  <div class="p-2 h-full flex flex-col">
    <Tabs :model-value="activeSource" @update:model-value="loadLogs($event as string)" class="flex-1 flex flex-col">
      <TabsList class="w-full justify-start flex-wrap h-auto gap-1">
        <TabsTrigger value="system" class="text-xs">系统</TabsTrigger>
        <TabsTrigger
          v-for="src in (sources ?? []).filter(s => s !== 'system')"
          :key="src"
          :value="src"
          class="text-xs"
        >
          {{ src }}
        </TabsTrigger>
      </TabsList>

      <TabsContent v-for="src in ['system', ...(sources ?? []).filter(s => s !== 'system')]" :key="src" :value="src" class="flex-1 mt-1">
        <ScrollArea class="h-[calc(100vh-180px)]">
          <div v-if="loading" class="text-xs text-muted-foreground p-1">加载中...</div>
          <div v-else-if="logEntries.length === 0" class="text-xs text-muted-foreground p-1">暂无日志</div>
          <div v-else class="space-y-0.5">
            <div
              v-for="(entry, i) in logEntries"
              :key="i"
              class="flex items-start gap-1.5 text-[10px] py-0.5 px-1.5 rounded-sm hover:bg-accent/50"
            >
              <span class="text-muted-foreground whitespace-nowrap">{{ entry.timestamp }}</span>
              <Badge :variant="getLevelVariant(entry.level)" class="text-[9px] px-1 py-0">
                {{ entry.level }}
              </Badge>
              <span class="flex-1 break-all">{{ entry.message }}</span>
            </div>
          </div>
        </ScrollArea>
      </TabsContent>
    </Tabs>
  </div>
</template>
