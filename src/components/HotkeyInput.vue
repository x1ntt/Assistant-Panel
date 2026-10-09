<script setup lang="ts">
import { useHotkeyRecorder, formatHotkey } from "@/composables/useHotkeyRecorder";
import { Input } from "@/components/ui/input";
import { ref, watch } from "vue";

const props = defineProps<{
  modelValue: string;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: string];
}>();

const { isRecording, recordedHotkey, startRecording } = useHotkeyRecorder();

const displayValue = ref(props.modelValue);

watch(() => props.modelValue, (v) => {
  displayValue.value = v;
});

watch(recordedHotkey, (v) => {
  if (v) {
    // Let the parent confirm before the label changes, so a failed
    // re-register does not leave a stale "new" binding on screen.
    emit("update:modelValue", v);
  }
});
</script>

<template>
  <Input
    :model-value="formatHotkey(displayValue)"
    readonly
    class="h-5 text-[9px] px-1 cursor-pointer"
    :class="isRecording ? 'ring-2 ring-primary animate-pulse' : ''"
    :placeholder="isRecording ? '请按键...' : '点击录制快捷键'"
    @click="startRecording"
  />
</template>
