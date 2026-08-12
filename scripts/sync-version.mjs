#!/usr/bin/env node
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const checkOnly = process.argv.includes("--check");

export function readAppVersion(fromRoot = root) {
  const value = readFileSync(join(fromRoot, "version"), "utf8").trim();
  if (!/^\d+\.\d+\.\d+$/.test(value)) {
    throw new Error(`version file must be semver X.Y.Z, got "${value}"`);
  }
  return value;
}

function read(path) {
  return readFileSync(path, "utf8");
}

function write(path, contents) {
  if (checkOnly) return;
  writeFileSync(path, contents);
}

function fail(message) {
  throw new Error(message);
}

export function replaceJsonVersion(source, version) {
  const next = source.replace(
    /^(\s*"version"\s*:\s*)"[^"]+"/m,
    `$1"${version}"`,
  );
  if (next === source) {
    fail("could not find a JSON version property to replace");
  }
  return next;
}

function syncJson(path, version) {
  const source = read(path);
  const json = JSON.parse(source);
  if (json.version === version) return;
  if (checkOnly) {
    fail(
      `${path} version is ${json.version}, expected ${version} from ./version`,
    );
  }
  const next = replaceJsonVersion(source, version);
  write(path, next);
}

function syncCargoWorkspace(path, version) {
  const source = read(path);
  const next = source.replace(
    /(\[workspace\.package\][\s\S]*?^version = )"[^"]+"/m,
    `$1"${version}"`,
  );
  if (next === source) {
    if (!source.includes(`version = "${version}"`)) {
      fail(`could not find [workspace.package] version in ${path}`);
    }
    return;
  }
  if (checkOnly) {
    fail(`${path} workspace version does not match ./version (${version})`);
  }
  write(path, next);
}

function syncTauri(path, version) {
  syncJson(path, version);
}

function isDirectRun() {
  return Boolean(
    process.argv[1] &&
      import.meta.url === pathToFileURL(resolve(process.argv[1])).href,
  );
}

function syncAll() {
  const version = readAppVersion();
  syncJson(join(root, "package.json"), version);
  syncJson(join(root, "apps/desktop/package.json"), version);
  syncCargoWorkspace(join(root, "Cargo.toml"), version);
  syncTauri(join(root, "src-tauri/tauri.conf.json"), version);
  if (!checkOnly) {
    process.stdout.write(`synced application version ${version}\n`);
  }
}

if (isDirectRun()) {
  syncAll();
}
