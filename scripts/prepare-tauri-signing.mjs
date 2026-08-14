#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { appendFileSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const SECRET_COMMENT =
  /untrusted comment: (?:minisign|rsign) encrypted secret key/;
const SIGNING_HELP = `Updater signing failed before packaging.

The NSIS / .deb / AppImage bundles already compiled; Tauri then could not decrypt
TAURI_SIGNING_PRIVATE_KEY. GitHub reports that as "incorrect updater private key
password: Missing comment in secret key".

Fix the repository secrets (Settings → Secrets and variables → Actions), not
Environment secrets, then re-run the tag workflow:

1. TAURI_SIGNING_PRIVATE_KEY must be the single-line Base64 value printed by
   \`pnpm tauri signer generate\`. Do not paste only the second line of the
   decoded minisign file.
2. If you typed a password when generating the key, set
   TAURI_SIGNING_PRIVATE_KEY_PASSWORD to that same password.
3. If the key is passwordless, delete the password secret or leave it empty.
4. After rotating keys, update plugins.updater.pubkey in src-tauri/tauri.conf.json
   in the same change.`;

export function normalizeUpdaterPassword(raw) {
  if (raw == null) {
    return undefined;
  }
  const value = String(raw)
    .replace(/^\uFEFF/, "")
    .trim();
  return value.length === 0 ? undefined : value;
}

function stripWrappingQuotes(value) {
  if (
    (value.startsWith('"') && value.endsWith('"')) ||
    (value.startsWith("'") && value.endsWith("'"))
  ) {
    return value.slice(1, -1).trim();
  }
  return value;
}

function restoreNewlines(value) {
  let next = value.replace(/\r\n/g, "\n").replace(/\r/g, "\n");
  if (!next.includes("\n") && next.includes("\\n")) {
    next = next.replace(/\\n/g, "\n");
  }
  const collapsed = next.match(
    /^(untrusted comment: (?:minisign|rsign) encrypted secret key)([A-Za-z0-9+/]+=*)$/,
  );
  if (collapsed) {
    return `${collapsed[1]}\n${collapsed[2]}\n`;
  }
  return next;
}

function encodeMinisignSecret(decoded) {
  const withNewline = decoded.endsWith("\n") ? decoded : `${decoded}\n`;
  if (!SECRET_COMMENT.test(withNewline.split("\n")[0] ?? "")) {
    throw new Error(
      "decoded updater private key is missing the minisign secret-key comment line",
    );
  }
  if (/minisign public key/i.test(withNewline)) {
    throw new Error(
      "TAURI_SIGNING_PRIVATE_KEY looks like a public key; store the private key instead",
    );
  }
  return Buffer.from(withNewline, "utf8").toString("base64");
}

export function normalizeUpdaterPrivateKey(raw) {
  if (raw == null) {
    throw new Error("TAURI_SIGNING_PRIVATE_KEY is missing");
  }
  let value = stripWrappingQuotes(
    String(raw)
      .replace(/^\uFEFF/, "")
      .trim(),
  );
  if (!value) {
    throw new Error("TAURI_SIGNING_PRIVATE_KEY is empty");
  }
  value = restoreNewlines(value).trim();
  if (value.startsWith("untrusted comment:")) {
    return encodeMinisignSecret(value);
  }
  const compact = value.replace(/\s+/g, "");
  let decoded;
  try {
    decoded = Buffer.from(compact, "base64").toString("utf8");
  } catch {
    throw new Error("TAURI_SIGNING_PRIVATE_KEY is not valid Base64");
  }
  if (!SECRET_COMMENT.test(decoded.split("\n")[0] ?? "")) {
    throw new Error(
      "TAURI_SIGNING_PRIVATE_KEY is not a Tauri minisign secret (missing untrusted comment after Base64 decode)",
    );
  }
  return compact;
}

export function prepareUpdaterSigning({
  rawKey,
  rawPassword,
  requireKey = false,
} = {}) {
  const password = normalizeUpdaterPassword(rawPassword);
  const missing = rawKey == null || String(rawKey).trim() === "";
  if (missing) {
    if (requireKey) {
      throw new Error(
        "TAURI_SIGNING_PRIVATE_KEY is not set. Add it as a GitHub repository secret.",
      );
    }
    return { unsigned: true, key: undefined, password: undefined };
  }
  return {
    unsigned: false,
    key: normalizeUpdaterPrivateKey(rawKey),
    password,
  };
}

function redact(text) {
  return String(text)
    .replace(/dW50cnVzdGVk[A-Za-z0-9+/]+=*/g, "[redacted-key]")
    .replace(/RWR[A-Za-z0-9+/]+=*/g, "[redacted-key]");
}

export function verifyUpdaterSigning(key, password, spawn = spawnSync) {
  const directory = mkdtempSync(join(tmpdir(), "biflow-updater-sign-"));
  const probe = join(directory, "probe.txt");
  writeFileSync(probe, "biflow updater signing probe\n");
  const env = { ...process.env, TAURI_PRIVATE_KEY: key };
  env.TAURI_PRIVATE_KEY_PASSWORD = password ?? "";
  delete env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD;
  const cli = join(root, "node_modules/@tauri-apps/cli/tauri.js");
  const args = ["signer", "sign", probe];
  if (password !== undefined) {
    args.push("-p", password);
  } else {
    args.push("-p", "");
  }
  try {
    const result = spawn(process.execPath, [cli, ...args], {
      cwd: root,
      env,
      encoding: "utf8",
      windowsHide: true,
    });
    if (result.status === 0) {
      return;
    }
    const spawnError = result.error ? `${result.error.message}\n` : "";
    let detail = redact(
      `${spawnError}${result.stderr ?? ""}\n${result.stdout ?? ""}`,
    );
    if (password) {
      detail = detail.split(password).join("[redacted-password]");
    }
    throw new Error(
      `${SIGNING_HELP}\n\nTauri signer output:\n${detail.trim()}`,
    );
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

function appendGitHubEnv(file, name, value) {
  if (value.includes("\n") || value.includes("%")) {
    appendFileSync(file, `${name}<<BIFLOW_EOF\n${value}\nBIFLOW_EOF\n`);
    return;
  }
  appendFileSync(file, `${name}=${value}\n`);
}

function writeGitHubEnv(prepared) {
  const file = process.env.GITHUB_ENV;
  if (!file) {
    console.log(
      "GITHUB_ENV is unset; skipping GitHub env export (local verify only)",
    );
    return;
  }
  if (prepared.unsigned) {
    return;
  }
  appendGitHubEnv(file, "TAURI_SIGNING_PRIVATE_KEY", prepared.key);
  if (prepared.password !== undefined) {
    appendGitHubEnv(
      file,
      "TAURI_SIGNING_PRIVATE_KEY_PASSWORD",
      prepared.password,
    );
  }
}

function isDirectRun() {
  return Boolean(
    process.argv[1] &&
      import.meta.url === pathToFileURL(resolve(process.argv[1])).href,
  );
}

function main(argv = process.argv.slice(2)) {
  const requireKey = argv.includes("--require");
  const githubEnv = argv.includes("--github-env");
  const verifySign = argv.includes("--verify-sign") || requireKey;
  const prepared = prepareUpdaterSigning({
    rawKey: process.env.TAURI_SIGNING_PRIVATE_KEY,
    rawPassword: process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD,
    requireKey,
  });
  if (prepared.unsigned) {
    console.log("updater signing skipped: TAURI_SIGNING_PRIVATE_KEY is unset");
    return;
  }
  if (verifySign) {
    verifyUpdaterSigning(prepared.key, prepared.password);
  }
  if (githubEnv) {
    writeGitHubEnv(prepared);
  }
  console.log("updater signing secrets are usable");
}

if (isDirectRun()) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    process.exit(1);
  }
}
