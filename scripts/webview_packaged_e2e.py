#!/usr/bin/env python3
"""Fail-closed browser E2E against the *packaged, running* Herdr WebView.app.

Unlike tests/wk_native_input.rs this starts the actual release executable, uses
its real control socket and MCP stdio adapter, and runs a loopback-only fixture.
No production website, private Chrome profile or person's Herdr home is touched.
"""
import argparse
import http.server
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import threading
import time


PAGE = """<!doctype html><html><head><title>Packaged Browser E2E</title>
<meta name="viewport" content="width=device-width,initial-scale=1"></head>
<body style="height:2500px;margin:0;padding:24px">
<h1>Packaged Browser E2E</h1>
<button id="safe" type="button">Next</button>
<button id="danger" type="button">Delete</button>
<input id="field" aria-label="Test Field" type="text">
<p id="status">untouched</p>
<script>
window.__deleted=false;window.__deleteCount=0;
document.querySelector('#safe').addEventListener('click',()=> {
  document.querySelector('#status').textContent='clicked';
});
document.querySelector('#danger').addEventListener('click',()=> {
  window.__deleted=true;window.__deleteCount++;
  document.querySelector('#status').textContent='DELETED-'+window.__deleteCount;
});
</script>
</body></html>"""


class Fixture(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.server.seen_requests.append(self.path)
        if self.path not in ("/", "/next"):
            self.send_error(404)
            return
        body = PAGE.encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, fmt, *args):
        return


def command(exe, env, *args, timeout=12):
    result = subprocess.run([str(exe), *args], text=True, capture_output=True,
                            env=env, timeout=timeout, check=False)
    if result.returncode and not result.stdout.strip():
        raise AssertionError(
            "CLI did not return JSON: rc={} stderr={}".format(
                result.returncode, result.stderr[-500:]))
    try:
        answer = json.loads(result.stdout)
    except ValueError as error:
        raise AssertionError(
            "CLI response is not JSON: stdout={!r} stderr={!r}".format(
                result.stdout[-500:], result.stderr[-500:])) from error
    return answer


def action(exe, env, name, args=None, *, allow_error=False, timeout=12):
    reply = command(exe, env, "do", name, json.dumps(args or {}), timeout=timeout)
    if not allow_error and ("error" in reply or reply.get("ok") is False):
        raise AssertionError("{} rejected: {}".format(name, reply))
    return reply


def await_job(exe, env, result, *, seconds=35):
    request_id = result.get("request_id")
    if not request_id or result.get("status") != "accepted":
        raise AssertionError("browser command did not return a request ID: {}".format(result))
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        reply = action(exe, env, "browser.result", {"request_id": request_id})
        if reply.get("approval_required"):
            return reply
        if reply.get("status") == "completed":
            if not reply.get("ok"):
                raise AssertionError("browser action completed unsuccessfully: {}".format(reply))
            return reply
        if reply.get("status") == "failed":
            raise AssertionError("browser action failed: {}".format(reply))
        time.sleep(0.18)
    raise AssertionError("browser command stalled: {}".format(result))


def wait_for_core_space(exe, env, home, *, seconds=40):
    """Socket readiness precedes official herdr-core layout readiness on macOS.

    Retry ONLY the known transient core-attachment error. Every other error
    remains an immediate hard failure; no fake core or skipped app command.
    """
    deadline = time.monotonic() + seconds
    last = None
    while time.monotonic() < deadline:
        result = action(exe, env, "space.new",
                        {"name": "E2E", "folder": str(home)}, allow_error=True)
        if result.get("space"):
            return result
        last = result.get("error", str(result))
        if "layout is unavailable" not in last and "when it reconnects" not in last:
            raise AssertionError("space.new failed nontransiently: {}".format(result))
        time.sleep(0.5)
    raise AssertionError("official herdr-core never provided a layout: {}".format(last))


def browser_action(exe, env, name, pane, data=None, *, seconds=35):
    args = {"pane": pane, **(data or {})}
    request = action(exe, env, name, args)
    return await_job(exe, env, request, seconds=seconds)


def until_ready(exe, env, pane, *, seconds=70, expected_url=None):
    deadline = time.monotonic() + seconds
    last = None
    while time.monotonic() < deadline:
        try:
            reply = browser_action(exe, env, "browser.snapshot", pane,
                                   {"max_nodes": 40}, seconds=18)
            page = reply["result"]
            if (page.get("title") == "Packaged Browser E2E"
                    and (expected_url is None
                         or page.get("url", "").startswith(expected_url))):
                return reply
            last = "unexpected page: title={!r}, url={!r}".format(
                reply["result"].get("title"), reply["result"].get("url"))
        except (AssertionError, subprocess.TimeoutExpired) as error:
            last = str(error)
        time.sleep(0.6)
    raise AssertionError("real browser pane never loaded: {}".format(last))


def check_mcp(exe, env, pane):
    # This is the same packaged binary, run as a separate stdio MCP process.
    requests = [
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
        {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}},
        {"jsonrpc": "2.0", "id": 3, "method": "tools/call",
         "params": {"name": "browser.list", "arguments": {}}},
        {"jsonrpc": "2.0", "id": 4, "method": "tools/call",
         "params": {"name": "browser.approve", "arguments": {}}},
    ]
    stdin = "\n".join(json.dumps(x) for x in requests) + "\n"
    reply = subprocess.run([str(exe), "browser-mcp"], env=env, input=stdin,
                           text=True, capture_output=True, timeout=30)
    if reply.returncode:
        raise AssertionError("packaged MCP failed: {}".format(reply.stderr[-1000:]))
    rows = [json.loads(line) for line in reply.stdout.splitlines() if line.strip()]
    if len(rows) != 4:
        raise AssertionError("MCP returned {} responses, expected four: {}".format(len(rows), rows))
    assert rows[0]["result"]["protocolVersion"], rows[0]
    names = {x["name"] for x in rows[1]["result"]["tools"]}
    assert "browser.click" in names and "browser.approve" not in names, names
    assert pane in json.dumps(rows[2]["result"]), rows[2]
    assert rows[3]["error"]["code"] == -32602, rows[3]
    print("PACKAGED_MCP_VERIFIED: tools/list and socket-backed browser.list, no approve tool", flush=True)


def native_gui_decide(exe, pid, choice):
    # Separate OS-level GUI automation: never an Agent/CLI/MCP approval path.
    time.sleep(0.5)
    reply = subprocess.run([str(exe), str(pid), choice], text=True,
                           capture_output=True, timeout=20)
    if reply.returncode or "GPUI_NATIVE_APPROVAL_INPUT:" not in reply.stdout:
        raise AssertionError("real OS approval input failed: {} {}".format(
            reply.stdout[-500:], reply.stderr[-500:]))
    print(reply.stdout.strip(), flush=True)


def wait_native_decision(exe, env, request_id, *, permitted):
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        result = action(exe, env, "browser.result",
                        {"request_id": request_id}, allow_error=True)
        if result.get("status") == "completed":
            assert permitted and result.get("ok") is True, result
            return result
        if result.get("status") == "failed":
            assert not permitted and "PERMISSION_DENIED" in str(result.get("error")), result
            return result
        time.sleep(0.2)
    raise AssertionError("native approval UI never resolved request " + request_id)


def test_native_gpui_approval(exe, env, pane, pid, driver):
    request = browser_action(exe, env, "browser.click", pane,
                             {"target": {"css": "#danger"}})
    assert request.get("approval_required"), request
    native_gui_decide(driver, pid, "approve")
    wait_native_decision(exe, env, request["request_id"], permitted=True)
    inspected = browser_action(exe, env, "browser.inspect", pane,
                               {"target": {"css": "#status"}})
    assert inspected["result"]["elements"][0]["text"] == "DELETED-1", inspected
    request = browser_action(exe, env, "browser.click", pane,
                             {"target": {"css": "#danger"}})
    assert request.get("approval_required"), request
    native_gui_decide(driver, pid, "deny")
    wait_native_decision(exe, env, request["request_id"], permitted=False)
    inspected = browser_action(exe, env, "browser.inspect", pane,
                               {"target": {"css": "#status"}})
    assert inspected["result"]["elements"][0]["text"] == "DELETED-1", inspected
    print("PACKAGED_GPUI_APPROVAL_E2E_VERIFIED: real native approve and deny with DOM postconditions", flush=True)


def run(app, core_bin, engine, approval_driver=None):
    assert app.name == "Herdr", "expected the packaged Contents/MacOS/Herdr binary"
    assert app.is_file(), "packaged application executable missing"
    version = subprocess.run([str(app), "--version"], text=True,
                             capture_output=True, timeout=10, check=True).stdout.strip()
    assert "-webview." in version, version
    with tempfile.TemporaryDirectory(prefix="hv-e2e-", dir="/tmp") as root:
        tmp = Path(root)
        home = tmp / "home"
        config = tmp / "herdr-home"
        home.mkdir()
        config.mkdir(mode=0o700)
        env = os.environ.copy()
        env.update({
            "HOME": str(home),
            "HERDR_APP_HOME": str(config),
            "XDG_CONFIG_HOME": str(home / ".config"),
            "HERDR_CORE_DIRS": str(core_bin.parent),
            "HERDR_APP_LOG": "warn",
            "PATH": str(core_bin.parent) + os.pathsep + env.get("PATH", ""),
        })
        assert subprocess.run([str(core_bin), "--version"], env=env,
                              capture_output=True, timeout=10).returncode == 0, "invalid official core"
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Fixture)
        server.seen_requests = []
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        url = "http://127.0.0.1:{}/".format(server.server_port)
        log = tmp / "app.log"
        print("Running {} from packaged {} with isolated home".format(engine, app.parent.parent.parent), flush=True)
        try:
            with log.open("w") as output:
                child = subprocess.Popen([str(app)], env=env, stdout=output,
                                         stderr=subprocess.STDOUT, start_new_session=True)
                try:
                    deadline = time.monotonic() + 100
                    last = ""
                    while time.monotonic() < deadline:
                        if child.poll() is not None:
                            raise AssertionError("packaged app exited rc={}".format(child.returncode))
                        try:
                            response = action(app, env, "browser.capabilities", timeout=8)
                            if response.get("engines"):
                                break
                            last = repr(response)
                        except (AssertionError, subprocess.TimeoutExpired) as error:
                            last = str(error)
                        time.sleep(0.7)
                    else:
                        raise AssertionError("packaged App did not start its CLI socket: {}".format(last))
                    space = wait_for_core_space(app, env, home)
                    assert space.get("space"), space
                    tab = action(app, env, "browser.open",
                                 {"url": url, "engine": engine, "name": "Native E2E"})
                    pane = tab.get("pane")
                    assert pane and tab.get("engine") == engine, tab
                    panes = action(app, env, "browser.list")["panes"]
                    assert any(p.get("pane") == pane and p.get("engine") == engine for p in panes), panes
                    snap = until_ready(app, env, pane)
                    assert snap["result"]["url"].startswith(url), snap
                    print("PACKAGED_BROWSER_OPEN_VERIFIED: " + engine, flush=True)
                    if engine == "chrome-cdp":
                        # Validate normal pane navigation as well as target
                        # creation; a green startup must not hide a broken
                        # Page.navigate for subsequent real user actions.
                        next_url = url + "next"
                        action(app, env, "browser.navigate",
                               {"pane": pane, "url": next_url})
                        navigated = until_ready(app, env, pane, seconds=30,
                                                expected_url=next_url)
                        assert navigated["result"].get("url", "").startswith(next_url), navigated
                        print("PACKAGED_CHROME_NAVIGATE_VERIFIED: real pane navigated to /next",
                              flush=True)

                    query = browser_action(app, env, "browser.query", pane,
                                           {"target": {"css": "#safe"}})
                    assert query["result"]["count"] == 1, query
                    inspection = browser_action(app, env, "browser.inspect", pane,
                                                {"target": {"css": "#field"}})
                    assert inspection["result"]["elements"], inspection
                    click = browser_action(app, env, "browser.click", pane,
                                           {"target": {"css": "#safe"}})
                    assert click["result"], click
                    waited = browser_action(app, env, "browser.wait", pane,
                                            {"condition": {"text": "clicked"}, "timeout_ms": 8000})
                    assert waited["result"]["matched"], waited
                    typed = browser_action(app, env, "browser.fill", pane,
                                           {"target": {"css": "#field"}, "value": "Hello World"})
                    assert typed["result"], typed
                    scroll = browser_action(app, env, "browser.scroll", pane, {"delta_y": 250})
                    assert scroll["result"], scroll
                    screenshot = browser_action(app, env, "browser.screenshot", pane)
                    shot = Path(screenshot["result"]["path"])
                    assert shot.is_file() and shot.read_bytes()[:8] == b"\x89PNG\r\n\x1a\n", screenshot
                    assert stat.S_IMODE(shot.stat().st_mode) & 0o077 == 0, "screenshot is not private"
                    print("PACKAGED_BROWSER_AGENT_VERIFIED: snapshot/query/inspect/click/fill/wait/scroll/screenshot", flush=True)

                    approval = browser_action(app, env, "browser.eval", pane,
                                              {"expression": "window.__deleted=true"})
                    assert approval.get("approval_required"), "eval must require native approval"
                    req_id = approval["request_id"]
                    forbidden = action(app, env, "browser.approve",
                                       {"request_id": req_id}, allow_error=True)
                    assert "error" in forbidden, "CLI must never approve the action"
                    cancelled = action(app, env, "browser.cancel", {"request_id": req_id})
                    assert cancelled["result"]["cancelled"], cancelled
                    # A cancelled request must never run its pending JavaScript.
                    assert action(app, env, "browser.result",
                                  {"request_id": req_id}, allow_error=True).get("error")
                    check_mcp(app, env, pane)
                    if approval_driver is not None:
                        test_native_gpui_approval(app, env, pane, child.pid, approval_driver)
                    closed = action(app, env, "tab.close",
                                    {"id": tab["tab"], "confirmed": True})
                    print("PACKAGED_BROWSER_E2E_VERIFIED: {} complete".format(engine), flush=True)
                finally:
                    child.terminate()
                    try:
                        child.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        child.kill()
                        child.wait(timeout=5)
        except Exception:
            print("PACKAGED_BROWSER_E2E_FAILED: recent app diagnostics:", file=sys.stderr)
            print("FIXTURE_HTTP_REQUESTS: {}".format(server.seen_requests[-40:]),
                  file=sys.stderr)
            if log.exists():
                print(log.read_text(errors="replace")[-6000:], file=sys.stderr)
            raise
        finally:
            server.shutdown()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--app", type=Path, required=True)
    parser.add_argument("--core", type=Path, required=True)
    parser.add_argument("--engine", choices=["wkwebview", "chrome-cdp"], required=True)
    parser.add_argument("--approval-driver", type=Path)
    args = parser.parse_args()
    run(args.app.resolve(), args.core.resolve(), args.engine,
        args.approval_driver.resolve() if args.approval_driver else None)
