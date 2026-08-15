import {
  CloudDownload,
  LoaderCircle,
  Minus,
  Plus,
  RefreshCw,
  Route,
  Search,
  Trash2,
} from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { desktop } from "../api/desktop";
import type { DirectRulesDocument, RouteTestResult } from "../api/models";
import { useAppStore } from "../store/app";

export function DirectRules({ rules }: { rules: DirectRulesDocument }) {
  const { t } = useTranslation();
  const {
    addRule,
    pinRoute,
    removeRule,
    refreshRules,
    syncCloudRules,
    cloudRules,
    actionPending,
  } = useAppStore();
  const [input, setInput] = useState("");
  const [search, setSearch] = useState("");
  const [route, setRoute] = useState<RouteTestResult | null>(null);
  const [testing, setTesting] = useState(false);
  const filtered = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return [
      ...rules.rules.map((rule) => ({ rule, outbound: "direct" as const })),
      ...rules.vpn_rules.map((rule) => ({ rule, outbound: "vpn" as const })),
    ].filter(({ rule }) => rule.target.value.includes(needle));
  }, [rules.rules, rules.vpn_rules, search]);
  const synced = cloudRules?.last_synced_at
    ? new Date(cloudRules.last_synced_at).toLocaleString()
    : t("neverSynced");
  const snapshotRevision =
    cloudRules?.snapshot_revision?.slice(0, 12) ??
    (cloudRules?.source === "bundled" ? t("bundledSnapshotRevision") : "—");

  async function test(target: string) {
    setTesting(true);
    try {
      setRoute(await desktop.testRoute(target));
    } finally {
      setTesting(false);
    }
  }

  return (
    <section aria-labelledby="rules-title" className="flex flex-col gap-4 pb-2">
      <header className="shrink-0">
        <h1 id="rules-title" className="text-2xl font-semibold tracking-tight">
          Direct rules
        </h1>
        <p className="mt-1 text-sm text-muted">
          Exact domains and literal IPs added here take precedence and apply
          without restarting the tunnel.
        </p>
      </header>

      <div className="rounded-2xl border border-ink/10 bg-surface p-5">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <h2 className="font-semibold">{t("cloudRules")}</h2>
            <p className="mt-1 max-w-2xl text-sm text-muted">
              {t("cloudRulesHelp")}
            </p>
          </div>
          <button
            type="button"
            disabled={actionPending}
            onClick={() => void syncCloudRules()}
            className="inline-flex items-center justify-center gap-2 rounded-xl bg-brand px-4 py-2.5 font-semibold text-white disabled:opacity-50"
          >
            <CloudDownload size={18} aria-hidden />
            {actionPending ? t("syncing") : t("updateFromCloud")}
          </button>
        </div>
        <dl className="mt-4 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          <Stat
            label={t("domains")}
            value={(cloudRules?.domain_count ?? 0).toLocaleString()}
          />
          <Stat
            label={t("ipRanges")}
            value={(cloudRules?.ip_count ?? 0).toLocaleString()}
          />
          <Stat label={t("lastSynced")} value={synced} />
          <Stat label={t("snapshotRevision")} value={snapshotRevision} />
        </dl>
        <p className="mt-3 text-sm text-muted">
          {t("cloudRulesSource")}: devlifeX/BiFlow
        </p>
      </div>

      <form
        className="flex flex-col gap-2 rounded-2xl border border-ink/10 bg-surface p-4 sm:flex-row"
        onSubmit={(event) => {
          event.preventDefault();
          if (!input.trim()) return;
          void addRule(input).then(() => setInput(""));
        }}
      >
        <label className="sr-only" htmlFor="rule-input">
          Exact domain or IP
        </label>
        <input
          id="rule-input"
          value={input}
          onChange={(event) => setInput(event.target.value)}
          required
          placeholder="example.ir or 203.0.113.8"
          className="min-w-0 flex-1 rounded-xl border-ink/15 bg-canvas"
        />
        <button
          disabled={actionPending}
          className="inline-flex items-center justify-center gap-2 rounded-xl bg-brand px-4 py-2.5 font-semibold text-white disabled:opacity-50"
        >
          <Plus size={18} aria-hidden /> Add rule
        </button>
      </form>

      <div className="flex flex-col gap-3 sm:flex-row">
        <label className="relative flex-1">
          <span className="sr-only">Search rules</span>
          <Search
            className="absolute left-3 top-3 text-muted"
            size={18}
            aria-hidden
          />
          <input
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder="Search direct rules"
            className="w-full rounded-xl border-ink/15 bg-surface pl-10"
          />
        </label>
        <button
          type="button"
          disabled={actionPending}
          onClick={() => void refreshRules()}
          className="inline-flex items-center justify-center gap-2 rounded-xl border border-ink/15 bg-surface px-4 py-2.5 font-semibold"
        >
          <RefreshCw size={18} aria-hidden /> Refresh resolutions
        </button>
      </div>

      <div className="rounded-2xl border border-ink/10 bg-surface">
        {filtered.length === 0 ? (
          <p className="p-8 text-center text-muted">
            No matching direct rules.
          </p>
        ) : (
          <ul className="divide-y divide-ink/10">
            {filtered.map(({ rule, outbound }) => (
              <li
                key={`${outbound}:${rule.target.kind}:${rule.target.value}`}
                className="flex flex-col gap-3 p-4 sm:flex-row sm:items-center sm:justify-between"
              >
                <div className="min-w-0">
                  <p className="flex items-center gap-2 truncate font-semibold">
                    <span className="truncate">{rule.target.value}</span>
                    <span
                      className={`shrink-0 rounded-md px-2 py-0.5 text-xs font-semibold ${
                        outbound === "vpn"
                          ? "bg-brand/10 text-brand"
                          : "bg-success/10 text-success"
                      }`}
                    >
                      {outbound === "vpn" ? t("vpn") : t("direct")}
                    </span>
                  </p>
                  <p className="mt-1 truncate text-sm text-muted">
                    {rule.target.kind.toUpperCase()} ·{" "}
                    {rule.resolved_ips.join(", ") || "No resolved IP"}
                  </p>
                </div>
                <div className="flex gap-2">
                  <button
                    type="button"
                    disabled={actionPending}
                    onClick={() =>
                      void pinRoute(
                        rule.target.value,
                        outbound === "vpn" ? "direct" : "vpn",
                      )
                    }
                    className="rounded-lg border border-ink/15 p-2 text-muted hover:text-brand"
                    aria-label={
                      outbound === "vpn"
                        ? t("moveToDirect", { target: rule.target.value })
                        : t("moveToVpn", { target: rule.target.value })
                    }
                  >
                    {outbound === "vpn" ? (
                      <Minus size={18} aria-hidden />
                    ) : (
                      <Plus size={18} aria-hidden />
                    )}
                  </button>
                  <button
                    type="button"
                    disabled={testing}
                    onClick={() => void test(rule.target.value)}
                    className="rounded-lg border border-ink/15 p-2 text-muted hover:text-brand"
                    aria-label={`Test route for ${rule.target.value}`}
                  >
                    <Route size={18} aria-hidden />
                  </button>
                  <button
                    type="button"
                    disabled={actionPending}
                    onClick={() => void removeRule(rule.target.value)}
                    className="rounded-lg border border-ink/15 p-2 text-muted hover:text-danger"
                    aria-label={`Remove ${rule.target.value}`}
                  >
                    <Trash2 size={18} aria-hidden />
                  </button>
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>

      {route ? <FlowResult route={route} /> : null}
    </section>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-xl bg-canvas p-4">
      <dt className="text-sm text-muted">{label}</dt>
      <dd className="mt-1 text-xl font-semibold">{value}</dd>
    </div>
  );
}

export function FlowResult({
  route,
  onMove,
  moving = false,
}: {
  route: RouteTestResult;
  /** Sends the host the other way: to direct when it is on the VPN, and back
   * to the VPN when it is a direct rule. Omitted where there is nothing to
   * act on. */
  onMove?: (target: string, to: "direct" | "vpn") => void;
  moving?: boolean;
}) {
  const { t } = useTranslation();
  const vpn = route.outbound === "vpn";
  const destination = vpn ? "direct" : "vpn";
  // Both directions are real pins now, so the only host that cannot move is a
  // loopback/LAN/CGNAT address: forcing those through the tunnel would cut the
  // machine off from its own network.
  const actionable = route.reason !== "private_or_local";
  return (
    <div
      className={`rounded-2xl border p-4 ${vpn ? "border-brand/20 bg-brand/5" : "border-success/20 bg-success/5"}`}
      role="status"
    >
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="min-w-0 break-all font-semibold">
          {route.target} → {route.outbound.toUpperCase()}
        </p>
        {onMove && actionable ? (
          <button
            type="button"
            disabled={moving}
            onClick={() => onMove(route.target, destination)}
            className="inline-flex shrink-0 items-center gap-2 rounded-xl border border-ink/15 px-3 py-2 text-sm font-semibold disabled:opacity-50"
          >
            {moving ? (
              <LoaderCircle className="animate-spin" size={16} aria-hidden />
            ) : vpn ? (
              <Plus size={16} aria-hidden />
            ) : (
              <Minus size={16} aria-hidden />
            )}
            {vpn
              ? t("moveToDirect", { target: route.target })
              : t("moveToVpn", { target: route.target })}
          </button>
        ) : null}
      </div>
      <p className="mt-1 text-sm text-muted">
        {route.reason.replaceAll("_", " ")} · matched{" "}
        {route.matched_rule ?? "none"}
      </p>
      {onMove && !actionable ? (
        <p className="mt-2 text-sm text-muted">{t("moveLocalUnavailable")}</p>
      ) : null}
    </div>
  );
}
