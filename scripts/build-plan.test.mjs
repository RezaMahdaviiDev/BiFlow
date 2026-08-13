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
  it("names all Linux and Windows artifacts from the version file", () => {
    const directory = mkdtempSync(join(tmpdir(), "biflow-version-"));
    writeFileSync(join(directory, "version"), "1.2.3\n");
    const plan = artifactLayout(directory);
    assert.equal(plan.version, "1.2.3");
    assert.equal(plan.linux.deb, "BiFlow_1.2.3_amd64.deb");
    assert.equal(plan.linux.appimage, "BiFlow_1.2.3_amd64.AppImage");
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
    assert.match(result.stdout, /AppImage/);
    assert.match(result.stdout, /NSIS installer/);
    assert.match(result.stdout, /\.exe/);
    assert.match(result.stdout, /One-shot/);
    assert.match(result.stdout, /installs missing tools/i);
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

  it("selects the renamed FUSE 2 package without breaking older Debian releases", () => {
    const source = readFileSync(join(root, "build.sh"), "utf8");
    const linuxDependencies = source.match(
      /ensure_linux_desktop_dependencies\(\) \{([\s\S]*?)\n\}/,
    );

    assert.ok(linuxDependencies);
    assert.match(linuxDependencies[1], /local fuse2="libfuse2"/);
    assert.match(linuxDependencies[1], /apt-cache show libfuse2t64/);
    assert.match(linuxDependencies[1], /fuse2="libfuse2t64"/);
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

  it("keeps the Linux backend out of Windows workspace builds", () => {
    const source = readFileSync(
      join(root, "crates/iran-split-platform-linux/src/lib.rs"),
      "utf8",
    );

    assert.match(source, /^#!\[cfg\(target_os = "linux"\)\]/);
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

    assert.equal(
      config.build.beforeDevCommand,
      "pnpm bundle:check && pnpm --dir apps/desktop dev",
    );
    assert.equal(
      config.build.beforeBuildCommand,
      "pnpm bundle:check && pnpm --dir apps/desktop build",
    );
    assert.equal(config.build.frontendDist, "../apps/desktop/dist");
  });

  it("validates and maps offline rules and Mihomo into every native build", () => {
    const workspace = JSON.parse(
      readFileSync(join(root, "package.json"), "utf8"),
    );
    const config = JSON.parse(
      readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"),
    );
    const linux = JSON.parse(
      readFileSync(join(root, "src-tauri/tauri.linux.conf.json"), "utf8"),
    );
    const windows = JSON.parse(
      readFileSync(join(root, "src-tauri/tauri.windows.conf.json"), "utf8"),
    );

    assert.match(workspace.scripts["bundle:check"], /rules:check/);
    assert.match(workspace.scripts["bundle:check"], /assets:check/);
    assert.equal(config.bundle.resources["../resources/rules/"], "rules/");
    assert.equal(
      linux.bundle.resources["../vendor/mihomo/linux-x86_64/mihomo"],
      "dependencies/mihomo",
    );
    assert.equal(
      windows.bundle.resources["../vendor/mihomo/windows-x86_64/mihomo.exe"],
      "dependencies/mihomo.exe",
    );
  });

  it("pins bundled rule bytes so Windows checkout does not rewrite line endings", () => {
    const attributes = readFileSync(join(root, ".gitattributes"), "utf8");
    assert.match(attributes, /resources\/rules\/\*\.txt -text/);
    assert.match(attributes, /resources\/rules\/manifest\.json -text/);
    const release = readFileSync(
      join(root, ".github/workflows/release.yml"),
      "utf8",
    );
    const ci = readFileSync(join(root, ".github/workflows/ci.yml"), "utf8");
    assert.match(release, /git config --global core\.autocrlf false/);
    assert.match(ci, /git config --global core\.autocrlf false/);
    const rustJob = ci.split(/^\s*rust:/m)[1]?.split(/^\s*security:/m)[0];
    assert.ok(rustJob, "CI rust job is missing");
    const autocrlf = rustJob.indexOf("git config --global core.autocrlf false");
    const checkout = rustJob.indexOf("actions/checkout@v4");
    assert.ok(
      autocrlf >= 0 && checkout > autocrlf,
      "Windows autocrlf must be set before checkout",
    );
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
    assert.match(nativeDev[1], /trap cleanup_dev_helper/);
    assert.match(nativeDev[1], /prepare_dev_helper/);
    assert.match(nativeDev[1], /command pnpm tauri dev/);
    assert.match(source, /cargo build -p iran-split-helper/);
    assert.match(source, /systemd-run/);
    assert.match(source, /BIFLOW_DEV_HELPER_SOCKET/);
    assert.match(source, /BIFLOW_DEV_SYSTEM_RUNTIME/);
    assert.match(source, /BIFLOW_DEV_MIHOMO_BINARY/);
    assert.match(source, /authorized_uid/);
    assert.match(source, /KillMode=control-group/);
    assert.match(source, /NoNewPrivileges=yes/);
    assert.match(source, /ProtectSystem=strict/);
    assert.match(source, /DeviceAllow="\/dev\/net\/tun rw"/);
    assert.match(source, /another native BiFlow development session/);
    assert.match(source, /sha256_of "\$\{DEV_HELPER_EXECUTABLE\}"/);
    assert.match(source, /sha256_of "\$\{DEV_HELPER_MIHOMO\}"/);
    assert.match(source, /systemctl stop "\$\{DEV_HELPER_UNIT\}"/);
    assert.match(source, /\^\/run\/biflow-dev-\[0-9\]\+\$/);
    assert.match(source, /\^\/var\/lib\/biflow-dev-\[0-9\]\+\$/);
    assert.ok(mockWeb);
    assert.match(mockWeb[1], /exec pnpm dev --host 127\.0\.0\.1/);
    assert.match(source, /case "\$\{1:-dev\}" in/);
    assert.match(source, /desktop\) run_dev ;;/);
    assert.match(source, /web\) run_web ;;/);
  });

  it("presents BiFlow to end users with a tagline, architecture, and FAQ", () => {
    const readme = readFileSync(join(root, "README.md"), "utf8");
    assert.match(readme, /Right traffic\. Right route/);
    assert.match(readme, /## Description/);
    assert.match(readme, /## How it works/);
    assert.match(readme, /## Architecture/);
    assert.match(readme, /## Develop/);
    assert.match(readme, /## FAQ/);
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

  it("publishes deb, appimage, portable exe, and nsis from v* tags only", () => {
    const workflow = readFileSync(
      join(root, ".github/workflows/release.yml"),
      "utf8",
    );
    const ci = readFileSync(join(root, ".github/workflows/ci.yml"), "utf8");

    assert.match(workflow, /tags:\s*\[["']v\*["']\]/);
    assert.doesNotMatch(workflow, /branches:\s*\[main\]/);
    assert.doesNotMatch(workflow, /ref_protected/);
    assert.match(workflow, /--bundles deb,appimage/);
    assert.match(workflow, /--bundles nsis/);
    assert.match(workflow, /ubuntu-24\.04/);
    assert.match(workflow, /windows-2025/);
    assert.match(workflow, /libfuse2t64/);
    assert.match(workflow, /BiFlow\.exe/);
    assert.match(workflow, /iran-split-desktop\.exe/);
    assert.match(workflow, /x64-setup\.exe|nsis/);
    assert.match(workflow, /needs:\s*verify/);
    assert.match(workflow, /pnpm check/);
    assert.match(workflow, /pnpm test:e2e/);
    assert.match(
      workflow,
      /cargo clippy --workspace --all-targets -- -D warnings/,
    );
    assert.match(workflow, /cargo test --workspace/);
    assert.match(workflow, /cargo deny check/);
    assert.match(workflow, /tauri-apps\/tauri-action@v1/);
    assert.match(workflow, /actions\/upload-artifact@v4/);
    assert.match(workflow, /actions\/download-artifact@v4/);
    assert.match(workflow, /gh release create/);
    assert.match(workflow, /--draft/);
    assert.match(workflow, /gh release upload/);
    assert.match(workflow, /gh release edit/);
    assert.match(workflow, /TAURI_SIGNING_PRIVATE_KEY/);
    assert.match(workflow, /generate-latest-json\.mjs/);
    assert.match(workflow, /latest\.json/);
    assert.match(workflow, /\.AppImage\.sig/);
    assert.doesNotMatch(workflow, /includeUpdaterJson|releaseDraft|tagName:/);
    assert.doesNotMatch(workflow, /hiddify/i);
    assert.doesNotMatch(workflow, /mihomo/i);
    assert.match(workflow, /ADR 0004/);
    assert.match(ci, /cargo deny check/);
    assert.match(ci, /fail-fast:\s*false/);
    assert.match(ci, /swatinem\/rust-cache@v2/);
    assert.match(ci, /workspaces:\s*["']\. -> target["']/);
    assert.match(workflow, /swatinem\/rust-cache@v2/);
    assert.match(workflow, /workspaces:\s*["']\. -> target["']/);
    assert.match(workflow, /choco install nsis/);
    assert.doesNotMatch(ci, /workspaces:\s*["']\.\/src-tauri -> target["']/);
    const rustJob = ci.split(/^\s*rust:/m)[1]?.split(/^\s*security:/m)[0];
    assert.ok(rustJob);
    assert.match(rustJob, /fail-fast:\s*false/);
    assert.match(rustJob, /ubuntu-24\.04/);
    assert.match(rustJob, /windows-2025/);
    const buildJob = workflow
      .split(/^\s*build:/m)[1]
      ?.split(/^\s*publish:/m)[0];
    assert.ok(buildJob);
    const autocrlf = buildJob.indexOf(
      "git config --global core.autocrlf false",
    );
    const checkout = buildJob.indexOf("actions/checkout@v4");
    assert.ok(
      autocrlf >= 0 && checkout > autocrlf,
      "Release Windows autocrlf must be set before checkout",
    );
  });

  it("pins Node 24 in build.sh and package engines", () => {
    const source = readFileSync(join(root, "build.sh"), "utf8");
    const workspace = JSON.parse(
      readFileSync(join(root, "package.json"), "utf8"),
    );
    assert.match(source, /NODE_VERSION="24\./);
    assert.match(source, /\[\[ "\$\{major\}" -ge 24 \]\]/);
    assert.equal(workspace.engines.node, ">=24");
  });

  it("documents focused check and ci build.sh modes", () => {
    const source = readFileSync(join(root, "build.sh"), "utf8");
    assert.match(source, /check-frontend/);
    assert.match(source, /check-rust/);
    assert.match(source, /ci-linux/);
    assert.match(source, /ci-windows/);
    assert.match(source, /check_frontend\(\)/);
    assert.match(source, /check_rust\(\)/);
    assert.match(source, /pnpm check/);
    assert.match(source, /cargo test -p/);
    assert.match(source, /cargo clippy -p/);
    assert.match(source, /ci-linux requires a native Linux runner/);
    assert.match(source, /ci-windows requires a native Windows runner/);
  });

  it("defines a non-publishing package dry-run workflow", () => {
    const workflow = readFileSync(
      join(root, ".github/workflows/package-dry-run.yml"),
      "utf8",
    );
    assert.match(workflow, /workflow_dispatch:/);
    assert.match(workflow, /ubuntu-24\.04/);
    assert.match(workflow, /windows-2025/);
    assert.match(workflow, /actions\/upload-artifact@v4/);
    assert.doesNotMatch(workflow, /gh release/);
    assert.match(workflow, /node-version: 24/);
    const autocrlf = workflow.indexOf(
      "git config --global core.autocrlf false",
    );
    const checkout = workflow.indexOf("actions/checkout@v4");
    assert.ok(
      autocrlf >= 0 && checkout > autocrlf,
      "Windows autocrlf must be set before checkout",
    );
  });

  it("allows MPL-2.0 and other lockfile licenses without a blanket allow", () => {
    const deny = readFileSync(join(root, "deny.toml"), "utf8");
    const allow = deny.match(/\[licenses\][\s\S]*?allow\s*=\s*\[([\s\S]*?)\]/);

    assert.ok(allow);
    assert.match(allow[1], /MPL-2\.0/);
    assert.match(allow[1], /Apache-2\.0 WITH LLVM-exception/);
    assert.match(allow[1], /CDLA-Permissive-2\.0/);
    assert.doesNotMatch(allow[1], /["']\*["']/);
    assert.match(deny, /allow-wildcard-paths\s*=\s*true/);
    assert.match(deny, /wildcards\s*=\s*"deny"/);
    assert.match(deny, /unmaintained\s*=\s*"workspace"/);
    const cargo = readFileSync(join(root, "Cargo.toml"), "utf8");
    assert.match(cargo, /\[workspace\.package\][\s\S]*?publish = false/);
  });
});
