import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";
import { artifactLayout } from "./build-plan.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

describe("release artifact names", () => {
  it("names linux deb and windows exe/installer from the version file", () => {
    const directory = mkdtempSync(join(tmpdir(), "biflow-version-"));
    writeFileSync(join(directory, "version"), "1.2.3\n");
    const plan = artifactLayout(directory);
    assert.equal(plan.version, "1.2.3");
    assert.equal(plan.linux.deb, "BiFlow_1.2.3_amd64.deb");
    assert.equal(plan.windows.exe, "BiFlow.exe");
    assert.equal(plan.windows.installer, "BiFlow_1.2.3_x64-setup.exe");
  });

  it("prints a linux deb name from the live version file", () => {
    const result = spawnSync(
      process.execPath,
      [join(root, "scripts/build-plan.mjs"), "linux.deb"],
      { encoding: "utf8" },
    );
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout.trim(), /^BiFlow_\d+\.\d+\.\d+_amd64\.deb$/);
  });

  it("documents linux deb and windows installer in build.sh help", () => {
    const result = spawnSync(join(root, "build.sh"), ["--help"], {
      encoding: "utf8",
    });
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /\.deb/);
    assert.match(result.stdout, /NSIS installer/);
    assert.match(result.stdout, /BiFlow\.exe/);
    assert.match(result.stdout, /One-shot/);
    assert.match(result.stdout, /missing tools are installed/i);
  });

  it("bootstraps cargo with rustup instead of exiting when cargo is missing", () => {
    const source = readFileSync(join(root, "build.sh"), "utf8");
    assert.match(source, /sh\.rustup\.rs/);
    assert.match(source, /ensure_rust/);
    assert.doesNotMatch(source, /need_command cargo/);
  });

  it("pins cargo-xwin to a release that builds on rustc 1.88", () => {
    const source = readFileSync(join(root, "build.sh"), "utf8");
    assert.match(source, /CARGO_XWIN_VERSION="0\.19\.2"/);
    assert.match(
      source,
      /cargo install cargo-xwin --locked --version "\$\{CARGO_XWIN_VERSION\}"/,
    );
    assert.doesNotMatch(source, /cargo install cargo-xwin --locked\n/);
  });

  it("requires a green frontend and rust build before a change is done", () => {
    const agents = readFileSync(join(root, "AGENTS.md"), "utf8");
    assert.match(agents, /Done gate \(hard rule\)/);
    assert.match(agents, /pnpm check/);
    assert.match(agents, /pnpm build/);
    assert.match(agents, /cargo test --workspace/);
    assert.match(agents, /cargo build --workspace/);
    assert.match(agents, /not done/i);
  });
});
