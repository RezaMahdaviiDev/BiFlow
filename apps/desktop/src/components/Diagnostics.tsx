import {
  CheckCircle2,
  CircleAlert,
  Download,
  FileText,
  FolderOpen,
  LoaderCircle,
  Play,
  RefreshCw,
  Route,
  Sparkles,
  Trash2,
} from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { desktop } from "../api/desktop";
import type {
  DiagnosticsReport,
  DebugLogStatus,
  ExportResult,
  FreshStartReport,
  LogEntry,
  RouteTestResult,
} from "../api/models";
import { extractHost } from "../lib/host";
import { useAppStore } from "../store/app";
import { FlowResult } from "./DirectRules";

export function Diagnostics({ report }: { report: DiagnosticsReport | null }) {
  const { t } = useTranslation();
  const { runDiagnostics, actionPending, addRule, removeRule } = useAppStore();
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [level, setLevel] = useState("all");
  const [exported, setExported] = useState<ExportResult | null>(null);
  const [target, setTarget] = useState("");
  const [route, setRoute] = useState<RouteTestResult | null>(null);
  const [testing, setTesting] = useState(false);
  const [debugLog, setDebugLog] = useState<DebugLogStatus | null>(null);
  const [logAction, setLogAction] = useState<
    "refresh" | "reveal" | "delete" | null
  >(null);
  const [logMessage, setLogMessage] = useState<string | null>(null);
  const [freshStarting, setFreshStarting] = useState(false);
  const [freshStart, setFreshStart] = useState<FreshStartReport | null>(null);
  const [freshStartError, setFreshStartError] = useState<string | null>(null);
  const [moving, setMoving] = useState(false);
  const [moveError, setMoveError] = useState<string | null>(null);

  const refreshDebugLog = () => {
    setLogAction("refresh");
    setLogMessage(null);
    void desktop
      .debugLogStatus()
      .then(setDebugLog)
      .catch((error: unknown) =>
        setLogMessage(
          error instanceof Error
            ? error.message
            : "Could not inspect debug.log",
        ),
      )
      .finally(() => setLogAction(null));
  };

  useEffect(() => {
    void desktop
      .queryLogs()
      .then(setLogs)
      .catch(() => setLogs([]));
  }, [report]);

  useEffect(() => {
    refreshDebugLog();
  }, []);

  const visibleLogs =
    level === "all" ? logs : logs.filter((entry) => entry.level === level);

  return (
    <section
      aria-labelledby="diagnostics-title"
      className="flex flex-col gap-4 pb-2"
    >
      <div className="flex shrink-0 flex-wrap items-start justify-between gap-4">
        <header>
          <h1
            id="diagnostics-title"
            className="text-2xl font-semibold tracking-tight"
          >
            Diagnostics
          </h1>
          <p className="mt-1 text-sm text-muted">
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
          // Accept whatever the user pasted — a full URL, a host:port, an IP —
          // and test the host a routing rule would actually match.
          const host = extractHost(target);
          if (!host) return;
          setTesting(true);
          setMoveError(null);
          void desktop
            .testRoute(host)
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
            disabled={testing || !extractHost(target)}
            className="inline-flex items-center justify-center gap-2 rounded-xl bg-brand px-4 py-2.5 font-semibold text-white disabled:opacity-50"
          >
            <Route size={18} aria-hidden />
            {t("testFlowButton")}
          </button>
        </div>
        {route ? (
          <div className="mt-4">
            <FlowResult
              route={route}
              moving={moving}
              onMove={(host, destination) => {
                setMoving(true);
                setMoveError(null);
                const applied =
                  destination === "direct" ? addRule(host) : removeRule(host);
                void applied
                  .then(() => desktop.testRoute(host))
                  .then(setRoute)
                  .catch((error: unknown) =>
                    setMoveError(
                      error instanceof Error ? error.message : String(error),
                    ),
                  )
                  .finally(() => setMoving(false));
              }}
            />
            {moveError ? (
              <p className="mt-2 text-sm text-danger" role="alert">
                {moveError}
              </p>
            ) : null}
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
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <h2 className="font-semibold">{t("freshHiddify")}</h2>
            <p className="mt-1 max-w-prose text-sm text-muted">
              {t("freshHiddifyHelp")}
            </p>
          </div>
          <button
            type="button"
            disabled={freshStarting}
            onClick={() => {
              if (!window.confirm(t("freshHiddifyConfirm"))) return;
              setFreshStarting(true);
              setFreshStart(null);
              setFreshStartError(null);
              void desktop
                .freshHiddifyStart()
                .then(setFreshStart)
                .catch((error: unknown) =>
                  setFreshStartError(
                    error instanceof Error ? error.message : String(error),
                  ),
                )
                .finally(() => setFreshStarting(false));
            }}
            className="inline-flex items-center gap-2 rounded-xl border border-ink/15 px-4 py-2.5 font-semibold disabled:opacity-50"
          >
            {freshStarting ? (
              <LoaderCircle className="animate-spin" size={18} aria-hidden />
            ) : (
              <Sparkles size={18} aria-hidden />
            )}
            {t("freshHiddifyButton")}
          </button>
        </div>
        {freshStart ? (
          <div
            className="mt-4 space-y-1 rounded-xl bg-canvas p-3 text-sm"
            role="status"
          >
            <p className="font-medium">
              {freshStart.started
                ? t("freshHiddifyDone")
                : t("freshHiddifyNotStarted")}
            </p>
            <p className="text-muted">
              {t("freshHiddifyCleared")}:{" "}
              {freshStart.cleared.length > 0
                ? freshStart.cleared.join(", ")
                : t("freshHiddifyNothing")}
            </p>
            <p className="text-muted">
              {t("freshHiddifyKept")}: {freshStart.preserved.join(", ")}
            </p>
            {freshStart.cleared.length > 0 ? (
              <p className="min-w-0 break-all font-mono text-xs text-muted">
                {t("freshHiddifyBackup")}: {freshStart.backup_dir}
              </p>
            ) : null}
          </div>
        ) : null}
        {freshStartError ? (
          <p
            className="mt-4 rounded-xl bg-canvas p-3 text-sm text-danger"
            role="alert"
          >
            {freshStartError}
          </p>
        ) : null}
      </div>

      <div className="rounded-2xl border border-ink/10 bg-surface p-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <h2 className="font-semibold">Permanent debug.log</h2>
            <p className="mt-1 text-sm text-muted">
              Preserved across app restarts until you delete it. Send this file
              to the support team when reporting a problem.
            </p>
            {debugLog ? (
              <dl className="mt-3 space-y-1 text-sm">
                <div className="flex gap-2">
                  <dt className="font-medium">Size:</dt>
                  <dd data-testid="debug-log-size">
                    {formatBytes(debugLog.size_bytes)}
                  </dd>
                </div>
                <div className="flex min-w-0 gap-2">
                  <dt className="shrink-0 font-medium">Location:</dt>
                  <dd className="min-w-0 break-all font-mono text-xs text-muted">
                    {debugLog.path}
                  </dd>
                </div>
              </dl>
            ) : (
              <p className="mt-3 text-sm text-muted">Reading file status…</p>
            )}
          </div>
          <div className="flex flex-wrap gap-2">
            <button
              type="button"
              disabled={logAction !== null}
              onClick={refreshDebugLog}
              className="inline-flex items-center gap-2 rounded-xl border border-ink/15 px-3 py-2 text-sm font-semibold disabled:opacity-50"
            >
              <RefreshCw size={16} aria-hidden /> Refresh size
            </button>
            <button
              type="button"
              disabled={logAction !== null || debugLog === null}
              onClick={() => {
                setLogAction("reveal");
                setLogMessage(null);
                void desktop
                  .revealDebugLog()
                  .then((status) => {
                    setDebugLog(status);
                    setLogMessage("Opened the folder containing debug.log.");
                  })
                  .catch((error: unknown) =>
                    setLogMessage(
                      error instanceof Error
                        ? error.message
                        : "Could not locate debug.log",
                    ),
                  )
                  .finally(() => setLogAction(null));
              }}
              className="inline-flex items-center gap-2 rounded-xl border border-ink/15 px-3 py-2 text-sm font-semibold disabled:opacity-50"
            >
              <FolderOpen size={16} aria-hidden /> Show file
            </button>
            <button
              type="button"
              disabled={logAction !== null || debugLog === null}
              onClick={() => {
                if (
                  !window.confirm(
                    "Delete all existing debug.log content? New app events will continue logging immediately.",
                  )
                ) {
                  return;
                }
                setLogAction("delete");
                setLogMessage(null);
                void desktop
                  .deleteDebugLog()
                  .then((status) => {
                    setDebugLog(status);
                    setLogMessage(
                      "Previous log content was deleted. New events are being recorded.",
                    );
                  })
                  .catch((error: unknown) =>
                    setLogMessage(
                      error instanceof Error
                        ? error.message
                        : "Could not delete debug.log",
                    ),
                  )
                  .finally(() => setLogAction(null));
              }}
              className="inline-flex items-center gap-2 rounded-xl border border-danger/30 px-3 py-2 text-sm font-semibold text-danger disabled:opacity-50"
            >
              <Trash2 size={16} aria-hidden /> Delete log
            </button>
          </div>
        </div>
        {logMessage ? (
          <p className="mt-3 rounded-xl bg-canvas p-3 text-sm" role="status">
            {logMessage}
          </p>
        ) : null}
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
        <div className="diagnostics-selectable max-h-48 overflow-auto rounded-xl bg-canvas p-3 font-mono text-xs">
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
              Versions, redacted config, state, and the permanent debug.log.
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

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KiB", "MiB", "GiB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value < 10 ? 1 : 0)} ${units[unit]}`;
}
