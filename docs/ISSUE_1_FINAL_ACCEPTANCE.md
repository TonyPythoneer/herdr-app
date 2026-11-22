# Issue #1 — unified completion and release acceptance

> **Status: IN PROGRESS / NOT RELEASE READY.** This is the single integration
> review surface for closing [Issue #1](https://github.com/TonyPythoneer/herdr-webview/issues/1).
> The Issue remains the *normative* source of every original requirement and
> decision. This document groups all remaining work without relaxing or
> silently treating any unchecked requirement as accepted.

## Current baseline (2026-11-22)

- Base: `feature/native-browser-tabs` at `ae25116`; PRs #2–#16 are merged.
- Browser engines: WKWebView inside the native Herdr pane and installed Chrome
  CDP via a managed headless process rendered into a Herdr pane. **Keep both.**
- A successful `browser-macos` run currently proves selected Chrome fixture
  interactions, WK native child-view pixels/trusted input, packaging and a
  temporary updater archive. It does **not** prove the shipped application and
  its real native GPUI approval control behave end-to-end.
- `webview-auto-release` deliberately skips publication until all eight
  exact-SHA check runs are present and green; green overall workflow status
  with skipped publishing steps is **not** a successful release.
- No verified stable GitHub Release or successfully exercised end-user
  in-app update has been established. Preserve the last working build.

## Completion definition: every workstream must be green

Each item below requires code **and** an executable acceptance test or explicit,
reviewed evidence; checking a box because a PR merged is insufficient.

### A — Upstream version adoption and maintenance boundary

- [ ] Manually dispatch the `upstream-sync` workflow from `master`;
  demonstrate the latest stable `herdrdev/herdr-app` Git tag and immutable SHA
  are detected, with idempotent no-op or one integration PR.
- [ ] Verify a conflict/incompatible GPUI or Wry change fails closed, opens or
  updates one actionable diagnostic Issue, and does not overwrite fork changes.
- [ ] Confirm the upstream Git tag selection, last-adopted tag state, weekly
  Monday 03:23 UTC schedule, missed-run catch-up and lockfile/notices checks.
- [ ] Reduce upstream-sensitive `src/window.rs` edits behind a narrow
  BrowserHost/native-pane adapter, with a tested lifecycle and a documented
  upstream compatibility matrix.
- [ ] Preserve both engines and their persisted pane identities across a
  proposed upstream tag integration; no silent WK/Chrome fallback.

### B — App identity, packaging and shared user data

- [ ] Verify the built macOS ARM64 app shows `Herdr WebView` in Dock, Finder,
  menu and Info.plist; `dev.herdr.webview` is distinct from official Herdr.
- [ ] Compare the icon bytes against adopted upstream; ensure no globe asset.
- [ ] Confirm the fork and official app use the exact upstream Herdr home,
  settings/schema, layouts, mods and migration behavior with no data reset.
- [ ] Prove launch-order, stale-socket, simultaneous-start, custom HOME and
  non-mutating refusal behavior with an **unmodified** official Herdr app.
- [ ] Package an ad-hoc signed DMG and an updater tarball rooted at
  `Herdr WebView.app`; verify `herdr-app --version`, Info.plist,
  fork-specific SemVer, integrity digest and notices against the real package.

### C — Fork updater and recovery

- [ ] Use only this fork's GitHub stable `latest.json` for app updates; keep
  `herdr-core` on its separate upstream update policy.
- [ ] Test first launch, 6d23h skip, seven-day deadline, missed launch,
  long-running app deadline, sleep/wake, offline backoff, and explicit manual
  check; successful checks persist without a background daemon.
- [ ] Test real app update download, SHA-256 verification, safe staging, UI
  notification, restart, replacement, recovery and never overwriting official
  `Herdr.app`. Fail closed on wrong version/repo/target/URL/hash.
- [ ] Verify Gatekeeper/ad-hoc signing behavior truthfully; do not bypass macOS
  security. Document the one-time manual upgrade from older browser-ci builds.
- [ ] Publish a first stable release **only after** every mandatory E2E gate
  is green at the **same exact integration commit** and all archive checks pass.

### D — CI, provenance and release integrity

- [x] Arrange for cross-platform `ci` to run on pushes to the maintained
  `feature/native-browser-tabs` branch, not just `master`, because the
  release gate requires exact-SHA named checks.
- [ ] Prove the integration push produces `test` plus each of the three named
  `platform-tests` check runs on its exact SHA, and preserves cross-platform
  notice/lockfile verification.
- [ ] Keep fast docs-only PR checks and stable dependency-aware Rust caches;
  report actual full source build and warmed docs-only timings rather than
  treating CI check success as speed evidence.
- [ ] Implement and wire two **genuine** required checks named
  `wkwebview-native-e2e` and `browser-gpui-approval-e2e`. They must execute
  the shipped Herdr app, not merely a synthetic fixture window or mocked
  approval. Missing, failing or skipped checks block publishing.
- [ ] Validate release guard on the exact SHA for missing, pending, failed,
  duplicate/rerun and stale check-runs; CI and manual dispatch must not bypass.
- [ ] Produce version-pinned DMG, tar, manifest and hashes from previously
  passing artifacts, publish stable Github Release only with manifest last,
  and never overwrite an existing published tag.

### E — BrowserHost and upstream isolation

- [ ] Define an engine-independent pane-scoped BrowserHost lifecycle/dispatch
  boundary (create, navigate, bounds, visibility, focus, input, observe,
  screenshot, cancel, close). Keep upstream's Tab/Pane model authoritative.
- [ ] Keep browser ownership, Chrome process/session/profile cleanup, WK child
  view lifecycle and z-order correct through split, resize, zoom, move, hide,
  show, tab switch, close and restart.
- [ ] Avoid parallel browser tab/persistence model, browser-specific copies of
  upstream tab logic and destructive config migration.
- [ ] Verify all browser commands stay scoped to their actual pane/document,
  and callbacks/results cannot be delivered to a different pane or an obsolete
  navigation epoch.

### F — Agent browser automation and authorization

- [x] The shipped WKWebView native Agent implementation currently supports
  snapshot/query/inspect/click/click_at/fill/type/press/wait/eval/screenshot;
  advertise these accurately, but do not claim unsupported scroll or IME.
- [ ] Add WKWebView native trusted scroll with accurate viewport mapping and
  real WebKit event/postcondition assertions; retain safety checks.
- [ ] Support required Unicode/IME entry and non-ASCII text through a verified
  trusted native-input route, without interpolating submitted text into page JS.
- [ ] Verify same `browser.open → snapshot → locate → click/type/fill →
  wait → eval → screenshot → close` sequence via **real Herdr CLI and MCP**
  against both engines in the shipped app.
- [ ] Verify cancellation, per-pane operation limits, stale snapshot/locator,
  navigation change, callback timeout, invalid input, bounded output and
  private screenshot artifacts for both engines.
- [ ] Keep normal browsing available, but require **native human approval**
  for payments, transfers, destructive actions, consequential form submissions,
  credential exports, arbitrary evaluation and ambiguous controls.
- [ ] Prove a page, CLI or MCP caller **cannot grant its own approval**, cannot
  bypass denial via eval/coordinate click/alternate path, and never leaks user
  cookies, passwords or page content into CI artifacts.
- [ ] Keep production-site permissions distinct from local/staging fixture
  testing; no real payments, deletions or messages in CI.

### G — Dual-engine UI and usability

- [ ] Keep both WKWebView and locally installed Chrome CDP selectable in the
  same Herdr Tab/Pane; Chrome-first only when usable, explicit engine never
  silently falls back, old persisted panes remain WK by default.
- [ ] Verify persistent, separate, owner-only Chrome profile, correct cleanup
  and ephemeral fixture profile; never bundle Chrome or touch the daily profile.
- [ ] Verify Chrome screencast ACK, viewport resize, mouse/keyboard/scroll,
  focus, navigation, screenshots and background-pane lifecycle in actual GPUI.
- [ ] Verify WK native child overlay/z-order, resize, split/tab switching,
  focus, actual screenshot pixels and trusted input in actual Herdr UI.
- [ ] Compare both engines on the same local fixture and publish measured
  latency, frame/focus limitations and known IME differences without claiming
  they are technically identical.

## Mandatory E2E evidence matrix

| Test | WKWebView | Chrome CDP | Where evidence must come from |
| --- | --- | --- | --- |
| Start packaged app, open/select real pane | Pending | Pending | Real macOS app + CLI |
| Navigate/snapshot/query/inspect | Pending | Pending | CLI and stdio MCP |
| Trusted click, keys, text, scroll | Partial fixture only | Partial CDP fixture only | Real pane + DOM postconditions |
| Real screenshots from correct pane | Child-view fixture passes | CDP fixture passes | Shipped app, PNG + dimensions |
| Focus, split, resize, tab hide/close, z-order | Pending | Pending | Native app GUI |
| Native approval and irreversible-action denial | Pending | Pending | Real GPUI controls |
| Cancellation, timeout, navigation-race isolation | Partial | Partial | CLI/MCP E2E with real panes |
| Update/restart with user data preserved | Pending | Pending | Packaged app lifecycle |

**Required final exact-SHA GitHub checks:** `test`,
`platform-tests (linux-x86_64, ubuntu-22.04)`,
`platform-tests (macos-aarch64, macos-15)`,
`platform-tests (windows-x86_64, windows-2022)`,
`Browser CI required check`, `browser-macos`,
`wkwebview-native-e2e`, and `browser-gpui-approval-e2e`.

The two native E2E checks **must not** be manufactured by renaming
`wk_native_pixels`, `wk_native_input`, a mocked-approval unit test or
a packaging smoke. A missing trusted runner is a blocker, not a skip/pass.

## Evidence and merge policy

1. For each checked item, link the exact source commit and a passing test,
   workflow run, or reviewed manual macOS test recording and the tested SHA.
2. Re-run all mandatory checks after merging this PR to the maintained
   integration branch. PR-head checks do not substitute for checks on that
   new integration SHA.
3. Keep this PR in **Draft** until implementation, checks, package, stable
   release and real update/recovery acceptance are all complete.
4. No bypasses, force-merges, mock-success check names or releases based only
   on `browser-macos` success. Keep Issue #1 open until release actually works.
5. Reconcile this grouped matrix with **all original Issue #1 checkboxes**
   before closing; nothing in this grouping waives any original requirement.
