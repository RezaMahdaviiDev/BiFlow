# ADR 0007: Build-and-test done gate

## Status

Accepted

## Context

Agents could finish a change after editing files even when the frontend or Rust build was red or not run. That left broken trees and unproven work.

## Decision

`AGENTS.md` has a hard **Done gate**: prove the parts that changed with zero failures and zero project warnings. Frontend changes run `pnpm check` + `pnpm build`. Rust changes run `cargo test -p <crate>` plus `cargo clippy -p <crate> --all-targets -- -D warnings` for each touched workspace crate. Do not `cargo clean`, and do not run `cargo test --workspace` plus `cargo build --workspace` after every task. A wide Cargo change uses one incremental `cargo test --workspace` plus warning-denying workspace Clippy. Missing toolchains must be installed, not skipped.

## Consequences

- Incremental Cargo rebuilds only dirty crates; a cold `target/` still compiles dependencies once.
- `./dev.sh check` covers formatting, lint, and tests; frontend production build remains `pnpm build` when the UI changed.
- Pedantic Clippy diagnostics are blocking; every project warning must be fixed before completion.
