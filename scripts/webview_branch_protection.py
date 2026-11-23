#!/usr/bin/env python3
"""Block automated upgrade and stable publishing from unprotected repositories.

GitHub check-runs do not prevent direct pushes to an unprotected branch.
Require branch-level rules on every code source in the release chain.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

BRANCHES = (
    ("host-master", "master"),
    ("host-integration", "feature/native-browser-tabs"),
    ("adapter-main", "main"),
)


def assess(reports: dict[str, dict]) -> list[str]:
    errors = []
    for key, expected_branch in BRANCHES:
        entry = reports.get(key)
        if not isinstance(entry, dict):
            errors.append(key + ": missing GitHub branch metadata")
        elif entry.get("name") != expected_branch:
            errors.append(key + ": wrong branch identity")
        elif entry.get("protected") is not True:
            errors.append(key + ": missing enforced GitHub branch protection")
    return errors


def main() -> int:
    p = argparse.ArgumentParser()
    for key, _ in BRANCHES:
        p.add_argument("--" + key, type=Path, required=True)
    args = p.parse_args()
    reports = {key: json.loads(getattr(args, key.replace("-", "_")).read_text())
               for key, _ in BRANCHES}
    issues = assess(reports)
    for issue in issues:
        print("SECURITY GATE BLOCKED: " + issue, file=sys.stderr)
    return 1 if issues else 0


if __name__ == "__main__":
    raise SystemExit(main())
