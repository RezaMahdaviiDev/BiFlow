# BiFlow agent rules

Follow these rules in every change. If a rule is missing or a new failure mode appears, update this file in the same change.

## Done gate (hard rule)

A change is **not done** until the parts you touched build and test with **zero failures and zero warnings from project code**. Do not report the task complete, skip the gate, or leave a warning or red command.

After every change, run only what that change affects:

1. Frontend (TypeScript, UI, scripts, or package manifests): `pnpm check` and `pnpm build`
2. Rust (`.rs`, crate `Cargo.toml`, or `Cargo.lock`): `cargo test -p <crate>` and `cargo clippy -p <crate> --all-targets -- -D warnings` for each **changed workspace crate**. Cargo already rebuilds only dirty units. Do **not** `cargo clean`. Do **not** run `cargo test --workspace` and `cargo build --workspace` after every task.

If many crates or workspace dependencies changed, use one incremental `cargo test --workspace` plus `cargo clippy --workspace --all-targets -- -D warnings`. Do not follow them with a second full `cargo build --workspace`.

If Node, pnpm, or Cargo is missing, install it first (see `./build.sh`) and re-run. A missing toolchain is not a pass.

If a required command fails or emits a warning from project code, fix it in the same change and re-run that command. Do not hide warnings with a broad `allow`; a narrow suppression is acceptable only when the condition is intentional and documented at the suppression site. Hundreds of `Compiling` lines mean a cold `target/` cache, not a required full rebuild.

## Testing

- Add or update an **e2e test** for every primary user flow (install missing apps, connect/disconnect, cloud rule sync, custom direct rules, diagnostics DIRECT vs VPN).
- Add or update **unit tests** for core logic (Rust crates, especially `iran-split-core`, rules, and installers) and UI logic (store, mock transport, React screens).
- `pnpm test` runs UI unit tests. `cargo test --workspace` runs core unit tests. `pnpm test:e2e` runs Playwright against the mock UI.
- Do not merge a behavior change that has no covering unit test, and do not add a primary flow without an e2e assertion.

## Errors and lessons

- When you hit an error and fix it, record the cause and the fix under [Lessons](#lessons) so the next agent does not repeat it.
- Prefer a regression test alongside the lesson.

## Architecture decisions

- Record non-obvious choices as ADRs in `docs/adr/`.
- After each behavioral or architectural change, add a new ADR or update the existing one. Keep `docs/adr/README.md` current.

## Version

- The only version source is the `version` file in the repository root (semver `X.Y.Z`).
- Initial version is `1.0.0`. Increase it after every user-facing or process change (`1.0.1`, `1.1.0`, …).
- Do not edit version strings in `package.json`, `Cargo.toml`, or `tauri.conf.json` by hand. Run `pnpm version:sync` (also runs at the start of `pnpm check` and `pnpm build`) so those files follow `version`.
- Runtime UI and Tauri bootstrap read `version` directly (`__APP_VERSION__` / `include_str!`).

## Lessons

- Older Pillow has no `Image.Resampling`; generate icons with `Image.LANCZOS` / `Image.BICUBIC`.
- Inner `#![allow(...)]` attributes must be the first item in a Rust module, before `use`.
- Vitest coverage config requires `provider: "v8"` (or `istanbul`) in this Vite version.
- `let unsubscribe = () => undefined` is typed `() => undefined` and cannot store a `() => void` unlisten function; annotate `let unsubscribe: () => void`.
- Playwright e2e against Vite mock state must call `window.__BIFLOW_RESET_MOCK()` and reload, because the mock module keeps process-wide state.
- `__APP_VERSION__` must be declared inside `declare global` in `vite-env.d.ts`. That file is a module (`export {}`), so a top-level `declare const` is not visible to `tsc`.
- Vitest does not enable Testing Library auto-cleanup when `globals` are off. Call `cleanup()` in `src/test/setup.ts` `afterEach`. Do not import `store/app` from setup: that loads `desktop` before `vi.mock` and breaks store unit tests.
- Typed ESLint (`recommendedTypeChecked`) must be scoped to `*.{ts,tsx}` and those files must be in a tsconfig `include`. Ignore `eslint.config.js` / `postcss.config.js`. Mock async methods and Vitest spies need `require-await` / `unbound-method` off in those files.
- The diagnostics **Test flow** button stays disabled until the target field is non-empty.
- The Zustand store is a process singleton. App tests that change `page` must reset store state in `beforeEach`, or the next test stays on Settings and never sees the dashboard heading.
- `getByRole(..., { name: "Install" })` substring-matches **Installing…**. Use `{ name: /^Install$/ }` in Vitest and `{ exact: true }` in Playwright.
- `scripts/sync-version.mjs` must only sync manifests when it is the process entry point. Importing `readAppVersion` from tests or `build-plan.mjs` must not rewrite `package.json`.
- After installing rustup, the same shell must prepend `$HOME/.cargo/bin` (or `source "$HOME/.cargo/env"`) or `cargo` is still missing. Both `./build.sh` and `./dev.sh` do this before every toolchain check, including clean/non-interactive shells.
- Hiddify/Mihomo Install buttons must use PATH and `~/.local/bin`, not only `~/.local/share/biflow`. Mock UI reads the same locations at Vite startup; Playwright still forces missing deps via `sessionStorage` so e2e can test Install.
- `zip` 2.6.1 is yanked on crates.io; pin `3.0.0` (2.4.2 also exists) or `cargo build` cannot resolve the crate.
- Edition 2021 + rustc 1.88 does not allow `if cond && let Some(...)` let-chains. Split into nested `if`.
- `u32::from([100, 64, 0, 0])` does not compile; use `u32::from_be_bytes([100, 64, 0, 0])` for CGNAT `100.64.0.0/10`.
- Workspace Clippy is `pedantic`, and warnings are errors. The Rust gate includes incremental `cargo clippy -p <changed crate> --all-targets -- -D warnings`; fix every diagnostic before completion.
- `iran-split-cli` uses `toml::from_str` in `main`; add `toml.workspace = true` or `cargo test --workspace` fails compiling the CLI binary tests.
- Tauri `bundle.resources` globs must match at least one file. Keep `resources/licenses/NOTICE.txt` so `../resources/licenses/*` does not fail the desktop build script.
- After `execute(..., request.payload)`, do not call `request.reply(...)`; `payload` was moved. Copy `request_id` / `protocol_version` first, then build a new `Envelope`.
- `iran-split-mihomo` tests use `chrono::Utc::now()`; add `chrono` under `[dev-dependencies]` or `cargo test --workspace` fails.
- Latest `cargo-xwin` (0.20+) needs rustc 1.89. Pin `0.19.2` in `build.sh` so Windows cross-compile install works on the repo toolchain 1.88.
- Run the Tauri CLI from the workspace root, which owns `src-tauri`. Do not delegate the root `tauri` script through `pnpm --filter @iran-split/desktop`; pnpm changes into `apps/desktop`, and Tauri then cannot discover `src-tauri/tauri.conf.json`. Tauri shell hooks also run from the workspace root, so `beforeDevCommand` / `beforeBuildCommand` use `apps/desktop`; `frontendDist` remains config-relative as `../apps/desktop/dist`.
- `./dev.sh` and `./dev.sh dev` must compile and launch the native Tauri application so the UI uses Rust commands and events. Keep browser/mock development behind the explicit `./dev.sh web` command; `desktop` remains a native alias.
- Build requirement checks must not run `apt-get update` or request `sudo` when all packages are already installed. Let `apt_install_missing` update package indexes only after it finds a missing package.
- Do not `cargo clean` or re-run `cargo test --workspace` plus `cargo build --workspace` after every task. That recompiles hundreds of dependency crates. Use `cargo test -p <changed crate>` so Cargo rebuilds only dirty units.
- A Linux Tauri CLI only accepts Linux values for `--bundles`. For a Windows cross-build, pass `--runner cargo-xwin --target x86_64-pc-windows-msvc` without `--bundles nsis`; Tauri selects NSIS from the target and uses host `makensis`.
- Snapshot the root version at build start, select exact versioned source artifacts, and reject a mid-build version change. Never copy the first wildcard match or label a package with a version different from its embedded metadata.
- Tauri's synchronous `setup` callback is not entered into a Tokio reactor. Do not call `tokio::spawn` implicitly from constructors used there; pass `tauri::async_runtime::handle().inner()` into the Rust engine and spawn through that explicit handle. Keep a regression test that constructs the engine outside an entered runtime.
- Tauri array resources containing `../` are placed below `_up_`; runtime code that reads `$RESOURCE/rules` or `$RESOURCE/dependencies` must use object resource mappings with explicit target paths. Validate the pinned rule manifest and target-specific Mihomo checksum in both dev and build hooks.
- Startup health inspection must probe helper, Hiddify, Mihomo, TUN, and DNS independently. A missing helper must not short-circuit the other probes or leave user-visible component state as `unknown`; show a precise stopped, running, degraded, unavailable, or error state with a reason.
- Ship the verified target-specific Mihomo executable as a Tauri resource and install from it before attempting the network. Network downloads remain a fallback, verify both archive and executable SHA-256, and retry once without environment proxies when the proxy-aware request fails.
- Public-IP connectivity checks must be bounded and performed by Rust, with proxy-aware and direct clients. The React status bar displays only the typed result and must not call third-party location services directly.
- Playwright forks inherit both `NO_COLOR` and `FORCE_COLOR` in this execution environment, which makes every Node child print a warning. Delete `process.env.NO_COLOR` in `playwright.config.ts` before Playwright starts its web server and workers.
- Version synchronization must replace only the existing JSON `version` value. Re-serializing the entire manifest changes Prettier's compact array layout, so `precheck` makes `format:check` fail immediately afterward.
- React Fast Refresh warns when a component module also exports a plain helper. Put shared helpers such as country-flag conversion in a separate module so `eslint --max-warnings 0` stays clean.
