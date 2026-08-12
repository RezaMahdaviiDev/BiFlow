# ADR 0001: Record architecture decisions

## Status

Accepted

## Context

BiFlow is changing quickly (branding, in-app installers, cloud rules, diagnostics). Without a short written decision log, later changes re-litigate the same trade-offs.

## Decision

Store ADRs in `docs/adr/` as numbered Markdown files. Update `README.md` in this folder whenever an ADR is added or superseded. Agents must add or update an ADR in the same change that introduces a behavioral or architectural decision.

## Consequences

- Decisions stay reviewable next to the code.
- `AGENTS.md` can point here instead of restating history.
