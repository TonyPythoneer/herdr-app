# Herdr WebView — upstream sync, CI/CD, and desktop auto-update

> **Review draft — tracked in [Issue #1](https://github.com/TonyPythoneer/herdr-webview/issues/1)**. The upstream-sync and desktop updater are not implemented yet.
>
> Repository: `TonyPythoneer/herdr-webview`; implementation branch: `feature/native-browser-tabs`.

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
- Original upstream `src/update.rs` already currently polls approximately hourly (upstream baseline; the planned fork app check is weekly), verifies SHA-256, stages a `.tar.gz`, and provides restart/swap recovery. It currently reads `https://releases.herdr.dev/app/latest.json`.
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
Installed fork checks OWN GitHub latest.json every 7 days while active;
missed/offline due date -> catch up at next App launch
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
  - **Fork app**: `https://github.com/TonyPythoneer/herdr-webview/releases/latest/download/latest.json`, adjusted centrally if repo renamed;
  - **herdr-core**: retain its upstream server source.
- [ ] Remove the fork-specific app-update disable switch **only when pointing to the fork's feed**. Never accidentally install `releases.herdr.dev` app tarballs over customized WebView.
- [ ] Reuse upstream SHA-256 download/stage/install/rollback; enforce expected GitHub repository and valid target/manifest, no downgrade, old app remains intact on download/checksum failure.
- [ ] GUI states: up-to-date, checking, new update available, downloading, ready to restart, error/retry. Do not silently quit a window with running agents or terminals.
- [ ] First migration: currently installed `browser-ci-7` cannot self-update because updates were compiled out. **One manual upgrade** to the first updater-enabled release is necessary.
- [ ] Ad-hoc/unnotarized updates may still encounter macOS Gatekeeper; do not promise zero prompts or advise disabling Gatekeeper.

### Weekly, catch-up-on-launch update check policy (user decision)

**This policy applies only to the locally installed WebView fork App, not to GitHub Actions' upstream tag polling.** The app does not need a background daemon, fixed weekday, OS scheduled task, or hourly GitHub network requests.

- [ ] **Default: 7 days between successful update-feed checks** (rolling interval; no fixed Monday/Sunday). Record `last_successful_app_update_check_at` **persistently on disk**, including when no newer version is found.
- [ ] On App **startup** and on returning from macOS **sleep / resume**, compare local time with the saved timestamp. If no successful check exists, or at least 7 days have elapsed, check the **fork's GitHub Release update feed** promptly. If the computer was off or the App closed on the due day, the first subsequent App launch checks then; missed weeks do **not** trigger multiple catch-up requests.
- [ ] When the App remains running continuously, schedule a one-shot check for the next 7-day deadline (reschedule after each success); handle macOS wake and significant system-clock changes. Do not fetch GitHub every hour just to discover the deadline.
- [ ] A failed/offline check must **not** reset the success timestamp. Persist `last_attempt_at` separately and apply a bounded retry backoff (for example 24 hours), with an explicit `Check for Updates Now` action that bypasses the schedule/backoff. Avoid rapid retry loops on repeated launches.
- [ ] Keep `herdr-core` update policy independent. This weekly cadence is **not** permission to disable or slow core security/compatibility updates.
- [ ] Tests: first launch, no-new-release success, 6d 23h (skip), >=7d (fetch once), powered-off for 10d followed by next launch (one catch-up fetch), multi-week absence (one fetch), App running across 7d boundary, sleep/wake, offline failure and 24h retry, and manual check.
- [ ] **Constraint:** a Mac App cannot check while both the Mac is off and the App not running. “At least weekly” means **once per 7-day period while operating, or at the first available launch after missed due date**, not a guaranteed check during power-off.

## Workstream D — CI performance, cache correctness, and measured baseline

**Measured on 2026-11-21 from GitHub Actions, not guessed:**

| Run | Result | Wall-clock job time | Notable bottleneck |
| --- | --- | --- | --- |
| [browser-macos #7](https://github.com/TonyPythoneer/herdr-webview/actions/runs/44865542880) | Success | 14m 20s | Release build / packaging step 603s; cache restored |
| [browser-macos #8](https://github.com/TonyPythoneer/herdr-webview/actions/runs/44891724495) | Success | 4m 04s | Restore Rust cache 51s, save Rust cache 53s, tests 49s, Cargo metadata 22s |

In #8 the Rust cache archive was **~2.26 GB compressed**. Restore + save accounted for **104s / 244s (43%)**. The release build was only **5s** thanks to warmed artifacts; #8 was a documentation-only push, so it is *not* representative of an upstream version upgrade. #7's release compile was ~10m 03s. GitHub step times are wall-clock measurements, not proof that optimizations have been achieved.

**Root cause:** current `.github/workflows/browser-macos.yml` uses `browser-rust-${{ hashFiles('Cargo.toml', 'Cargo.lock') }}-${{ github.run_number }}`, creating a **new cache key on every run**, and `actions/cache/save` runs with `if: always()`, uploading ~2.26 GB even after an effective restore. The checked-in `Cargo.lock` does not yet lock the new `gpui-wry` / `lb-wry` browser dependencies; CI resolves the dependency graph with `cargo metadata` every run and uploads that lockfile as a temporary artifact. Upstream `ci.yml` already uses stable dependency/toolchain-keyed caches, `CARGO_INCREMENTAL=0`, and saves only when necessary.

### Priority P0 — remove avoidable CI overhead

- [ ] **Commit the resolved browser-compatible `Cargo.lock`** (for the pinned `gpui-wry = 0.7.2`, `lb-wry = 0.53.3`, and matching GPUI dependency versions), verify reproducibility using `cargo metadata --locked` / `cargo test --locked`; remove unconditional dependency resolution and per-run resolved-lockfile artifact. Don't hand-edit lockfile.
- [ ] **Remove `github.run_number` from the Rust cache key.** Use a stable macOS-aarch64 key tied to pinned Rust/Zig toolchains, resolved lockfile and dependency/vendor source inputs (or a well-tested upstream-equivalent key). Separate test/release profiles **only if measurement proves** it helps; avoid accidental cache invalidation.
- [ ] **Save only on a genuine cache miss and only from a trusted canonical build** (maintained branch / explicit cache warmup), not every PR or every failed run. Keep restore available to PRs. Never overwrite a valid known-good cache with a failed build; allow cache recovery on dependency updates. GitHub cache entries are immutable.
- [ ] Follow upstream's `scripts/ci-sources restore/record` handling for vendored Rust/Ghostty files, ensuring source changes still force recompilation. Do **not** hide source changes behind stale timestamps.
- [ ] Set `CARGO_INCREMENTAL=0` for this CI workflow, as upstream does, and preserve pinned toolchains. Confirm it does not regress actual runtime in repeated builds.
- [ ] Cache sizing check: inspect what's in `target/`; exclude truly disposable state only after verifying subsequent compile is not slower. Prioritize cache key/save fixes before attempting aggressive cache pruning.

### Priority P1 — avoid wasted build and release runs

- [ ] A **docs-only/Markdown-only push must not rebuild and publish another app release**. Use a lightweight always-running change classifier and gate expensive jobs and release steps; avoid `paths:` filters on required PR workflows that would strand required checks as Pending. Keep an explicit manual `workflow_dispatch` override for rebuilds.
- [ ] Split **fast validation** (compile/check, Rust tests, native Browser Tab smoke when relevant) from **packaging/publishing**. Publish only on a promoted version/tag with passing required tests, not on every feature branch push. Never drop real macOS browser integration checks just to save time.
- [ ] Preserve GitHub Actions cache isolation/permissions: read cache on untrusted PRs; restrict cache writes and releases to trusted workflows/branches; pin third-party Actions to approved immutable SHAs.

### Priority P2 — show evidence, not speed claims

- [ ] Emit a **GitHub Actions Job Summary** for cache hit/miss, Rust cache compressed size, time for restore, metadata, test build, tests, release build, DMG and upload, plus the overall duration; compare cold and warm runs.
- [ ] After implementation, benchmark (1) docs-only, (2) source-only code change, (3) dependency or upstream-version change, and record *actual* timings here.
- [ ] Initial directional target for a #8-like warm run: **~2m 49s** by saving the measured 53s redundant upload and ~22s metadata step. This is a *non-guaranteed estimate*; it does not include future true WebView E2E tests, which remain mandatory. Cold builds may take substantially longer.
- [ ] Failure/merge incompatibility from a new upstream tag still opens or updates an AI-actionable issue and keeps the last successful release unchanged; CI speed optimizations never bypass safety gates.

### Acceptance

- [ ] No run-number-based cache keys; true hits never re-upload the same 2.26 GB archive.
- [ ] `Cargo.lock` includes browser deps and `cargo test --locked` is reproducible.
- [ ] Documentation-only changes do not publish new DMGs/releases.
- [ ] Real WebView-in-Tab macOS integration checks and updater safety checks remain mandatory for upstream-sync releases.
- [ ] Performance changes have measured cold/warm before-and-after results in Actions summaries.

## Test and acceptance matrix (mandatory)

- [ ] Upstream monitor ignores duplicate, older, moving/unverified and prerelease tags; picks exact stable commit; creates one integration PR.
- [ ] New upstream changes preserve Browser tab, address UI, navigation and original icon; conflicts and dependency breaks block release with a visible log.
- [ ] Rust unit tests cover valid/invalid URLs, version comparisons (especially numeric webview revisions), manifest format, invalid/foreign URLs and integrity failures.
- [ ] **Real macOS WebView E2E** creates a Browser tab, serves a deterministic local fixture, loads real HTML inside the tab, navigates to a second page, tests back/forward/reload, tab switching and clean close; generic GPU screenshot tests alone are insufficient.
- [ ] Packaging tests verify original icon, .app bundle contents, ad-hoc code signature, license/third-party notices, archive/manifest SHA-256, and correct fork update feed.
- [ ] App update integration tests verify newer release stage/restart/rollback and failure cases: wrong checksum, older version, malformed manifest, network failures and missing assets; plus weekly rolling cadence and missed-week catch-up scenarios.
- [ ] Release step runs only when required checks pass; only macOS ARM64 deliverables, no Apple certificate/notarization secret required.
- [ ] A newly installed fork checks only its GitHub Release and never downloads the official Herdr app.
- [ ] Report no Apple Developer signing: initial macOS Gatekeeper warning may occur.

- [ ] CI regression suite covers cache correctness, docs-only release gating, warm/cold timing and preserves all required browser checks.

## Suggested PR sequence

0. **PR 0 (CI speed / reproducibility)** — commit complete Cargo.lock, stable cache key, cache-write gate, docs-only skip of heavy jobs; baseline/timing summary.
1. **PR A** — stable SemVer + browser-tab E2E + original-icon packaging verification, `Cargo.lock` reproducibility.
2. **PR B** — independent GitHub manifest / macOS updater tarball + fork-specific app update code, tests and one-time migration.
3. **PR C** — default-branch upstream-tag polling + safe integration PRs + compatible WebView reapplication + release gating.
4. Enable fully automatic release only after all tests and a successful manual dry-run of the workflow.

## Implementation prompt for coding agent

> Implement the above plan against the **actual** `TonyPythoneer/herdr-webview` code and latest upstream stable Git tag. Preserve the original icon and do not introduce Apple Developer ID signing or notarization. Keep browser rendering inside existing GPUI Tab/Pane and minimize modifications to upstream. Add default-branch tag polling, safe upstream integration with conflict-blocking PRs, macOS-only CI, deterministic real browser UI tests, and a fork-only GitHub Releases update feed using the existing Herdr auto-updater with stable versioning and SHA-256. Release both a manually installable DMG and auto-updater tar.gz/manifest, but only after all tests pass. Never replace a fork app with an upstream app; retain rollback and explicit restart UX. Do not claim success until checks run green on GitHub Actions. Submit reviewable PRs and preserve full build logs. **Before other changes, optimize the existing CI**: commit the resolved Cargo.lock, use stable toolchain/dependency cache keys without run_number, only save cache on verified misses from trusted builds, skip expensive jobs/release for docs-only pushes without breaking required checks, and report measured timing. Do not remove browser E2E gates.

## Review decisions

- [ ] Automatic release after green CI, but conflicts always require manual review: **recommended**.
- [ ] Original official Herdr and fork can coexist on the same macOS machine? If yes, use an independent Bundle ID and app name (icon remains unchanged).
- [ ] Poll intervals: upstream Git tags every ~6 hours (GitHub Actions); **local WebView App update feed once per 7 days, catching up at the next launch after a missed due date**, not hourly.

## Tracking

Implementation review and acceptance tracking: [GitHub Issue #1](https://github.com/TonyPythoneer/herdr-webview/issues/1). This document alone does not enable CI improvements, upstream auto-sync or in-app updating.
