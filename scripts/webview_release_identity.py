#!/usr/bin/env python3
"""Fail-closed immutable release identity gate.

A green workflow is NOT permission to republish a different build under an
already published version. Confirm tag/ref/release all identify this SHA.
"""
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


def assess(sha: str, tag: str, release: dict | None, tag_ref: dict | None) -> tuple[bool, str]:
    if re.fullmatch(r"[a-f0-9]{40}", sha) is None:
        return False, "invalid candidate SHA"
    if re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+-webview\.[0-9]+", tag) is None:
        return False, "invalid WebView release tag"
    if release is not None:
        if not isinstance(release, dict) or release.get("tag_name") != tag:
            return False, "release tag metadata mismatch"
        if release.get("target_commitish") != sha:
            return False, "release version is already associated with a DIFFERENT commit; bump Cargo.toml version"
        if release.get("draft") not in (True, False):
            return False, "release draft state missing"
    if tag_ref is not None:
        if not isinstance(tag_ref, dict) or tag_ref.get("ref") != "refs/tags/" + tag:
            return False, "unexpected Git tag ref"
        obj = tag_ref.get("object")
        if not isinstance(obj, dict) or obj.get("type") != "commit" or obj.get("sha") != sha:
            return False, "immutable Git tag points to a DIFFERENT commit or annotated object"
    if release is not None and tag_ref is None:
        return False, "release exists without an inspectable matching Git tag"
    if release is not None and not release["draft"]:
        return True, "already-published"
    return True, "eligible"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sha", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--release-json", type=Path, required=True)
    parser.add_argument("--tag-json", type=Path, required=True)
    args = parser.parse_args()
    okay, outcome = assess(args.sha, args.tag,
                           json.loads(args.release_json.read_text()),
                           json.loads(args.tag_json.read_text()))
    print(outcome)
    return 0 if okay else 1


if __name__ == "__main__":
    raise SystemExit(main())
