import { createHash } from "node:crypto";
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const PLATFORM_KEYS = ["linux-x86_64", "windows-x86_64"];
const SEMVER =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;

/**
 * @typedef {Object} SignedArtifact
 * @property {string} platform
 * @property {string} bundlePath
 * @property {string} fileName
 * @property {string} signature
 */

/**
 * @param {string} version
 */
export function normalizeVersion(version) {
  return version.trim().replace(/^v/i, "");
}

/**
 * @param {string} repo
 * @param {string} releaseTag
 * @param {SignedArtifact[]} artifacts
 * @param {{ notes?: string, pubDate?: string }} [options]
 */
export function buildLatestJson(repo, releaseTag, artifacts, options = {}) {
  const version = normalizeVersion(releaseTag);
  const pubDate = options.pubDate ?? new Date().toISOString();
  const notes = options.notes ?? `BiFlow ${version}`;
  const base = `https://github.com/${repo}/releases/download/${releaseTag}`;
  const platforms = {};

  for (const artifact of artifacts) {
    platforms[artifact.platform] = {
      url: `${base}/${artifact.fileName}`,
      signature: artifact.signature.trim(),
    };
  }

  return {
    version,
    notes,
    pub_date: pubDate,
    platforms,
  };
}

/**
 * @param {unknown} manifest
 */
export function validateLatestJson(manifest) {
  if (!manifest || typeof manifest !== "object") {
    throw new Error("latest.json must be an object");
  }
  const record = /** @type {Record<string, unknown>} */ (manifest);
  const version = normalizeVersion(String(record.version ?? ""));
  if (!SEMVER.test(version)) {
    throw new Error(`invalid semver version: ${record.version}`);
  }
  if (typeof record.notes !== "string") {
    throw new Error("notes must be a string");
  }
  if (
    typeof record.pub_date !== "string" ||
    Number.isNaN(Date.parse(record.pub_date))
  ) {
    throw new Error("pub_date must be an RFC 3339 timestamp");
  }
  if (!record.platforms || typeof record.platforms !== "object") {
    throw new Error("platforms must be an object");
  }

  const platforms = /** @type {Record<string, unknown>} */ (record.platforms);
  for (const key of PLATFORM_KEYS) {
    const entry = platforms[key];
    if (!entry || typeof entry !== "object") {
      throw new Error(`missing platform entry: ${key}`);
    }
    const platform = /** @type {Record<string, unknown>} */ (entry);
    if (
      typeof platform.url !== "string" ||
      !platform.url.startsWith("https://github.com/")
    ) {
      throw new Error(`${key}.url must be a GitHub HTTPS release asset URL`);
    }
    if (
      typeof platform.signature !== "string" ||
      platform.signature.trim().length === 0
    ) {
      throw new Error(`${key}.signature must contain the .sig file contents`);
    }
    if (platform.signature.startsWith("http")) {
      throw new Error(`${key}.signature must not be a URL`);
    }
  }

  return {
    version,
    notes: record.notes,
    pub_date: record.pub_date,
    platforms,
  };
}

/**
 * @param {string} directory
 */
export function discoverSignedArtifacts(directory) {
  const files = readdirSync(directory);
  const artifacts = [];

  const appImage = files.find((name) => name.endsWith(".AppImage"));
  if (appImage) {
    const signature = readSignature(join(directory, `${appImage}.sig`));
    artifacts.push({
      platform: "linux-x86_64",
      bundlePath: join(directory, appImage),
      fileName: appImage,
      signature,
    });
  }

  const nsis = files.find((name) => name.endsWith("-setup.exe"));
  if (nsis) {
    const signature = readSignature(join(directory, `${nsis}.sig`));
    artifacts.push({
      platform: "windows-x86_64",
      bundlePath: join(directory, nsis),
      fileName: nsis,
      signature,
    });
  }

  return artifacts;
}

/**
 * @param {string} signaturePath
 */
function readSignature(signaturePath) {
  return readFileSync(signaturePath, "utf8").trim();
}

/**
 * @param {string} bundlePath
 */
export function sha256File(bundlePath) {
  const bytes = readFileSync(bundlePath);
  return createHash("sha256").update(bytes).digest("hex");
}

/**
 * @param {string} directory
 * @param {string} repo
 * @param {string} releaseTag
 * @param {{ notes?: string, pubDate?: string }} [options]
 */
export function generateLatestJsonFromDirectory(
  directory,
  repo,
  releaseTag,
  options = {},
) {
  const artifacts = discoverSignedArtifacts(directory);
  if (artifacts.length !== PLATFORM_KEYS.length) {
    throw new Error(
      `expected ${PLATFORM_KEYS.length} signed updater artifacts, found ${artifacts.length}`,
    );
  }
  for (const artifact of artifacts) {
    sha256File(artifact.bundlePath);
  }
  const manifest = buildLatestJson(repo, releaseTag, artifacts, options);
  validateLatestJson(manifest);
  return manifest;
}

function parseArgs(argv) {
  const args = {
    assets: "",
    repo: "devlifeX/BiFlow",
    tag: "",
    output: "",
    notes: "",
    pubDate: "",
  };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    if (token === "--assets") args.assets = argv[++index] ?? "";
    else if (token === "--repo") args.repo = argv[++index] ?? args.repo;
    else if (token === "--tag") args.tag = argv[++index] ?? "";
    else if (token === "--output") args.output = argv[++index] ?? "";
    else if (token === "--notes") args.notes = argv[++index] ?? "";
    else if (token === "--pub-date") args.pubDate = argv[++index] ?? "";
  }
  return args;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  if (!args.assets || !args.tag || !args.output) {
    throw new Error(
      "usage: node scripts/generate-latest-json.mjs --assets <dir> --tag vX.Y.Z --output latest.json [--repo owner/name]",
    );
  }
  const manifest = generateLatestJsonFromDirectory(
    args.assets,
    args.repo,
    args.tag,
    {
      notes: args.notes || undefined,
      pubDate: args.pubDate || undefined,
    },
  );
  writeFileSync(args.output, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main();
}
