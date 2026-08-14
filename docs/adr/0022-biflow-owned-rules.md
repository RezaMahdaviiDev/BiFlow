# ADR 0022: BiFlow-owned rule distribution

## Status

Accepted

## Context

Installed clients previously refreshed Iran domain and CIDR lists directly from
third-party GitHub raw, jsDelivr, and GitHub release URLs. That made runtime
routing depend on external hosts outside BiFlow control and duplicated update
semantics between maintainers and users.

## Decision

- Runtime rule refresh contacts only `devlifeX/BiFlow`.
- The client fetches
  `https://raw.githubusercontent.com/devlifeX/BiFlow/main/resources/rules/manifest.json`,
  then downloads each listed file from the same manifest commit under
  `resources/rules/`.
- Every file must match manifest SHA-256, declared entry count, and size limits
  before atomic publication. A partial generation is never activated.
- When refresh fails, keep the last complete cache; otherwise use the bundled
  snapshot shipped in the installer.
- Upstream provenance (for example Chocolate4U/Iran-clash-rules) remains in the
  committed manifest for maintainer attribution, but those hosts are not
  runtime fallbacks.
- Maintainers update the bundled snapshot with `./scripts/update-rules.sh`
  (also exposed as `pnpm rules:update`). The script does not commit or push.
- The Direct rules screen shows `devlifeX/BiFlow`, snapshot revision, counts,
  and last sync time.

## Consequences

- Routing still works offline from the bundled snapshot.
- When the packaged `rules/` folder is missing (common for a Windows portable
  `BiFlow.exe`), the desktop materializes the same snapshot from bytes embedded
  in `iran-split-rules`. See [0028](./0028-embedded-windows-rules.md).
- `test_route` and runtime generation read cache before bundled files.
- External URL allowlists for user-facing links include BiFlow GitHub/raw
  prefixes and no longer include third-party rule hosts.
- Supersedes [0005](./0005-cloud-rule-fail-safe.md).
