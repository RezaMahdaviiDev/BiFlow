# ADR 0031: Local GitHub Actions mirror

## Status

Accepted

## Context

Pushes kept failing in GitHub Actions on checks that a Linux developer machine
never runs. The `rust (windows-2025)` job is the repeat offender: Clippy only
compiles the host cfg, so `#[cfg(windows)]` modules — `clippy::doc_markdown` on
a doc comment, `dead_code` on a Linux-only binding — are invisible until CI
reports them. `cargo deny check` is a second gap, because `cargo-deny` is a CI
action rather than a local tool.

`nektos/act` does not close either gap. It replays workflows in Linux
containers and cannot run a `windows-2025` job at all, which is exactly the job
that breaks.

## Decision

- `pnpm github:action-test` (`scripts/ci-local.mjs`) runs each `ci.yml` gate
  locally and prints, per step, the CI job it mirrors and the command it runs.
- The `rust (windows-2025)` Clippy job is mirrored with
  `cargo xwin clippy --workspace --all-targets --target x86_64-pc-windows-msvc
-- -D warnings`. On a Windows host that job runs natively instead and the
  Linux job becomes the uncovered one.
- Steps run to completion and report together, matching the workflow's
  `fail-fast: false`. `--bail` stops at the first failure, `--only` and `--skip`
  select steps, `--list` prints the mapping.
- A required tool that is not installed is a **failure** with the install
  command, never a silent skip. A skipped Windows Clippy is how these breakages
  reach CI in the first place.
- Coverage gaps are printed on every run rather than left implicit. Windows
  `cargo test --workspace` cannot execute Windows test binaries on a Linux
  host; `--all-targets` in the cross Clippy step type-checks them instead.
- `scripts/ci-local.test.mjs` extracts every `run:` command from `ci.yml` and
  from the `release.yml` verify job and fails when one is neither mirrored by a
  step nor listed in `SETUP_ONLY`, so the mirror cannot drift from the
  workflows.

## Consequences

- Developers need `cargo-xwin` and `cargo-deny` installed; the runner names the
  exact install command when either is missing.
- Adding a workflow step requires adding a mirrored step or a `SETUP_ONLY`
  entry in the same change, enforced by the script contract test.
- Local green still does not guarantee CI green: Windows unit tests and any
  runner-provisioning step remain unverified until the push.
