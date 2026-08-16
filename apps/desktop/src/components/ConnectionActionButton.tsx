import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { StackSnapshot } from "../api/models";
import {
  connectionButtonProgress,
  type ConnectionAction,
} from "../lib/connectionProgress";

export function ConnectionActionButton({
  action,
  snapshot,
  installingId,
  actionPending,
  disabled,
  onClick,
  icon,
  variant,
}: {
  action: ConnectionAction;
  snapshot: StackSnapshot;
  installingId?: string | null;
  actionPending?: boolean;
  disabled: boolean;
  onClick: () => void;
  icon: ReactNode;
  variant: "primary" | "secondary";
}) {
  const { t } = useTranslation();
  const progress = connectionButtonProgress(
    snapshot,
    action,
    installingId,
    actionPending,
  );
  const label = t(progress.labelKey);
  const glow = action === "connect" && !disabled && !progress.processing;

  return (
    <button
      type="button"
      data-connection-action={action}
      data-progress={String(progress.percent)}
      data-processing={progress.processing ? "true" : "false"}
      data-connect-glow={glow ? "available" : "off"}
      aria-busy={progress.processing}
      disabled={disabled}
      onClick={onClick}
      className={`connection-action connection-action-${variant} ${
        progress.processing ? "connection-action-processing" : ""
      } ${glow ? "connect-button-glow" : ""} ${
        variant === "primary"
          ? "bg-brand text-white shadow-lg shadow-brand/20"
          : "border border-ink/15 bg-surface"
      } relative isolate inline-flex min-h-14 min-w-0 flex-1 basis-[9.5rem] items-center justify-center rounded-2xl px-5 py-3.5 text-sm font-semibold sm:text-base disabled:cursor-not-allowed disabled:opacity-55`}
    >
      <span className="connection-action-fill-clip" aria-hidden>
        <span
          className="connection-action-fill"
          style={{ width: `${progress.percent}%` }}
        />
      </span>
      <span className="relative z-10 inline-flex min-w-0 max-w-full items-center justify-center gap-2">
        {icon}
        <span className="connection-action-label min-w-0 text-center leading-tight break-words">
          {label}
        </span>
      </span>
    </button>
  );
}
