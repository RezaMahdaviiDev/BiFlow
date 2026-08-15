# ADR 0032: Fresh Hiddify start

## Status

Accepted

## Context

Hiddify intermittently opens on a blank window. Removing its generated profile
configs and runtime files fixes it, but doing that by hand means finding a
Flutter data directory whose location differs per platform, and the same folder
holds `db.sqlite` with the user's subscriptions. A recovery action that deletes
the wrong file costs the user their subscription links.

## Decision

- Diagnostics exposes **Fresh Hiddify start** (`fresh_hiddify_start`). It stops
  Hiddify, moves the regenerable state aside, and launches Hiddify again.
- Only `configs/`, `data/`, and top-level `*.log` files are cleared. Hiddify
  rebuilds all three. `db.sqlite` and `shared_preferences.json` are never
  touched, so subscriptions and settings survive, and the report lists what was
  kept.
- Nothing is deleted. Entries are moved into
  `<data>/backups/hiddify-<YYYYMMDD-HHMMSS>/`, and the UI shows that path so a
  bad outcome is recoverable. `rename` falls back to copy-then-remove because
  the backup can land on another filesystem.
- The data directory is resolved by probing the known Flutter layouts
  (`$XDG_DATA_HOME/hiddify` on Linux, `%APPDATA%`/`%LOCALAPPDATA%` variants on
  Windows) and accepting only a directory that holds one of `db.sqlite`,
  `shared_preferences.json`, `configs`, or `app.log`. A directory that merely
  shares the name is rejected: the resolved path gets emptied.
- The running instance is terminated by matching the process executable against
  the discovered Hiddify binary — `/proc/<pid>/exe` on Linux, the image name
  for `taskkill` on Windows. Matching a command-line substring would also kill
  terminals and editors that mention Hiddify. Hiddify holds these files open
  and is single-instance, so a relaunch without stopping it would only refocus
  the broken window.
- The button confirms before acting, in English and Persian.

## Consequences

- A user whose blank window comes from a corrupt `db.sqlite` or
  `shared_preferences.json` is not helped by this button; that would need a
  wider reset with a different consent flow.
- Backups accumulate under `<data>/backups/`. They are small once `data/box.log`
  is excluded from the next run, but nothing prunes them yet.
- Terminating Hiddify drops an active VPN session, which is acceptable because
  the button is for a window that is already unusable.
