# Herdr WebView — Browser Automation API Contract v1.0 (design specification)

> **Status: SPECIFICATION / NOT IMPLEMENTED.** Required for the first AI-operable WebView release. Source of truth for Agent-facing browser commands, compatibility, behavior, failure semantics and release tests. Tracks [Issue #1](https://github.com/TonyPythoneer/herdr-webview/issues/1). This is a **Herdr-defined contract informed by industry precedents**, not an implementation of Playwright, CDP or the W3C WebDriver BiDi protocol.

## 1. Basis: what is established versus custom

| Reference | Established design used by Herdr | What Herdr must NOT claim |
| --- | --- | --- |
| [Playwright MCP capabilities](https://playwright.dev/mcp/capabilities), [interaction](https://playwright.dev/mcp/tools/interaction), [snapshots](https://playwright.dev/mcp/snapshots) | Snapshot → element ref → click/type → new snapshot; wait, evaluate, screenshot; optional vision tools | That `browser.*` is Playwright MCP or that using JS click is equivalent to a genuine user click |
| [Chrome DevTools MCP tools](https://github.com/ChromeDevTools/chrome-devtools-mcp/blob/main/docs/tool-reference.md) | List/select page, snapshot UID, fill, click, evaluate, screenshot, structured errors | That an Apple WKWebView implements Chrome DevTools Protocol |
| [Playwright locators](https://playwright.dev/docs/locators) | Prefer role/name and label; allow text/test ID/CSS; wait for visible/enabled/stable and require unique target | That raw CSS selectors are the most reliable default |
| [WebDriver BiDi](https://www.w3.org/TR/2026/WD-webdriver-bidi-20260903/) | Separate context/document identity, input actions, script evaluation, navigation and screenshot lifecycle | That the fork fully implements W3C WebDriver BiDi |
| [MCP tools specification](https://modelcontextprotocol.io/specification/2026-07-28/server/tools) | Discoverable tools with explicit `inputSchema` / `outputSchema`, structured results and tool errors | That a bare Herdr CLI command automatically becomes an MCP tool |
| [Apple WKWebView snapshot API](https://developer.apple.com/documentation/webkit/wkwebview) | Capture the actual native WKWebView pixels | That GPUI screenshot tests alone prove WKWebView pixels are present |

The existing fork uses `gpui_wry` with `lb-wry = 0.53.3`, whose `evaluate_script_with_callback` provides a JSON-serialized result. Verify all macOS behavior against the **pinned versions**, not documentation for a newer package: [longbridge/wry source](https://github.com/longbridge/wry/blob/dev/src/lib.rs).

## 2. Hard product requirements

1. Browser lives **inside a real Herdr Tab/Pane** via either native WKWebView or a GPUI-rendered screen stream from a locally installed headless Chrome CDP Target. No extra visible browser window; Chrome is a separate background process when selected.
2. Agents can complete `open → snapshot → locate → click/fill/type → wait → eval → screenshot` entirely through the existing `herdr-app do` control path.
3. Use the **Native Pane Adapter / BrowserHost** from Workstream E; do not implement a second hidden browser or separate tab model.
4. The external command/JSON contract is **versioned and testable**. `browser.capabilities` returns engine availability and selected pane engine, e.g. `{"api_version":"1.0","engine":"wkwebview", "available_engines":["wkwebview","chrome-cdp"],...}` (illustrative, Chrome may be unavailable). Breaking changes require a new major version or explicit capability-negotiation path.
5. Use **explicit pane IDs** for every Agent command that reads or mutates a page (except `browser.list` / `browser.open` / `browser.capabilities`). Avoid selecting an arbitrary active pane when multiple agents run.
6. Keep the app named **Herdr WebView**, original icon and original `~/.config/herdr-app` configuration. Normal original-vs-fork single-instance restrictions apply.
7. Agent automation must be a **mandatory release gate** and must survive upstream Herdr merges.

## 3. CLI / command transport

Existing surface is `herdr-app do <command> '<JSON arguments>'`, registered in `src/commands.rs` and carried by the local Unix control socket in `src/control.rs`. The CLI output is one JSON object on stdout; diagnostic logs go to stderr. Nonzero CLI exit on an operation error. No globally exposed HTTP server.

Examples **after implementation**:

```bash
# Planned API (not implemented): herdr-app do browser.open '{"url":"http://localhost:3210"}'
# Planned API (not implemented): herdr-app do browser.snapshot '{"pane":"pane_1","mode":"a11y"}'
# Planned API (not implemented): herdr-app do browser.click '{"pane":"pane_1","target":{"role":"button","name":"Search"}}'
# Planned API (not implemented): herdr-app do browser.fill '{"pane":"pane_1","target":{"label":"Search"},"value":"Herdr"}'
# Planned API (not implemented): herdr-app do browser.eval '{"pane":"pane_1","expression":"document.title"}'
# Planned API (not implemented): herdr-app do browser.screenshot '{"pane":"pane_1","scope":"viewport"}'
```

The IDs and responses above are illustrative, not evidence that the commands exist yet. Preserve the established `browser.open/navigate/back/forward/reload` names and behavior; add new commands without silently changing old call sites. The adapter exposes a **local MCP stdio tool server** as a separate optional executable backed by the *same* Herdr control socket so MCP-capable Agents can discover tools via `tools/list`. Implement that adapter before declaring cross-Agent MCP support; it must not spawn a separate unrelated browser session; it connects to the already selected Herdr pane, whose Chrome backend may itself manage a headless Chrome process. Document how to register it in supported Agent CLIs. Both transports use the same validated backend operation handlers and response schemas.

### Envelope: request / completed / accepted / error

```json
{
  "api_version": "1.0",
  "request_id": "req_01",
  "ok": true,
  "status": "completed",
  "pane": "pane_1",
  "document_id": "doc_7",
  "url": "https://example.com/",
  "result": { "title": "Example Domain" }
}
```

- Every reply must carry `api_version`, `request_id`, `ok` and `status`. For page actions, include `pane` and current `document_id` when available. Results are JSON-serializable and size bounded.
- `status` is `completed`, `accepted` or `failed`. `accepted` means **not yet completed** and must include `request_id`; it is not represented as success of the browser action. `browser.result` looks up or waits briefly for the final result; `browser.cancel` cancels an operation. MCP adapter may await pending operations asynchronously without hanging Herdr's UI.
- Error example:

```json
{
  "api_version": "1.0",
  "request_id": "req_02",
  "ok": false,
  "status": "failed",
  "pane": "pane_1",
  "error": {
    "code": "ELEMENT_NOT_FOUND",
    "message": "No visible button named Search",
    "retryable": true,
    "details": { "target": { "role": "button", "name": "Search" } }
  }
}
```

- For transport failures, use nonzero CLI exit and a structured response when possible. An MCP tool maps operation failures to `isError: true` and returns the structured payload in `structuredContent` plus a portable text representation. Respect the negotiated MCP specification version.
- Request IDs must be generated by the host if absent, scoped to one instance, collision-safe, and must never be reused for a different operation. Pending operations must have bounded retention and cancellation.

## 4. Browser identity, snapshots and targets

- `pane`: stable Herdr Pane ID while the pane exists. `browser.list` reveals IDs and URLs/titles; do **not** equate app tab index with identity.
- `document_id`: changes on full navigation/reload; helps prevent an old command acting on a new document.
- `snapshot_id`: identifies a fresh observation of the specified pane/document. `ref` values are valid **only for that snapshot** and may become stale on meaningful DOM mutation/navigation; return `STALE_ELEMENT` and require a fresh snapshot, not silent fallback to another button.
- Locator alternatives, **exactly one** of: `{"ref":"e12","snapshot_id":"snap_5"}`, `{"role":"button","name":"Search","exact":true}`, `{"label":"Search"}`, `{"text":"Continue","exact":true}`, `{"test_id":"search-btn"}`, `{"css":"button[type=submit]"}`. For explicit target coordinate use `{"x":150,"y":220}` only with `browser.click_at` and a current viewport snapshot/screenshot. All coordinates are **WebView viewport CSS pixels**, not screen pixels, and must be converted correctly for display scaling.
- Prioritize semantic role/name or label; then snapshot ref, test ID, text and CSS as appropriate. Any locator matching multiple elements for a destructive/mutating action returns `AMBIGUOUS_TARGET` unless disambiguated; no implicit first match.
- `browser.snapshot` returns a compact **accessibility-oriented** tree with roles, names, labels, text, states, bounds for interactable elements and refs, plus `snapshot_id`/`document_id`. Implement semantic DOM/ARIA name extraction accurately enough for covered fixtures. **Do not claim full native accessibility-tree parity** if implemented using DOM extraction; describe backend capability explicitly.
- `browser.inspect` returns focused DOM details for a target (tag, selected attributes, text, value where permitted, state, bounds). `browser.query` resolves a locator to bounded matching elements/refs; prevent full DOM/HTML exfiltration by default.
- A page navigation or detach invalidates all refs for that old document. SPA transitions and significant DOM updates must invalidate or revalidate refs. Stale refs must never be redirected to a different element.

## 5. v1.0 command table (all required unless identified as compatibility)

| Command | Request parameters | Result / behavior |
| --- | --- | --- |
| `browser.capabilities` | none | version/backend, supported locators, image and input capabilities, limits |
| `browser.list` | none | panes[]: pane, url, title, loading, visible |
| `browser.open` | url, name?, space?, focus?, engine?=`auto` | New Tab without explicit engine chooses usable installed Chrome CDP, otherwise WKWebView; returns actual engine. Explicit choice never silently fails over. Existing saved panes missing engine remain WKWebView. |
| `browser.navigate` | pane, url, wait_until?, timeout_ms? | navigation completion / structured error; existing command extended |
| `browser.back/forward/reload` | pane, wait_until?, timeout_ms? | existing commands, deterministic target |
| `browser.snapshot` | pane, mode?=a11y, max_nodes? | snapshot_id, document_id, elements/tree, truncated flag |
| `browser.query` | pane, target, timeout_ms? | matching elements, counts; bounded output |
| `browser.inspect` | pane, target, include_attributes? | sanitized DOM element details |
| `browser.click` | pane, target, timeout_ms?, wait_until? | focus/scroll into view/actionability checks, user-like click, result |
| `browser.click_at` | pane, x, y, snapshot_id? | actual native pointer input fallback for canvas/visual-only targets |
| `browser.fill` | pane, target, value, timeout_ms? | focus + replacement + input/change semantics; no echo of secrets |
| `browser.type` | pane, target, text, timeout_ms? | keyboard typing, distinct from field replace |
| `browser.press` | pane, key, target? | Enter/Tab/keyboard combinations |
| `browser.scroll` | pane, x?, y?, delta_y, delta_x? | WebView scrolling |
| `browser.wait` | pane, condition: {url?, selector?, text?, state?}, timeout_ms? | waits for navigation/selector/visibility; explicit timeout |
| `browser.eval` | pane, expression, timeout_ms?, await_promise? | executes JS in page origin; returns JSON value or exception |
| `browser.screenshot` | pane, scope?=viewport | native WKWebView PNG artifact path and width/height |
| `browser.result` | request_id, wait_ms? | final result or still-pending status |
| `browser.cancel` | request_id | cancellation acknowledgement; completion cannot be delivered to another pane |

Optional improvements AFTER the above is actually working: `browser.fill_form` (multi-field form in one call), hover, drag/drop, select_option, dialog handling, network/console logs, full-page/element screenshots. These may be promoted into v1.0 if safely feasible, but cannot substitute for mandatory commands.

### Schema fragments (normative examples, validate against JSON Schema in code)

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://github.com/TonyPythoneer/herdr-webview/blob/feature/native-browser-tabs/docs/BROWSER_AUTOMATION_API_CONTRACT.md#target",
  "title": "HerdrBrowserTarget",
  "oneOf": [
    { "type": "object", "properties": { "ref": { "type": "string" }, "snapshot_id": { "type": "string" } }, "required": ["ref", "snapshot_id"], "additionalProperties": false },
    { "type": "object", "properties": { "role": { "type": "string" }, "name": { "type": "string" }, "exact": { "type": "boolean" } }, "required": ["role", "name"], "additionalProperties": false },
    { "type": "object", "properties": { "label": { "type": "string" } }, "required": ["label"], "additionalProperties": false },
    { "type": "object", "properties": { "text": { "type": "string" }, "exact": { "type": "boolean" } }, "required": ["text"], "additionalProperties": false },
    { "type": "object", "properties": { "test_id": { "type": "string" } }, "required": ["test_id"], "additionalProperties": false },
    { "type": "object", "properties": { "css": { "type": "string" } }, "required": ["css"], "additionalProperties": false }
  ]
}
```

```json
{
  "type": "object",
  "properties": {
    "pane": { "type": "string", "minLength": 1 },
    "target": { "$ref": "#/$defs/HerdrBrowserTarget" },
    "timeout_ms": { "type": "integer", "minimum": 100, "maximum": 30000 }
  },
  "required": ["pane", "target"],
  "additionalProperties": false
}
```

The second fragment is the `browser.click` schema **fragment**; implementations must embed the target definition in `$defs` and generate machine-validated `inputSchema` and `outputSchema` for **every** command from shared typed definitions. The docs' version number is not the MCP protocol version.

## 6. Required behavior and technical boundaries

### Native input versus DOM scripting

- For `browser.click` and `browser.type`, prefer verified user-equivalent native WKWebView/NSView input and actionability checks (visible, enabled, attached, stable, in viewport). A JavaScript call such as `element.click()` alone is NOT equivalent to trusted mouse/keyboard input and may fail on sites requiring trusted events.
- Implement a tested native pointer/keyboard dispatch path or a documented reliable WebKit automation route **without** forcing an external browser or blindly switching GPUI engine. Include a deterministic fixture that inspects event order and `event.isTrusted` where applicable; never falsely report that the click is trusted.
- A canvas or non-DOM control requires `browser.screenshot` + `browser.click_at`; coordinate clicks must be bound to the right pane and updated viewport geometry.
- Cross-origin iframe, closed shadow DOM, CAPTCHA, OS permission prompts, and site-specific anti-bot restrictions are **not promised as universally automatable**. Fail visibly with capability/permission errors; do not silently claim success.

### Nonblocking async bridge and lifecycle

- `lb-wry` has callback-capable script evaluation; integrate callback → request result using a queue/state machine, no UI-thread synchronous wait. Existing `src/control.rs` has a ~5s normal server reply timeout and ~35s CLI read timeout: **do not pretend that a 30s page wait can run synchronously in the current socket without plumbing changes**.
- On accepted async operations, return request_id promptly; a local MCP stdio adapter may repeatedly resolve the result until complete or timeout. Bound pending operations per pane, serialize conflicting actions and cancel on tab close, navigation, app quit or document identity change.
- Avoid per-page IPC exposing privileged native operations. If using Wry `with_ipc_handler`, any page-originated message is untrusted: verify origin, capabilities, authentication/binding and enforce no native filesystem/Agent privilege. Prefer host-initiated eval callbacks without injecting privileged bridges.
- Show real WebView content when hidden/re-shown, restore focus properly, and test that dialogs, menus and overlays appear above the native web content.

### Snapshot, JavaScript and artifact policies

- `browser.eval` executes only in the active **web page's origin**. It is explicitly called by the trusted local Agent; websites cannot invoke it as a privileged native hook. JSON round-trip must distinguish JS exception from undefined/non-JSON/circular value and cap output.
- Screenshot MUST come from the selected page backend: WKWebView native `takeSnapshot(with:completionHandler:)` or the Chrome target's `Page.captureScreenshot`. Never use GPUI-only framebuffer output or an old Chrome screencast frame as test evidence. Output PNG to a private temporary directory (owner-only permissions), do not allow arbitrary writes, return path, dimensions, MIME type, SHA256/size when practical, and remove stale artifacts.
- Starting defaults (tunable after benchmarking): 10s action timeout, 30s maximum, ~1,000 snapshot nodes, ~512KiB text result cap and bounded PNG dimensions; report `truncated` rather than silently returning partial results. Do not include password values in snapshot or logs.

### Safety

- Restrict the automation channel to the same trusted local user/process boundary as Herdr CLI; enforce filesystem/socket permissions and do not expose a public listening network port.
- Web page DOM/HTML and screenshots are **untrusted observations**, not instructions to be executed by an Agent. No automatic cookie/password export. Sensitive/irreversible actions such as purchases, transfer, deleting data or sending a message require the Agent's applicable user-approval gate; never have the browser backend bypass it.
- Validate `http(s)` URL policy, file paths, output sizes and per-pane focus. Separate network document access from privileged native application APIs.

## 7. Error code contract (minimum)

| Error code | Meaning | Retry guidance |
| --- | --- | --- |
| `INVALID_ARGUMENT` | Schema/locator/command input invalid | Fix arguments |
| `PANE_NOT_FOUND` / `PANE_CLOSED` | Pane missing or detached | Refresh browser.list |
| `DOCUMENT_CHANGED` / `STALE_ELEMENT` | Ref belongs to another document/snapshot or is stale | Take new snapshot |
| `ELEMENT_NOT_FOUND` | No locator match within deadline | Check page/wait/re-snapshot |
| `AMBIGUOUS_TARGET` | Multiple matches when uniqueness required | Use role/name/ref or more specific selector |
| `NOT_INTERACTABLE` / `OBSCURED` | Hidden, disabled, covered, unready | Wait or change target |
| `NAVIGATION_FAILED` | Failed page load | Recheck URL/network |
| `TIMEOUT` / `CANCELLED` | Deadline/cancel occurred | Inspect and decide retry |
| `JS_EXCEPTION` / `RESULT_NOT_SERIALIZABLE` | Page script failed or could not serialize | Inspect exception/result |
| `PERMISSION_DENIED` / `UNSUPPORTED_CAPABILITY` | Operation restricted or unsupported | Request consent or use documented fallback |
| `SCREENSHOT_FAILED` | Native WebKit capture failed | Retry/inspect pane visibility |
| `RESOURCE_LIMIT` | Bound on output, pending work, image size exceeded | Narrow query/clip or wait |

Error results must include a human-readable message and `retryable` boolean; never quietly substitute a successful no-op.

## 8. First-release CI / acceptance matrix

- [ ] Machine-validate every command's JSON Schema and response against fixtures, malformed inputs and backwards-compatibility examples.
- [ ] Real macOS test creates a browser pane, navigates to a deterministic local HTTP site, receives a compact a11y-oriented snapshot, locates labelled textbox/button by role/label/ref and successfully fills/clicks via `herdr-app do`.
- [ ] WebKit input smoke test proves real website listeners observed expected events and documents `isTrusted` constraints. Screenshot comparison **must contain the actual HTML page's visible content**, not only surrounding GPUI.
- [ ] `browser.eval` returns actual values and structured JS exceptions; `browser.wait` honors deadlines and cancellation; `browser.result` cannot hang/return stale data.
- [ ] Multiple panes: wrong pane and stale document targets produce errors; two Agents cannot accidentally send actions to each other's pane; switching, split, background, resize, close, app quit and reload cancel pending operations correctly.
- [ ] MCP `tools/list` advertises the same supported command contracts with real `inputSchema` and `outputSchema`; MCP tool invocation routes into the same Herdr window, **not a new WebView or external browser**.
- [ ] Security: a malicious page cannot invoke local Agent commands, read arbitrary filesystem files or save screenshots outside private artifact storage; approval is respected for sensitive actions.
- [ ] No upstream auto-integrated release is published without the browser automation suite. When a future upstream tag breaks the contract, create/update an actionable Issue and preserve the last working release.
- [ ] Preserve original Herdr icon, `Herdr WebView` Dock identity, shared original settings and fork-only updates.

## 9. Implementation sequencing

1. **API contract and backend**: typed request/response/locator definitions, schema generation, stable pane/document identity, error codes.
2. **Two backend implementations in parallel**: existing WKWebView bridge and installed-Chrome-CDP adapter (including GPUI screencast renderer), each providing async eval/snapshot/query/inspect/input/native screenshots/wait/cancel.
3. **Herdr CLI**: command registration and working local socket calls; no app UI deadlocks; same tests across multiple panes.
4. **MCP stdio bridge**: expose the same Herdr pane backend through MCP; must not start a second unrelated browser, but the selected Chrome engine may own a managed background Chrome process. Add Agent CLI configuration examples.
5. **CI promotion**: both backend suites (real macOS WKWebView and headless local Chrome/CDP shown inside GPUI), shared Agent API and security gates must pass before stable release.

**No assumptions of pre-existing support:** v1.0 is a normative *design target*. Record actual implementation status per command in the PR and only label commands `supported` once exercised through the shipped Agent interface on macOS.

## 10. Chrome-like UI testing: feasibility boundaries, risk gates and safer defaults (2026-11-21)

### 10.1 Protocol compatibility: precise commitment

- **What is promised for the WKWebView engine:** Playwright/Chrome-DevTools-like **Agent ergonomics** through a first-party Herdr `browser.*` API controlling the already displayed WKWebView; the separate `chrome-cdp` engine implements the same external API while using real CDP internally.
- **What is not promised for WKWebView:** raw Chrome DevTools Protocol (CDP) WebSocket connectivity or drop-in support for `chromium.connectOverCDP()` / `playwright.connectOverCDP()`. WKWebView uses WebKit/JavaScriptCore, not Chromium CDP. The optional Chrome engine uses native CDP internally, but Herdr's CLI/MCP stays custom. WebKit Inspector/remote inspector does not mean stock WKWebView supports the full Playwright protocol or Chrome-only domains. Playwright's shipped WebKit is patched; even Playwright's internal stock-WebKit experiments are not a verified general macOS WKWebView driver.
- **Technical feasibility gate (P0, before large implementation):** a minimal proof of concept must operate the *existing visible* GPUI child WKWebView through `herdr-app do`: JS evaluate-with-result, stable DOM querying, form input/click using a verified input path, a native WKWebView PNG capture, and post-action assertion. Measure Mac permissions and native-event `isTrusted` instead of assuming success. If native input cannot be made reliable with supported macOS APIs and the existing architecture, block the release and document the compatibility limitation; **do not claim Playwright-grade input automation based solely on `element.click()`.**

### 10.2 Two distinct UI surfaces and test scope

| Surface under test | Can Herdr WebView browser API test it? | Required test harness |
| --- | --- | --- |
| HTML/CSS/JS inside the embedded WKWebView | Yes, after bridge implementation | Herdr `browser.*`, DOM assertions, screenshot and native input checks |
| Herdr GPUI tabs, pane split/resize, native toolbar/menu, permission dialogs and Dock behavior | **Not solely through page DOM/JS** | macOS AppKit/accessibility/UI event harness plus real displayed WebView for integration |
| OS dialogs, Safari/Chrome or arbitrary external apps | Not supported by the WebView browser API | Separate native/OS automation only if deliberately scoped; never imply the browser API tests it |
| Chrome-specific browser behavior | Not directly | Separate Chromium testing only if explicitly desired; not part of “same embedded WebView” guarantee |

Mandatory tests target both: (A) **web-app behavior** in the real embedded WKWebView; (B) **native container integration** (focus, z-order, resize, split, close). Both tests run against the shipped Herdr WebView app. The WK engine uses native WKWebView, while Chrome mode uses a managed separate headless Chrome process whose screen is streamed into the GPUI Tab, not a separate visible browser window.

### 10.3 Risk register and mitigations (normative)

| Severity | Failure mode | Required mitigation and passing proof |
| --- | --- | --- |
| P0 | JS-dispatched `click()` passes a test but lacks **trusted user event** semantics | Use real native event delivery or a verified equivalent where possible; fixture records `isTrusted` and event sequence; document engine capability and report unsupported instead of false success |
| P0 | Native macOS automation permissions / TCC block click, keyboard or screenshot on CI | Test actual macOS 15+ environment and required grants early; retain deterministic DOM-only coverage on standard macOS Actions and add **a trusted, configured macOS UI runner** for permission-requiring real-input gates if needed; do not bypass OS security or hide a skipped gate |
| P0 | Agent changes production data or leaks secrets while “testing” | Default browser-test sessions to disposable nonpersistent website data, loopback/local fixture origin, staging/test accounts, explicit allowed origins and test-only credentials. Require explicit approval for irreversible real-site actions; never assume automatic detection of every destructive button is sufficient |
| P0 | Page, frame or pane changes while an action is pending | Include `pane`, `document_id` and `snapshot_id` on all target mutations; cancel on navigation/close; require locators to uniquely identify one actionable element; never silently retarget or retry non-idempotent mutations |
| P0 | Untrusted web content injects instructions or calls privileged Herdr API | Treat DOM/screenshot as **untrusted observations**, never command authorization; no privileged page→host IPC; restrict Unix socket permissions and verify same-user peer where feasible; keep MCP adapter attached to existing local session only |
| P1 | WebView screenshot misses native pixels, is blank, clipped or captures another pane | Use native `WKWebView.takeSnapshot`, tag screenshot with pane/document/URL/viewport/scale/timestamp; check actual fixture text/marker pixels and exact dimensions; no implicit switch to GPUI window-only screenshot |
| P1 | Flaky timing (SPA hydration, AJAX, animation, page load), leading to phantom success | Locator auto-wait for actionable state, bounded `browser.wait` and postcondition/assertions; serialize per-pane mutations, no arbitrary sleeps as primary synchronization. For visual diffs, normalize scaling/fonts/animations and document tolerance |
| P1 | Cross-origin frames, closed shadow roots, popups, CAPTCHA or site defenses | Capability-detect and explicitly return `UNSUPPORTED_CAPABILITY`/`PERMISSION_DENIED`; do not promise universal iframe or protected-website automation; test same-origin basics separately |
| P1 | Embedded browser test overwrites existing Herdr user data | Test app uses `HERDR_APP_HOME` pointing at an isolated temporary fixture; web profile is ephemeral; never run tests against the actual `~/.config/herdr-app` or production login cookies. **End-user application still reuses upstream config as requested** |
| P1 | CI builds green but UI is broken after upstream release | Real native App UI + browser-automation fixture must be a hard release gate. Failed or unavailable required runner blocks release, creates/updates a diagnostic GitHub Issue and preserves last known good build |
| P2 | Excessive screenshot/DOM output, stale artifacts or oversharing secrets | Bound text/images and concurrent tasks, redact password fields, mask sensitive test data, store captures in owner-only temporary paths, do not publish CI artifacts containing private production sessions |

### 10.4 Observe → act → assert; not click-and-hope

A passing browser UI test **must** demonstrate:
1. Identify the exact visible `pane` and `document_id`; obtain a fresh accessibility-oriented snapshot or DOM lookup.
2. Resolve one unique, interactable target using role/name/label/test-id (CSS only if necessary); reject stale/ambiguous references.
3. Perform action using the documented input backend; record source `native` / `dom_synthetic` in result metadata and **never call an input event “trusted” without verification**.
4. Await a **specific observable outcome** (URL, DOM state, visible content, updated input value, enabled state), not merely successful JS completion.
5. Capture a native WebView screenshot when visual evidence is relevant; ensure evidence and assertion refer to the same pane/document.
6. On timeout or uncertain completion, **observe again before retrying**. Do not replay purchase/submit/delete/payment or any other non-idempotent action automatically.

API extension: `browser.assert` is recommended to explicitly support UI test postconditions (e.g. element visibility, text, URL or value). If not added as a command in v1.0, the test runner MUST implement equivalent structured assertions using `browser.query`/`browser.eval`, with machine-readable pass/fail and evidence. An automation returning `ok: true` is **not** itself a UI test pass.

### 10.5 CI test pyramid and evidence

1. **Schema/unit tests:** typed operation validation, locator ambiguity, request IDs, timeout, pending-action cancellation, permission denial, never-implicit cross-pane behavior.
2. **Web integration on a visible embedded WKWebView:** local deterministic HTTP fixture, DOM, fill, click, JS return, navigation, native PNG capture, explicit DOM+visual assertions. Can run on macOS-hosted GitHub Actions only after the environment is proven capable of exercising WebKit.
3. **Native macOS input and container UI tests:** real pointer/keyboard events, focus, scroll, split/resizing, overlay/dialog z-order, app single-instance. Mac UI accessibility/test permission availability must be **verified**, not assumed; move permission-dependent gates to a trusted configured macOS runner if needed.
4. **Upstream-sync release gate:** no screenshot-only or mock-only substitute for the real browser tests. Summaries include test case status, engine/backend/version, pane/document, input backend, failures and redacted evidence. Missing UI runner or skipped mandatory native test ⇒ **no stable release**, open a diagnostic Issue.

### 10.6 Decisions assumed unless the user requests otherwise

- **First release:** test and automate web UI *inside* Herdr; also verify the native Herdr container's integration using macOS native UI testing (not webpage JS).
- **Priority:** correctness and reproducibility over pretending WKWebView has CDP. Chrome backend uses real Chrome CDP internally; do not implement a fake CDP server.
- **Test state:** isolated fixture/staging profile by default; production sessions not used in unattended UI tests.
- **Manual/native permissions:** no Apple Developer certificate required for packaging; this does **not** imply Accessibility/TCC input permissions are automatically granted.
- **Approval:** ordinary local site exploration and staging UI checks can run unattended; high-impact transactions on live accounts require explicit approval by the Agent's applicable permission layer.

References: [Apple WKWebView](https://developer.apple.com/documentation/webkit/wkwebview), [WKSnapshotConfiguration](https://developer.apple.com/documentation/webkit/wksnapshotconfiguration), [Playwright MCP snapshots](https://playwright.dev/mcp/snapshots), [Playwright browsers](https://playwright.dev/docs/browsers), [WebKit contributor notes on WebDriver BiDi and UI testing](https://docs.webkit.org/Other/Contributor%20Meetings/ContributorMeeting2025.html), [Apple inspectable WebViews](https://webkit.org/blog/13936/enabling-the-inspection-of-web-content-in-apps/).



## 11. Dual-engine extension — two backends retained (2026-11-21)

**Normative architecture:** [DUAL_BROWSER_BACKEND_PLAN.md](DUAL_BROWSER_BACKEND_PLAN.md). **Both strategies must be developed, shipped, and tested in parallel. Do not remove either one unless the user later explicitly decides.** Earlier WKWebView-only language in sections 1–10 applies to that backend specifically; where requirements conflict, this dual-engine extension controls.

### Shared browser API

- Engine enum: `wkwebview` | `chrome-cdp`, plus creation-only `auto` when opening a **new** Tab. Unspecified `engine` chooses available installed `chrome-cdp` first, otherwise `wkwebview`. Older persisted panes **without** an `engine` remain WKWebView for backwards compatibility. Save selected engine in existing browser-pane properties, and display the new-Tab default in its UI selector.
- All browser commands use the Herdr pane identifier, not Chrome target IDs exposed to an Agent. `browser.list` includes engine per pane; `browser.capabilities` lists installed/available engines, versions and capabilities. Every action response includes `engine` and `pane`. Locator, snapshot, document ID, error handling and CLI/MCP schemas are identical in both backends where supported.
- No automatic backend substitution for explicitly selected engines or already open panes. During **new Tab default resolution** only, missing/unsupported Chrome means select WKWebView. Explicit `engine: "chrome-cdp"` without Chrome yields `CHROME_NOT_INSTALLED` and actionable guidance. Switching a running pane's engine requires explicit close/reopen; session/login state may not transfer.
- The MCP adapter and CLI use one BrowserHost and the same existing Herdr pane. They must not open an additional hidden test browser.

Example (planned, **not implemented**):

~~~bash
# Planned API (not implemented): herdr-app do browser.open '{"url":"https://example.com","engine":"chrome-cdp"}'
# Planned API (not implemented): herdr-app do browser.snapshot '{"pane":"p1"}'
~~~

~~~json
{
  "api_version":"1.0",
  "request_id":"r1",
  "ok":true,
  "status":"completed",
  "pane":"p1",
  "engine":"chrome-cdp",
  "document_id":"d1",
  "url":"https://example.com/",
  "result":{"title":"Example Domain"}
}
~~~

### Backend-specific facts

| Contract dimension | WKWebView | Local Chrome/CDP |
| --- | --- | --- |
| Display | Real macOS WKWebView child inside GPUI Tab | CDP `Page.startScreencast` → decoded frames → GPUI image/texture; Chrome remains headless |
| DOM/JS | WebKit JS callback bridge | CDP Runtime/DOM/Accessibility |
| Input | Verified macOS native input, permissions may be required | GPUI event mapped to Chrome viewport then CDP Input |
| Screenshots | WK native `takeSnapshot` | CDP `Page.captureScreenshot` from target, not screencast-frame cache |
| Viewport/resize | Native view bounds | CDP Emulation metrics + coordinate mapping |
| Distribution | WebKit built into macOS | Detect user's installed Chrome, **never bundle/download Chrome in the released app** |
| Session | WK profile/store | Distinct private Chrome automation profile, not default daily Chrome profile |

Chrome 136+ requires a separate non-default `--user-data-dir` for remote debugging. For normal browsing, that Chrome profile is **persistent and dedicated to Herdr WebView**, remembering cookies/logins across launches without touching the daily Chrome profile; CI and unattended fixtures instead use ephemeral profiles. Prefer `--remote-debugging-pipe` to avoid exposing a listening port. Chrome `Page.startScreencast` is **experimental** and might be unsuitable for interactive video or IME until verified on the actual macOS + Chrome version. It is a feasibility gate, not a presumed solved problem.

### Release acceptance (both required)

- [ ] Same browser automation fixture in WKWebView and Chrome engines: open → snapshot → locate → click/fill/type → assert DOM → navigate → evaluate → screenshot → close via Herdr CLI **and** MCP.
- [ ] Chrome integration P0: installed Chrome detected; headless CDP start, continuous screencast and ACK, images painted in a real GPUI Tab, mouse/wheel/keyboard and IME behavior verified. No extra visible Chrome window. No Chrome binaries in DMG.
- [ ] WK integration P0: actual embedded WKView rendering, trusted/native input proof, JS results, native PNG snapshot, overlay/focus tests. Existing WK route is not removed.
- [ ] Engine-identity and session-isolation tests: no accidental cross-pane/engine commands, no stale document action, controlled crash/close, no silent fallback.
- [ ] Real macOS GUI test of Herdr's container, not merely web DOM assertions, for split/resize, tab focus and controls with both renderers.
- [ ] CI runs mandatory macOS E2E for **both** engines; missing Chrome on CI runner must be resolved by CI provisioning (not shipping Chrome), not by skipping Chrome tests. Failing/skipped mandatory backend blocks release and opens or updates a diagnostic Issue.
- [ ] Chrome-less user Mac: packaged Herdr opens normally and WKWebView works; Chrome selection explains unavailability. App continues using original Herdr settings and fork-only updater.
- [ ] Engine removal requires a later **explicit user decision** with a migration path for saved pane metadata, separate PR and regression tests.

**References:** [Chrome CDP Page](https://chromedevtools.github.io/devtools-protocol/tot/Page/), [Chrome CDP Input](https://chromedevtools.github.io/devtools-protocol/tot/Input/), [Chrome remote debugging security rules](https://developer.chrome.com/blog/remote-debugging-port), [Apple WKWebView](https://developer.apple.com/documentation/webkit/wkwebview).

## 12. Confirmed defaults, authorization and release policy (user decision, 2026-11-21)

### 12.1 New-tab engine resolution versus restoring a saved pane

- **New Browser Tab:** without `engine` (also when GUI creates a Tab), detect supported locally installed Chrome. If usable, resolve to **`chrome-cdp`**; otherwise choose **`wkwebview`**. Expose `default_engine`, `available_engines` and clear availability reasons in `browser.capabilities`, and return resolved `engine` on `browser.open`. No automatic install or bundle download.
- **Saved Browser Tab:** absent historical `engine` metadata still means **WKWebView**. Do not convert existing panes to Chrome or discard saved sessions; newly created panes persist the resolved explicit engine.
- **Explicit engine selection:** `engine: "chrome-cdp"` and `engine: "wkwebview"` are authoritative. Unavailable Chrome reports `CHROME_NOT_INSTALLED` / `CHROME_UNSUPPORTED_VERSION`; never silently switch explicit or running panes.

### 12.2 Persistent Chrome profile and isolated testing

- Normal Chrome CDP browsing uses an **independent persistent, non-default Chrome user-data directory** managed by Herdr. Cookies and login state can survive Tab closure and app restart. Do not attach to daily Chrome's profile; protect with restrictive permissions, profile locking and a deliberate clear-data/logout command. Do not delete the persistent profile on Tab close.
- CI, unattended UI automation and fixture/staging tests use isolated **ephemeral profiles** and test accounts; do not read production cookies, share the user's interactive Chrome profile or alter the original Herdr settings home. Persistent login is the **normal interactive browsing** setting, not an instruction to reuse secrets in tests.

### 12.3 Agent actions on real websites — approval contract

- Ordinary navigation, search, read, click, input and UI checks on real production websites are **allowed without repeated per-site approval** through either backend. Do not restrict normal Agent browsing to staging-only sites.
- **Require explicit, action-scoped approval before high-impact or irreversible actions:** payments/purchases, bank transfers, destructive deletes/overwrites, credential/token/password export, sending messages, submission of consequential forms, changing security settings or similar actions. Approval records the origin, target, action and scope; consume/expire approvals to prevent accidental blanket reuse.
- **One permission policy for all transports/actions.** The Agent and host-level browser policy must not permit bypass via `browser.eval`, a synthetic click, CDP access, JavaScript fetch/network requests, CLI versus MCP, or alternate commands. A browser backend cannot reliably infer arbitrary action intent from DOM/JS, so uncertain-impact actions **must request permission**, not claim automatic perfect detection. Return `APPROVAL_REQUIRED` / `PERMISSION_DENIED` with safe, actionable details and without leaking credentials.
- Unattended test suites still default to local/staging fixtures with temporary profiles; user permission for ordinary production browsing does not permit destructive tests on live accounts.

### 12.4 Automatic release after passing CI; conflicts require review

- A qualifying upstream integration with **all mandatory** macOS WKWebView and Chrome CDP UI E2E, CLI/MCP, build/package/security and release checks passing, and **no unresolved integration conflicts**, is **automatically released** without routine manual approval. Keep manifest-last atomic publish and fork-only feed.
- **Merge/rebase/incompatible dependency conflicts require human review** before integration/release. Stop automatic promotion, keep the previous published build intact and create/update a deduplicated GitHub Issue with PR, failure context and logs.
- Required CI tests that fail, are unavailable or are skipped **also block publication** and create a diagnostic Issue. Do not mistake failing CI for permission to force a release. User will later decide if either browser engine should be removed.
