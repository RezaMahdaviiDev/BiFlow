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

Committed shots live in `docs/screenshots/`. Refresh them with
`BIFLOW_CAPTURE_README=1 pnpm exec playwright test e2e/readme-screenshots.spec.ts`.

## Consequences

- Contributors still find `./dev.sh` and `./build.sh` in the same file, below
  the user-facing sections.
- Architecture diagrams in the README stay aligned with the helper IPC and
  split-routing ADRs.
- The features list and screenshot files are covered by the README contract
  test so a landing-page edit cannot drop them.
