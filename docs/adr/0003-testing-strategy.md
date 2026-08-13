# ADR 0003: Unit and e2e testing strategy

## Status

Accepted

## Context

Primary flows (install, connect, cloud rules, route test) can regress independently in the Rust core and the React UI. A mock Vite transport already exists and must not touch TUN or the helper.

## Decision

- **Unit tests:** Vitest + Testing Library for UI/store/mock; `cargo test --workspace` for core, rules, and installers.
- **E2E tests:** Playwright against `pnpm dev` (mock transport) covering install → connect, cloud rule sync, custom rules, and DIRECT/VPN diagnostics.
- **Interactive development:** `./dev.sh` compiles and launches native Tauri by default so manual demonstrations exercise the React-to-Rust bridge. On Linux it builds a helper, copies the helper and pinned Mihomo binary into a root-owned per-UID directory under `/run`, starts a hardened transient systemd service, verifies the private socket, passes debug-only helper paths to Tauri, and stops/removes the transient service on exit. Only the helper is elevated; the UI remains the developer account. Browser/mock development is explicit through `./dev.sh web`; `desktop` is retained as a native alias.
- Mock module state is reset through `window.__BIFLOW_RESET_MOCK` so e2e tests stay isolated without restarting Vite.
- E2E is a separate CI job from `pnpm check` because it needs a browser.

## Consequences

- Fast feedback on logic without a signed helper.
- Running `./dev.sh` proves native startup plus the privileged Linux helper boundary rather than silently demonstrating the mock backend.
- E2E does not prove TUN/helper behavior; those remain release-gate VM tests.
