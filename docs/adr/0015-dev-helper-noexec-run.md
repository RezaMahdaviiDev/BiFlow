# 0015: Dev helper binaries outside noexec `/run`

## Status

Accepted

## Context

`./dev.sh` starts a transient systemd helper for native Linux development. It
copied `iran-split-helper` and Mihomo into `/run/biflow-dev-<uid>/bin/` and
used that path as `ExecStart`.

On typical Linux systems `/run` is a `tmpfs` mounted with `noexec`. systemd
then fails to start the unit with:

```text
Failed to find executable /run/biflow-dev-<uid>/bin/iran-split-helper: Permission denied
```

Sockets and ephemeral runtime state belong under `/run`; executables do not.

## Decision

Split the development helper layout:

- `/run/biflow-dev-<uid>` — socket, runtime directory, root-owned config
- `/var/lib/biflow-dev-<uid>/bin` — verified helper and Mihomo executables

The transient unit keeps `ProtectSystem=strict` and lists both paths in
`ReadWritePaths`. Cleanup removes both directories when `dev.sh` exits.

## Consequences

- Native `./dev.sh` works on hosts where `/run` is `noexec` (common default).
- Developers may see short-lived root-owned files under `/var/lib/biflow-dev-<uid>`
  only while a dev session is active.
- Production installs are unchanged; only the development bootstrap moves
  executables off `/run`.
