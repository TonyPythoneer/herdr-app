#!/usr/bin/env python3
"""Validated immutable Adapter pin updates for maintained Herdr WebView.

Never unpins Git dependencies, bypasses user approval, or auto-merges PRs.
"""
import argparse
import json
import re
import sys
from pathlib import Path

URL = "https://github.com/TonyPythoneer/herdr-webview-adapter"
PIN = re.compile(r'(?m)^(herdr-webview-adapter\s*=\s*\{\s*git\s*=\s*"' +
                 re.escape(URL) +
                 r'"\s*,\s*rev\s*=\s*")([a-f0-9]{40})("\s*\})$')
VERSION = re.compile(r'(?m)^(version\s*=\s*")(\d+)\.(\d+)\.(\d+)-webview\.(\d+)(")$')
REQUIRED_ADAPTER_CHECKS = {"portable-tests", "macos-native-tests"}


def current_pin(manifest):
    matches = list(PIN.finditer(manifest))
    if len(matches) != 1:
        raise ValueError("expected exactly one official immutable Adapter dependency")
    return matches[0].group(2)


def patch_manifest(manifest, sha):
    if re.fullmatch(r"[a-f0-9]{40}", sha) is None:
        raise ValueError("Adapter Git revision must be a 40-character SHA")
    old = current_pin(manifest)
    if old == sha:
        raise ValueError("Adapter already pinned to the requested SHA")
    updated = PIN.sub(lambda m: m.group(1) + sha + m.group(3), manifest, count=1)
    versions = list(VERSION.finditer(updated))
    if len(versions) != 1:
        raise ValueError("expected exactly one WebView stable package version")
    updated = VERSION.sub(lambda m: m.group(1) + ".".join(m.group(i) for i in (2, 3, 4))
                          + "-webview." + str(int(m.group(5)) + 1) + m.group(6), updated, count=1)
    return updated


def checks_pass(payload):
    found = {}
    for item in payload.get("check_runs", []):
        name = item.get("name")
        if name not in REQUIRED_ADAPTER_CHECKS:
            continue
        if name not in found or (item.get("started_at") or "") >= (found[name].get("started_at") or ""):
            found[name] = item
    return all(found.get(name, {}).get("status") == "completed" and
               found[name].get("conclusion") == "success" for name in REQUIRED_ADAPTER_CHECKS)


def runtime_changes(compare):
    if compare.get("status") not in ("ahead", "identical"):
        raise ValueError("Adapter HEAD is not a fast-forward from currently pinned revision")
    if compare.get("status") == "identical":
        return False
    files = compare.get("files")
    if not isinstance(files, list) or len(files) >= 300:
        raise ValueError("missing or truncated GitHub compare results; manual review required")
    # Only explicitly non-runtime files can be skipped. Unknown paths,
    # build.rs, tests, new build hooks and contract docs REQUIRE Host E2E.
    harmless = {"README.md", "LICENSE", ".github/dependabot.yml",
                ".github/upstream-compat.json", "scripts/upstream_compat.py",
                "scripts/test_upstream_compat.py"}
    for file in files:
        path = file.get("filename")
        if not isinstance(path, str):
            raise ValueError("missing Adapter changed-file path")
        if path in harmless or path.startswith(".github/workflows/"):
            continue
        if path.startswith("docs/") and path != "docs/OWNERSHIP.md":
            continue
        return True
    return False


def lock_valid(lock, sha):
    if re.fullmatch(r"[a-f0-9]{40}", sha) is None:
        return False
    expected = "git+" + URL + "?rev=" + sha + "#" + sha
    matched = []
    for section in lock.split("[[package]]")[1:]:
        fields = {}
        for line in section.splitlines():
            match = re.fullmatch(r'(name|source) = "([^"]*)"', line.strip())
            if match:
                fields[match.group(1)] = match.group(2)
        if fields.get("name") == "herdr-webview-adapter":
            matched.append(fields.get("source"))
    return matched == [expected]


def main():
    p = argparse.ArgumentParser()
    sub = p.add_subparsers(dest="action", required=True)
    pin = sub.add_parser("pin")
    pin.add_argument("manifest", type=Path)
    apply = sub.add_parser("apply")
    apply.add_argument("manifest", type=Path)
    apply.add_argument("sha")
    checked = sub.add_parser("checks")
    checked.add_argument("payload", type=Path)
    compare = sub.add_parser("changes")
    compare.add_argument("payload", type=Path)
    lock = sub.add_parser("lock")
    lock.add_argument("lockfile", type=Path)
    lock.add_argument("sha")
    a = p.parse_args()
    if a.action == "pin":
        print(current_pin(a.manifest.read_text()))
    elif a.action == "apply":
        a.manifest.write_text(patch_manifest(a.manifest.read_text(), a.sha))
    elif a.action == "checks":
        if not checks_pass(json.loads(a.payload.read_text())):
            print("Adapter main has no completed successful portable+macOS CI at this SHA", file=sys.stderr)
            return 1
    elif a.action == "changes":
        if not runtime_changes(json.loads(a.payload.read_text())):
            print("unchanged")
            return 2
    else:
        if not lock_valid(a.lockfile.read_text(), a.sha):
            print("Cargo.lock does not pin the new exact Adapter SHA", file=sys.stderr)
            return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
