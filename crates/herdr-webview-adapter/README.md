# herdr-webview-adapter

Rust package extraction staging area for the planned standalone `TonyPythoneer/herdr-webview-adapter` repository. This is the **first portable slice**, not a complete browser implementation or external dependency yet.

## Owned today

- URL allowlist, normalization and rejection of credential-bearing URLs (`url_policy`).
- Engine selection (`chrome-cdp` or `wkwebview`) with explicit availability injected by the host (`engine_policy`).
- Persisted engine defaults and URL metadata mutation, preserving legacy WKWebView tabs.

## Deliberately host-owned today

- GPUI/Wry/WKWebView construction and native event injection.
- Chrome process ownership, app-specific profile directories, screenshots, shared sessions.
- Herdr Tabs/Panes, layout, focus and lifecycle, approval UI, CLI/MCP transport and updater.

This crate must remain free of `crate::window`, app config paths, GPUI and native UI types. Maintain `API_VERSION` compatibly; any behavior/contract change must be accompanied by adapter and shipped-app E2E tests.

## Test

```sh
cargo test -p herdr-webview-adapter --locked
```

## Independent repository extraction

After repository creation, use `git subtree split --prefix=crates/herdr-webview-adapter` from the integration branch, push to the new repository, and consume **a pinned immutable Git revision**. Do not replace the working path dependency with an unresolved Git URL. See `docs/WEBVIEW_ADAPTER_EXTRACTION.md` in the host repository and Issue #18.
