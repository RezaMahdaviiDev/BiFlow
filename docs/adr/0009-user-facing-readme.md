# ADR 0009: User-facing README

## Status

Accepted

## Context

The root README mixed product intent with developer commands. End users landing
on the repository needed a tagline, a plain description, how routing works, and
answers to common questions.

## Decision

`README.md` is the product landing page. It leads with the BiFlow tagline
**Right traffic. Right route.**, then desktop and mobile screenshots, a
features list, description, how it works, architecture, develop, and FAQ.
Agent rules and crate-level lessons stay in `AGENTS.md`.

Committed shots live in `docs/screenshots/` (desktop dashboard, Diagnostics
with Reachability, and the phone-sized shell). Refresh them with
`BIFLOW_CAPTURE_README=1 pnpm exec playwright test e2e/readme-screenshots.spec.ts`.
The Diagnostics shot waits for the Reachability card so the landing page
shows the current probe table. A user-visible UI change must recapture
those files in the same change; stale landing-page images fail the done
gate.

## Consequences

- Contributors still find `./dev.sh` and `./build.sh` in the same file, below
  the user-facing sections.
- Architecture diagrams in the README stay aligned with the helper IPC and
  split-routing ADRs.
- The features list and screenshot files are covered by the README contract
  test so a landing-page edit cannot drop them.
- Agents recapture the three shots whenever Dashboard, Diagnostics, or the
  phone-sized shell changes, so GitHub's first-run view matches the build.
