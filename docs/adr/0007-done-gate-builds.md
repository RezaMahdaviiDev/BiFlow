# ADR 0007: Build-and-test done gate

## Status

Accepted

## Context

Agents could finish a change after editing files even when the frontend or Rust build was red or not run. That left broken trees and unproven work.

## Decision

`AGENTS.md` has a hard **Done gate**: every change must pass frontend `pnpm check` + `pnpm build` and Rust `cargo test --workspace` + `cargo build --workspace` with zero failures before the task is done. Missing toolchains must be installed, not skipped. Clippy style warnings are not the done gate.

## Consequences

- Tasks take longer but land green.
- `./dev.sh check` covers formatting, lint, and tests; the gate still requires production builds (`pnpm build`, `cargo build --workspace`).
- Pedantic Clippy remains available for review, but a red `-D warnings` run does not block done if the app builds and tests pass.
