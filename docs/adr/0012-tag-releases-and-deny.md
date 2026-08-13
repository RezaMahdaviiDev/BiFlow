# ADR 0012: Atomic tag releases and cargo-deny policy

## Status

Accepted

## Context

CI `cargo deny check` rejected current lockfile licenses and workspace path
dependencies. The release workflow was also gated on `github.ref_protected`,
which is false for ordinary `v*` tags, and direct matrix publication could
leave a public release with only one platform if the other build failed.

## Decision

- Allow only the additional lockfile licenses `MPL-2.0`,
  `Apache-2.0 WITH LLVM-exception`, and `CDLA-Permissive-2.0`.
- Keep registry wildcard dependencies denied, allow workspace path
  dependencies, and mark every workspace crate unpublished.
- Scope unavoidable transitive unmaintained advisories to workspace checks;
  vulnerability and unsound advisories still fail.
- Trigger releases only for pushed `v*` tags and require the tag to match the
  root `version` file.
- Run the frontend, e2e, Rust, and cargo-deny gates before packaging.
- Build Linux and Windows without publishing from the matrix. Upload the
  `.deb`, AppImage, portable `.exe`, and NSIS installer as workflow
  artifacts, validate that exact set, then publish them together in the final
  job. A new release remains a draft until every upload succeeds.
- Use `libfuse2t64` on the Ubuntu 24.04 runner and the current Tauri release
  action. Third-party VPN applications remain in-app installs.
- Gate the Linux platform crate at its crate root on `target_os = "linux"`.
  Cargo still enumerates every workspace member during Windows Clippy, even
  though the desktop depends on that crate only for Linux.
- Treat Windows named-pipe open as a synchronous attempt. Retry
  `ERROR_PIPE_BUSY` inside a bounded async loop; fail other open errors
  immediately.
- Keep the Windows desktop path clean under the same pedantic Clippy policy:
  executable extensions are case-insensitive and the unit backend is
  constructed directly.

## Consequences

A failed verification or platform build cannot create a partial public
release. Rerunning the final job replaces assets safely, and every `vX.Y.Z`
tag must match the root `version` value exactly.
