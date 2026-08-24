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
import type {
  DirectRule,
  DirectRulesDocument,
  RouteTestResult,
} from "../api/models";
import type { SortState } from "../lib/tableSort";
import { sortRows, toggleSort } from "../lib/tableSort";
import { useAppStore } from "../store/app";
import { SortHeader } from "./SortHeader";

type PinnedOutbound = "direct" | "vpn" | "openvpn";
type PinnedRow = { rule: DirectRule; outbound: PinnedOutbound };
type PinnedSortKey = "target" | "kind" | "outbound";

const PINNED_SORT_ACCESSORS: Record<
  PinnedSortKey,
  (row: PinnedRow) => string | number
> = {
  target: (row) => row.rule.target.value,
  kind: (row) => row.rule.target.kind,
  outbound: (row) => row.outbound,
};

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
  const [sort, setSort] = useState<SortState<PinnedSortKey>>({
    key: "target",
    dir: "asc",
  });
  const filtered = useMemo(() => {
    const needle = search.trim().toLowerCase();
    const rows: PinnedRow[] = [
      ...rules.rules.map((rule) => ({ rule, outbound: "direct" as const })),
      ...rules.vpn_rules.map((rule) => ({ rule, outbound: "vpn" as const })),
      ...rules.openvpn_rules.map((rule) => ({
        rule,
        outbound: "openvpn" as const,
      })),
    ].filter(({ rule }) => rule.target.value.includes(needle));
    return sortRows(rows, sort, PINNED_SORT_ACCESSORS);
  }, [rules.rules, rules.vpn_rules, rules.openvpn_rules, search, sort]);
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
        <p className="mt-1 text-sm text-muted">{t("directRulesHelp")}</p>
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
          void addRule(input)
            .then(() => setInput(""))
            .catch(() => undefined);
        }}
      >
        <label className="sr-only" htmlFor="rule-input">
          {t("directRuleInput")}
        </label>
        <input
          id="rule-input"
          value={input}
          onChange={(event) => setInput(event.target.value)}
          required
          placeholder={t("directRulePlaceholder")}
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
          <div className="overflow-x-auto rounded-2xl">
            <table className="w-full min-w-[36rem] text-start text-sm">
              <thead>
                <tr className="bg-canvas text-muted">
                  <SortHeader
                    label={t("liveConnectionsHost")}
                    sortKey="target"
                    state={sort}
                    onToggle={(key) => setSort((prev) => toggleSort(prev, key))}
                  />
                  <SortHeader
                    label={t("ruleKind")}
                    sortKey="kind"
                    state={sort}
                    onToggle={(key) => setSort((prev) => toggleSort(prev, key))}
                  />
                  <SortHeader
                    label={t("liveConnectionsOutbound")}
                    sortKey="outbound"
                    state={sort}
                    onToggle={(key) => setSort((prev) => toggleSort(prev, key))}
                  />
                  <th className="px-3 py-2 text-start font-medium">
                    {t("liveConnectionsIp")}
                  </th>
                  <th className="px-3 py-2 text-start font-medium">
                    {t("tableActions")}
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-ink/10">
                {filtered.map(({ rule, outbound }) => (
                  <tr
                    key={`${outbound}:${rule.target.kind}:${rule.target.value}`}
                    className="hover:bg-canvas/60"
                  >
                    <td className="px-3 py-2 font-medium break-all">
                      {rule.target.value}
                    </td>
                    <td className="px-3 py-2 font-mono text-xs text-muted">
                      {rule.target.kind.toUpperCase()}
                    </td>
                    <td className="px-3 py-2">
                      <span
                        className={`rounded-md px-2 py-0.5 text-xs font-semibold ${routeTone(outbound)}`}
                      >
                        {t(outbound)}
                      </span>
                    </td>
                    <td className="px-3 py-2 font-mono text-xs text-muted break-all">
                      {rule.resolved_ips.join(", ") || "—"}
                    </td>
                    <td className="px-3 py-2">
                      <div className="flex gap-1.5">
                        {/* Three routes no longer fit a two-way toggle, so
                            the row carries a compact selector instead. Its
                            aria-label is the only accessible name; a visible
                            one would widen the narrowest layout. */}
                        <select
                          disabled={actionPending}
                          value={outbound}
                          aria-label={t("moveRoute", {
                            target: rule.target.value,
                          })}
                          onChange={(event) =>
                            void pinRoute(
                              rule.target.value,
                              event.target.value as PinnedOutbound,
                            ).catch(() => undefined)
                          }
                          className="rounded-lg border border-ink/15 bg-canvas py-1 ps-2 pe-6 text-xs font-semibold text-muted disabled:opacity-50"
                        >
                          <option value="direct">{t("direct")}</option>
                          <option value="vpn">{t("vpn")}</option>
                          <option value="openvpn">{t("openvpn")}</option>
                        </select>
                        <button
                          type="button"
                          disabled={testing}
                          onClick={() => void test(rule.target.value)}
                          className="rounded-lg border border-ink/15 p-1.5 text-muted hover:text-brand"
                          title={`Test route for ${rule.target.value}`}
                          aria-label={`Test route for ${rule.target.value}`}
                        >
                          <Route size={16} aria-hidden />
                        </button>
                        <button
                          type="button"
                          disabled={actionPending}
                          onClick={() => void removeRule(rule.target.value)}
                          className="rounded-lg border border-ink/15 p-1.5 text-muted hover:text-danger"
                          title={`Remove ${rule.target.value}`}
                          aria-label={`Remove ${rule.target.value}`}
                        >
                          <Trash2 size={16} aria-hidden />
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {route ? <FlowResult route={route} /> : null}
    </section>
  );
}

/** Badge colour per route, so the three are distinguishable at a glance. */
function routeTone(outbound: PinnedOutbound): string {
  switch (outbound) {
    case "vpn":
      return "bg-brand/10 text-brand";
    case "openvpn":
      return "bg-amber-400/15 text-amber-600 dark:text-amber-300";
    default:
      return "bg-success/10 text-success";
  }
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
  const vpn = route.outbound !== "direct";
  // From DIRECT the useful move is onto the tunnel; from either tunnel the
  // useful move is back to DIRECT. Pinning to the side tunnel specifically is
  // done from the rules table, where all three routes are listed.
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
