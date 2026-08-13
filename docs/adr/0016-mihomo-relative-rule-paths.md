# 0016: Mihomo rule providers use generation-relative paths

## Status

Accepted

## Context

Mihomo Meta v1.19+ restricts every filesystem path in a configuration to the
process workdir (`-d` / `CLASH_HOME_DIR`) unless it is also listed in
`SAFE_PATHS`. BiFlow generated absolute rule-provider paths under the helper
system runtime (for example `/run/biflow-dev-<uid>/runtime/generations/...`).

Desktop validation runs `mihomo -t` before the helper publishes a generation.
That failed with an empty rejection message because:

1. Rule files were still in user staging, not at the absolute paths embedded
   in the YAML.
2. Mihomo logged the rejection on stdout, but validation only captured stderr.
3. Even after publish, absolute paths outside the Mihomo workdir would still
   fail unless every deployment added matching `SAFE_PATHS` entries.

## Decision

- Emit rule-provider `path` values as filenames relative to the generation
  directory (`private.txt`, `iran-domains.txt`, …).
- Run `mihomo -t` with `-d` set to the generation directory that already
  contains the config and rule files.
- Capture stdout and stderr when surfacing validation failures.

The helper already starts Mihomo with `-d` pointed at the published generation
root, so runtime behaviour stays aligned with validation.

## Consequences

- Connect/start works on Mihomo Meta 1.19+ without per-host `SAFE_PATHS`
  configuration.
- Validation errors include Mihomo log lines instead of a blank suffix.
- Rule files must remain co-located with `config.yaml` in each generation
  directory (unchanged helper publish contract).
