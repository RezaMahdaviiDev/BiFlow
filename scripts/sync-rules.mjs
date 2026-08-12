import assert from "node:assert/strict";
import {
  mkdtempSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const rulesDir = join(root, "resources/rules");
const manifestPath = join(rulesDir, "manifest.json");
const repository = "Chocolate4U/Iran-clash-rules";
const branch = "release";
const catalog = [
  {
    id: "iran-domains",
    localName: "iran-domains.txt",
    remoteName: "ir.txt",
    kind: "domain",
    minimumEntries: 1_000,
  },
  {
    id: "iran-networks",
    localName: "iran-networks.txt",
    remoteName: "ircidr.txt",
    kind: "ip_cidr",
    minimumEntries: 100,
  },
  {
    id: "private",
    localName: "private.txt",
    remoteName: "private.txt",
    kind: "ip_cidr",
    minimumEntries: 5,
  },
];

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function entryCount(bytes) {
  return bytes
    .toString("utf8")
    .split(/\r?\n/u)
    .map((line) => line.trim())
    .filter(
      (line) => line.length > 0 && !line.startsWith("#") && line !== "payload:",
    ).length;
}

function validateEntry(entry, bytes) {
  const count = entryCount(bytes);
  assert.ok(
    count >= entry.minimumEntries,
    `${entry.localName} has ${count} entries; expected at least ${entry.minimumEntries}`,
  );
  return count;
}

function readManifest() {
  return JSON.parse(readFileSync(manifestPath, "utf8"));
}

function checkSnapshot() {
  const manifest = readManifest();
  assert.equal(manifest.schema_version, 1);
  assert.equal(manifest.repository, repository);
  assert.match(manifest.commit, /^[0-9a-f]{40}$/u);
  assert.equal(manifest.rules.length, catalog.length);

  for (const entry of catalog) {
    const recorded = manifest.rules.find((rule) => rule.id === entry.id);
    assert.ok(recorded, `manifest is missing ${entry.id}`);
    assert.equal(recorded.file, entry.localName);
    assert.equal(recorded.upstream_file, entry.remoteName);
    assert.equal(recorded.kind, entry.kind);
    const bytes = readFileSync(join(rulesDir, entry.localName));
    assert.equal(recorded.sha256, sha256(bytes));
    assert.equal(recorded.entry_count, validateEntry(entry, bytes));
  }
  process.stdout.write(
    `verified ${catalog.length} bundled rule sets from ${manifest.commit.slice(0, 12)}\n`,
  );
}

async function fetchBytes(url) {
  const response = await fetch(url, {
    headers: { "user-agent": "BiFlow rule snapshot builder" },
    redirect: "follow",
    signal: AbortSignal.timeout(60_000),
  });
  if (!response.ok) {
    throw new Error(`${url} returned HTTP ${response.status}`);
  }
  return Buffer.from(await response.arrayBuffer());
}

async function resolveCommit() {
  const bytes = await fetchBytes(
    `https://api.github.com/repos/${repository}/commits/${branch}`,
  );
  const response = JSON.parse(bytes.toString("utf8"));
  assert.match(response.sha, /^[0-9a-f]{40}$/u);
  return response.sha;
}

async function downloadRule(entry, commit) {
  const urls = [
    `https://raw.githubusercontent.com/${repository}/${commit}/${entry.remoteName}`,
    `https://cdn.jsdelivr.net/gh/${repository}@${commit}/${entry.remoteName}`,
    `https://fastly.jsdelivr.net/gh/${repository}@${commit}/${entry.remoteName}`,
  ];
  let lastError;
  for (const url of urls) {
    try {
      return { bytes: await fetchBytes(url), source: url };
    } catch (error) {
      lastError = error;
    }
  }
  throw lastError;
}

async function updateSnapshot() {
  const commit = await resolveCommit();
  const temporary = mkdtempSync(join(tmpdir(), "biflow-rules-"));
  try {
    const rules = [];
    for (const entry of catalog) {
      const { bytes, source } = await downloadRule(entry, commit);
      const count = validateEntry(entry, bytes);
      writeFileSync(join(temporary, entry.localName), bytes);
      rules.push({
        id: entry.id,
        file: entry.localName,
        upstream_file: entry.remoteName,
        kind: entry.kind,
        entry_count: count,
        sha256: sha256(bytes),
        source,
      });
    }
    const manifest = {
      schema_version: 1,
      repository,
      branch,
      commit,
      fetched_at: new Date().toISOString(),
      rules,
    };
    writeFileSync(
      join(temporary, basename(manifestPath)),
      `${JSON.stringify(manifest, null, 2)}\n`,
    );
    for (const entry of catalog) {
      renameSync(
        join(temporary, entry.localName),
        join(rulesDir, entry.localName),
      );
    }
    renameSync(join(temporary, basename(manifestPath)), manifestPath);
  } finally {
    rmSync(temporary, { force: true, recursive: true });
  }
  checkSnapshot();
}

if (process.argv.includes("--check")) {
  checkSnapshot();
} else {
  await updateSnapshot();
}
