<script setup lang="ts">
import { ref, computed, inject } from "vue";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import {
  Trash2, Pencil, ChevronDown, ChevronRight,
  Unplug, Check, X, Pause, Play,
} from "lucide-vue-next";
import { usePortForward } from "./composables/usePortForward";
import { CardHeaderKey } from "@/composables/cardHeader";

const {
  rules, interfaces, addRule, removeRule, disconnect,
  pauseRule, resumeRule,
  loading, ready, busy, loadError, saveError, operationError,
  restoreRules, retrySave,
  toggleExpand, toggleEdit, formatBytes, formatSpeed,
} = usePortForward();
const canModify = computed(() => ready.value && !loading.value && !busy.value && !saveError.value);

// Push count + add action into the shared card title bar.
const count = computed(() => rules.value.length);
const cardHeader = inject(CardHeaderKey, null);
cardHeader?.register({ count, onAdd: () => openAddRow() });

// Add row
const showAddRow = ref(false);
const newRule = ref({
  listenAddr: "0.0.0.0", listenPort: 0,
  targetAddr: "", targetPort: 0, protocol: "tcp",
});
const addError = ref("");

function openAddRow() {
  if (!canModify.value) return;
  showAddRow.value = true;
  newRule.value = { listenAddr: "0.0.0.0", listenPort: 0, targetAddr: "", targetPort: 0, protocol: "tcp" };
  addError.value = "";
}

function cancelAdd() {
  showAddRow.value = false;
  addError.value = "";
}

function onListenPortBlur() {
  if (newRule.value.listenPort > 0 && newRule.value.targetPort === 0) {
    newRule.value.targetPort = newRule.value.listenPort;
  }
}

async function confirmAdd() {
  addError.value = "";
  if (!newRule.value.listenPort || !newRule.value.targetAddr || !newRule.value.targetPort) {
    addError.value = "请填写完整"; return;
  }
  try {
    if (await addRule(newRule.value)) showAddRow.value = false;
  } catch (e) { addError.value = String(e); }
}

// Edit
const editData = ref({ listenAddr: "", listenPort: 0, targetAddr: "", targetPort: 0, protocol: "tcp" });

function startEdit(rule: any) {
  editData.value = {
    listenAddr: rule.listenAddr, listenPort: rule.listenPort,
    targetAddr: rule.targetAddr, targetPort: rule.targetPort, protocol: rule.protocol,
  };
  toggleEdit(rule.id);
}

function confirmEdit(rule: any) {
  toggleEdit(rule.id);
}

function cancelEdit(rule: any) {
  toggleEdit(rule.id);
}

function protoLabel(p: string) {
  return p === "tcp" ? "TCP" : p === "udp" ? "UDP" : "ALL";
}
</script>

<template>
  <div class="w-full overflow-hidden">
    <div v-if="loading" class="py-1 text-[9px] text-muted-foreground" role="status">正在恢复转发规则…</div>
    <div v-if="loadError" class="py-1 text-[9px] text-destructive" role="alert">
      {{ loadError }}
      <Button variant="ghost" class="h-5 px-1 text-[9px]" :disabled="loading" @click="restoreRules">重试加载</Button>
    </div>
    <div v-if="saveError" class="py-1 text-[9px] text-destructive" role="alert">
      {{ saveError }}
      <Button variant="ghost" class="h-5 px-1 text-[9px]" :disabled="busy" @click="retrySave">重试保存</Button>
    </div>
    <div v-if="operationError" class="py-1 text-[9px] text-destructive" role="alert">{{ operationError }}</div>
    <fieldset :disabled="!canModify" class="min-w-0 border-0 p-0 m-0">
    <div class="overflow-x-auto">
      <table class="w-full text-[9px] border-collapse">
        <thead>
          <tr class="border-b border-border text-muted-foreground">
            <th class="text-left px-1 py-0.5 font-medium">监听</th>
            <th class="text-left px-1 py-0.5 font-medium">端口</th>
            <th class="text-left px-1 py-0.5 font-medium">目标</th>
            <th class="text-left px-1 py-0.5 font-medium">协议</th>
            <th class="text-center px-1 py-0.5 font-medium">连接</th>
            <th class="text-center px-1 py-0.5 font-medium"><div class="leading-tight">速度</div></th>
            <th class="text-center px-1 py-0.5 font-medium"><div class="leading-tight">流量</div></th>
            <th class="text-center px-1 py-0.5 font-medium">操作</th>
          </tr>
        </thead>
        <tbody>
          <template v-for="rule in rules" :key="rule.id">
            <!-- Normal row -->
            <tr v-if="!rule.editing" class="border-b border-border/50 hover:bg-accent/30">
              <td class="px-1 py-0.5">{{ rule.listenAddr }}</td>
              <td class="px-1 py-0.5">{{ rule.listenPort }}</td>
              <td class="px-1 py-0.5">{{ rule.targetAddr }}:{{ rule.targetPort }}</td>
              <td class="px-1 py-0.5">
                <Badge variant="outline" class="text-[8px] px-0.5 py-0 h-3">{{ protoLabel(rule.protocol) }}</Badge>
              </td>
              <td class="px-1 py-0.5 text-center">
                <span :class="rule.paused ? 'text-muted-foreground' : ''">{{ rule.connectionCount }}</span>
              </td>
              <td class="px-1 py-0.5 text-center">
                <div v-if="rule.error" class="text-destructive">启动失败</div>
                <div v-else-if="rule.paused" class="text-muted-foreground">已暂停</div>
                <div v-else class="leading-tight">
                  <div>↑{{ formatSpeed(rule.speedOut) }}</div>
                  <div>↓{{ formatSpeed(rule.speedIn) }}</div>
                </div>
              </td>
              <td class="px-1 py-0.5 text-center">
                <div class="leading-tight">
                  <div>↑{{ formatBytes(rule.totalBytesOut) }}</div>
                  <div>↓{{ formatBytes(rule.totalBytesIn) }}</div>
                </div>
              </td>
              <td class="px-1 py-0.5 text-center">
                <div class="flex items-center justify-center gap-0.5">
                  <Button variant="ghost" size="icon" class="h-4 w-4" @click="toggleExpand(rule.id)">
                    <ChevronDown v-if="rule.expanded" class="h-2.5 w-2.5" />
                    <ChevronRight v-else class="h-2.5 w-2.5" />
                  </Button>
                  <Button v-if="!rule.running" variant="ghost" size="icon" class="h-4 w-4 text-primary"
                    :title="rule.error ? '重试启动' : '恢复转发'" :aria-label="rule.error ? '重试启动' : '恢复转发'"
                    @click="resumeRule(rule.id)">
                    <Play class="h-2.5 w-2.5" />
                  </Button>
                  <Button v-else variant="ghost" size="icon" class="h-4 w-4"
                    @click="pauseRule(rule.id)">
                    <Pause class="h-2.5 w-2.5" />
                  </Button>
                  <Button variant="ghost" size="icon" class="h-4 w-4" @click="startEdit(rule)">
                    <Pencil class="h-2.5 w-2.5" />
                  </Button>
                  <Button variant="ghost" size="icon" class="h-4 w-4 text-destructive" @click="removeRule(rule.id)">
                    <Trash2 class="h-2.5 w-2.5" />
                  </Button>
                </div>
              </td>
            </tr>

            <!-- Edit row -->
            <tr v-else class="border-b border-primary/30 bg-primary/5">
              <td colspan="7" class="px-0.5 py-0.5">
                <div class="flex gap-1 items-center">
                  <select v-model="editData.listenAddr"
                    class="h-5 text-[9px] bg-background border border-border rounded-sm px-0.5 w-16">
                    <option v-for="iface in interfaces" :key="iface.ip" :value="iface.ip">{{ iface.ip }}</option>
                  </select>
                  <Input v-model.number="editData.listenPort" type="number" class="h-5 text-[9px] px-1 w-12" placeholder="端口" />
                  <span class="text-muted-foreground">→</span>
                  <Input v-model="editData.targetAddr" class="h-5 text-[9px] px-1 flex-1" placeholder="目标IP/域名" />
                  <Input v-model.number="editData.targetPort" type="number" class="h-5 text-[9px] px-1 w-12" placeholder="端口" />
                  <select v-model="editData.protocol"
                    class="h-5 text-[9px] bg-background border border-border rounded-sm px-0.5 w-12">
                    <option value="tcp">TCP</option>
                    <option value="udp">UDP</option>
                    <option value="both">ALL</option>
                  </select>
                </div>
              </td>
              <td class="px-0.5 py-0.5 text-center">
                <div class="flex items-center justify-center gap-0.5">
                  <Button variant="ghost" size="icon" class="h-4 w-4 text-primary" @click="confirmEdit(rule)">
                    <Check class="h-2.5 w-2.5" />
                  </Button>
                  <Button variant="ghost" size="icon" class="h-4 w-4" @click="cancelEdit(rule)">
                    <X class="h-2.5 w-2.5" />
                  </Button>
                </div>
              </td>
            </tr>

            <tr v-if="rule.error" class="bg-destructive/10">
              <td colspan="8" class="px-1 py-0.5 text-[9px] text-destructive" role="alert">{{ rule.error }}</td>
            </tr>

            <!-- Expanded: connection sub-rows -->
            <template v-if="rule.expanded && !rule.editing && !rule.paused && rule.connections.length > 0">
              <tr v-for="conn in rule.connections" :key="conn.clientAddr"
                class="bg-muted/30 border-b border-border/30">
                <td class="px-1 py-0.5 text-muted-foreground" colspan="2">└ {{ conn.clientAddr }}</td>
                <td class="px-1 py-0.5 text-muted-foreground" colspan="2"></td>
                <td class="px-1 py-0.5"></td>
                <td class="px-1 py-0.5 text-center text-muted-foreground">
                  <div class="leading-tight">
                    <div>↑{{ formatSpeed(conn.speedOut) }}</div>
                    <div>↓{{ formatSpeed(conn.speedIn) }}</div>
                  </div>
                </td>
                <td class="px-1 py-0.5 text-center text-muted-foreground">
                  <div class="leading-tight">
                    <div>↑{{ formatBytes(conn.bytesOut) }}</div>
                    <div>↓{{ formatBytes(conn.bytesIn) }}</div>
                  </div>
                </td>
                <td class="px-1 py-0.5 text-center">
                  <Button variant="ghost" size="icon" class="h-4 w-4 text-destructive"
                    @click="disconnect(rule.id, conn.clientAddr)">
                    <Unplug class="h-2.5 w-2.5" />
                  </Button>
                </td>
              </tr>
            </template>
            <template v-else-if="rule.expanded && !rule.editing && rule.running">
              <tr class="bg-muted/30 border-b border-border/30">
                <td colspan="8" class="px-1 py-0.5 text-center text-muted-foreground">暂无连接</td>
              </tr>
            </template>
          </template>

          <!-- Add row -->
          <tr v-if="showAddRow" class="border-b border-primary/30 bg-primary/5">
            <td colspan="7" class="px-0.5 py-0.5">
              <div class="flex gap-1 items-center">
                <select v-model="newRule.listenAddr"
                  class="h-5 text-[9px] bg-background border border-border rounded-sm px-0.5 w-16">
                  <option v-for="iface in interfaces" :key="iface.ip" :value="iface.ip">{{ iface.ip }}</option>
                </select>
                <Input v-model.number="newRule.listenPort" type="number" class="h-5 text-[9px] px-1 w-12" placeholder="端口"
                  @blur="onListenPortBlur" />
                <span class="text-muted-foreground">→</span>
                <Input v-model="newRule.targetAddr" class="h-5 text-[9px] px-1 flex-1" placeholder="目标IP/域名" />
                <Input v-model.number="newRule.targetPort" type="number" class="h-5 text-[9px] px-1 w-12" placeholder="端口" />
                <select v-model="newRule.protocol"
                  class="h-5 text-[9px] bg-background border border-border rounded-sm px-0.5 w-12">
                  <option value="tcp">TCP</option>
                  <option value="udp">UDP</option>
                  <option value="both">ALL</option>
                </select>
              </div>
            </td>
            <td class="px-0.5 py-0.5 text-center">
              <div class="flex items-center justify-center gap-0.5">
                <Button variant="ghost" size="icon" class="h-4 w-4 text-primary" @click="confirmAdd()">
                  <Check class="h-2.5 w-2.5" />
                </Button>
                <Button variant="ghost" size="icon" class="h-4 w-4" @click="cancelAdd()">
                  <X class="h-2.5 w-2.5" />
                </Button>
              </div>
            </td>
          </tr>

          <tr v-if="addError" class="bg-destructive/10">
            <td colspan="8" class="px-1 py-0.5 text-destructive text-[8px] text-center">{{ addError }}</td>
          </tr>
          <tr v-if="ready && rules.length === 0 && !showAddRow">
            <td colspan="8" class="px-1 py-2 text-center text-muted-foreground text-[9px]">暂无转发规则</td>
          </tr>
        </tbody>
      </table>
    </div>
    </fieldset>
  </div>
</template>
