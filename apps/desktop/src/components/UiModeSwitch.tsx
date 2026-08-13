import { useTranslation } from "react-i18next";
import type { UiMode } from "../lib/uiMode";
import { writeUiMode } from "../lib/uiMode";

export function UiModeSwitch({
  mode,
  onChange,
}: {
  mode: UiMode;
  onChange: (mode: UiMode) => void;
}) {
  const { t, i18n } = useTranslation();
  const rtl = i18n.dir() === "rtl";

  function select(next: UiMode) {
    if (next === mode) return;
    writeUiMode(next);
    onChange(next);
  }

  function onKeyDown(event: React.KeyboardEvent<HTMLDivElement>) {
    const forward = rtl
      ? event.key === "ArrowLeft"
      : event.key === "ArrowRight";
    const backward = rtl
      ? event.key === "ArrowRight"
      : event.key === "ArrowLeft";
    if (forward) {
      event.preventDefault();
      select("advanced");
    } else if (backward) {
      event.preventDefault();
      select("basic");
    }
  }

  return (
    <div
      role="radiogroup"
      aria-label={t("uiModeLabel")}
      className="relative inline-grid w-full max-w-md grid-cols-2 rounded-xl border border-ink/10 bg-canvas p-1"
      onKeyDown={onKeyDown}
    >
      <div
        aria-hidden
        className="pointer-events-none absolute inset-y-1 w-[calc(50%-0.25rem)] rounded-lg bg-surface shadow-sm transition-[inset-inline-start] duration-200 motion-reduce:transition-none"
        style={{
          insetInlineStart:
            mode === "basic" ? "0.25rem" : "calc(50% + 0.125rem)",
        }}
      />
      <ModeOption
        label={t("uiModeBasic")}
        checked={mode === "basic"}
        onSelect={() => select("basic")}
      />
      <ModeOption
        label={t("uiModeAdvanced")}
        checked={mode === "advanced"}
        onSelect={() => select("advanced")}
      />
    </div>
  );
}

function ModeOption({
  label,
  checked,
  onSelect,
}: {
  label: string;
  checked: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={checked}
      tabIndex={checked ? 0 : -1}
      onClick={onSelect}
      className={`relative z-10 rounded-lg px-4 py-2.5 text-sm font-semibold transition-colors ${
        checked ? "text-brand" : "text-muted hover:text-ink"
      }`}
    >
      {label}
    </button>
  );
}
