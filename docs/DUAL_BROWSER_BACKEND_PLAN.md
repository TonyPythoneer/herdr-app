# Herdr WebView — Dual Browser Backend Architecture and Delivery Plan

> Status: **DESIGN / NOT YET IMPLEMENTED.** User decision (2026-11-21): **develop and preserve BOTH WKWebView and system-installed Chrome CDP strategies in parallel.** Do not select or remove one until the user reviews working implementations and benchmarks. This document supplements [GitHub Issue #1](https://github.com/TonyPythoneer/herdr-webview/issues/1), [the release/CI plan](WEBVIEW_RELEASE_AND_AUTOUPDATE_PLAN.md) and the [normative Browser Automation API Contract](BROWSER_AUTOMATION_API_CONTRACT.md).

## Decision: two engines, one Herdr experience

| Concern | Engine A — Native WKWebView | Engine B — Installed Chrome + CDP |
| --- | --- | --- |
| Runtime | macOS system WebKit via existing `wry` / `gpui-wry` | **Locally installed** Google Chrome executable, headless process controlled by CDP |
| Display inside Herdr Tab | Real native child WKWebView under GPUI | Decoded Chrome CDP screencast frames drawn as GPUI image/texture inside Tab |
| AI automation | WKWebView JS/eval bridge + verified input + native screenshot | Chrome DevTools Protocol: Runtime/DOM/Accessibility/Input/Page |
| Screenshot for Agent | Native `WKWebView.takeSnapshot` (actual webpage) | CDP `Page.captureScreenshot` from the matching Target |
| User click, scroll, keyboard | Verified native macOS event route, subject to permissions | GPUI event → CSS-pixel mapping → CDP Input dispatch, subject to IME/keyboard validation |
| Extra bundled browser | **No** (uses macOS WebKit) | **No** (detect user's installed Chrome, never download/bundle it in a DMG) |
| Availability | macOS only | Optional when a supported local Chrome executable is installed |
| Principal risk | Trusted native input, WebKit overlay/z-order and automation | Experimental screencast performance, input/IME, headless site compatibility, Chrome installation/version |

**Important:** Chrome CDP mode starts **a separate background headless Chrome process**, but **never opens a second visible browser window**. Chrome is not a framework embedded inside the Herdr process. Herdr's GPUI screen is a decoded visual stream, not a native Chrome surface. Do not label CDP screencast as zero-copy native embedding.

The shipped app remains **Herdr WebView**, with the **original Herdr icon**, **original Herdr config home** (`~/.config/herdr-app` / upstream overrides), **distinct fork bundle identifier**, fork-only weekly update feed, and original-vs-fork single-instance restriction. Both engines run within the same Herdr WebView app.

## 1. Shared architecture, no duplication of upstream Tab logic

```text
                 Coding Agent (Herdr CLI / local MCP stdio)
                                   |
                         Browser Automation API v1.0
                      (one schema + one error contract)
                                   |
                           BrowserService / Host
                       pane-id -> engine + document-id
                                   |
                     +-------------+-------------+
                     |                           |
               WkBrowserAdapter            ChromeCdpAdapter
              gpui-wry / WKWebView         locally installed Chrome
               native child view         CDP Target, Runtime/Input
                native snapshot          screencast + GPU texture
                     |                           |
                     +-------------+-------------+
                                   |
                           Existing GPUI Tab/Pane
                       (Herdr model owns identity)
```

- [ ] Define `BrowserEngine = WKWebView | ChromeCdp`, `BrowserSession` and typed, asynchronous `BrowserBackend` interface owned by `BrowserHost` (create, navigate, snapshot/inspect/query, click/fill/type/press/scroll, eval, screenshot, wait, close, set_bounds, set_visible, focus, capabilities). Prefer small traits or explicit dispatch; **avoid adding methods to Herdr upstream core types**.
- [ ] Keep **Herdr `Tab` / `Pane` IDs** authoritative. One existing pane owns exactly one engine/session. Browser backend is stored in the existing `Content::Mod.props` browser pane metadata (`engine: "wkwebview" | "chrome-cdp"`), **not** a parallel custom tab schema. Backwards-compatible absent `engine` means `wkwebview`.
- [ ] Minimal additions to `src/window.rs`: pass pane lifecycle/geometry/input to BrowserHost and receive a GPUI renderable. Avoid duplicating browser tab state, address bar, commands or popup logic for each engine.
- [ ] Keep the existing WebKit module intact until the WK adapter is refactored and tested. Do not remove `gpui-wry` or `lb-wry` until the user **later explicitly chooses** to retire WKWebView.
- [ ] `browser.open` accepts optional `engine`; menu / browser creation UI exposes a simple engine choice, e.g. `WKWebView` and `Local Chrome (CDP)`. Return selected engine with pane ID in CLI/MCP results. The Chrome option is disabled with a clear install requirement when unavailable. No silent engine fallback or Chrome download.
- [ ] `browser.list` and `browser.capabilities` advertise per-pane engine and engine-supported features. Other `browser.*` requests always include an explicit pane and route to its selected engine. Engine is stable for the pane's session; switching engines is an explicit **close-and-reopen** operation, warning that page state and login sessions may not transfer.
- [ ] Preserve all pre-existing `Content::Mod` documents and upstream startup behavior. Engine choice must not cause settings/layout migrations that reset the user's existing Herdr preferences or Mods.

## 2. Backend A — existing WKWebView

- [ ] Keep `src/browser.rs`'s current safe URL normalization, `Wry::WebViewBuilder`, WKWebView in native Herdr Tab, tab switch/bounds/focus and WebKit navigation controls.
- [ ] Move WKWebView lifecycle/state out of high-churn `src/window.rs` into `WkBrowserAdapter` behind BrowserHost, with narrow hooks and no change to the Herdr core model.
- [ ] Implement DOM snapshot/locators, async JavaScript result, actions/verification, native input and `WKWebView.takeSnapshot`. Distinguish `dom_synthetic` from proven `native_trusted`; do not claim universal event trust.
- [ ] Test real WebKit tab with deterministic local page, input methods, overlays, multipane resize and split, screenshot and agent CLI/MCP completion.
- [ ] No external Chrome dependency for this engine; **WKWebView must remain fully functional even if Chrome isn't installed**.

## 3. Backend B — installed Google Chrome + CDP (no bundled Chromium)

### Chrome discovery and lifecycle

- [ ] On macOS, discover legitimate existing Chrome installation by known bundle ID / LaunchServices and stable paths; validate executable exists, is the intended app, executable, and is a supported Chrome version. An explicitly configured custom path is optional and must be validated. **Do not download Chrome, Chromium or Chrome-for-Testing in the shipped app**.
- [ ] Start `--headless=new` or another tested compatible headless mode, plus **non-default `--user-data-dir`** and a private, dedicated Herdr Chrome profile. Chrome 136+ requires a nonstandard user-data-dir when using `--remote-debugging-pipe` or `--remote-debugging-port`. Prefer **CDP `--remote-debugging-pipe`** scoped to the spawned process; if fallback TCP debugging is necessary, use ephemeral local loopback only, random port and no remote exposure. Never attach to the user's already-running Chrome's default profile.
- [ ] Browser profile is independent of the **Herdr settings schema**: use a dedicated subordinate data location for Chrome cookies/session. Allow persistent profile for normal Herdr use (remember site logins), but ephemeral profile in CI/staging, never mix with daily Chrome's profile. Secure permissions, lifecycle cleanup, crash recovery and no orphan processes; multiple Herdr panes may share one dedicated Chrome process with separate Target contexts.
- [ ] `CHROME_NOT_INSTALLED`, `CHROME_UNSUPPORTED_VERSION`, `CHROME_START_FAILED`, `CDP_DISCONNECTED` are explicit errors; don't silently load the page into Safari or WKWebView. Expose `browser.capabilities`/UI availability status. Version negotiation and target/session recovery are verified against actual installed Chrome.
- [ ] Use Chrome's `Target` domain to create/attach one page target per Herdr Browser pane; route CDP messages using explicit browser / target / session IDs. Prevent cross-pane mixing; cancel or fail pending actions on target close/crash.

### GPUI rendering and user input

- [ ] Run **P0 feasibility spike**: in real Herdr GPUI Tab, launch installed headless Chrome, navigate to a local test page, stream frames through `Page.startScreencast` / `Page.screencastFrame` and `Page.screencastFrameAck`, decode and paint in the GPUI pane; confirm that this **experimental** protocol works with the tested headless mode. Avoid interpreting a one-off screenshot as live rendering.
- [ ] Throttle/backpressure frames, acknowledge only according to stream protocol, discard outdated frames on resize/navigation, keep memory bounded, and stop stream when pane is hidden if viable. Avoid blocking GPUI render thread. Compare CPU/RAM, responsiveness, scrolling, video, long session stability and tab switching against WKWebView; no guarantees before benchmark.
- [ ] Synchronize Chrome viewport size and scale (`Emulation.setDeviceMetricsOverride`) with GPUI pane bounds; correctly convert pointer coordinates, pixel density, scroll offset and CSS pixels; support multiple spaces, split panes, resize, focus and background/visible changes.
- [ ] Forward GPUI mouse, wheel and key events to CDP `Input.dispatchMouseEvent`, `Input.dispatchKeyEvent` and appropriate text insertion APIs; separately test macOS Chinese/Japanese IME composition, shortcuts, selection, clipboard/paste, drag behaviors, virtual keyboards and focus restoration. Do **not** assume all macOS IME events can be faithfully reconstructed through CDP without a PoC.
- [ ] Native Herdr menus/dialogs must draw above Chrome stream, unlike an independent OS window. Screen readout remains in GPUI; Chrome produces only page content.

### Agent automation, evidence and safety

- [ ] Implement same observable Browser Automation API v1.0 using CDP's `Runtime`, `DOM`, `Accessibility`, `Input`, `Page` and `Target` domains. Use stable Herdr snapshot refs/locators and errors; raw CDP exposes extra capabilities internally only and does not change CLI/MCP contract without API versioning.
- [ ] Use `Page.captureScreenshot` for **action evidence**, with matching pane/target/document metadata and PNG checksum; don't use a stale compressed stream frame as test proof.
- [ ] Do not give websites access to CDP, host CLI, filesystem or privileged IPC. Never expose the debugging endpoint over an untrusted network; review Chrome flags for security regressions. Browser output/HTML remains untrusted prompt injection data. Gate irreversible live-site actions and mask secrets in CI artifacts.

## 4. Shared API contract / explicit capability parity

**[Browser Automation API Contract v1.0](BROWSER_AUTOMATION_API_CONTRACT.md) is normative for both engines.**

- [ ] For each engine, implement: `browser.open/list/capabilities/navigate/back/forward/reload/snapshot/query/inspect/click/click_at/fill/type/press/scroll/wait/eval/screenshot/result/cancel`; don't report `supported` until **that engine** passes real CLI and MCP tests.
- [ ] Responses include an explicit `engine` field (example `"wkwebview"` or `"chrome-cdp"`) and refer to a Herdr pane. Unified errors, timeouts, status and snapshot/document identity across engines. An optional feature unsupported on one engine yields `UNSUPPORTED_CAPABILITY` with actionable detail rather than silent fallback.
- [ ] Present distinct engine-specific capabilities when input trust, cross-origin iframes, downloads, popup behavior or screencast features differ. **Do not present both engines as pixel or website-behavior equivalent.**
- [ ] No direct Chrome remote browser port for Agent access; CLI and local MCP stdio use the same Herdr BrowserService and local socket, so automation always controls the actual displayed pane.
- [ ] Preserve session/engine identity across document navigation; never let an Agent action accidentally attach to a reused Chrome Target ID or a stale WK document.
- [ ] Keep Agent-facing demos and tests identical across both engines where behaviors are shared; engine-specific diagnostic assertions explicitly labelled.

## 5. CI gates and rollout (both remain, no winner yet)

- [ ] **P0 spikes in parallel**, each producing independent evidence: (A) WK actual native input & snapshot, (B) Chrome headless screencast + GPUI rendering & CDP input, all with real embedded Tab. This establishes feasibility before rewriting high-churn Herdr files.
- [ ] **CI engines matrix:** run typed JSON Schema/unit tests once; run complete browser API integration fixture **for `wkwebview` and `chrome-cdp`**, each with true page interaction, screenshot, assertion, multipane focus/close and crash recovery; native container GPUI tests still run once with both engine renderers.
- [ ] CI may provision an isolated Chrome installation **on the runner** or use a runner with Chrome already installed; that is **not** bundling Chrome with the released DMG. Record Chrome channel/version in test summary and test the locally installed stable channel; handle changes safely.
- [ ] A user Mac without Chrome can install/run Herdr WebView and use WKWebView without error; attempting Chrome engine results in a helpful installation requirement and does not corrupt an existing Tab. The packaged app must not carry `Google Chrome.app`, Chromium binaries or Playwright browser caches.
- [ ] **Both engines are required release gates while this dual-engine decision is active**: a failure or skipped required E2E in either backend blocks promotion/release and creates or updates a diagnostic Issue. A missing Chrome on a test runner is a CI configuration problem, not a reason to skip the Chrome release gate.
- [ ] Upstream weekly integration always carries both adapters and runs the two-engine E2E suite before a new fork release. No automatic deletion or silent disabling of either engine.
- [ ] Benchmark actual WK vs Chrome: first meaningful paint, sustained scroll/input responsiveness, CPU, RSS/memory, background idle, screenshot reliability, video/IME support, crash restart and long running stability. Report measured data with Chrome version, macOS and hardware, not guesses.
- [ ] Once user explicitly selects an engine to remove later: separate reviewable removal PR, configuration migration/notice for panes using that engine, no loss of user data or silent engine substitution. Do not remove based on performance preference inferred by AI.

## 6. Recommended implementation PR breakdown

0. **PR 0**: existing CI cache & Cargo.lock reproducibility optimization.
1. **PR A1**: shared BrowserBackend/BrowserHost API, pane engine metadata and UI selection, old WK code preserved and functional.
2. **PR A2-WK**: finish WK input/DOM/eval/native screenshot + macOS real E2E.
3. **PR A2-CDP**: independent installed-Chrome discovery, headless launch, CDP transport + screencast GPUI renderer, CDP input/automation, native UI tests.
4. **PR A3**: shared CLI + MCP, capability parity matrix, browser user-visible engine selector, security and performance tests, both engines in packaged App.
5. **PR B/C**: fork-only installer/updater and weekly upstream Git tag CI integration with strict two-engine release gates.

**Do not conflate implementation of the old Browser UI with completed Agent automation or completed Chrome screen integration.**

## Official technical sources

- Chrome DevTools Protocol [Page](https://chromedevtools.github.io/devtools-protocol/tot/Page/), [Input](https://chromedevtools.github.io/devtools-protocol/tot/Input/), [Emulation](https://chromedevtools.github.io/devtools-protocol/tot/Emulation/), [Target](https://chromedevtools.github.io/devtools-protocol/tot/Target/).
- Chrome Security, [remote debugging switches and default-profile restriction since Chrome 136](https://developer.chrome.com/blog/remote-debugging-port).
- Apple [WKWebView](https://developer.apple.com/documentation/webkit/wkwebview).
- [Browser Automation API Contract](BROWSER_AUTOMATION_API_CONTRACT.md) and existing [Herdr fork release plan](WEBVIEW_RELEASE_AND_AUTOUPDATE_PLAN.md).
