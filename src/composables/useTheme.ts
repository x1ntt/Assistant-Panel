import { ref, onMounted } from "vue";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

const isDark = ref(false);

export function useTheme() {
  async function loadTheme() {
    try {
      const theme = await invoke<string>("read_theme");
      isDark.value = theme === "dark";
      applyTheme();
    } catch (_) {}
  }

  function applyTheme() {
    if (isDark.value) {
      document.documentElement.classList.add("dark");
    } else {
      document.documentElement.classList.remove("dark");
    }
  }

  async function toggleTheme() {
    isDark.value = !isDark.value;
    applyTheme();
    try {
      await invoke("write_theme", { isDark: isDark.value });
    } catch (_) {}
    try {
      await invoke("broadcast_theme", { isDark: isDark.value });
    } catch (_) {}
  }

  onMounted(async () => {
    await listen<boolean>("theme:change", (event) => {
      isDark.value = event.payload;
      applyTheme();
    });
    await loadTheme();
  });

  return { isDark, toggleTheme };
}
