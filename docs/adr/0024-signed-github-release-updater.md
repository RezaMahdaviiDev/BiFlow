# ADR 0024: Signed GitHub Release updater

## Status

Superseded for in-app check/install by [0057](./0057-github-releases-in-app-update.md).
Release jobs still produce `.sig` and `latest.json` for older clients.

## Context

BiFlow already registered the Tauri updater plugin and exposed `check_for_update`
and `install_update`, but the endpoint and public key were placeholders, release
builds did not emit signed updater artifacts, and the UI discarded download
progress. Automatic updates must verify signatures and publish Linux AppImage
and Windows NSIS channels together with a static `latest.json` manifest.

## Decision

- Commit only the minisign **public** key content in `src-tauri/tauri.conf.json`.
  Set `bundle.createUpdaterArtifacts` to `true` and point the updater endpoint at
  `https://github.com/devlifeX/BiFlow/releases/latest/download/latest.json`.
- Store `TAURI_SIGNING_PRIVATE_KEY` and optional
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` as GitHub **repository** Actions secrets
  only (Settings → Secrets and variables → Actions). Environment secrets are
  invisible unless the job sets `environment:`. Never log, commit, or upload the
  private key. The secret value is the single-line Base64 blob from
  `pnpm tauri signer generate` (it decodes to `untrusted comment: … encrypted
secret key`). If a password was set at generate time, the password secret must
  match; a passwordless key must not use a dummy password secret. An empty
  workflow-level `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` still decrypts as `""`,
  and minisign reports that as `Missing comment in secret key`.
- Release jobs run `scripts/prepare-tauri-signing.mjs --require --verify-sign`
  after `pnpm install` so a bad key or password fails before the release compile.
  That probe invokes `node_modules/@tauri-apps/cli/tauri.js` directly; spawning
  `pnpm` from Node on Windows GitHub runners returns `ENOENT` and an empty
  signer log. Rotating keys requires updating `plugins.updater.pubkey` in the
  same change as the GitHub private-key secret, or installed apps reject the
  new signatures.
  Local `./build.sh` packaging without the private key passes
  `--config '{"bundle":{"createUpdaterArtifacts":false}}'` so unsigned `.deb` /
  AppImage / NSIS still complete. Package dry-run uses the same unsigned overlay.
  GitHub release jobs keep the secret and produce `.sig` files.
- Build signed AppImage and NSIS updater bundles on native GitHub runners. Upload
  `.sig` files with the release artifacts.
- Generate and schema-validate `latest.json` in the publish job with
  `scripts/generate-latest-json.mjs`, then upload installers, signatures, and the
  manifest atomically. Discovery walks the downloaded asset tree recursively:
  `actions/download-artifact` keeps each upload's directory layout (`appimage/`,
  `target/release/bundle/nsis/`) even with `merge-multiple: true`, so a flat
  directory listing sees no bundles. Match on the file's base name, treat two
  bundles for one platform as a failure, and name the staged files when a
  platform is missing.
- Before installation, pause the owned stack so TUN, routes, and Mihomo are
  detached safely while Hiddify keeps running.
- Emit bounded `update-progress` events from Rust during download and install,
  then call `AppHandle::restart()` after a successful signed install. Do not log
  raw update URLs.
- Windows NSIS and Linux AppImage are first-class automatic update paths. `.deb`
  installs remain manual download/open until a separately reviewed privileged
  replacement flow exists.

## Consequences

- Maintainers must rotate signing keys through a documented ops procedure and
  update both `tauri.conf.json` and GitHub secrets together.
- Release publication fails unless both signed updater platforms and a valid
  `latest.json` are present.
- The About page surfaces update state, progress, install, and retry actions in
  both English and Persian.
