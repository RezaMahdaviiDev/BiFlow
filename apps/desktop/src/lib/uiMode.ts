export type UiMode = "basic" | "advanced";

export const UI_MODE_STORAGE_KEY = "biflow-ui-mode-v1";

export function readUiMode(): UiMode {
  const stored = localStorage.getItem(UI_MODE_STORAGE_KEY);
  if (stored === "basic" || stored === "advanced") return stored;
  return "basic";
}

export function writeUiMode(mode: UiMode): void {
  localStorage.setItem(UI_MODE_STORAGE_KEY, mode);
}
