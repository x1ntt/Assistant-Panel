import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

export function useTauriCommand<TArgs = Record<string, unknown>, TResult = unknown>(
  command: string
) {
  const loading = ref(false);
  const error = ref<string | null>(null);
  const data = ref<TResult | null>(null);

  async function execute(args?: TArgs): Promise<TResult | null> {
    loading.value = true;
    error.value = null;
    try {
      data.value = await invoke<TResult>(command, args as Record<string, unknown>);
      return data.value;
    } catch (e) {
      error.value = String(e);
      return null;
    } finally {
      loading.value = false;
    }
  }

  return { execute, loading, error, data };
}
