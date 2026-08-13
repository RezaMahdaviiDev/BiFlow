import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import {
  MAX_DELTA_RATIO,
  assertCountDelta,
  normalizeProviderBytes,
  parseProviderLine,
  renderSnapshotMarkdown,
} from "./sync-rules.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

test("normalizes, dedupes, and rejects unsafe provider lines", () => {
  const bytes = Buffer.from(
    "# header\r\n+.Example.COM\n+.example.com\n+.safe.ir\n",
    "utf8",
  );
  const normalized = normalizeProviderBytes("domain", bytes).toString("utf8");
  assert.equal(normalized, "# header\n+.example.com\n+.safe.ir\n");
  assert.equal(parseProviderLine("ip_cidr", "10.0.0.0/8"), "10.0.0.0/8");
  assert.equal(parseProviderLine("ip_cidr", "fc00::/7"), "fc00::/7");
  assert.throws(() => parseProviderLine("domain", "https://evil.example"));
  assert.throws(() => parseProviderLine("domain", "bad..example.com"));
  assert.throws(() => parseProviderLine("ip_cidr", "999.1.1.1/32"));
});

test("rejects count deltas above the reviewed maximum", () => {
  const previous = {
    rules: [
      { id: "iran-domains", file: "iran-domains.txt", entry_count: 1000 },
    ],
  };
  assertCountDelta(previous, {
    id: "iran-domains",
    file: "iran-domains.txt",
    entry_count: 1100,
  });
  assert.throws(
    () =>
      assertCountDelta(previous, {
        id: "iran-domains",
        file: "iran-domains.txt",
        entry_count: 1000 + Math.round(1000 * MAX_DELTA_RATIO) + 1,
      }),
    /max allowed/,
  );
});

test("snapshot markdown names BiFlow as the runtime source", () => {
  const markdown = renderSnapshotMarkdown({
    repository: "Chocolate4U/Iran-clash-rules",
    runtime_repository: "devlifeX/BiFlow",
    license: "GPL-3.0",
    commit: "a".repeat(40),
    fetched_at: "2026-08-13T00:00:00.000Z",
    rules: [
      {
        file: "private.txt",
        entry_count: 18,
        sha256: "b".repeat(64),
      },
    ],
  });
  assert.match(markdown, /devlifeX\/BiFlow/);
  assert.match(markdown, /does not commit or push/);
  assert.doesNotMatch(markdown, /Live updates come from/);
});

test("update-rules.sh never commits or pushes", () => {
  const source = readFileSync(join(root, "scripts/update-rules.sh"), "utf8");
  assert.match(source, /sync-rules\.mjs/);
  assert.match(source, /"\$\{MIHOMO\}" -t -d/);
  assert.match(source, /does not commit or push/);
  assert.doesNotMatch(source, /\bgit (?:add|commit|push)\b/);
});
