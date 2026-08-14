# ADR 0028: Embedded Iran rules for portable Windows

## Status

Accepted

## Context

Windows `resource_dir()` is the directory that contains `BiFlow.exe`. Bootstrap
reads the bundled Iran snapshot from `$RESOURCE/rules`. The NSIS installer can
place that folder next to the executable, but `build.sh` collected only
`BiFlow.exe` as the portable artifact. Launching that exe showed:

`rule I/O failed: The system cannot find the path specified. (os error 3)`

because `rules/` was not next to the binary. Windows uses `ERROR_PATH_NOT_FOUND`
(3) when an intermediate directory is missing, which is how
`CloudRuleStore::status` failed during `bootstrap_app`.

## Decision

- Embed the three bundled provider files in `iran-split-rules` and materialize
  them under the per-user data directory when the packaged `rules/` folder is
  incomplete.
- Search `$RESOURCE/rules`, `$RESOURCE/resources/rules`, and
  `$RESOURCE/_up_/resources/rules` before falling back to the embedded copy.
- Copy `resources/rules` (and vendored `mihomo.exe` when present) next to the
  collected portable `BiFlow.exe` so a folder drop-in also works offline.

## Consequences

Opening the Windows app no longer depends on the installer having copied
`rules/` beside the exe. Cloud sync still prefers a complete cache over the
bundled snapshot.
