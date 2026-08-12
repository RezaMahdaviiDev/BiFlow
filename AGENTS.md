# BiFlow agent rules

Follow these rules in every change. If a rule is missing or a new failure mode appears, update this file in the same change.

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
