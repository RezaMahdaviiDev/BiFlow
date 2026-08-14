# ADR 0021: Shared local and GitHub build contract

## Status

Accepted

## Context

Local developers on disk-constrained machines cannot run full multi-platform
packaging or duplicate Cargo caches. GitHub Actions already runs native Linux and
Windows jobs, but `build.sh` and local Node versions diverged from CI.

## Decision

- Pin **Node 24** everywhere (`build.sh`, `engines.node`, Actions).
- Add focused `build.sh` modes:

  - `check-frontend` — `pnpm check` and `pnpm build` only
  - `check-rust <crate> [...]` — per-crate `cargo test` and Clippy
  - `ci-linux` / `ci-windows` — GitHub-hosted packaging entry points

- Keep `linux`, `windows`, and `all` for machines with spare disk; they are not
  the local developer default. Re-running those modes skips finished versioned
  packages. `--from STAGE` starts at a failed stage; `--force` rebuilds all
  packaging stages.
- Add `workflow_dispatch` **Package dry-run** (`.github/workflows/package-dry-run.yml`)
  that uploads native artifacts without publishing a release.
- Filter the dry-run OS matrix with a `select` job and
  `fromJSON(needs.select.outputs.include)`. GitHub job-level `if` cannot use
  the `matrix` context, so `if: ... && matrix.upload_*` fails workflow
  validation on every push.
- Keep `pnpm-lock.yaml` specifiers identical to `apps/desktop/package.json`
  (including `@types/node` 24.x for the Node 24 pin). CI uses
  `pnpm install --frozen-lockfile`.
- Pin `actionlint` **v1.7.7** in developer docs only; do not install `act` on
  disk-constrained machines.

## Consequences

- Local done gates stay incremental; full packaging evidence comes from GitHub.
- Workflow contract tests in `scripts/build-plan.test.mjs` guard mode names,
  Node pin, runner labels, lockfile specifiers, dry-run matrix selection, and
  non-publishing dry-run behavior.
- Extends [0020](./0020-native-linux-windows-ci.md) and [0006](./0006-linux-windows-packages.md).
