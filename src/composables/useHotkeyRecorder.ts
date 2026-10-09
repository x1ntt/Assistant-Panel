import { ref, onUnmounted } from "vue";

const CODE_DISPLAY: Record<string, string> = {
  Backquote: "`",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Space: "␣",
};

export function formatHotkey(hk: string): string {
  if (!hk) return "";
  return hk
    .split("+")
    .map((part) => {
      if (part === "Control") return "Ctrl";
      if (CODE_DISPLAY[part]) return CODE_DISPLAY[part];
      if (/^Key[A-Z]$/.test(part)) return part.slice(3);
      if (/^Digit[0-9]$/.test(part)) return part.slice(5);
      if (/^Numpad[0-9]$/.test(part)) return "Num" + part.slice(6);
      return part;
    })
    .join("+");
}

export function useHotkeyRecorder() {
  const isRecording = ref(false);
  const recordedHotkey = ref("");
  let timeoutId: ReturnType<typeof setTimeout> | null = null;

  function startRecording() {
    isRecording.value = true;
    recordedHotkey.value = "";

    // Auto-cancel after 5 seconds
    timeoutId = setTimeout(() => {
      stopRecording();
    }, 5000);

    document.addEventListener("keydown", onKeyDown);
  }

  function stopRecording() {
    isRecording.value = false;
    if (timeoutId) {
      clearTimeout(timeoutId);
      timeoutId = null;
    }
    document.removeEventListener("keydown", onKeyDown);
  }

  function onKeyDown(e: KeyboardEvent) {
    e.preventDefault();
    e.stopPropagation();

    // Ignore modifier-only and lock keys
    if (["Control", "Alt", "Shift", "Meta"].includes(e.key)) {
      return;
    }
    if (["CapsLock", "NumLock", "ScrollLock"].includes(e.code)) {
      return;
    }

    const parts: string[] = [];
    if (e.ctrlKey) parts.push("Control");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");
    if (e.metaKey) parts.push("Super");

    // Use physical key code so names match keyboard_types::Code
    // (KeyA / Digit1 / Backquote / F1 …) regardless of Shift or layout.
    if (!e.code) return;
    parts.push(e.code);
    recordedHotkey.value = parts.join("+");
    stopRecording();
  }

  onUnmounted(() => {
    stopRecording();
  });

  return {
    isRecording,
    recordedHotkey,
    startRecording,
    stopRecording,
  };
}
