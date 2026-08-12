import { Activity, ArrowDownUp, CircleDot, Gauge, Globe2, Network } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { StackPhase, StackSnapshot } from "../api/models";
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
  const { actionPending, toggleConnection, cancel, boot } = useAppStore();
  const active = snapshot.phase === "running" || snapshot.phase === "degraded";
  const operating = progressPhases.includes(snapshot.phase) || snapshot.phase === "stopping";
  const progressIndex = progressPhases.indexOf(snapshot.phase);

  return (
    <section aria-labelledby="dashboard-title" className="space-y-6">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <p className="mb-1 text-sm font-medium text-brand">{t("status")}</p>
          <h1 id="dashboard-title" className="text-3xl font-semibold tracking-tight">
            {active ? "Protected split routing is active" : "Ready when you are"}
          </h1>
          <p className="mt-2 max-w-2xl text-muted">
            Iranian and private traffic stays direct. Other traffic follows your existing Hiddify
            connection.
          </p>
        </div>
        <div className="flex gap-2">
          {operating && snapshot.operation_id ? (
            <button
              type="button"
              onClick={() => void cancel()}
              className="rounded-xl border border-ink/15 bg-surface px-4 py-3 font-semibold"
            >
              {t("cancel")}
            </button>
          ) : null}
          <button
            type="button"
            disabled={actionPending || operating}
            onClick={() => void toggleConnection()}
            className="min-w-36 rounded-xl bg-brand px-5 py-3 font-semibold text-white shadow-lg shadow-brand/20 transition hover:brightness-105 disabled:cursor-not-allowed disabled:opacity-55"
          >
            {active ? t("disconnect") : t("connect")}
          </button>
        </div>
      </div>

      {operating ? (
        <div className="rounded-2xl border border-brand/15 bg-brand/5 p-4" role="status">
          <div className="mb-2 flex justify-between text-sm font-medium">
            <span>{snapshot.phase.replaceAll("_", " ")}</span>
            <span>{Math.max(10, ((progressIndex + 1) / progressPhases.length) * 100).toFixed(0)}%</span>
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
        <Metric icon={<Network aria-hidden />} label={t("backend")} value="External Hiddify" />
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
        <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
          <Component name="Hiddify" status={snapshot.hiddify.phase} icon={<CircleDot />} />
          <Component name="Mihomo" status={snapshot.mihomo.phase} icon={<Activity />} />
          <Component name="TUN" status={snapshot.tun.phase} icon={<ArrowDownUp />} />
          <Component name="DNS" status={snapshot.dns.phase} icon={<Network />} />
        </div>
      </div>

      <p className="text-xs text-muted">
        {t("lastUpdated")}: {new Date(snapshot.updated_at).toLocaleTimeString()}
        {boot?.mock_mode ? ` · ${t("mockMode")}` : ""}
      </p>
    </section>
  );
}

function Metric({ icon, label, value }: { icon: React.ReactNode; label: string; value: string }) {
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
}: {
  name: string;
  status: StackSnapshot["mihomo"]["phase"];
  icon: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between rounded-2xl border border-ink/10 bg-surface p-4">
      <div className="flex items-center gap-3">
        <span className="text-muted" aria-hidden>
          {icon}
        </span>
        <span className="font-semibold">{name}</span>
      </div>
      <StatusPill phase={status} />
    </div>
  );
}
