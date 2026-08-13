# BiFlow next implementation plan

Status: proposed; planning document only  
Prepared: 2026-08-13  
Repository: `devlifeX/BiFlow`  
Product owner/author shown in the application: Dariush Vesal

## Outcome

This plan covers the requested build, desktop-shell, updater, pause, rules, and
Basic/Advanced mode work. It does not implement those changes yet.

The local machine has a strict disk budget. Full-workspace Rust builds,
multi-platform local packaging, isolated Cargo target directories, and local
Docker copies of the GitHub jobs are intentionally outside this plan. Local
validation stays incremental and reuses the repository's existing `target/`;
complete native packaging runs on GitHub-hosted runners.

The short answer about GitHub Actions is **yes, partly**:

- [`nektos/act`](https://github.com/nektos/act) can run Ubuntu GitHub Actions
  jobs locally in Docker, but its runner images and Docker layers are too large
  for this development machine and are not required here.
- `act` is not an exact copy of GitHub-hosted runners. Its
  [runner documentation](https://nektosact.com/usage/runners.html) notes that
  the default container images are incomplete. In particular, it cannot
  faithfully reproduce this repository's GitHub-hosted native `windows-2025`
  job.
- The reliable workflow will therefore be: small shared local/CI commands,
  optional `act` use only on a machine with spare Docker storage, and a
  non-publishing GitHub run on native Ubuntu and Windows before a release.
- A green local run is necessary but is not enough to claim that GitHub will be
  green. The final acceptance gate includes a real green GitHub run.

## Current repository baseline

The plan should extend what already exists instead of creating parallel
systems.

| Requested area             | Current evidence                                                                                     | Gap to close                                                                                                                                                                                                             |
| -------------------------- | ---------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| GitHub Actions validation  | `.github/workflows/ci.yml` has frontend, e2e, Linux/Windows Rust, and security jobs.                 | Local Docker runner copies exceed the disk budget. Add lightweight workflow contract checks and a manual native GitHub packaging dry-run instead.                                                                        |
| `build.sh`                 | Builds Linux `.deb`/AppImage and Windows portable/NSIS artifacts and installs missing prerequisites. | Its multi-target path consumes too much local disk, packaging and verification are coupled too broadly, Node differs from Actions, and Linux-to-Windows cross-builds are not equivalent to native GitHub Windows builds. |
| Icons                      | Bundle icons exist in `src-tauri/icons/`, and bundle configuration lists them.                       | The tray builder does not assign the application icon explicitly. Taskbar, tray, desktop entry, and installer visuals have not been verified on both native platforms.                                                   |
| Window/layout              | Main window starts at 1120 x 760 and is resizable. The React shell uses `min-h-screen`.              | The window and document are not fixed, and long pages create outer scrolling.                                                                                                                                            |
| Text selection/right-click | No global selection or context-menu policy exists.                                                   | App chrome can be selected, and the WebView context menu is not blocked.                                                                                                                                                 |
| About                      | Sidebar and tray menus have no About entry.                                                          | Add product, author, repository, version, and update controls.                                                                                                                                                           |
| Updater                    | Rust already has `check_for_update` and `install_update`; the updater plugin is registered.          | The endpoint and public key are placeholders, updater artifacts/signatures are not produced, the UI exposes only a check call, progress is discarded, and a successful install does not explicitly relaunch.             |
| Pause                      | Core lifecycle has start/stop/restart but no paused phase.                                           | Add pause/resume without stopping Hiddify, with owned routes/TUN/DNS safely removed.                                                                                                                                     |
| Rules                      | A validated snapshot is bundled; `scripts/sync-rules.mjs` updates it.                                | Runtime refresh still contacts `Chocolate4U/Iran-clash-rules`, and the requested maintainer-facing shell entry point does not exist.                                                                                     |
| Basic/Advanced UI          | Current dashboard always shows navigation, status cards, components, and routing detail.             | Add a persisted animated segmented control and a distraction-free Basic view.                                                                                                                                            |

## Requirement interpretations to use during implementation

These choices resolve conflicts between the requests and existing primary
flows.

1. **Fixed page** means the native main window remains 1120 x 760, cannot be
   resized, and the document/body never scrolls. Unbounded content such as a
   rule list or a modal may use a clearly bounded internal scrolling region;
   otherwise users could not reach all entries.
2. **No user selection** applies to application chrome, labels, buttons, cards,
   and navigation. Inputs, textareas, editable controls, and diagnostic text
   retain text selection so editing and support-copy flows remain usable.
3. **Block right-click** means suppress the WebView `contextmenu` event across
   the whole application. It does not disable keyboard accessibility.
4. **Basic mode** initially shows only the Basic/Advanced switch and Connect.
   Once connected, it also shows the lifecycle controls required by this plan:
   Pause/Resume and Disconnect. All status cards, navigation, diagnostics,
   rules, and settings remain hidden until Advanced is selected.
5. **Pause** removes BiFlow-owned TUN, routes, and DNS interception and stops
   Mihomo if it is no longer needed, but leaves the external Hiddify process
   running. Resume rebuilds the owned routing generation using that Hiddify
   process. The implementation must verify actual direct egress after pause;
   it must not assume that every Hiddify mode leaves the operating-system proxy
   disabled.
6. **Rules from our repository** means installed clients contact only
   `devlifeX/BiFlow` for maintained rule updates. Maintainers may fetch and
   evaluate third-party or authoritative inputs through the update script, but
   those sources are never runtime dependencies for users.
7. **Automatic application update** uses signed GitHub Release artifacts.
   Windows NSIS and Linux AppImage are the first-class automatic paths. A
   system-installed `.deb` cannot safely replace itself without privilege, so
   it should receive a clear download/open-installer path unless a separately
   reviewed privileged package-update design is approved.
8. **Disk-bounded verification** means local commands cover only the frontend
   or Rust crates changed in the current step. Do not run `./build.sh all`,
   `cargo test --workspace`, workspace-wide Clippy, local Windows cross-builds,
   or the complete `act` matrix on this machine. Do not create a second
   `CARGO_TARGET_DIR` or run `cargo clean`; reuse the shared incremental cache.
   GitHub-hosted jobs provide the complete native platform and packaging
   evidence without consuming local disk.

## Delivery sequence

### Phase 0: capture a trustworthy baseline

- Confirm the worktree state and preserve all unrelated user changes.
- Record available disk space and the existing `target/` and artifact sizes.
  Stop before a command would require another target cache or multi-platform
  package set.
- Read the current per-user `biflow/debug.log` before diagnosing any native
  runtime failure. Do not create or commit a generated log.
- Reproduce only the smallest command relevant to the first implementation
  step: frontend checks for frontend work, or per-crate test/Clippy commands
  for Rust work. Save its exact first failure in the implementation notes.
- Inspect the latest failed GitHub run and map its failing command to the local
  focused equivalent. Use GitHub's logs instead of reproducing a complete
  native package job locally when local and Actions environments differ.
- Record every newly discovered failure cause and fix in `AGENTS.md` Lessons,
  with a regression test where practical.

Acceptance:

- There is one focused reproducible baseline failure or a recorded clean
  baseline for the first affected area.
- Local tool versions, available disk, the affected target, and the failing
  GitHub job are known before code changes begin.

### Phase 1: make local verification and GitHub Actions share one contract

#### 1.1 Pin the same tools everywhere

- Select one Node major and exact pnpm version for `build.sh`, package metadata,
  developer documentation, and both workflows. The current Node 22 versus
  Node 24 mismatch must be removed.
- Continue using Rust 1.88 from `rust-toolchain.toml`. Run Windows compilation
  and packaging on the native GitHub runner instead of maintaining a local
  cargo-xwin target cache.
- Pin `actionlint` in the developer bootstrap. Record an exact `act` version
  only in the optional-runner documentation; do not install it on this
  disk-constrained machine.
- Keep Ubuntu 24.04 package names aligned with Actions, including
  `libfuse2t64`, WebKitGTK 4.1, GTK 3, appindicator, `patchelf`, and `xdg-utils`.

#### 1.2 Refactor verification into disk-bounded reusable commands

Add explicit `build.sh` modes, with a small shared implementation used by both
local development and Actions:

```text
./build.sh check-frontend             # frontend check/build only
./build.sh check-rust <crate> [...]   # test and Clippy only named crates
./build.sh ci-linux                    # GitHub-hosted native Linux entry point
./build.sh ci-windows                  # GitHub-hosted native Windows entry point
```

Focused local modes must follow the repository done gate without expanding the
scope:

- frontend changes: `pnpm check`, `pnpm build`, and the relevant unit/e2e
  tests;
- Rust changes: `cargo test -p <crate>` and
  `cargo clippy -p <crate> --all-targets -- -D warnings` for each changed
  crate;
- formatting, `cargo deny check`, rule validation, and external-asset checks
  only when the files they govern changed.

The CI packaging modes must validate exact versioned artifact names and
metadata, as they do now, without triggering a second verification pass. Do
not expose local packaging or `all` modes in the disk-constrained workflow.
The complete package matrix belongs to GitHub-hosted jobs and is not a local
done gate.

#### 1.3 Keep local workflow validation lightweight

- Run `actionlint` and workflow contract tests locally; they validate syntax,
  expressions, runner names, and job structure without pulling large runner
  images.
- Document `act` as an optional tool for a different machine with sufficient
  Docker storage. It is not installed or run as part of this plan's local done
  gate, and its artifact directory must remain outside tracked files.
- Add a safe pull-request event fixture for optional use. Never place tokens in
  configuration, shell history, or committed event files.
- Mark native Windows execution as unsupported by `act`. Do not create a local
  cargo-xwin build cache merely to approximate it; use the native GitHub job.

#### 1.4 Add a real GitHub packaging dry-run

- Add `workflow_dispatch` support that runs native Linux/Windows package jobs
  on GitHub-hosted runners, uploads workflow artifacts, and never creates or
  publishes a GitHub Release.
- Allow a platform/job input so a developer can rerun only the affected remote
  target. Reserve the complete matrix for release-workflow changes and release
  candidates.
- Keep tag-triggered publication separate and atomic.
- Use the dry-run after build/release workflow changes and before creating a
  version tag. This is the final answer to platform-only failures that local
  Docker cannot reproduce.

Tests and evidence:

- Extend `scripts/build-plan.test.mjs` for every new build mode and tool-version
  invariant.
- Add workflow contract tests for checkout ordering, CRLF policy, native runner
  names, artifact set, and non-publishing dry-run behavior.
- Run `actionlint` and the focused workflow/build-script contract tests
  locally.
- Run only the affected manual GitHub job during implementation. Retain links
  to both green native platform jobs when the release candidate matrix is run.

### Phase 2: make `build.sh` targeted and disk-safe

- Re-run only the original failing subcommand or target-specific mode after the
  Phase 1 refactor.
- Fix each red command at its source; do not suppress project warnings or skip
  affected-target validation.
- Do not run `cargo clean`, use an isolated `CARGO_TARGET_DIR`, or build both
  platforms locally. Preserve and reuse the shared incremental Cargo cache.
- Make downloads bounded, checksum-verified where artifacts are pinned, and
  retry only network failures that are safe to retry.
- Keep native GitHub Windows packaging authoritative; no Linux cargo-xwin build
  is required locally.
- Verify the complete artifact set on GitHub-hosted runners:

  - `artifacts/linux/BiFlow_<version>_amd64.deb`
  - `artifacts/linux/BiFlow_<version>_amd64.AppImage`
  - `artifacts/windows/BiFlow.exe`
  - `artifacts/windows/BiFlow_<version>_x64-setup.exe`

- Inspect embedded/package versions rather than trusting filenames.
- Launch the Linux package in a GitHub job or disposable machine with adequate
  space, and perform the Windows install/uninstall smoke test in the native
  workflow or a Windows VM.

Acceptance:

- Focused `build.sh` contract tests and affected-area local gates exit zero
  with no project warnings.
- The target-specific native GitHub package job exits zero while developing;
  the release-candidate matrix exits zero on both platforms.
- The artifact checks prove exact version and expected contents.

### Phase 3: add a real Pause/Resume lifecycle

This is core behavior and should be implemented before wiring the new buttons.

#### 3.1 Model the state explicitly

- Add `paused` to the Rust lifecycle state machine and TypeScript model.
- Add typed `pause_stack` and `resume_stack` commands; do not overload `stop` or
  infer pause from partially stopped components.
- Define idempotency and coalescing for repeated Pause/Resume clicks.
- Preserve a stable operation/trace UUID from the UI command through engine,
  helper requests, result, and frontend snapshot.

#### 3.2 Define safe transitions

Pause sequence:

1. Reject or coalesce incompatible in-flight operations.
2. Remove BiFlow-owned routes, TUN, and DNS changes through the privileged
   helper.
3. Stop the owned Mihomo instance after traffic interception is removed.
4. Leave Hiddify running regardless of `stop_with_stack`.
5. Probe the effective OS route, DNS, and public egress and publish `paused`
   only when BiFlow is no longer intercepting traffic.

Resume sequence:

1. Verify Hiddify is still reachable; start it only if it disappeared and the
   existing start policy allows that.
2. Prepare and validate a fresh Mihomo generation.
3. Start Mihomo, attach TUN/routes/DNS, and run the existing readiness probes.
4. Roll back to the safe paused state if resume fails.

Disconnect remains a separate user intent and may follow the configured
Hiddify stop policy. App update and quit paths must choose explicitly between
Pause and Disconnect.

#### 3.3 Expose lifecycle controls

- When running, show both Pause and Disconnect.
- When paused, show Resume and Disconnect.
- Add Pause/Resume to the tray menu and update enabled labels from the latest
  snapshot.
- Keep action progress, cancellation, errors, and remediation visible in
  Advanced mode; Basic mode stays minimal.

Tests:

- Core unit tests for running -> paused -> running, repeated requests,
  cancellation, failed detach, failed resume, and Hiddify preservation.
- Platform/helper tests proving Pause removes only owned network state.
- Mock transport/store tests for typed commands and snapshots.
- Playwright assertions for Connect -> Pause -> Resume -> Disconnect in Basic
  and Advanced modes.
- A native Linux test that checks the TUN interface, routes, DNS, Hiddify PID,
  and direct public IP before and after pause.
- Corresponding structured `debug.log` and helper-journal event assertions,
  with no URLs, targets, secrets, or rule values logged.

### Phase 4: move live rule distribution to the BiFlow repository

#### 4.1 Add the requested maintainer shell workflow

Add executable `scripts/update-rules.sh` as the supported entry point. It may
call focused Node/Rust helpers already in the repository, but maintainers run
one shell command:

```bash
./scripts/update-rules.sh
git diff -- resources/rules
# review, run the printed validation commands, then commit normally
```

The script must not commit or push automatically. It should:

- download each approved upstream at an immutable commit or dated API
  snapshot;
- normalize line endings and provider syntax deterministically;
- parse every domain and IPv4/IPv6 CIDR, remove exact duplicates, and reject
  malformed or unsafe control characters;
- enforce minimum counts and a reviewed maximum percentage delta so a bad
  upstream cannot erase or massively expand routing silently;
- generate `resources/rules/manifest.json` with source URL, source revision,
  license, fetch time, entry count, and SHA-256 for every output;
- update `resources/rules/SNAPSHOT.md` from the manifest rather than by hand;
- run the offline snapshot check and validate a generated config with the
  pinned Mihomo binary;
- leave a readable diff for human review.

`pnpm rules:update` should delegate to this shell entry point so the old command
remains usable.

#### 4.2 Make users fetch only from `devlifeX/BiFlow`

- Replace runtime Chocolate4U/raw/jsDelivr URLs in `iran-split-rules` with a
  BiFlow-owned manifest endpoint.
- Fetch the manifest first, then files from the same BiFlow snapshot; verify
  size, type, entry count, and SHA-256 before atomic publication.
- Never activate a partially downloaded generation. Retain the last complete
  good cache, then fall back to bundled rules.
- Keep upstream provenance in the committed manifest, but do not expose those
  upstreams as runtime fallbacks.
- Show `devlifeX/BiFlow` and the snapshot revision as the update source in the
  Direct Rules UI.
- Add a test that fails if an installed runtime source contains an unapproved
  third-party rule host.

A raw GitHub path is easy but can briefly expose a mixed commit while several
files change. Prefer a versioned rules asset or immutable commit URL selected
by a small BiFlow-owned latest manifest. Hash validation remains mandatory in
either design.

#### 4.3 Preserve offline behavior

- Every installer continues to contain a complete verified rule snapshot.
- Starting or connecting never requires rule-network availability.
- “Update rules” remains an explicit, recoverable operation with progress and
  provenance.

Documentation:

- Supersede or update ADR 0005, which currently requires direct third-party
  runtime fallbacks.
- Update README cloud-rule language and maintainer instructions.
- Document licenses and attribution for every accepted input in NOTICE and the
  generated manifest.

### Phase 5: fixed desktop shell, icons, selection, and context menu

#### 5.1 Use one icon family everywhere

- Treat the square transparent source under `src-tauri/icons/` as canonical.
- Regenerate the full Tauri icon family from that source, including Windows
  ICO sizes and Linux PNG sizes; do not stretch the non-square in-app logo.
- Keep bundle icon configuration and explicitly assign the default application
  icon to `TrayIconBuilder`.
- Verify the main-window/taskbar icon, tray icon, Linux desktop entry, `.deb`,
  AppImage, Windows executable properties, and NSIS installer/uninstaller.
- Test both light and dark taskbar/tray backgrounds; prepare a monochrome tray
  variant only if a platform demonstrably needs one.

#### 5.2 Fix the viewport without losing content

- Set native min/max/current width and height to 1120 x 760 and disable resize.
- Set `html`, `body`, and `#root` to exactly 100% width/height with outer
  `overflow: hidden`.
- Replace `min-h-screen` shells with a fixed-height grid/flex layout.
- Give only data regions that can grow without bound an internal overflow
  container. Split the long Settings page into compact sub-sections/tabs so
  the body never scrolls.
- Verify at English/Persian, LTR/RTL, light/dark, 100%/125% scaling, and the
  supported minimum screen size.

#### 5.3 Disable accidental selection and native WebView context menus

- Apply `user-select: none` (with vendor prefix where needed) to app chrome.
- Restore `user-select: text` for inputs, textareas, editable fields, and
  support/log output.
- Install one application-lifetime `contextmenu` handler before React renders
  and call `preventDefault()`.
- Do not add a second handler on each render.

Tests:

- Source-level Tauri contract tests for fixed dimensions and explicit tray
  icon assignment.
- UI tests for selection exceptions and context-menu prevention.
- Playwright at the exact viewport asserting no document-level horizontal or
  vertical overflow on every page and in both languages.
- Native visual checklist on Linux and Windows for taskbar/tray/package icons.

### Phase 6: add Basic/Advanced mode and About

#### 6.1 Animated mode control

- Place a two-option segmented control at the top of the front page.
- Use one sliding background pill driven by `transform`, not two independently
  highlighted buttons.
- Implement it as an accessible labelled radio group or equivalent with
  keyboard arrows, visible focus, `aria-checked`, RTL behavior, and reduced
  motion support.
- Persist the preference under a versioned BiFlow key and default existing
  users to Advanced so no current capability disappears unexpectedly.

Basic mode contains:

- the Basic/Advanced switch;
- Connect while stopped;
- progress/cancel while connecting;
- Pause and Disconnect while running;
- Resume and Disconnect while paused;
- a concise error/remediation action only when required.

Advanced mode retains the current dashboard, component health, navigation,
rules, diagnostics, settings, and status bar.

#### 6.2 About page/menu

- Add About to Advanced sidebar navigation and to the tray menu so it remains
  reachable even when the window is in Basic mode.
- Show:

  - BiFlow name and icon;
  - current version from the root `version` source through `APP_VERSION`;
  - author: **Dariush Vesal**;
  - repository: <https://github.com/devlifeX/BiFlow>;
  - license/third-party notices link;
  - Check for updates button and update state.

- Add the BiFlow repository to the Rust external-URL allowlist. Keep the
  existing rule that arbitrary frontend URLs cannot be opened.
- Localize Basic/Advanced/About/update text in English and Persian.

Tests:

- Component/store tests for mode persistence and visibility.
- E2E assertions that Basic hides every advanced component and can return to
  Advanced.
- E2E keyboard and reduced-motion coverage for the segmented control.
- About tests for author, repository, root-sourced version, safe link opening,
  and update-state rendering.

### Phase 7: complete signed GitHub Release updates

The existing updater code is a useful starting point, but placeholder
configuration must never ship. The
[Tauri updater](https://v2.tauri.app/plugin/updater/) requires signed update
artifacts; signature verification cannot be disabled.

#### 7.1 Establish signing and release metadata

- Generate a Tauri updater key pair offline.
- Commit only the public key in `tauri.conf.json`.
- Store the private key and optional password in GitHub Actions secrets. Never
  write them to logs, artifacts, `.env`, or the repository.
- Enable `bundle.createUpdaterArtifacts`.
- Replace the placeholder endpoint with:

  `https://github.com/devlifeX/BiFlow/releases/latest/download/latest.json`

- Build signed Linux AppImage and Windows NSIS updater artifacts on
  GitHub-hosted runners and retain their `.sig` files. Do not build either
  updater package on the disk-constrained development machine.

#### 7.2 Keep release publication atomic

- Extend the current build matrix to upload installers and signatures as
  workflow artifacts.
- In the final publish job, validate the exact Linux/Windows set and generate a
  single `latest.json` containing both platforms, exact GitHub download URLs,
  embedded signature contents, version, release notes, and publication date.
- Validate `latest.json` against a schema and the staged files before uploading
  anything.
- Publish installers, signatures, and `latest.json` together; do not expose a
  latest manifest that references a missing platform.
- Preserve the current tag == root `version` check and draft-until-complete
  behavior.

#### 7.3 Finish the in-app flow

- Add the missing typed frontend `installUpdate` API and mock implementation.
- Store updater states explicitly: idle, checking, current, available,
  downloading, installing, restarting, and failed.
- Emit bounded download progress from Rust rather than discarding updater
  callbacks.
- Before installation, safely detach BiFlow-owned TUN/routes/DNS and stop
  Mihomo. Leave Hiddify running unless the application must exit in a way that
  violates platform installer requirements.
- After signature verification and successful install, flush `debug.log` and
  relaunch the application explicitly.
- On check/download/install failure, keep the current app usable, show a
  retryable error, and never leave partial network state.
- Log structured safe events for check, availability, download progress
  buckets, verification, install, restart request, and failure. Do not log
  release URLs or notes verbatim.

#### 7.4 Platform behavior

- Windows NSIS: test update from version N to N+1, including the existing
  per-machine elevation behavior and restart.
- Linux AppImage: test update from version N to N+1 and restart from the new
  AppImage.
- Linux `.deb`: show Download/Open Release unless a separately approved,
  privilege-aware package replacement flow is implemented.

Tests:

- Unit-test semantic version comparison, no-update, available, malformed
  manifest, bad signature, interrupted download, and install failure.
- Test release-manifest generation using temporary fake signed artifacts.
- Test frontend progress and retry states with mock transport.
- Add an end-to-end About -> Check -> Download/Install confirmation flow in the
  mock UI.
- Perform a real staged N -> N+1 update on native Linux and Windows before the
  feature is accepted.

## Iranian IP/domain sources to review

These are candidates only. None should be added automatically until coverage,
license, false-direct risk, and maintenance quality are reviewed.

| Candidate                                                                                                 | What it provides                                                                                    | Why inspect it                                                                                                     | Caution                                                                                                                                                              |
| --------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [RIPEstat Country Resource List](https://stat.ripe.net/docs/data-api/api-endpoints/country-resource-list) | RIR-derived Iranian ASNs, IPv4, and IPv6; `resource=ir&v4_format=prefix` returns CIDR notation.     | Best authoritative baseline for delegated country resources and an independent comparison for `iran-networks.txt`. | Delegation country is not proof that a prefix is currently routed or hosts only Iranian services. Compare BGP visibility and current behavior before marking DIRECT. |
| [bootmortis/iran-hosted-domains](https://github.com/bootmortis/iran-hosted-domains)                       | Curated Iranian hosted domains, Clash formats, release files, and SHA-256 companions; MIT licensed. | Useful second domain corpus with categories separating direct, proxy, and ads.                                     | Do not merge ads/proxy categories into DIRECT. Measure unique additions and false positives first.                                                                   |
| [v2fly/domain-list-community](https://github.com/v2fly/domain-list-community)                             | Community geosite data, including Iran-related domain data; MIT licensed.                           | Widely consumed upstream and a useful independent domain comparison.                                               | Domain syntax and category semantics need deterministic conversion to Mihomo text-provider format. It does not replace an IP source.                                 |
| [MetaCubeX/meta-rules-dat](https://github.com/MetaCubeX/meta-rules-dat)                                   | Mihomo-native geosite/geoip outputs aggregated from several projects; GPL-3.0.                      | Convenient compatibility reference and cross-check against generated Mihomo behavior.                              | Aggregated provenance and GPL obligations require review before copying or redistributing derived lists. Prefer it as a comparison initially.                        |
| [ipverse/country-ip-blocks](https://github.com/ipverse/country-ip-blocks)                                 | Compact daily country IPv4/IPv6 lists derived from official delegation data.                        | Convenient second representation for detecting transformation mistakes in our RIPEstat output.                     | It is still a third-party generated view; RIPE/RIR data should remain the authoritative input.                                                                       |

Recommended first experiment:

1. Generate a candidate IPv4/IPv6 snapshot directly from RIPEstat.
2. Diff it against the current `iran-networks.txt` by exact prefix, covered
   address space, and origin ASN.
3. Compare domain-only additions from bootmortis and v2fly.
4. Probe a stratified sample from inside and outside Iran before changing
   production DIRECT behavior.
5. Present the diff and false-positive sample for approval; do not silently
   union every source.

## Iranian DNS candidates to review

These resolvers are **not approved defaults**. Anti-sanction/game DNS services
may intentionally synthesize answers or proxy selected domains, so they are not
equivalent to neutral recursive DNS. DNS endpoints also do not belong in the
Iran CIDR routing file.

| Provider                          | Candidate IPv4 resolvers                                                                 | Evidence found on 2026-08-13                                                                                                                                                                                   | Review status                                                                           |
| --------------------------------- | ---------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| [Shecan](https://shecan.ir/)      | `178.22.122.100`, `185.51.200.2` (free); `178.22.122.101`, `185.51.200.1` (professional) | The official site lists these addresses, but currently warns that public service is restricted and usable only by people who already have international access.                                                | Do not default now. Re-test when the official warning is removed.                       |
| [Electro](https://electrotm.org/) | `78.157.42.100`, `78.157.42.101`                                                         | The provider's [official channel](https://t.me/s/ELTeam_ir) has published these addresses. Its current site says the earlier anti-censorship phase concluded as the service shifts toward gaming/LAN products. | Verify that plain DNS still answers correctly on each target ISP before considering it. |
| [Radar Game](https://radar.game/) | Reported as `10.202.10.10`, `10.202.10.11`                                               | The official site timed out during this research; addresses were found only in a [secondary list](https://github.com/ALIILAPRO/dns-changer). They are private-range domestic endpoints.                        | Unverified. Test only from Iranian networks and confirm on an official source.          |
| [403](https://403.online/)        | Reported as `10.202.10.202`, `10.202.10.102`                                             | The official site could not be verified during this research; addresses were found only in a [secondary list](https://github.com/ALIILAPRO/dns-changer). They are private-range domestic endpoints.            | Unverified. Do not ship until ownership and current service are confirmed.              |
| Begzar                            | Reported as `185.55.226.26`, `185.55.225.25`                                             | Found in a [secondary Iranian DNS list](https://github.com/ALIILAPRO/dns-changer); a current authoritative provider page was not confirmed.                                                                    | Unverified. Check ownership, terms, privacy, and live behavior first.                   |

DNS evaluation checklist:

- Query A, AAAA, CNAME, DNSSEC-valid, DNSSEC-bogus, and nonexistent names over
  UDP and TCP.
- Test from at least two Iranian ISPs, with BiFlow stopped, running, and paused.
- Detect synthesized/proxy answers for supported and unsupported domains.
- Measure timeout, median/p95 latency, failure rate, and response consistency.
- Check privacy/retention terms, filtering policy, EDNS Client Subnet, and
  DoH/DoT availability from an official source.
- Verify Mihomo bootstrap routing cannot recurse through its own TUN.
- Keep a neutral, well-defined fallback and never leak a DIRECT-only Iranian
  query through the VPN path or vice versa without an explicit policy.
- Present results to the product owner before changing the bundled DNS policy.

## Test and acceptance matrix

| Flow/change                | Required automated coverage                                                           | Required native evidence                                                                        |
| -------------------------- | ------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| Build/CI parity            | Focused build-script tests, workflow contract tests, and actionlint                   | Affected native GitHub job green; both jobs green for a release candidate                       |
| Icons                      | Config/resource contract tests                                                        | Linux and Windows taskbar, tray, desktop/Start menu, installer screenshots/checklist            |
| Fixed page/no outer scroll | Component tests and Playwright overflow assertions on every page/language             | Native 1120 x 760 at 100% and 125% scaling                                                      |
| Selection/context menu     | CSS/source tests and browser event tests                                              | Right-click does nothing in native WebView; fields remain editable                              |
| About/update               | Store/component/e2e tests, release manifest/signature tests                           | Staged N -> N+1 update and restart on AppImage and NSIS                                         |
| Pause/resume               | Core/platform/helper/store tests and e2e primary flow                                 | TUN/routes/DNS removed, Hiddify remains, public egress is direct, resume restores split routing |
| Owned rule channel         | Parser/hash/atomic cache tests, runtime-host allowlist test, Mihomo config validation | Update through BiFlow endpoint; upstream outage does not affect users                           |
| Basic/Advanced             | UI unit tests and Playwright persistence/visibility/keyboard assertions               | Native LTR/RTL visual check                                                                     |

Every new or changed primary flow needs an e2e assertion. Every core behavior
change needs a unit test. Rust user actions and failure paths need safe
structured tracing that reaches `debug.log`.

## Documentation, decisions, and versioning

- Add or update ADRs for:

  - shared local/GitHub build contract and the native Windows boundary;
  - paused lifecycle semantics and Hiddify ownership;
  - BiFlow-owned rule distribution and maintainer update provenance;
  - signed GitHub Release updater and supported package channels;
  - fixed shell/Basic mode behavior if it introduces non-obvious layout state.

- Keep `docs/adr/README.md` current.
- Update README development, release, rule-update, pause, Basic mode, and
  updater documentation.
- Update `AGENTS.md` Lessons whenever implementation reveals and fixes a new
  failure mode.
- Treat this set as a user-facing minor release and move root `version` from
  1.1.x to 1.2.0 during implementation, then run `pnpm version:sync`. Do not
  hand-edit manifest versions.

## Final done gate

Completion is evaluated incrementally for the files changed in each phase.
Every required focused command must be green with zero project warnings, but a
local full-workspace or package build is not part of the gate.

Run only the applicable local commands:

```bash
# Frontend, TypeScript, UI, scripts, or package-manifest changes
pnpm check
pnpm build

# Rust changes; repeat these two commands for each changed crate
cargo test -p <crate>
cargo clippy -p <crate> --all-targets -- -D warnings
```

Run the relevant frontend unit test and primary-flow e2e spec when their
behavior changes. Run `cargo fmt --all --check` for Rust edits. Run
`cargo deny check`, rule checks, and asset checks only when their governed
inputs change.

Do not run `cargo test --workspace`, workspace-wide Clippy, `./build.sh all`, a
local Windows cross-build, or the `act` matrix on this machine. Do not create
an isolated Cargo target cache. Use a target-specific GitHub workflow run for
platform evidence during implementation, then run both native package jobs
once for the release candidate. GitHub-hosted artifacts, not local duplicates,
are the release evidence.

Final manual acceptance:

- Basic and Advanced modes switch, animate, persist, and remain keyboard
  accessible.
- Connect, Pause, Resume, and Disconnect behave as specified.
- Pause proves direct OS egress while Hiddify remains running.
- No document scrollbar appears at the fixed window size.
- Right-click shows no WebView context menu and app chrome cannot be selected.
- The correct BiFlow icon appears in the window/taskbar, tray, package, and
  installer on Linux and Windows.
- About shows Dariush Vesal, the correct repository and root-sourced version.
- Check for updates handles current, available, progress, failure, signed
  installation, and restart states.
- Installed users fetch rules only from a BiFlow-owned endpoint; invalid or
  unavailable updates preserve the last good/bundled rules.
- All four GitHub-built release packages and updater metadata are present and
  version matched; they are not duplicated locally.
- The real GitHub run is green on native Ubuntu and Windows.
