#!/usr/bin/env python3
"""Fail-closed gate for automatically publishing a Herdr WebView stable release.

This is deliberately stricter than a green generic CI pipeline. Without native
WKWebView and GPUI approval E2E checks, a release is NOT eligible.
"""
from __future__ import annotations

import json
import sys

REQUIRED = {
    "test",
    "Browser CI required check",
    "browser-macos",
    "platform-tests (linux-x86_64, ubuntu-22.04)",
    "platform-tests (macos-aarch64, macos-15)",
    "platform-tests (windows-x86_64, windows-2022)",
    # Must be emitted by REAL native-app E2E workflows, not mocks.
    "wkwebview-native-e2e",
    "browser-gpui-approval-e2e",
}


def assess(payload: dict) -> tuple[bool, list[str]]:
    by_name: dict[str, dict] = {}
    for item in payload.get("check_runs", []):
        if not isinstance(item, dict):
            return False, ["invalid check run record"]
        name = item.get("name")
        if not isinstance(name, str):
            return False, ["unnamed check run"]
        # Most recent check wins when GitHub includes rerun attempts.
        if name not in by_name or (item.get("started_at") or "") >= (by_name[name].get("started_at") or ""):
            by_name[name] = item
    failures = []
    for name in sorted(REQUIRED):
        record = by_name.get(name)
        if record is None:
            failures.append(f"missing native/release check: {name}")
        elif record.get("status") != "completed" or record.get("conclusion") != "success":
            failures.append(f"not successful: {name} ({record.get('status')}/{record.get('conclusion')})")
    return not failures, failures


def main() -> int:
    payload = json.load(sys.stdin)
    ok, reasons = assess(payload)
    print("Release gate: " + ("PASS" if ok else "BLOCKED"))
    for issue in reasons:
        print("- " + issue)
    return 0 if ok else 3


if __name__ == "__main__":
    raise SystemExit(main())
