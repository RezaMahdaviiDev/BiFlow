import {
  Download,
  LoaderCircle,
  Pause,
  Play,
  Power,
  PowerOff,
  X,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type { StackPhase, StackSnapshot } from "../api/models";
import { controlsLocked } from "../lib/lifecycle";
import { useAppStore } from "../store/app";
import { AppButton, BUTTON_ICON_PX } from "./AppButton";

const progressPhases: StackPhase[] = [
  "starting_hiddify",
  "preparing_runtime",
  "validating_config",
  "starting_core",
  "checking_readiness",
];

export function BasicDashboard({ snapshot }: { snapshot: StackSnapshot }) {
  const { t } = useTranslation();
  const {
    actionPending,
    toggleConnection,
    pauseConnection,
    resumeConnection,
    cancel,
    error,
    installDependency,
  } = useAppStore();
  const active = snapshot.phase === "running" || snapshot.phase === "degraded";
  const paused = snapshot.phase === "paused";
  const locked = controlsLocked(snapshot, actionPending);
  const operating =
    progressPhases.includes(snapshot.phase) || snapshot.phase === "stopping";
  const progressIndex = progressPhases.indexOf(snapshot.phase);
  const missing = snapshot.last_error?.remediation === "install_dependency";
  const missingId =
    snapshot.last_error?.code === "MIHOMO_NOT_FOUND" ? "mihomo" : "hiddify";
  const showError =
    error ?? (snapshot.last_error ? t(snapshot.last_error.message_key) : null);

  return (
    <section
      aria-labelledby="basic-dashboard-title"
      className="flex h-full flex-col items-center justify-center gap-6 px-4 text-center"
    >
      <div className="max-w-md space-y-2">
        <h1 id="basic-dashboard-title" className="text-2xl font-semibold">
          {active
            ? t("activeTitle")
            : paused
              ? t("pausedTitle")
              : t("readyTitle")}
        </h1>
        <p className="text-sm text-muted">{t("basicModeHelp")}</p>
      </div>

      {operating ? (
        <div
          className="w-full max-w-md rounded-2xl border border-brand/15 bg-brand/5 p-4"
          role="status"
        >
          <div className="mb-2 flex justify-between text-sm font-medium">
            <span>{snapshot.phase.replaceAll("_", " ")}</span>
            <span>
              {Math.max(
                10,
                ((progressIndex + 1) / progressPhases.length) * 100,
              ).toFixed(0)}
              %
            </span>
          </div>
          <div className="h-2 overflow-hidden rounded-full bg-brand/10">
            <div
              className="h-full rounded-full bg-brand transition-all duration-300"
              style={{
                width: `${Math.max(10, ((progressIndex + 1) / progressPhases.length) * 100)}%`,
              }}
            />
          </div>
        </div>
      ) : null}

      {showError ? (
        <div
          className="w-full max-w-md rounded-2xl border border-danger/20 bg-danger/5 p-4 text-sm text-danger"
          role="alert"
        >
          <p>{showError}</p>
          {missing ? (
            <AppButton
              icon={<Download size={BUTTON_ICON_PX} aria-hidden />}
              className="mt-3 rounded-xl bg-brand px-4 py-2 font-semibold text-white"
              onClick={() => void installDependency(missingId)}
            >
              {t("install")} {missingId === "mihomo" ? "Mihomo" : "Hiddify"}
            </AppButton>
          ) : null}
        </div>
      ) : null}

      <div className="flex flex-wrap items-center justify-center gap-2">
        {operating && snapshot.operation_id ? (
          <AppButton
            icon={<X size={BUTTON_ICON_PX} aria-hidden />}
            onClick={() => void cancel()}
            className="rounded-xl border border-ink/15 bg-surface px-4 py-3 font-semibold"
          >
            {t("cancel")}
          </AppButton>
        ) : null}
        {active ? (
          <AppButton
            icon={<Pause size={BUTTON_ICON_PX} aria-hidden />}
            disabled={locked}
            onClick={() => void pauseConnection()}
            className="rounded-xl border border-ink/15 bg-surface px-4 py-3 font-semibold disabled:opacity-50"
          >
            {t("pause")}
          </AppButton>
        ) : null}
        {paused ? (
          <AppButton
            icon={<Play size={BUTTON_ICON_PX} aria-hidden />}
            disabled={locked}
            onClick={() => void resumeConnection()}
            className="min-w-36 rounded-xl bg-brand px-5 py-3 font-semibold text-white disabled:opacity-50"
          >
            {t("resume")}
          </AppButton>
        ) : null}
        {!paused ? (
          <AppButton
            icon={
              actionPending && !operating ? (
                <LoaderCircle
                  className="animate-spin"
                  size={BUTTON_ICON_PX}
                  aria-hidden
                />
              ) : active ? (
                <PowerOff size={BUTTON_ICON_PX} aria-hidden />
              ) : (
                <Power size={BUTTON_ICON_PX} aria-hidden />
              )
            }
            disabled={locked}
            onClick={() => void toggleConnection()}
            className="min-w-36 rounded-xl bg-brand px-5 py-3 font-semibold text-white disabled:opacity-50"
          >
            {active ? t("disconnect") : t("connect")}
          </AppButton>
        ) : (
          <AppButton
            icon={<PowerOff size={BUTTON_ICON_PX} aria-hidden />}
            disabled={locked}
            onClick={() => void toggleConnection()}
            className="rounded-xl border border-ink/15 bg-surface px-4 py-3 font-semibold disabled:opacity-50"
          >
            {t("disconnect")}
          </AppButton>
        )}
      </div>
    </section>
  );
}
