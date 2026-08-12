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

  it("does not require apt access when Linux build packages are installed", () => {
    const source = readFileSync(join(root, "build.sh"), "utf8");
    const linuxDependencies = source.match(
      /ensure_linux_desktop_dependencies\(\) \{([\s\S]*?)\n\}/,
    );

    assert.ok(linuxDependencies);
    assert.doesNotMatch(linuxDependencies[1], /^\s*apt_update_once\s*$/m);
    assert.match(linuxDependencies[1], /apt_install_missing/);
  });

  it("lets the Windows target select NSIS during Linux cross-builds", () => {
    const source = readFileSync(join(root, "build.sh"), "utf8");
    const crossSetup = source.match(
      /ensure_windows_cross_from_linux\(\) \{([\s\S]*?)\n\}/,
    );

    assert.ok(crossSetup);
    assert.match(crossSetup[1], /--runner cargo-xwin/);
    assert.match(crossSetup[1], /--target x86_64-pc-windows-msvc/);
    assert.doesNotMatch(
      crossSetup[1],
      /WINDOWS_TAURI_ARGS=\([^)]*--bundles nsis/,
    );
  });

  it("pins one build version and validates exact package metadata", () => {
    const source = readFileSync(join(root, "build.sh"), "utf8");
    assert.match(source, /BUILD_VERSION="\$\(plan version\)"/);
    assert.match(source, /assert_build_version/);
    assert.match(source, /dpkg-deb -f "\$\{source\}" Version/);
    assert.doesNotMatch(source, /first_match/);
  });

  it("runs the Tauri CLI from the workspace root that owns src-tauri", () => {
    const workspace = JSON.parse(
      readFileSync(join(root, "package.json"), "utf8"),
    );
    const frontend = JSON.parse(
      readFileSync(join(root, "apps/desktop/package.json"), "utf8"),
    );

    assert.equal(workspace.scripts.tauri, "tauri");
    assert.equal(workspace.devDependencies["@tauri-apps/cli"], "2.5.0");
    assert.equal(frontend.scripts.tauri, undefined);
    assert.equal(frontend.devDependencies["@tauri-apps/cli"], undefined);
  });

  it("runs Tauri frontend hooks from the workspace root", () => {
    const config = JSON.parse(
      readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"),
    );

    assert.equal(config.build.beforeDevCommand, "pnpm --dir apps/desktop dev");
    assert.equal(
      config.build.beforeBuildCommand,
      "pnpm --dir apps/desktop build",
    );
    assert.equal(config.build.frontendDist, "../apps/desktop/dist");
  });

  it("starts the native Rust-backed application by default", () => {
    const source = readFileSync(join(root, "dev.sh"), "utf8");
    const nativeDev = source.match(/run_dev\(\) \{([\s\S]*?)\n\}/);
    const mockWeb = source.match(/run_web\(\) \{([\s\S]*?)\n\}/);

    assert.ok(nativeDev);
    assert.match(source, /activate_rust_path\(\)/);
    assert.match(source, /ensure_rust\(\) \{[\s\S]*?activate_rust_path/);
    assert.match(nativeDev[1], /ensure_rust/);
    assert.match(nativeDev[1], /ensure_linux_desktop_dependencies/);
    assert.match(nativeDev[1], /exec pnpm tauri dev/);
    assert.ok(mockWeb);
    assert.match(mockWeb[1], /exec pnpm dev --host 127\.0\.0\.1/);
    assert.match(source, /case "\$\{1:-dev\}" in/);
    assert.match(source, /desktop\) run_dev ;;/);
    assert.match(source, /web\) run_web ;;/);
  });

  it("requires a green frontend and rust build before a change is done", () => {
    const agents = readFileSync(join(root, "AGENTS.md"), "utf8");
    assert.match(agents, /Done gate \(hard rule\)/);
    assert.match(agents, /pnpm check/);
    assert.match(agents, /pnpm build/);
    assert.match(agents, /cargo test -p <crate>/);
    assert.match(
      agents,
      /cargo clippy -p <crate> --all-targets -- -D warnings/,
    );
    assert.match(agents, /zero warnings from project code/);
    assert.match(agents, /Do \*\*not\*\* `cargo clean`/);
    assert.match(agents, /not done/i);
  });
});
