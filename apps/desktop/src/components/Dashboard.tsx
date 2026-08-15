import {
  Activity,
  ArrowDownUp,
  CircleDot,
  Download,
  Gauge,
  Globe2,
  LoaderCircle,
  Network,
  ShieldCheck,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ComponentStatus, StackPhase, StackSnapshot } from "../api/models";
import { useAppStore } from "../store/app";
import { StatusPill } from "./StatusPill";

const progressPhases: StackPhase[] = [
  "starting_hiddify",
  "preparing_runtime",
  "validating_config",
  "starting_core",
  "checking_readiness",
];

export function Dashboard({ snapshot }: { snapshot: StackSnapshot }) {
  const { t } = useTranslation();
  const {
    actionPending,
    toggleConnection,
    pauseConnection,
    resumeConnection,
    cancel,
    boot,
    dependencies,
    installingId,
    installDependency,
    installHelper,
  } = useAppStore();
  const active = snapshot.phase === "running" || snapshot.phase === "degraded";
  const paused = snapshot.phase === "paused";
  const operating =
    progressPhases.includes(snapshot.phase) || snapshot.phase === "stopping";
  const progressIndex = progressPhases.indexOf(snapshot.phase);
  const needsAttention = [
    snapshot.helper,
    snapshot.hiddify,
    snapshot.mihomo,
    snapshot.tun,
    snapshot.dns,
  ].some(({ phase }) => phase === "error" || phase === "unavailable");

  return (
    <section
      aria-labelledby="dashboard-title"
      className="flex h-full min-h-0 flex-col gap-4 overflow-y-auto"
    >
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <p className="mb-1 text-sm font-medium text-brand">{t("status")}</p>
          <h1
            id="dashboard-title"
            className="text-2xl font-semibold tracking-tight"
          >
            {active
              ? t("activeTitle")
              : paused
                ? t("pausedTitle")
                : needsAttention
                  ? t("setupNeedsAttention")
                  : t("readyTitle")}
          </h1>
          <p className="mt-2 max-w-2xl text-muted">{t("routingSummary")}</p>
        </div>
        <div className="flex flex-wrap gap-2">
          {operating && snapshot.operation_id ? (
            <button
              type="button"
              onClick={() => void cancel()}
              className="rounded-xl border border-ink/15 bg-surface px-4 py-3 font-semibold"
            >
              {t("cancel")}
            </button>
          ) : null}
          {active ? (
            <button
              type="button"
              disabled={actionPending || operating}
              onClick={() => void pauseConnection()}
              className="rounded-xl border border-ink/15 bg-surface px-4 py-3 font-semibold"
            >
              {t("pause")}
            </button>
          ) : null}
          {paused ? (
            <button
              type="button"
              disabled={actionPending || operating}
              onClick={() => void resumeConnection()}
              className="min-w-36 rounded-xl bg-brand px-5 py-3 font-semibold text-white shadow-lg shadow-brand/20 transition hover:brightness-105 disabled:cursor-not-allowed disabled:opacity-55"
            >
              {t("resume")}
            </button>
          ) : null}
          {!paused ? (
            <button
              type="button"
              disabled={actionPending || operating}
              onClick={() => void toggleConnection()}
              className="min-w-36 rounded-xl bg-brand px-5 py-3 font-semibold text-white shadow-lg shadow-brand/20 transition hover:brightness-105 disabled:cursor-not-allowed disabled:opacity-55"
            >
              {active ? t("disconnect") : t("connect")}
            </button>
          ) : (
            <button
              type="button"
              disabled={actionPending || operating}
              onClick={() => void toggleConnection()}
              className="rounded-xl border border-ink/15 bg-surface px-4 py-3 font-semibold"
            >
              {t("disconnect")}
            </button>
          )}
        </div>
      </div>

      {operating ? (
        <div
          className="rounded-2xl border border-brand/15 bg-brand/5 p-4"
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

      <div className="grid gap-4 md:grid-cols-3">
        <Metric
          icon={<Globe2 aria-hidden />}
          label={t("exitIp")}
          value={snapshot.exit_ip ?? t("noExitIp")}
        />
        <Metric
          icon={<Network aria-hidden />}
          label={t("backend")}
          value="External Hiddify"
        />
        <Metric
          icon={<Gauge aria-hidden />}
          label={t("providers")}
          value={`${snapshot.providers.ready} / ${snapshot.providers.total}`}
        />
      </div>

      <div>
        <div className="mb-3 flex items-center justify-between">
          <h2 className="text-lg font-semibold">{t("components")}</h2>
          <StatusPill phase={snapshot.phase} />
        </div>
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-5">
          <Component
            name={t("helper")}
            status={snapshot.helper}
            icon={<ShieldCheck />}
            installed={
              snapshot.helper.phase !== "unavailable" &&
              snapshot.helper.phase !== "error"
            }
            installing={installingId === "helper"}
            onInstall={() => void installHelper()}
          />
          <Component
            name="Hiddify"
            status={snapshot.hiddify}
            icon={<CircleDot />}
            installed={
              dependencies.find((item) => item.id === "hiddify")?.installed
            }
            installing={installingId === "hiddify"}
            onInstall={() => void installDependency("hiddify")}
          />
          <Component
            name="Mihomo"
            status={snapshot.mihomo}
            icon={<Activity />}
            installed={
              dependencies.find((item) => item.id === "mihomo")?.installed
            }
            installing={installingId === "mihomo"}
            onInstall={() => void installDependency("mihomo")}
          />
          <Component name="TUN" status={snapshot.tun} icon={<ArrowDownUp />} />
          <Component name="DNS" status={snapshot.dns} icon={<Network />} />
        </div>
      </div>

      {active ? <TrafficFlow /> : null}

      <p className="text-xs text-muted">
        {t("lastUpdated")}: {new Date(snapshot.updated_at).toLocaleTimeString()}
        {boot?.mock_mode ? ` · ${t("mockMode")}` : ""}
      </p>
    </section>
  );
}

function Metric({
  icon,
  label,
  value,
}: {
  icon: React.ReactNode;
  label: string;
  value: string;
}) {
  return (
    <div className="rounded-2xl border border-ink/10 bg-surface p-5 shadow-card">
      <div className="mb-4 flex h-10 w-10 items-center justify-center rounded-xl bg-brand/10 text-brand">
        {icon}
      </div>
      <p className="text-sm text-muted">{label}</p>
      <p className="mt-1 truncate text-lg font-semibold" title={value}>
        {value}
      </p>
    </div>
  );
}

function Component({
  name,
  status,
  icon,
  installed,
  installing,
  onInstall,
}: {
  name: string;
  status: ComponentStatus;
  icon: React.ReactNode;
  installed?: boolean;
  installing?: boolean;
  onInstall?: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="rounded-2xl border border-ink/10 bg-surface p-4">
      <div className="flex items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-3">
          <span className="text-muted" aria-hidden>
            {icon}
          </span>
          <span className="font-semibold">{name}</span>
        </div>
        <StatusPill phase={status.phase} />
      </div>
      <p className="mt-3 min-h-10 text-xs leading-5 text-muted">
        {status.message ?? t("statusDetailUnavailable")}
      </p>
      {installed === false && onInstall ? (
        <button
          type="button"
          disabled={installing}
          onClick={onInstall}
          className="mt-3 inline-flex items-center gap-1.5 rounded-lg bg-brand px-3 py-1.5 text-xs font-semibold text-white disabled:opacity-50"
        >
          {installing ? (
            <LoaderCircle className="animate-spin" size={14} aria-hidden />
          ) : (
            <Download size={14} aria-hidden />
          )}
          {installing ? t("installing") : t("install")}
        </button>
      ) : null}
    </div>
  );
}

function TrafficFlow() {
  const { t } = useTranslation();
  return (
    <section className="overflow-hidden rounded-2xl border border-brand/15 bg-surface p-5 shadow-card">
      <h2 className="text-lg font-semibold">{t("liveRouting")}</h2>
      <p className="mt-1 text-sm text-muted">{t("liveRoutingHelp")}</p>
      <svg
        className="mt-4 h-auto w-full"
        viewBox="0 0 760 220"
        role="img"
        aria-label={t("liveRoutingAria")}
      >
        <defs>
          <marker
            id="traffic-arrow-direct"
            markerWidth="8"
            markerHeight="8"
            refX="7"
            refY="4"
            orient="auto"
          >
            <path d="M0,0 L8,4 L0,8 Z" fill="rgb(var(--success))" />
          </marker>
          <marker
            id="traffic-arrow-vpn"
            markerWidth="8"
            markerHeight="8"
            refX="7"
            refY="4"
            orient="auto"
          >
            <path d="M0,0 L8,4 L0,8 Z" fill="rgb(var(--brand))" />
          </marker>
        </defs>
        <path
          className="traffic-flow-base"
          d="M96 110 H292 C390 110 410 55 520 55 H660"
        />
        <path
          className="traffic-flow-base"
          d="M96 110 H292 C390 110 410 165 520 165 H660"
        />
        <path
          className="traffic-flow-route traffic-flow-route-direct"
          d="M96 110 H292 C390 110 410 55 520 55 H660"
          markerEnd="url(#traffic-arrow-direct)"
        />
        <path
          className="traffic-flow-route traffic-flow-route-vpn"
          d="M96 110 H292 C390 110 410 165 520 165 H660"
          markerEnd="url(#traffic-arrow-vpn)"
        />
        <circle cx="76" cy="110" r="30" fill="rgb(var(--brand) / 0.12)" />
        <circle cx="76" cy="110" r="9" fill="rgb(var(--brand))" />
        <circle cx="686" cy="55" r="27" fill="rgb(var(--success) / 0.12)" />
        <circle cx="686" cy="55" r="8" fill="rgb(var(--success))" />
        <circle cx="686" cy="165" r="27" fill="rgb(var(--brand) / 0.12)" />
        <circle cx="686" cy="165" r="8" fill="rgb(var(--brand))" />
        <text className="traffic-flow-label" x="76" y="158" textAnchor="middle">
          {t("device")}
        </text>
        <text className="traffic-flow-label" x="686" y="97" textAnchor="middle">
          {t("direct")}
        </text>
        <text
          className="traffic-flow-label"
          x="686"
          y="207"
          textAnchor="middle"
        >
          {t("vpn")}
        </text>
      </svg>
    </section>
  );
}
