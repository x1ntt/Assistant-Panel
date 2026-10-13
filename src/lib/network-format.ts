/** Presentation only: exact cumulative counters remain strings in backend snapshots. */
export function formatBytes(value: number | string): string {
  const bytes = typeof value === "string" ? Number(value) : value;
  if (bytes === 0) return "0";
  if (bytes < 1024) return bytes + "B";
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + "K";
  if (bytes < 1024 * 1024 * 1024) return (bytes / (1024 * 1024)).toFixed(1) + "M";
  return (bytes / (1024 * 1024 * 1024)).toFixed(2) + "G";
}

export function formatSpeed(bytesPerSec: number): string {
  if (bytesPerSec === 0) return "0";
  if (bytesPerSec > 0 && bytesPerSec < 1024) {
    return bytesPerSec.toFixed(1).replace(/\.0$/, "") + "B/s";
  }
  return formatBytes(bytesPerSec) + "/s";
}
