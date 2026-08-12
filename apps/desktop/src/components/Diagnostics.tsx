import {
  CheckCircle2,
  CircleAlert,
  Download,
  FileText,
  LoaderCircle,
  Play,
  Route,
} from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { desktop } from "../api/desktop";
import type {
  DiagnosticsReport,
  ExportResult,
  LogEntry,
  RouteTestResult,
} from "../api/models";
import { useAppStore } from "../store/app";
import { FlowResult } from "./DirectRules";

export function Diagnostics({ report }: { report: DiagnosticsReport | null }) {
  const { t } = useTranslation();
  const { runDiagnostics, actionPending } = useAppStore();
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [level, setLevel] = useState("all");
  const [exported, setExported] = useState<ExportResult | null>(null);
  const [target, setTarget] = useState("");
  const [route, setRoute] = useState<RouteTestResult | null>(null);
  const [testing, setTesting] = useState(false);

  useEffect(() => {
    void desktop
      .queryLogs()
      .then(setLogs)
      .catch(() => setLogs([]));
  }, [report]);

  const visibleLogs =
    level === "all" ? logs : logs.filter((entry) => entry.level === level);

  return (
    <section aria-labelledby="diagnostics-title" className="space-y-5">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <header>
          <h1
            id="diagnostics-title"
            className="text-3xl font-semibold tracking-tight"
          >
            Diagnostics
          </h1>
          <p className="mt-2 text-muted">
            Check the helper, upstream, providers, tunnel cleanup, and routing
            in one bounded run.
          </p>
        </header>
        <button
          type="button"
          disabled={actionPending}
          onClick={() => void runDiagnostics()}
          className="inline-flex items-center gap-2 rounded-xl bg-brand px-4 py-3 font-semibold text-white disabled:opacity-50"
        >
          {actionPending ? (
            <LoaderCircle className="animate-spin" size={18} aria-hidden />
          ) : (
            <Play size={18} aria-hidden />
          )}
          Run full diagnostics
        </button>
      </div>

      <form
        className="rounded-2xl border border-ink/10 bg-surface p-4"
        onSubmit={(event) => {
          event.preventDefault();
          if (!target.trim()) return;
          setTesting(true);
          void desktop
            .testRoute(target.trim())
            .then(setRoute)
            .finally(() => setTesting(false));
        }}
      >
        <h2 className="font-semibold">{t("testFlow")}</h2>
        <p className="mt-1 text-sm text-muted">{t("testFlowHelp")}</p>
        <div className="mt-3 flex flex-col gap-2 sm:flex-row">
          <label className="sr-only" htmlFor="flow-target">
            {t("testFlow")}
          </label>
          <input
            id="flow-target"
            value={target}
            onChange={(event) => setTarget(event.target.value)}
            placeholder={t("testFlowPlaceholder")}
            className="min-w-0 flex-1 rounded-xl border-ink/15 bg-canvas"
          />
          <button
            type="submit"
            disabled={testing || !target.trim()}
            className="inline-flex items-center justify-center gap-2 rounded-xl bg-brand px-4 py-2.5 font-semibold text-white disabled:opacity-50"
          >
            <Route size={18} aria-hidden />
            {t("testFlowButton")}
          </button>
        </div>
        {route ? (
          <div className="mt-4">
            <FlowResult route={route} />
          </div>
        ) : null}
      </form>

      <div className="rounded-2xl border border-ink/10 bg-surface p-4">
        <h2 className="mb-3 font-semibold">Test timeline</h2>
        {!report ? (
          <p className="text-sm text-muted">
            No diagnostic run in this session.
          </p>
        ) : (
          <ol className="space-y-3">
            {report.steps.map((step) => (
              <li key={step.id} className="flex items-start gap-3">
                {step.status === "passed" ? (
                  <CheckCircle2
                    className="mt-0.5 text-success"
                    size={19}
                    aria-hidden
                  />
                ) : step.status === "warning" || step.status === "failed" ? (
                  <CircleAlert
                    className="mt-0.5 text-amber-500"
                    size={19}
                    aria-hidden
                  />
                ) : (
                  <LoaderCircle
                    className="mt-0.5 text-muted"
                    size={19}
                    aria-hidden
                  />
                )}
                <div>
                  <p className="font-medium">{step.label}</p>
                  {step.detail ? (
                    <p className="text-sm text-muted">{step.detail}</p>
                  ) : null}
                </div>
              </li>
            ))}
          </ol>
        )}
      </div>

      <div className="rounded-2xl border border-ink/10 bg-surface p-4">
        <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
          <h2 className="font-semibold">Recent redacted logs</h2>
          <select
            aria-label="Log level"
            value={level}
            onChange={(event) => setLevel(event.target.value)}
            className="rounded-lg border-ink/15 bg-canvas py-1.5 text-sm"
          >
            <option value="all">All levels</option>
            <option value="info">Info</option>
            <option value="warn">Warning</option>
            <option value="error">Error</option>
          </select>
        </div>
        <div className="max-h-72 overflow-auto rounded-xl bg-canvas p-3 font-mono text-xs">
          {visibleLogs.length === 0 ? (
            <p className="text-muted">No logs in this filter.</p>
          ) : (
            visibleLogs.map((entry, index) => (
              <p
                key={`${entry.timestamp}-${index}`}
                className="mb-1 whitespace-pre-wrap break-all"
              >
                <span className="text-muted">{entry.timestamp}</span>{" "}
                {entry.level.toUpperCase()} {entry.event}{" "}
                {JSON.stringify(entry.fields)}
              </p>
            ))
          )}
        </div>
      </div>

      <div className="rounded-2xl border border-ink/10 bg-surface p-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="font-semibold">Support bundle</h2>
            <p className="mt-1 text-sm text-muted">
              Versions, redacted config, state, bounded logs, and diagnostic
              results only.
            </p>
          </div>
          <button
            type="button"
            onClick={() => void desktop.exportBundle().then(setExported)}
            className="inline-flex items-center gap-2 rounded-xl border border-ink/15 px-4 py-2.5 font-semibold"
          >
            <Download size={18} aria-hidden /> Export
          </button>
        </div>
        {exported ? (
          <div className="mt-4 rounded-xl bg-canvas p-3 text-sm" role="status">
            <p className="flex items-center gap-2 font-medium">
              <FileText size={17} aria-hidden /> {exported.path}
            </p>
            <p className="mt-1 text-muted">
              Included: {exported.files.join(", ")}
            </p>
          </div>
        ) : null}
      </div>
    </section>
  );
}
