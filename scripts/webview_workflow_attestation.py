#!/usr/bin/env python3
"""Attest tested CI definitions against trusted default-branch copies.

A green check-run name alone is not proof that native browser / approval E2E
still executes. A feature branch cannot silently edit what the release gate
considers a passing test. Any test-definition drift blocks release.
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

FORK_EXPLANATION = (
    "    # The stable fork release gate evaluates check-runs on the exact maintained\n"
    "    # integration SHA. PR checks and master-only CI cannot satisfy that gate.\n"
)
TRUSTED_TRIGGER = "    branches: [master]\n"
FEATURE_TRIGGER = "    branches: [master, feature/native-browser-tabs]\n"
DISPATCH = "  workflow_dispatch:"


def attest(trusted_ci: str, tested_ci: str, trusted_browser: str,
           tested_browser: str) -> list[str]:
    errors = []
    if DISPATCH not in trusted_ci or DISPATCH not in tested_ci:
        errors.append("platform CI cannot be explicitly dispatched")
    if DISPATCH not in trusted_browser or DISPATCH not in tested_browser:
        errors.append("native browser CI cannot be explicitly dispatched")
    if TRUSTED_TRIGGER not in trusted_ci:
        errors.append("trusted platform CI baseline missing master trigger")
    if FEATURE_TRIGGER not in tested_ci:
        errors.append("tested platform CI missing maintained feature trigger")
    # Exactly two pre-agreed non-executable differences:
    # the feature push trigger and its explanatory comments.
    if tested_ci.count(FORK_EXPLANATION) != 1 or tested_ci.count(FEATURE_TRIGGER) != 1:
        errors.append("unexpected platform CI fork policy/trigger shape")
    normalized = tested_ci.replace(FORK_EXPLANATION, "", 1)
    normalized = normalized.replace(FEATURE_TRIGGER, TRUSTED_TRIGGER, 1)
    if normalized != trusted_ci:
        errors.append("platform CI job definition differs from trusted master")
    if tested_browser != trusted_browser:
        errors.append("WKWebView/Chrome/GPUI E2E workflow differs from trusted master")
    return errors


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--trusted-ci", type=Path, required=True)
    p.add_argument("--tested-ci", type=Path, required=True)
    p.add_argument("--trusted-browser", type=Path, required=True)
    p.add_argument("--tested-browser", type=Path, required=True)
    args = p.parse_args()
    problems = attest(*(getattr(args, item).read_text()
                        for item in ("trusted_ci", "tested_ci", "trusted_browser", "tested_browser")))
    for issue in problems:
        print("RELEASE BLOCKED: " + issue, file=sys.stderr)
    if not problems:
        print("Trusted platform + native browser E2E workflow definitions match tested commit.")
    return 1 if problems else 0


if __name__ == "__main__":
    raise SystemExit(main())
