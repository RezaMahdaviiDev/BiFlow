# ADR 0019: Clippy cfg tail expressions

## Status

Accepted

## Context

GitHub Actions `rust (ubuntu-24.04)` failed on `v1.1.4` during
`cargo clippy --workspace --all-targets -- -D warnings`. Clippy reported
`clippy::needless_return` in `src-tauri/src/lib.rs` for Linux helper and Mihomo
path helpers: the `#[cfg(debug_assertions)]` branch ended with `return expr;`.

CI compiles debug assertions. Cross-cfg `if`/`else` is not visible to Clippy, so
that `return` is the last statement of the compiled function body.

## Decision

Linux debug path overrides use a block tail expression, not an explicit `return`.
A Tauri contract test rejects `#[cfg(debug_assertions)] { return ... }` in
`src-tauri/src/lib.rs`.

## Consequences

Ubuntu CI clippy stays green for these helpers. Release builds still compile the
`not(debug_assertions)` production path block independently.
