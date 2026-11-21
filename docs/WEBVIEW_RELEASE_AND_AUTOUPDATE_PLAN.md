# Herdr WebView — upstream sync, CI/CD, and desktop auto-update

> **Review draft — tracked in [Issue #1](https://github.com/TonyPythoneer/herdr-webview/issues/1)**. The upstream-sync and desktop updater are not implemented yet.
>
> Repository: `TonyPythoneer/herdr-webview`; implementation branch: `feature/native-browser-tabs`.

## Locked requirements

> **New user decision (2026-11-21): both WKWebView and locally installed Chrome CDP must be implemented in parallel and kept as selectable engines.** Only the user will decide later whether to remove one. Read Workstream G and the dual-backend specification; it supersedes WK-only assumptions.

> **Confirmed scope: browser automation by Agents is required in this release** — click, fill/type, inspect DOM, evaluate JavaScript with results, and true native WebView screenshots through Herdr's existing CLI. A display-only WebView does not satisfy the requirement.

> **macOS app label decision:** the fork's visible name is **Herdr WebView** (Dock/Finder), while it uses the **same underlying Herdr configuration path and schema**. The original icon remains unchanged.

1. **Keep original upstream Herdr icon** (SVG and `.icns`): do not add any globe.
2. **No Apple Developer certificate or notarization.** Use ad-hoc macOS signing only. macOS may still display Gatekeeper warnings; do not bypass the OS security system.
3. Keep **both browser strategies inside existing Herdr Tab/Pane UI**: native WKWebView as a child view, or a GPUI-rendered stream from a managed headless **locally installed** Chrome CDP instance. No separate visible browser window, no Chrome bundled. Preserve layout behavior and minimize upstream modifications.
4. Release **macOS Apple Silicon** only; use GitHub Actions for build, unit/integration tests, native browser smoke tests and package verification.
5. Automatically follow new **upstream stable Git tags** and keep the Browser customization. Failure/conflict must block release.
6. Enable **in-app update notification/download/restart** from this fork's own GitHub Releases, never from the official upstream app release feed. Keep `herdr-core` update policy separate.

## Confirmed baseline

- Official tag reference: `herdrdev/herdr-app@v0.1.16` (dynamically discover newer upstream stable tags).
- Existing browser implementation: `src/browser.rs`, `src/window.rs`, `src/commands.rs`, `defaults/mods/browser-url/` in `feature/native-browser-tabs`.
- CI: `.github/workflows/browser-macos.yml` currently triggers on pushes to this feature branch only.
- `browser-ci-7` was a successful macOS CI/release with one experimental `.dmg`. It has **no updater tarball or latest.json** and does NOT prove real browser tab end-to-end behavior.
- Fork `src/update.rs::checks_app()` currently disables app updates when `HERDR_BROWSER_FORK` is compiled in.
- Original upstream `src/update.rs` currently polls approximately hourly (upstream baseline; the planned fork app check is weekly), verifies SHA-256, stages a `.tar.gz`, and provides restart/swap recovery. It currently reads `https://releases.herdr.dev/app/latest.json`.
- The existing `tests/shot_pixels.rs` checks GPUI pixels, **not actual WKWebView navigation**.

## Architecture

```text
Upstream stable vX.Y.Z tag
          |
          v
Fork default-branch GitHub Action (once weekly; manual dispatch too)
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
Ad-hoc signed Herdr WebView.app (ORIGINAL icon), updater .tar.gz, manual-install .dmg
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

- [ ] Add `.github/workflows/upstream-sync.yml` to **default branch** (`master` currently): `schedule: '23 3 * * 1'` (every Monday 03:23 UTC / 11:23 Taiwan time) and `workflow_dispatch` for manual checks. A schedule on a feature branch alone will never execute automatically. GitHub scheduled workflows run independently of the user's Mac, are best-effort (can be delayed/dropped), and public-repo schedules may be disabled for inactivity. Each run must compare the latest stable upstream Git tag/SHA against the last integrated tag, so a missed or delayed weekly run catches up on its next actual execution (or manual dispatch).
- [ ] Fetch `herdrdev/herdr-app` **Git tags** rather than GitHub Releases (upstream GitHub Release listing can be empty). Parse semver to identify newest stable; ignore pre-releases unless explicitly enabled.
- [ ] Persist last adopted upstream tag + exact commit SHA, track custom revision separately, enforce idempotency.
- [ ] Update a dedicated integration branch from the tagged source, preserving the browser patch set, original SVG + `.icns`, vendored crates, third-party notices and CI. Produce a PR with meaningful upstream diff. On conflicts, leave PR blocked and never force push away custom changes.
- [ ] Check pinned GPUI, gpui-wry and lb-wry compatibility against **the upstream's new GPUI version**. Only update dependencies when tests pass, and commit Cargo.lock.
- [ ] Auto-publish only after full CI gate passes; do not blindly publish on upstream tag detection.

### Weekly upstream watch policy (user decision)

- [ ] **Upstream GitHub Actions cadence: once weekly** using the default-branch cron above. Do not poll every 6 hours.
- [ ] **Manual override:** `workflow_dispatch` can trigger a check immediately when an urgent upstream version needs integrating.
- [ ] **Catch-up:** each invocation compares the current newest eligible stable Git tag and immutable commit SHA to the last successfully integrated upstream tag, so skipped/delayed schedules do not permanently miss an upstream update; never create duplicate integration PRs/issues for the same tag/SHA.
- [ ] **Failure isolation:** if the upgrade cannot be merged, compiled, or passes no real WebView UI test, keep the last published version unchanged and open/update a deduplicated AI-actionable Issue with the failing logs.
- [ ] **Caveat:** GitHub scheduled workflows are best-effort and may not run exactly every seven days. This weekly server-side schedule is separate from the local App's rolling seven-day catch-up-on-launch update checks.

## Workstream B — Build and distribution

- [ ] Keep `assets/icons/Herdr.icns` **byte-identical** to the adopted upstream icon (optionally CI SHA check); never use the new WebView globe artwork.
- [ ] Reuse upstream `scripts/package` + `scripts/mac-sign` ad-hoc path. Do not invoke `scripts/mac-dmg` because its official Developer ID + notarization secrets are required.
- [ ] From a passing macOS ARM64 CI generate and verify:
  - `Herdr-WebView-macos-arm64.dmg` for manual installs.
  - `herdr-app-macos-aarch64.tar.gz` containing `Herdr WebView.app` for the fork updater (update `src/update.rs::PACKED` accordingly).
  - `latest.json` (stable release asset) with version, target key `macos-aarch64`, download URL, and SHA256 of tarball.
- [ ] Use **fork-specific monotonic SemVer** `0.1.16-webview.1`, `0.1.16-webview.2`, `0.1.17-webview.1` etc. Stamp `Cargo.toml`, `herdr-app --version`, bundle Info.plist, release tag and manifest consistently. The existing simplified `newer()` function must be replaced with correct SemVer numeric component comparisons (e.g. `webview.10 > webview.9`).
- [ ] Publish full release only after test, packaging and digest checks pass. Use stable Releases for `/releases/latest/download/latest.json`; GitHub prereleases aren't included in latest.
- [ ] Keep release artifacts available long-term via GitHub Releases (not temporary Actions artifacts), with required permissions and concurrency control.
- [ ] Adopt the confirmed visible macOS name **Herdr WebView** and a distinct bundle ID while **sharing the exact upstream config directory**; preserve one active instance per shared home (details in App identity section).

### App identity and shared configuration (user decision)

**Only the macOS app identity/display name changes, not the original icon, Herdr configuration format or user data location.**

- [ ] The macOS **Dock hover label, Finder app name and app menu** must visibly read **`Herdr WebView`** (suffix "WebView"). Distribute it as `Herdr WebView.app`, with `CFBundleName` and `CFBundleDisplayName` set to `Herdr WebView` in `scripts/package`. Verify the actual Dock label on macOS CI/a manual macOS smoke test; changing only a DMG name or Release title is insufficient.
- [ ] Retain the **official original Herdr icon** (`Herdr.icns` and SVG) byte-for-byte; do not generate a new globe artwork.
- [ ] Use a **distinct macOS CFBundleIdentifier** for the fork (proposed `dev.herdr.webview`) to prevent LaunchServices identity conflicts with the official `dev.herdr.app`. A different bundle identifier does **not** require a different configuration directory. Use ad-hoc signing; no Apple Developer ID credentials.
- [ ] **Share the exact upstream Herdr config home and schema**: `HERDR_APP_HOME` override first, otherwise `$XDG_CONFIG_HOME/herdr-app` or `~/.config/herdr-app` (see `src/mods.rs::app_dir()`). Do not rename or fork `settings.json`, `mods/`, layout/workspace files, or reinitialize user settings for the WebView flavor. Maintain upstream migration/compatibility rules; never silently overwrite or reset the original data. Do not introduce a new `~/.config/herdr-webview`.
- [ ] Keep `herdr-core` configuration and CLI integration consistent with upstream. Keep the internal binary/CLI identifier (`herdr-app`) where possible; the macOS display name is the intended visible distinction. Display a fork-specific version string such as `0.1.16-webview.1` in the UI/About page, while satisfying macOS Info.plist version-field constraints.
- [ ] **Explicit shared-home concurrency behavior:** `src/control.rs` uses `<app_dir>/herdr-app.sock`. `raise_running()` normally exits a second launch, **but `listen()` currently only warns on a live socket**, leaving a race. Add a strict WebView startup refusal when official Herdr is already running and a hard ownership gate for concurrent launch (see Strict original-vs-WebView process exclusion section). Both variants share one config directory; never run independent concurrent writers against it.
- [ ] Update all hard-coded macOS bundle/package/update installer references together, especially `scripts/package` (bundle + tar root + plist), `src/update.rs::PACKED`, DMG assembly, and any custom installation scripts. The updater must correctly recognize/extract `Herdr WebView.app` without overwriting `Herdr.app`, and still use only the fork's own GitHub update feed.
- [ ] Acceptance: Dock tooltip says **Herdr WebView**; original icon unchanged; Settings/Mods/layout load from the **same Herdr home**; both app bundles can exist on disk; only one process owns the shared-home control socket; the fork update correctly replaces only `Herdr WebView.app`; no Apple signing credentials are required.

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

### Strict original-vs-WebView process exclusion (user requirement)

**Invariant:** With the official Herdr already running, launching **Herdr WebView must not open another application window/process that uses the shared config**. Both variants share the original `~/.config/herdr-app` (or the same resolved `HERDR_APP_HOME`/`XDG_CONFIG_HOME`) and must never write concurrently to it.

**What existing code does:** `src/main.rs` invokes `control::raise_running()` for a bare launch, which connects to `<app_dir>/herdr-app.sock` (`src/control.rs`). If the existing app is listening, the second invocation returns before opening its own window. However, `control::listen()` currently **only logs and returns when a socket already exists**, which is insufficient to enforce exclusion under simultaneous startup, since the new UI may continue without owning the socket. The existing `raise_running()` also raises whichever variant is running, rather than explicitly refusing with a clear message.

- [ ] At **WebView launch**, before settings/workspace writes, check for an already running **official Herdr app** (bundle id `dev.herdr.app`, macOS running-app lookup) and for a live control socket at the same resolved config home. When one is active, **do not launch Herdr WebView**, do not modify configs, do not launch its update worker, and show a concise notice if feasible: `Herdr is already running. Quit it before opening Herdr WebView.` Do not unexpectedly raise the official App.
- [ ] Preserve **at most one active Herdr window owner per shared home** for both launch orders. If the custom app is running, the unmodified official app's existing `raise_running()` is expected to raise the custom app and exit (as upstream currently behaves); do not claim the fork can force changes to official Herdr itself.
- [ ] Treat the socket ownership as a **hard startup gate**, not merely a warning in `control::listen()`. Close/refuse startup if ownership cannot be established; handle an orphaned/stale socket safely. Add a second check immediately before committing to ownership to narrow the launch race. Do not delete a live owner's socket or silently create a second socket.
- [ ] Add macOS launch tests for: official already running → WebView blocked; WebView already running → second WebView not duplicated; WebView running → launching official does not create a second active owner; simultaneous double-launch, stale socket recovery, alternate `HERDR_APP_HOME`, and refusal must not alter settings/workspaces.
- [ ] **Limit statement:** complete race-free mutual exclusion with an *unmodified official binary* cannot be absolutely guaranteed by a WebView-only file lock; rely on both the existing upstream shared socket behavior and explicit macOS process detection, and test each scenario. If a remaining race is discovered, block release and document a safe mitigation rather than claiming a formal guarantee.

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

## Workstream E — Native Pane Adapter / BrowserHost isolation (mandatory before automated upstream release)

**Decision:** Implement the Native Pane Adapter as a concrete refactor, not just a concept in an Issue comment. It keeps browser UI embedded in a Herdr Tab while minimizing modifications to upstream `src/window.rs`.

### Current code and risk

- `feature/native-browser-tabs` currently has `src/browser.rs` (URL policy and WKWebView creation), while **browser state, reconciliation, commands, navigation and toolbar remain mostly inside `src/window.rs`** (~195 additions in this fork). Upstream `src/window.rs` changes significantly across releases, so this is the main merge and behavioral regression risk.
- The existing `Content::Mod` and `Tab::with_pane` types should remain the source of truth for tab/pane persistence. No independent second tab model.

### Implementation checklist

- [ ] Introduce a **`BrowserHost` / `NativePaneAdapter`** module or small internal crate that owns the native `gpui_wry::WebView` handles and their lifecycle, URL policy/navigation, visibility, bounds, focus, toolbar and errors. It should provide a narrow, typed API such as `sync(model_view, window, cx)`, `render(pane_id, ...)`, `navigate(...)`, `close(...)` rather than spreading browser-specific code through `src/window.rs`.
- [ ] Keep **only the smallest documented integration hooks** in upstream `src/main.rs`, `src/window.rs` and command dispatch: module registration, pane render delegation, lifecycle synchronization and Browser command forwarding. Avoid changing the core layout/persistence schema and avoid vendor/GPUI patches unless a tested native composition fix is truly required.
- [ ] Preserve the same native WebView inside an actual Herdr Tab/Pane (not a separate window), including tab switch/close, splits, resize, focus, back/forward/reload and safe dismissal of dialogs/overlays.
- [ ] Add contract tests for the adapter-host boundary plus deterministic macOS WKWebView end-to-end smoke tests. The Browser adapter must not be silently omitted by a successful merge.
- [ ] Perform **one-time historical upgrade rehearsals** against upstream v0.1.14, v0.1.15 and v0.1.16; record compile/test results honestly and any necessary adapter changes. Passing historical tests cannot guarantee future compatibility.
- [ ] In each weekly upstream integration, if host APIs, dependency versions, z-order or real Browser behavior fail, automatically open/update an AI-actionable GitHub Issue and stop release; preserve the last working version.

### Acceptance

- [ ] `src/window.rs` contains only minimal Browser bridge code, with one shared BrowserHost and engine-specific adapters, ownership and responsibilities documented.
- [ ] True native WebView is displayed and usable inside a Herdr Tab on macOS CI; menus/dialogs remain accessible.
- [ ] Upstream merge failure or any Browser regression blocks release, with an Issue carrying context for automated AI repair.

## Workstream F — AI-controlled WebView browser automation (MANDATORY for this release; user decision)

**Non-negotiable user outcome:** A coding Agent operating in Herdr must be able to **navigate and interact with real web pages inside the existing Herdr browser pane**, without opening an external *visible* browser window. With the selected Chrome backend, a managed headless Chrome background process is permitted. This release **must implement**, not merely reserve hooks for, DOM inspection, click, form input, page JavaScript evaluation and accurate WebView screenshots.

### Verified technical foundation and current gap

- Existing fork implements `browser.open`, `browser.navigate`, `browser.address`, `browser.back`, `browser.forward`, `browser.reload` in `src/commands.rs` and `src/window.rs`. **It does not implement Agent click/fill/inspect/eval/screenshot yet**.
- `src/browser.rs::create()` creates `wry::WebViewBuilder` with a URL and attaches a `gpui_wry::WebView` inside GPUI. The pinned `lb-wry` v0.53.3 exposes `WebView::evaluate_script_with_callback` (JSON-serialized callback response), `evaluate_script`, `WebViewBuilder::with_ipc_handler`, and initialization scripts; inspect exact behavior on macOS/WKWebView and avoid blindly changing framework versions. Source: [longbridge/wry src/lib.rs](https://github.com/longbridge/wry/blob/dev/src/lib.rs).
- The current `herdr-app do <command> <JSON args>` route already crosses the local control socket in `src/control.rs` into Herdr commands. **Reuse this path** for Agent access; no standalone browser HTTP server or external UI is required. The **Chrome CDP backend** deliberately starts a managed headless local Chrome process when selected. Make capabilities discoverable via `herdr-app commands` / Agent guidance.
- A GPUI screenshot of a window is not proof it contains pixels from a native `WKWebView`. Provide a native **WebView-specific image capture** path and verify it captures actual web content.

### Agent-facing command contract (implement and document in `src/commands.rs`)

- [ ] `browser.list`: enumerate open native browser panes; return stable pane IDs, active pane, current URL/title/loading state, and (where possible) visible status. Never choose a hidden pane implicitly for mutation.
- [ ] `browser.open` / `browser.navigate` / back / forward / reload: preserve existing commands; return a reliable completion/error state. All commands support an **explicit `pane` ID** for deterministic multi-pane operation.
- [ ] `browser.inspect`: retrieve a compact, structured DOM or accessibility-oriented snapshot (tag, accessible role/name/label/text, link href, input metadata, visibility, element references). Avoid dumping the whole page by default. Add `browser.query` for CSS selector or role/text/label lookups, with stable locator semantics.
- [ ] `browser.click`: click a target using a CSS selector or snapshot element reference / semantic locator. Verify the target exists, is interactable and is in the intended pane; wait for navigation/DOM changes when requested. **Do not silently equate JS `.click()` with native trusted mouse input**; use verified WKWebView-backed native input where needed and document sites that prohibit synthetic events.
- [ ] `browser.fill` / `browser.type`: focus and fill inputs/textareas/contenteditable; fire appropriate input/change events; support clear/replace and real keyboard events when required. Add `browser.press` for Enter/Tab/shortcuts and `browser.scroll` for scrolling in the WebView, not the terminal.
- [ ] `browser.eval`: evaluate explicit page-context JavaScript and **return the actual JSON-serializable value or structured exception** (not just fire-and-forget). Size-limit output; support timeout/cancel; page JS has page-origin permissions **only**, no privileged filesystem or arbitrary native APIs.
- [ ] `browser.screenshot`: capture **the embedded WKWebView's actual rendered pixels** as PNG, optional viewport/clip if supported; return a file path/metadata that the calling local Agent can consume. Prefer `WKWebView.takeSnapshot(with:completionHandler:)` or another documented native capture API after checking compatibility; do not fake with GPUI framebuffer-only tests or DOM-to-canvas screenshots. Apply sensible output limits, lifecycle handling and redaction controls for sensitive pages.
- [ ] `browser.wait`: await page readiness / element appearance/disappearance or navigation with a bounded timeout. All actions return actionable JSON `{ok, pane, url, result|artifact, error?, request_id?}` (exact shape to be codified) and distinguish stale element reference, detached pane, navigation in progress, timeout and permission denied.
- [ ] `herdr-app do browser.click '{"pane":"...","selector":"button[type=submit]"}'` and analogous `browser.fill`, `browser.inspect`, `browser.eval`, `browser.screenshot` calls must actually work from an Agent CLI running inside Herdr; a UI-only toolbar is **not sufficient**. Add usage examples in the Agent skill / command docs.
- [ ] Support more than one WebView pane and prevent accidental cross-pane action. Invalidate ephemeral element references when the document navigates or reloads; identify active document/frame explicitly. Same-origin main-frame DOM automation is the baseline; cross-origin iframes/closed shadow DOM need documented limitations or a verified browser-native workaround, not misleading success.

### Architecture / asynchronous control

- [ ] Extend the mandatory **Native Pane Adapter / BrowserHost** so the automation engine and its WebView handles live in that module; only narrow command dispatch hooks remain in high-churn upstream `src/window.rs`. Reuse Herdr's normal Tab/Pane IDs and model. No separate browser window.
- [ ] Implement asynchronous JS results and screenshot completion **without blocking the macOS UI thread**. Request ID, pending-op registry, per-pane action serialization or explicit concurrency policy, cancellation, bounded queue, structured errors, and strict timeouts. The current `src/control.rs` uses synchronous line-delimited request/reply with approximately 35s client and 5s normal server timeout: adapt the control response plumbing safely so browser operations can complete and report results without deadlocking UI or being cut off unexpectedly.
- [ ] Navigation, tab switch, pane close, app quit, or failed load must cancel/resolve pending commands and release callbacks/native handles. Do not leak resources or send delayed action results to the wrong pane/document.
- [ ] Prefer stable semantic locators and an Agent observe→act→verify cycle. Permit selector-based DOM operations for a reliable baseline but verify actual user-equivalent input where websites need trusted mouse/keyboard events. Do not falsely promise universal automation of CAPTCHA, login challenges or protected pages.
- [ ] Confirm window composition and overlay z-order: native WKWebView must not hide Herdr's own dialogs/menus and must not cause the Agent to operate an obscured or incorrect target.

### Safety / permissions (must not undermine the user's ability to automate)

- [ ] Bind control to the **trusted local Herdr command channel** and explicit browser-pane identity. The current local socket is not a general-purpose public network endpoint; do not expose unrestricted DOM/eval or screenshot RPC to web content. Untrusted pages may contain prompt-injection text: browser DOM results are data, never instructions that authorize native actions.
- [ ] Keep strict URL scheme validation and do not add `file://`, `javascript:` URL navigation or automatic OS-native privileges just to enable scripting. JavaScript evaluation is a separate, deliberate `browser.eval` operation **inside the page origin**.
- [ ] Provide practical approval/permission gating for sensitive or irreversible actions (purchase, payments, credential export, deleting data, sending messages, permission prompts), without disrupting ordinary navigation, inspect, click or fill. Never silently extract cookies/passwords/private browser data to Agent logs or artifacts.
- [ ] Restrict and validate screenshot file paths, clip to requested pane, bound screenshot dimensions/results, avoid logging submitted form values or page secrets, and prevent page-origin IPC from calling privileged Agent/browser automation commands.

### Mandatory macOS E2E acceptance

- [ ] In a **real embedded WKWebView** inside a Herdr Tab, launch a local deterministic HTTP fixture with a labelled input, a button, navigation links and a dynamic DOM status.
- [ ] From the same Agent-exposed `herdr-app do` interface: open → wait → inspect/query → fill/type → click/press → observe DOM change → navigate → back/forward/reload → evaluate JavaScript returning JSON → screenshot; verify the PNG includes actual website pixels and its returned path is accessible to the Agent.
- [ ] Test selector missing/stale, JS exception, wrong pane, timeout/cancel, navigation during action, background tab, close during action, two simultaneous panes, offline page, permission denial, and app quit; validate clear JSON errors and no UI hangs.
- [ ] Test a page requiring keyboard/mouse **trusted events** or explicitly record limitations with an alternative native-input path. Verify z-order/overlay visibility and focus.
- [ ] The browser-automation tests must be a **release gate** and also part of upstream-version integration checks. A build that only shows a web page, or only passes Rust/GPUI screenshot tests, **must not be released as the completed feature**.

### Formal industry-informed Browser Automation API Contract v1.0 (normative)

**Full, versioned contract:** [`docs/BROWSER_AUTOMATION_API_CONTRACT.md`](https://github.com/TonyPythoneer/herdr-webview/blob/feature/native-browser-tabs/docs/BROWSER_AUTOMATION_API_CONTRACT.md) (created 2026-11-21). This document is **mandatory for first-release implementation and release gating**; the command list below is a product-level summary, not a replacement for its definitions.

- [ ] Explicitly distinguish **industry precedents** (Playwright MCP snapshots/locators, Chrome DevTools MCP UIDs and tools, W3C WebDriver BiDi contexts, MCP `inputSchema`/`outputSchema`) from **Herdr's custom `browser.*` CLI contract**. Do not claim Playwright/CDP/BiDi wire compatibility with WKWebView.
- [ ] Implement the contract's stable command/argument/response/error definitions and JSON Schema validation, including `pane`, `document_id`, `snapshot_id`, expiring element `ref`, role/label/text/test_id/CSS locators, navigation/actionability waits, true WKWebView screenshots and native input fallback.
- [ ] Expose the same backend to Agents via **existing `herdr-app do`** and a **local MCP stdio adapter** with discoverable tool schemas. MCP must attach to the *existing selected Herdr browser pane*, whether WKWebView or managed headless local Chrome CDP; it must not create an additional unrelated browser session. Document Agent CLI connection steps.
- [ ] The host must handle async callback results without deadlocking GPUI: accepted operations return `request_id` and are resolved via `browser.result`/`browser.cancel` or the MCP adapter's async waiting. Never report `accepted` as action completed.
- [ ] Include explicit errors and safe defaults: stale refs, ambiguous targets, hidden/blocked buttons, wrong pane, navigation interruption, JS exception, timeout, permission denial and screenshot failures. Enforce user approval for sensitive actions.
- [ ] Add contract tests (JSON Schema + CLI + MCP + actual macOS WKWebView fixture). **A feature is not complete if it only renders web pages.** Capture a native screenshot and verify DOM observe→click/type→verify through the deployed CLI.
- [ ] Scope milestone: all mandatory v1.0 commands in the linked spec and its first-release acceptance matrix must pass before stable publishing; advanced form batching/drag/hover/network debugging may follow. Report native input and cross-origin frame limitations honestly.

**API contract release gate:** [Browser Automation Contract v1.0](https://github.com/TonyPythoneer/herdr-webview/blob/feature/native-browser-tabs/docs/BROWSER_AUTOMATION_API_CONTRACT.md) is normative for **both** engines; all mandatory `browser.*` commands must pass CLI and MCP macOS tests for WKWebView **and** the Herdr-rendered local Chrome CDP mode. No external visible browser.

### WKWebView Chrome-like automation and UI-test reliability policy (mandatory)

> **Full risk register and release gates:** [`Browser Automation API Contract v1.0 — `10. Chrome-like UI testing`](https://github.com/TonyPythoneer/herdr-webview/blob/feature/native-browser-tabs/docs/BROWSER_AUTOMATION_API_CONTRACT.md#10-chrome-like-ui-testing-feasibility-boundaries-risk-gates-and-safer-defaults-2026-11-21). This is **Chrome/Playwright-like Agent semantics**, not wire-level CDP, Playwright or WebDriver BiDi compatibility.

- [ ] **Proof of feasibility before implementation expansion:** demonstrate one test through the running, visible, embedded WKWebView: Agent CLI request → DOM snapshot/query → form fill/click → awaited DOM assertion → real native screenshot. Show trusted-vs-synthetic input evidence. Do not infer support from `lb-wry` JS eval alone.
- [ ] **True native input is a P0 gate:** synthetic JS `element.click()` does not imply real mouse/keyboard events. Validate AppKit/CGEvent or supported WebKit automation input on macOS, including permissions and `isTrusted` where relevant. If unavailable, fail clearly rather than silently substitute a fake pass.
- [ ] **Two UI-test layers:** embedded web page via `browser.*` commands; Herdr GPUI chrome (Tab, split, menu, dialog, focus, overlay) via **macOS native UI harness operating the same Herdr WebView App**. DOM-only browser tools cannot inspect unrelated native GPUI widgets.
- [ ] **Avoid CI illusions:** GitHub-hosted macOS runners may lack Accessibility/TCC grants for native input. Prove the environment first; if necessary use a trusted configured macOS runner for required end-to-end native tests. A skipped/unavailable native gate blocks automatic Release and creates a deduplicated Issue.
- [ ] **Isolated test state:** `HERDR_APP_HOME` points to ephemeral test data; browser test session uses nonpersistent WebKit storage and local/staging pages, not the user's real saved login/session or production configuration. Normal user app continues sharing the original Herdr settings as agreed.
- [ ] **Postcondition is required:** inspect → act → assert state/URL/text/visibility. Do not equate “JS executed” with test success; re-observe before retrying non-idempotent submissions.
- [ ] **One pane, one document:** use exact pane/document/snapshot IDs; invalidate stale refs on navigation, prevent cross-pane Agent actions, limit outstanding async calls, cancel pending work on close/reload. Record structured errors and bounded redacted evidence.
- [ ] **Security boundaries:** browser DOM/screenshot results are untrusted; WebKit page scripts cannot access native/Agent privileges. Validate local Unix socket permissions and peer where supported; no public automation port. Sensitive live-site purchases/sends/deletes require explicit approval; staging fixture tests run unattended.
- [ ] **Platform limits are explicit:** cross-origin iframe, CAPTCHA, site defenses and Chrome-only API domains must not be claimed supported without verified tests. Native screenshots use WKWebView APIs, not GPUI-only frame capture.

**Release acceptance (WK track):** real macOS fixture passes end-to-end CLI and MCP for WKWebView, verifies native input and screenshots. **Chrome track has its own mandatory E2E gate under Workstream G**. Both must pass for a dual-engine stable release; either failed/unavailable required test blocks release and triggers a repair Issue.

### Confirmed operational decisions — 2026-11-21 (implementation requirements)

**All four choices below are final user-approved policies; no further clarification is required for this phase.** Apply them consistently to both Browser engines, CLI, MCP, App UI, CI and GitHub publishing. Detailed contracts: [Dual Backend Plan](docs/DUAL_BROWSER_BACKEND_PLAN.md) and [Browser API Contract](docs/BROWSER_AUTOMATION_API_CONTRACT.md).

- [x] **Auto-release:** on an eligible stable upstream integration, once all required build, WKWebView + Chrome CDP real macOS E2E, CLI/MCP, packaging, integrity and security checks pass **and no integration conflicts remain**, automatically publish the fork's stable GitHub Release and updater manifest **without routine human approval**. Merge/rebase/dependency conflicts **must stop and require human review** through a reviewable PR and actionable Issue. Failed, unavailable or skipped mandatory tests also block publication and open/update a diagnostic Issue (not force-merge/release). Retain one initial manually verified dry run before enabling automated publication.
- [x] **New Browser Tab default:** when `browser.open` or the GUI omits `engine`, use **local Chrome CDP if installed, supported and available**; otherwise **WKWebView**. Explicitly show and return which engine was selected. **This automatic decision only applies at *new Tab creation*.** For an existing saved browser pane without `engine` metadata, keep the historical **WKWebView** behavior. If an Agent/user explicitly selected Chrome, an absent/unusable Chrome returns a clear error **without** silently switching the pane to WKWebView.
- [x] **Chrome login/profile:** normal browsing with Chrome CDP uses a **private, independent, persistent Herdr-managed Chrome user-data directory**, preserving website login/cookies between launches; do not touch the person's daily Chrome default profile and do not delete the persistent profile at Tab close. CI, staging, unattended UI fixtures and tests use isolated **ephemeral** profiles and safe test credentials. Persistent Chrome website data is separate from the original Herdr configuration schema (which remains shared with upstream Herdr as previously decided).
- [x] **Agent production-site permission:** AI may navigate, inspect, search, click ordinary controls, fill routine fields and run UI observations on real websites without repeated per-site permission. **Explicit, action-scoped user approval is required for payment/purchase, transfer, destructive deletion/overwrite, credential export, sending messages, consequential submissions, security changes and similar high-impact actions**. This must be enforced consistently on CLI, MCP, DOM/native clicks, JS eval and direct CDP/script routes; never permit a bypass via `browser.eval`. Because arbitrary website actions cannot be perfectly classified, uncertain-impact cases **request approval**. Unattended CI still uses staging/local fixtures; normal production browsing does not authorize destructive unattended production testing.

## Workstream G — Dual browser backends in parallel (WKWebView + installed Chrome CDP; user decision)

> **Normative implementation specification:** [Dual Browser Backend Architecture and Delivery Plan](https://github.com/TonyPythoneer/herdr-webview/blob/feature/native-browser-tabs/docs/DUAL_BROWSER_BACKEND_PLAN.md). **Both engines must remain in the product and be tested; the user, not the coding agent, will decide which (if any) to remove later.** The shared [Browser Automation API Contract v1.0](https://github.com/TonyPythoneer/herdr-webview/blob/feature/native-browser-tabs/docs/BROWSER_AUTOMATION_API_CONTRACT.md) governs **both** implementations. Earlier Workstream E/F references to WKWebView are **WK backend requirements, not permission to exclude Chrome**.

### Final topology (two strategies, one UI/API)

~~~text
                  Coding Agents (Herdr CLI / local MCP)
                                 |
                  Browser Automation API v1.0
                       (one command contract)
                                 |
                         BrowserHost / Pane
                        engine selected per pane
                    _____________|_____________
                   |                           |
         WKWebViewAdapter                 ChromeCdpAdapter
         macOS WKWebView                 local Chrome headless
         native child in GPUI           CDP Target + Screencast
                   |                           |
                   +---------------------------+
                                 |
                    Existing Herdr GPUI Tab/Pane
~~~

- [ ] Reuse **the same Herdr Tab/Pane, address controls, BrowserHost, CLI, local MCP and automation JSON/error schemas**. Each pane stores `engine = "wkwebview" | "chrome-cdp"` in existing browser props; **old persisted panes missing an engine remain WKWebView**, whereas **new Tabs without explicit engine prefer installed usable Chrome CDP, otherwise WKWebView**. Avoid a parallel Herdr tab/persistence framework or extensive upstream edits.
- [ ] User can explicitly select **WKWebView** or **Local Chrome (CDP)** when creating a Browser Tab; GUI shows **Chrome CDP as the default when installed/usable**, otherwise WKWebView. `browser.open` with omitted `engine` selects that same default, records and returns its resolved engine. The engine is shown in `browser.capabilities`. **Explicit engine choice must never silently fall back**: unavailable explicit Chrome returns an error. No hidden Chrome install; changing an existing pane's engine requires a deliberate close/reopen with a session warning.
- [ ] **WKWebView remains**: existing `gpui-wry`/`lb-wry` child WebView path is retained and refactored behind BrowserHost with native WebKit screenshots and verified input; it works even when Chrome is not present.
- [ ] **Chrome runs as a separate managed, non-visible process using user's installed Google Chrome**: detect install, validate executable/version, launch headless with a dedicated, **persistent** non-default profile for normal browsing, preserving site login/cookies across App restarts. CI and unattended tests instead use isolated ephemeral profiles. Do **not** bundle, auto-download, install or silently update Chrome in the shipped app; do **not** attach to or expose the user's normal Chrome profile/cookies. Chrome 136+ requires non-default `--user-data-dir` for CDP debugging; prefer scoped `--remote-debugging-pipe` rather than listening port. Browser pane shows streamed *page pixels*, **not a native embedded Chrome window**.
- [ ] For Chrome display, prove `Page.startScreencast`/`screencastFrame`/`screencastFrameAck` works **with headless Chrome on macOS**; decode frames and render GPUI image/texture without blocking render thread. It is **experimental**, so do not assert high frame rate or video/IME support without real measurements. Use `Emulation.setDeviceMetricsOverride` for viewport sizing and coordinate conversion; send mouse/scroll/key/text via CDP `Input`. Verify Chinese IME, tab focus, split, resize, long-running sessions, crash recovery and frame backpressure.
- [ ] For Chrome AI operations use actual CDP `Target`/`Runtime`/`DOM`/`Accessibility`/`Input`/`Page`, but expose **only the shared Herdr `browser.*` contract** externally. Each Herdr Pane maps to one CDP target/session and returns selected engine metadata. Chrome screenshots for Agent/UI-test evidence must use `Page.captureScreenshot`, not old screencast frames.
- [ ] Both engines use explicit Herdr pane/document/snapshot IDs and independent pending-action lifecycles; no accidental cross-engine/cross-pane operations. Backend errors are explicit (`CHROME_NOT_INSTALLED`, `CDP_DISCONNECTED`, etc.). Never claim that WKWebView implements CDP.
- [ ] Both engines ship as options in the **same Herdr WebView.app** (original icon, unchanged original Herdr config directory, fork-only self-update and shared-home single-instance guard); Chrome installation remains external. Use separate per-engine website storage/profile; do not imply browser cookies/login state are shared between engines.
- [ ] **CI must test both** backends end-to-end using the same local fixture (open→snapshot→fill/click→assert→eval→screenshot→close) from CLI and MCP, plus native GPUI integration. Prepare Chrome only **inside CI runner**, never the released DMG. Missing Chrome on end-user Mac is graceful (`wkwebview` still usable); missing Chrome on a runner required for the Chrome gate is a CI failure to fix, **not** an excuse to skip.
- [ ] Both backend required checks must pass for automated stable release and upstream merges. Failed/skipped required test on either engine blocks release and opens/updates AI-actionable GitHub Issue; preserve last successful release.
- [ ] Compare **measured** response latency, scroll FPS/visual quality, memory/CPU, video, Chinese IME, snapshot correctness, native UI/overlay behavior, screenshot evidence and recovery across engines. Keep both despite differing performance until the user explicitly decides.
- [ ] If/when user chooses to retire an engine later, plan a separate PR with stored-pane migration/compatibility and rollback; **do not unilaterally prune either backend**.

### P0 feasibility before large refactor

1. WKWebView existing real child view → DOM action/result → trusted/native-input and PNG capture smoke test.
2. Installed Chrome → headless CDP → continuous screencast in **actual GPUI Tab** → real pointer/key/scroll from GPUI → DOM postcondition → CDP screenshot. If headless screencast is unusable, leave the Chrome track blocked with diagnostic proof; **do not remove WK or claim success**.
3. Shared BrowserHost/Agent contract route and process/crash/security behavior for both, including Chrome-absent user Mac.

**Primary rule:** developing two implementations in parallel does not imply silently switching at runtime or failing over between different sites/sessions; the selected engine is explicit and visible.

## Test and acceptance matrix (mandatory)

- [ ] Weekly upstream monitor ignores duplicate, older, moving/unverified and prerelease tags; picks exact stable commit; creates one integration PR. A skipped week is caught up by the next scheduled run or manual dispatch.
- [ ] New upstream changes preserve Browser tab, address UI, navigation and original icon; conflicts and dependency breaks block release with a visible log.
- [ ] Rust unit tests cover valid/invalid URLs, version comparisons (especially numeric webview revisions), manifest format, invalid/foreign URLs and integrity failures.
- [ ] **Real macOS dual-engine E2E + Agent automation:** run the same fixture in WKWebView **and** Chrome CDP shown in a GPUI Tab, using Agent inspect/query/click/fill/type/press/scroll/eval/screenshot through `herdr-app do` and local MCP, asserting true input/result, URL and visual evidence. Additionally validate native GPUI Tab/split/focus/overlay UI. WK screenshot must be native WebKit, Chrome screenshot must come from CDP `Page.captureScreenshot`; GPUI-only screenshots are insufficient.
- [ ] Packaging tests verify original icon, **Herdr WebView.app Dock/Finder identity**, .app bundle contents, shared config home, **hard refusal of WebView launch while original Herdr is running (including race/stale-socket tests)**, ad-hoc code signature, license/third-party notices, archive/manifest SHA-256, and correct fork update feed.
- [ ] App update integration tests verify newer release stage/restart/rollback and failure cases: wrong checksum, older version, malformed manifest, network failures and missing assets; plus weekly rolling cadence and missed-week catch-up scenarios.
- [ ] Release step runs only when required checks pass; only macOS ARM64 deliverables, no Apple certificate/notarization secret required.
- [ ] A newly installed fork checks only its GitHub Release and never downloads the official Herdr app.
- [ ] Report no Apple Developer signing: initial macOS Gatekeeper warning may occur.

- [ ] CI regression suite covers cache correctness, docs-only release gating, warm/cold timing and preserves all required browser checks.

## Suggested PR sequence

0. **PR 0 (CI speed / reproducibility)** — commit complete Cargo.lock, stable cache key, cache-write gate, docs-only skip of heavy jobs; baseline/timing summary.
1. **PR A** — mandatory Native Pane Adapter/BrowserHost isolation, strict original-vs-WebView startup gate, browser-tab E2E, stable SemVer and original-icon package verification.
1a. **PR A2-WK (parallel; required before release)** — implement [Browser Automation Contract v1.0](https://github.com/TonyPythoneer/herdr-webview/blob/feature/native-browser-tabs/docs/BROWSER_AUTOMATION_API_CONTRACT.md) end-to-end: CLI + stdio MCP, schema-validated results, snapshot/locators, async DOM/JS bridge, true native input and WKWebView screenshots, permission checks and macOS E2E tests.
1b. **PR A2-CDP (parallel; required before release)** — local installed Chrome detection, headless CDP process, GPUI screencast, input/events, automation, screenshot, profile security and Chrome E2E.
1c. **PR A3** — shared per-pane engine selector, CLI/MCP contract-parity and dual-engine release gates, performance/compatibility findings.
2. **PR B** — independent GitHub manifest / macOS updater tarball + fork-specific app update code, tests and one-time migration.
3. **PR C** — default-branch upstream-tag polling + safe integration PRs + compatible WebView reapplication + release gating.
4. Enable fully automatic release only after all tests and a successful manual dry-run of the workflow.

## Implementation prompt for coding agent

> **Approved operational rules:** automatically Release only after **all required CI is green** with **no unresolved integration conflicts**; conflicts require **human review**. New Browser Tabs prefer **installed usable Chrome CDP**, else WKWebView; **historical Tabs without engine metadata remain WKWebView** and explicit engine failures do not fall back. Keep a dedicated **persistent** Chrome browser Profile for normal browsing and **ephemeral** CI/test profiles. Agents can perform ordinary production website actions without repeated prompts; require explicit **action-scoped approval** for payment/transfer/deletion/credential export/sending/consequential changes including through JS eval/CDP. Preserve both browser engines and every previous locked requirement.
>
> **Important updated scope:** implement the [Dual Browser Backend Plan](https://github.com/TonyPythoneer/herdr-webview/blob/feature/native-browser-tabs/docs/DUAL_BROWSER_BACKEND_PLAN.md) **in parallel with, not instead of**, the WKWebView work. Keep two selectable engines `wkwebview` and `chrome-cdp` behind the same BrowserHost and Agent API; use only installed local headless Chrome and render its frames in GPUI. Do not bundle Chrome, automatically install it, silently switch backends, or remove WKWebView. Require both engines' real macOS E2E/CLI/MCP gates before stable release. The user alone decides future removal.
>
> Implement the above plan against the **actual** `TonyPythoneer/herdr-webview` code and latest upstream stable Git tag. Preserve the original icon, use the macOS display name **Herdr WebView**, and retain the *original Herdr configuration paths and settings* (not a new app-specific home). Use a distinct macOS bundle identifier to prevent app identity conflicts, but keep the shared-home single-instance rule. Do not introduce Apple Developer ID signing or notarization. Implement [the normative Browser Automation API Contract v1.0](https://github.com/TonyPythoneer/herdr-webview/blob/feature/native-browser-tabs/docs/BROWSER_AUTOMATION_API_CONTRACT.md) and all of **Workstream F** as first-release requirements, including the **same BrowserHost and selected pane backend** reachable through CLI and a local MCP stdio adapter: the Agent must actually be able to inspect DOM, query locators, click, fill/type, press/scroll, evaluate page JavaScript and receive results, wait, and capture true backend-specific PNG screenshots (**native WKWebView capture or CDP `Page.captureScreenshot`**) through `herdr-app do`; do not settle for display-only browser support or any separate *visible* Chrome/Safari UI. Implement the mandatory Native Pane Adapter/BrowserHost refactor before enabling automatic upstream release: move WebView state/navigation/lifecycle into an isolated module and retain only narrow hooks in upstream code. Keep browser rendering inside existing GPUI Tab/Pane and minimize modifications to upstream. Enforce a hard macOS startup refusal if official Herdr is already running with the same shared config; a live socket must never be treated as a nonfatal warning, and tests must cover both launch orders and startup races. Add default-branch tag polling, safe upstream integration with conflict-blocking PRs, macOS-only CI, deterministic real browser UI tests, and a fork-only GitHub Releases update feed using the existing Herdr auto-updater with stable versioning and SHA-256. Release both a manually installable DMG and auto-updater tar.gz/manifest, but only after all tests pass. Never replace a fork app with an upstream app; retain rollback and explicit restart UX. Do not claim success until checks run green on GitHub Actions. Submit reviewable PRs and preserve full build logs. **Before other changes, optimize the existing CI**: commit the resolved Cargo.lock, use stable toolchain/dependency cache keys without run_number, only save cache on verified misses from trusted builds, skip expensive jobs/release for docs-only pushes without breaking required checks, and report measured timing. Do not remove browser E2E gates.

## Review decisions

- [x] New Browser Tab default: prefer usable locally installed Chrome CDP; otherwise WKWebView. Old Tabs missing metadata remain WKWebView; explicit engine failures never silently fall back.
- [x] Chrome browser data: dedicated **persistent** Herdr Chrome profile in normal browsing; **ephemeral** test/CI profiles; never touch Chrome default profile.
- [x] Agent real-site access: ordinary actions allowed; high-impact actions require specific user approval across CLI/MCP/JS/CDP, with no silent bypass.
- [x] **Dual Browser Backend:** keep both WKWebView and installed local Chrome CDP for this implementation, select per pane, same API/MCP/GPUI Tab. Do not bundle Chrome, do not remove either until an explicit user decision. Both E2E gates mandatory; Chrome absence on user Mac is handled gracefully.
- [x] UI testing: Agent operates the **selected Herdr pane backend** (native WKWebView or local Chrome CDP image stream inside GPUI Tab) via the same CLI/MCP browser commands. Native macOS UI harness tests non-web Herdr chrome. WK does not implement CDP; Chrome backend uses genuine CDP internally, with no separate visible Chrome window. Must validate input and backend-accurate screenshots; skipped mandatory CI gates block release.
- [x] **Production websites:** routine Agent browsing and ordinary UI interaction allowed; high-impact operations require action-scoped approval (also for scripts/eval). **Unattended tests remain on isolated local/staging fixtures and ephemeral profiles**; don't use live accounts or original Herdr config in CI.
- [x] Browser Automation API Contract v1.0: inspired by Playwright MCP, Chrome DevTools MCP, WebDriver BiDi and MCP; **Herdr custom API**, not false protocol compatibility. CLI + local MCP adapter and native macOS WebView E2E are release requirements.
- [x] First-release scope: AI can directly operate embedded WebView (DOM, click, input, JavaScript, native screenshots); not postponed.
- [x] **Automatic publishing approved:** publish after **all mandatory CI checks pass and no integration conflicts remain**; merge/rebase/dependency conflicts require human review, and CI failures or skipped required tests block release with an Issue.
- [x] App identity decision: **Herdr WebView** in Finder/Dock, distinct macOS bundle ID, unchanged original icon, shared Herdr config home; both bundles may be installed but cannot simultaneously own the same config socket.
- [ ] Poll intervals: upstream Git tags **once per week** (GitHub Actions, Mondays 03:23 UTC; `workflow_dispatch` for on-demand checks); **local WebView App update feed once per 7 days, catching up at the next launch after a missed due date**, not hourly.

## Tracking

Implementation review and acceptance tracking: [GitHub Issue #1](https://github.com/TonyPythoneer/herdr-webview/issues/1). This document alone does not enable CI improvements, upstream auto-sync or in-app updating.
