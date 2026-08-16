const UNITS = ["KiB", "MiB", "GiB", "TiB", "PiB"] as const;

/** Formats a byte count with enough precision for lifetime VPN totals. */
export function formatTrafficBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 B";
  }
  if (bytes < 1024) {
    return `${Math.round(bytes)} B`;
  }
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = value < 100 ? 2 : 1;
  return `${value.toFixed(digits)} ${UNITS[unit]}`;
}
