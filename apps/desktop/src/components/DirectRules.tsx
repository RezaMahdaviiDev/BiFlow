import { Plus, RefreshCw, Route, Search, Trash2 } from "lucide-react";
import { useMemo, useState } from "react";
import { desktop } from "../api/desktop";
import type { DirectRulesDocument, RouteTestResult } from "../api/models";
import { useAppStore } from "../store/app";

export function DirectRules({ rules }: { rules: DirectRulesDocument }) {
  const { addRule, removeRule, refreshRules, actionPending } = useAppStore();
  const [input, setInput] = useState("");
  const [search, setSearch] = useState("");
  const [route, setRoute] = useState<RouteTestResult | null>(null);
  const [testing, setTesting] = useState(false);
  const filtered = useMemo(
    () => rules.rules.filter((rule) => rule.target.value.includes(search.trim().toLowerCase())),
    [rules.rules, search],
  );

  async function test(target: string) {
    setTesting(true);
    try {
      setRoute(await desktop.testRoute(target));
    } finally {
      setTesting(false);
    }
  }

  return (
    <section aria-labelledby="rules-title" className="space-y-5">
      <header>
        <h1 id="rules-title" className="text-3xl font-semibold tracking-tight">
          Direct rules
        </h1>
        <p className="mt-2 text-muted">
          Exact domains and literal IPs added here take precedence and apply without restarting the
          tunnel.
        </p>
      </header>

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
          <Search className="absolute left-3 top-3 text-muted" size={18} aria-hidden />
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

      <div className="overflow-hidden rounded-2xl border border-ink/10 bg-surface">
        {filtered.length === 0 ? (
          <p className="p-8 text-center text-muted">No matching direct rules.</p>
        ) : (
          <ul className="divide-y divide-ink/10">
            {filtered.map((rule) => (
              <li
                key={`${rule.target.kind}:${rule.target.value}`}
                className="flex flex-col gap-3 p-4 sm:flex-row sm:items-center sm:justify-between"
              >
                <div className="min-w-0">
                  <p className="truncate font-semibold">{rule.target.value}</p>
                  <p className="mt-1 truncate text-sm text-muted">
                    {rule.target.kind.toUpperCase()} · {rule.resolved_ips.join(", ") || "No resolved IP"}
                  </p>
                </div>
                <div className="flex gap-2">
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

      {route ? (
        <div className="rounded-2xl border border-brand/20 bg-brand/5 p-4" role="status">
          <p className="font-semibold">
            {route.target} → {route.outbound.toUpperCase()}
          </p>
          <p className="mt-1 text-sm text-muted">
            {route.reason.replaceAll("_", " ")} · matched {route.matched_rule ?? "none"}
          </p>
        </div>
      ) : null}
    </section>
  );
}
