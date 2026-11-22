# WebView adapter extraction: ownership, migration and release gates

Track: [Issue #18](https://github.com/TonyPythoneer/herdr-webview/issues/18). First implementation targets `feature/native-browser-tabs`, NOT `master`. Browser PR #17 merged into that integration branch on 2026-11-22. Issue #1 remains authoritative for shipped App release acceptance.

## Purpose

Reduce fork-versus-upstream merge conflicts without losing native BrowserPane integration. The desired topology is:

```
herdrdev/herdr-app (upstream)
         | upstream sync
         v
TonyPythoneer/herdr-webview (application and host bridge)
         | immutable Rust crate dependency
         v
TonyPythoneer/herdr-webview-adapter (browser policy / engine / automation)
```

This is a Rust application. Use a Cargo crate, not an npm package or runtime downloaded executable. Package **source** must be pinned/reproducible through Cargo.lock. The app must never silently fetch executable code on launch, or self-update the adapter separately from its tested app binary.

## Ownership boundaries

| Component | Adapter package | Host app repo |
| --- | --- | --- |
| URL validation, backend selection, persisted metadata rule | Own | Call package + remain compatible |
| Browser protocol / shared command schemas / typed errors (next phase) | Own | Transport, orchestration |
| CDP process/session, WK operation adapters (later) | Own behind ports/features | Inject config, ownership, native handles |
| Herdr `Tab` / `Pane` / `Content::Mod` / workspace persistence | Never own | Own (upstream authoritative) |
| GPUI child view placement, focus, z-index, model mutation | Never own | Own |
| Native human approval presentation and final decision | Never grant | Own; no self-approval from CLI/MCP/page |
| App identity, shared config home, upstream sync, Release and updater | Never own | Own |

## P1 — compile-safe seam (this change)

- Introduce `crates/herdr-webview-adapter` as a Cargo workspace member, standalone-ready, with no desktop or macOS dependencies.
- Move **unchanged** policy algorithms from `src/browser.rs` to `url_policy`; host keeps `browser::create` and reexports the old public module functions.
- Move **unchanged** `BrowserEngine`, new-tab resolver, persisted-engine parsing and property update to `engine_policy`; host keeps real Chrome availability detection, `capabilities()` and a compatibility wrapper for `Availability` to avoid touching `src/window.rs`.
- Keep `__herdr_native_browser`, `API_VERSION = 1.0`, error messages and engine metadata stable. Both old-persisted-pane WK behavior and Chrome-first new panes must remain the same.
- Lock new crate graph in Cargo.lock; new adapter CI runs unit tests on Ubuntu, while existing browser-macos / packaged E2E gate continues to cover host behavior on macOS.

## P2 — actual external repository (pending repository creation)

1. Create `https://github.com/TonyPythoneer/herdr-webview-adapter` with GitHub repository write/admin capability. The current connected GitHub tool has **no create-repository action**; P1 intentionally does not fake a remote.
2. From a verified integration checkout run:

   ```sh
   git subtree split --prefix=crates/herdr-webview-adapter -b export/webview-adapter-v0.1.0
   # After independently creating the remote repository:
   git push https://github.com/TonyPythoneer/herdr-webview-adapter.git export/webview-adapter-v0.1.0:main
   ```

3. On the external repository, add independent Linux/macOS Rust CI, LICENSE (Apache-2.0), compatibility docs and an audited `v0.1.0` tag. Verify the adapter has no hidden host path imports.
4. In a separate app PR, replace the staging path dependency with an **immutable commit SHA**:

   ```toml
   herdr-webview-adapter = { git = "https://github.com/TonyPythoneer/herdr-webview-adapter", rev = "<verified-full-commit-sha>" }
   ```

   Regenerate Cargo.lock with the pinned Git SHA, test cold/offline workflows and preserve the previous working tag for rollback. A branch or main HEAD dependency is disallowed.
5. Remove staged crate only after cross-repository integration CI and a packaged App test passes at the **same exact adapter/app SHA pair**. Keeping a temporary local path dependency is safer than breaking CI before the new repository exists.

## P3/P4 — extracting native/browser code safely

Introduce a host-neutral BrowserHost boundary for pane-scoped create/navigate/bounds/visibility/focus/observe/input/screenshot/cancel/close with document epoch, pane owner ID, bounded resources and no cross-pane callbacks. Split modules in this order:

1. Shared command validation + typed result/error/approval classification from `browser_api.rs` and `wk_agent.rs`; preserve native approval as an explicit host callback and fail closed if unimplemented.
2. Chrome CDP transport, session and automation engine; inject profile path and lifecycle dependencies instead of calling `crate::mods::app_dir()`.
3. WK native bridge behind macOS feature gates; isolate GPUI and AppKit types in the app-facing boundary. Never move `AppWindow` or duplicate the upstream Tab/Pane state model.
4. Simplify `src/window.rs` last; treat this as a **separate measured refactor** with real lifecycle tests, not a textual file relocation.

## Compatibility and release checklist

- [ ] Adapter tests pass on Linux/macOS with `--locked`; crate can compile standalone after export.
- [ ] Full app Rust tests and packaged dual-backend E2E remain green on exact commit.
- [ ] Existing tab metadata, engine precedence, missing-engine restore, URL scheme rejection, CLI and MCP API are byte/behavior compatible where specified.
- [ ] Chrome profile privacy, cancellation, screenshot containment, native WebKit trusted input, event/epoch isolation and approval denial remain enforced.
- [ ] Upstream sync conflict rehearsal no longer conflicts in pure adapter modules; native host diff surface is explicitly measured.
- [ ] No merge, tag or stable release until mandatory Issue #1 production gates independently pass. On adapter regression roll back its pinned SHA and app lockfile without migrating user data.

Current status: **only P1 is staged by this PR**; it is not a completed independent GitHub repository. Native engines remain in the app until tested migrations in future PRs.
