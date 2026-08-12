# BiFlow

BiFlow is a security-focused Tauri 2 desktop application for routing
Iranian traffic directly while sending other traffic through an existing Hiddify
local proxy. The UI always runs as the signed-in user; privileged TUN and route
operations are isolated in `iran-split-helper`.

This repository implements the architecture in
[`iran-split-desktop-implementation-roadmap-fa.md`](./iran-split-desktop-implementation-roadmap-fa.md).

## Development

Prerequisites:

- Node.js 22 or newer and pnpm 9.0.1
- Rust 1.88.0
- Linux desktop builds: WebKitGTK 4.1 and GTK 3 development packages

```bash
./dev.sh
```

This compiles and starts the complete native Tauri application, so the React UI
uses the Rust command and event bridge. `./dev.sh desktop` is an equivalent
explicit alias. To run only the browser UI with its deterministic mock transport
(which cannot modify TUN, routes, or DNS), use `./dev.sh web`.

For checks and Linux artifacts:

```bash
./dev.sh check
./dev.sh build
./build.sh            # Linux .deb and Windows .exe + NSIS installer
./build.sh linux      # artifacts/linux/BiFlow_<version>_amd64.deb
./build.sh windows    # artifacts/windows/BiFlow.exe and NSIS setup
```

`./build.sh` is one-shot: it installs missing Node.js, pnpm, Rust, Linux
desktop libraries, NSIS, and cargo-xwin, then writes packages under
`artifacts/`. Version comes from the root `version` file. Linux packages are
built on Linux. Windows packages are built on Windows, or cross-compiled from
Linux.

If Hiddify or Mihomo is missing, BiFlow shows an Install button and downloads
the official Linux or Windows build into the user data directory. If that fails,
a step-by-step manual install guide is shown.

Iran DIRECT domain and IP lists can be refreshed from
[Chocolate4U/Iran-clash-rules](https://github.com/Chocolate4U/Iran-clash-rules)
using GitHub, then jsDelivr, as fail-safe sources. A failed refresh keeps the
last known good cache or the bundled snapshot.

The application version lives in the root `version` file. Change that file only,
then run `pnpm version:sync` (also part of `pnpm check` / `pnpm build`).

## Tests

```bash
pnpm test          # UI unit tests
cargo test --workspace
pnpm test:e2e      # Playwright primary flows against the mock UI
pnpm check         # format, lint, typecheck, unit tests, version sync
```

The equivalent underlying commands are:

```bash
pnpm install --frozen-lockfile
cargo test --workspace
pnpm check
pnpm --filter @iran-split/desktop test
pnpm tauri dev
```

Run the frontend against its deterministic mock transport without a helper:

```bash
pnpm dev
```

Run the internal CLI with an in-memory backend:

```bash
cargo run -p iran-split-cli -- demo
```

## Security boundaries

- The webview has no shell capability and no general filesystem capability.
- All frontend calls pass through `apps/desktop/src/api/desktop.ts`.
- Helper IPC is versioned, framed, size-limited, and command-allowlisted.
- The helper accepts generation identifiers and hashes, never executable paths,
  shell strings, arbitrary URLs, or arbitrary file paths.
- Third-party downloads use allowlisted GitHub release URLs only.
- Mihomo's controller binds to loopback and uses a generated non-empty secret.

See [docs/operations/development.md](docs/operations/development.md) for setup,
[docs/protocol/helper-ipc-v1.md](docs/protocol/helper-ipc-v1.md) for the protocol,
and [docs/operations/release-gates.md](docs/operations/release-gates.md) for the
VM, signing, and live-network checks that cannot be proven by unit tests.

## Repository status

The repository contains a locally testable vertical slice: typed configuration,
lifecycle state machine and rollback, domain rules, Mihomo configuration and
controller client, framed helper IPC, Linux helper service, platform clients,
Tauri commands, React UI, diagnostics bundle redaction, packaging assets, and CI.
Production release still requires the real Windows/Linux matrix and signing
credentials listed in the release gates.
