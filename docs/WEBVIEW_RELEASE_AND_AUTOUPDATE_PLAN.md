# Herdr WebView — upstream sync, CI/CD, and desktop auto-update

> **Review draft / GitHub Issue body** — 2026-11-21. Implementation is NOT yet complete. This draft was placed in the feature branch because GitHub Issues is disabled for this repository (HTTP 410).
>
> Repository: `TonyPythoneer/herdr-app`; implementation branch: `feature/native-browser-tabs`. The proposed `herdr-webview` rename has not happened.

## Locked requirements

1. **Keep original upstream Herdr icon** (SVG and `.icns`): do not add any globe.
2. **No Apple Developer certificate or notarization.** Use ad-hoc macOS signing only. macOS may still display Gatekeeper warnings; do not bypass the OS security system.
3. Keep WKWebView **inside an existing Herdr tab/pane**, not as a separate browser window. Preserve app layout behavior and minimize modifications to upstream.
4. Release **macOS Apple Silicon** only; use GitHub Actions for build, unit/integration tests, native browser smoke tests and package verification.
5. Automatically follow new **upstream stable Git tags** and keep the Browser customization. Failure/conflict must block release.
6. Enable **in-app update notification/download/restart** from this fork's own GitHub Releases, never from the official upstream app release feed. Keep `herdr-core` update policy separate.

## Confirmed baseline

- Official tag reference: `herdrdev/herdr-app@v0.1.16` (dynamically discover newer upstream stable tags).
- Existing browser implementation: `src/browser.rs`, `src/window.rs`, `src/commands.rs`, `defaults/mods/browser-url/` in `feature/native-browser-tabs`.
- CI: `.github/workflows/browser-macos.yml` currently triggers on pushes to this feature branch only.
- `browser-ci-7` was a successful macOS CI/release with one experimental `.dmg`. It has **no updater tarball or latest.json** and does NOT prove real browser tab end-to-end behavior.
- Fork `src/update.rs::checks_app()` currently disables app updates when `HERDR_BROWSER_FORK` is compiled in.
- Original upstream `src/update.rs` already polls approximately hourly, verifies SHA-256, stages a `.tar.gz`, and provides restart/swap recovery. It currently reads `https://releases.herdr.dev/app/latest.json`.
- The existing `tests/shot_pixels.rs` checks GPUI pixels, **not actual WKWebView navigation**.

## Architecture

```text
Upstream stable vX.Y.Z tag
          |
          v
Fork default-branch GitHub Action (schedule every ~6h; manual dispatch too)
          |
          v
Select exact stable upstream tag + immutable SHA; skip if already integrated
          |
          v
Merge/rebase upstream into maintained WebView integration branch / update PR
          | conflict/incompatible dependency => PR with failure details; STOP
          v
macOS ARM64 Actions checks
  cargo check/test + native WebView inside-tab smoke + package checks
          | fail => STOP, no release
          v
Ad-hoc signed Herdr.app (ORIGINAL icon), updater .tar.gz, manual-install .dmg
          |
          v
GitHub stable Release: assets published, latest.json / checksum published LAST
          |
          v
Installed fork polls its OWN GitHub latest.json (~hourly)
          |
          v
SemVer compare => download .tar.gz => SHA256 => stage
          |
          v
Notify Restart to Update => safe replace / recovery if failed
```

## Workstream A — Automatically follow upstream

- [ ] Add `.github/workflows/upstream-sync.yml` to **default branch** (`master` currently): `schedule: '17 */6 * * *'` and `workflow_dispatch`. A schedule on feature branch alone will never execute automatically. GitHub scheduled workflows are best-effort and public repository schedules may be disabled for inactivity.
- [ ] Fetch `herdrdev/herdr-app` **Git tags** rather than GitHub Releases (upstream GitHub Release listing can be empty). Parse semver to identify newest stable; ignore pre-releases unless explicitly enabled.
- [ ] Persist last adopted upstream tag + exact commit SHA, track custom revision separately, enforce idempotency.
- [ ] Update a dedicated integration branch from the tagged source, preserving the browser patch set, original SVG + `.icns`, vendored crates, third-party notices and CI. Produce a PR with meaningful upstream diff. On conflicts, leave PR blocked and never force push away custom changes.
- [ ] Check pinned GPUI, gpui-wry and lb-wry compatibility against **the upstream's new GPUI version**. Only update dependencies when tests pass, and commit Cargo.lock.
- [ ] Auto-publish only after full CI gate passes; do not blindly publish on upstream tag detection.

## Workstream B — Build and distribution

- [ ] Keep `assets/icons/Herdr.icns` **byte-identical** to the adopted upstream icon (optionally CI SHA check); never use the new WebView globe artwork.
- [ ] Reuse upstream `scripts/package` + `scripts/mac-sign` ad-hoc path. Do not invoke `scripts/mac-dmg` because its official Developer ID + notarization secrets are required.
- [ ] From a passing macOS ARM64 CI generate and verify:
  - `Herdr-WebView-macos-arm64.dmg` for manual installs.
  - `herdr-app-macos-aarch64.tar.gz` containing `Herdr.app` for existing updater.
  - `latest.json` (stable release asset) with version, target key `macos-aarch64`, download URL, and SHA256 of tarball.
- [ ] Use **fork-specific monotonic SemVer** `0.1.16-webview.1`, `0.1.16-webview.2`, `0.1.17-webview.1` etc. Stamp `Cargo.toml`, `herdr-app --version`, bundle Info.plist, release tag and manifest consistently. The existing simplified `newer()` function must be replaced with correct SemVer numeric component comparisons (e.g. `webview.10 > webview.9`).
- [ ] Publish full release only after test, packaging and digest checks pass. Use stable Releases for `/releases/latest/download/latest.json`; GitHub prereleases aren't included in latest.
- [ ] Keep release artifacts available long-term via GitHub Releases (not temporary Actions artifacts), with required permissions and concurrency control.
- [ ] If original and custom apps should coexist, separately plan an alternative Bundle ID/installer name without changing the icon. This is a design decision, not an approved code change.

## Workstream C — In-app updater

- [ ] Split update feeds in `src/update.rs`:
  - **Fork app**: `https://github.com/TonyPythoneer/herdr-app/releases/latest/download/latest.json`, adjusted centrally if repo renamed;
  - **herdr-core**: retain its upstream server source.
- [ ] Remove the fork-specific app-update disable switch **only when pointing to the fork's feed**. Never accidentally install `releases.herdr.dev` app tarballs over customized WebView.
- [ ] Reuse upstream SHA-256 download/stage/install/rollback; enforce expected GitHub repository and valid target/manifest, no downgrade, old app remains intact on download/checksum failure.
- [ ] GUI states: up-to-date, checking, new update available, downloading, ready to restart, error/retry. Do not silently quit a window with running agents or terminals.
- [ ] First migration: currently installed `browser-ci-7` cannot self-update because updates were compiled out. **One manual upgrade** to the first updater-enabled release is necessary.
- [ ] Ad-hoc/unnotarized updates may still encounter macOS Gatekeeper; do not promise zero prompts or advise disabling Gatekeeper.

## Test and acceptance matrix (mandatory)

- [ ] Upstream monitor ignores duplicate, older, moving/unverified and prerelease tags; picks exact stable commit; creates one integration PR.
- [ ] New upstream changes preserve Browser tab, address UI, navigation and original icon; conflicts and dependency breaks block release with a visible log.
- [ ] Rust unit tests cover valid/invalid URLs, version comparisons (especially numeric webview revisions), manifest format, invalid/foreign URLs and integrity failures.
- [ ] **Real macOS WebView E2E** creates a Browser tab, serves a deterministic local fixture, loads real HTML inside the tab, navigates to a second page, tests back/forward/reload, tab switching and clean close; generic GPU screenshot tests alone are insufficient.
- [ ] Packaging tests verify original icon, .app bundle contents, ad-hoc code signature, license/third-party notices, archive/manifest SHA-256, and correct fork update feed.
- [ ] App update integration tests verify newer release stage/restart/rollback and failure cases: wrong checksum, older version, malformed manifest, network failures and missing assets.
- [ ] Release step runs only when required checks pass; only macOS ARM64 deliverables, no Apple certificate/notarization secret required.
- [ ] A newly installed fork checks only its GitHub Release and never downloads the official Herdr app.
- [ ] Report no Apple Developer signing: initial macOS Gatekeeper warning may occur.

## Suggested PR sequence

1. **PR A** — stable SemVer + browser-tab E2E + original-icon packaging verification, `Cargo.lock` reproducibility.
2. **PR B** — independent GitHub manifest / macOS updater tarball + fork-specific app update code, tests and one-time migration.
3. **PR C** — default-branch upstream-tag polling + safe integration PRs + compatible WebView reapplication + release gating.
4. Enable fully automatic release only after all tests and a successful manual dry-run of the workflow.

## Implementation prompt for coding agent

> Implement the above plan against the **actual** `TonyPythoneer/herdr-app` code and latest upstream stable Git tag. Preserve the original icon and do not introduce Apple Developer ID signing or notarization. Keep browser rendering inside existing GPUI Tab/Pane and minimize modifications to upstream. Add default-branch tag polling, safe upstream integration with conflict-blocking PRs, macOS-only CI, deterministic real browser UI tests, and a fork-only GitHub Releases update feed using the existing Herdr auto-updater with stable versioning and SHA-256. Release both a manually installable DMG and auto-updater tar.gz/manifest, but only after all tests pass. Never replace a fork app with an upstream app; retain rollback and explicit restart UX. Do not claim success until checks run green on GitHub Actions. Submit reviewable PRs and preserve full build logs.

## Review decisions

- [ ] Automatic release after green CI, but conflicts always require manual review: **recommended**.
- [ ] Original official Herdr and fork can coexist on the same macOS machine? If yes, use an independent Bundle ID and app name (icon remains unchanged).
- [ ] Poll intervals: upstream every ~6 hours; local app hourly (same as existing Herdr).

## GitHub Issue setup blocker

Attempted to open this spec as a GitHub Issue in `TonyPythoneer/herdr-app`, but GitHub returned HTTP 410 `Issues has been disabled in this repository`. Enable **Settings → General → Features → Issues**, then use this document as the Issue body. This document itself does not implement or enable the new update pipeline.
