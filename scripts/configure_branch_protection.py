#!/usr/bin/env python3
"""Set exact Herdr branch protection rules with an authenticated local gh CLI.

Default: read-only preview. Use --apply to write; --verify to audit.
Never run with GitHub Actions GITHUB_TOKEN. Requires repo admin privileges.
"""
import argparse
import json
import subprocess
import sys
from urllib.parse import quote

BRANCHES = (
    ("TonyPythoneer/herdr-webview-adapter", "main", ("portable-tests", "macos-native-tests")),
    ("TonyPythoneer/herdr-webview", "master", ("test",)),
    ("TonyPythoneer/herdr-webview", "feature/native-browser-tabs",
     ("test", "Browser CI required check")),
)


def endpoint(repo, branch, protection=False):
    path = f"repos/{repo}/branches/{quote(branch, safe='')}"
    return path + ("/protection" if protection else "")


def desired_rule(checks):
    # strict:false allows bot-created PRs whose only possible check source is
    # explicit workflow_dispatch on the candidate SHA. Post-merge CI still
    # gates exact-SHA auto-release, and all PRs require normal passing checks.
    return {
        "required_status_checks": {"strict": False, "contexts": list(checks)},
        "enforce_admins": True,
        "required_pull_request_reviews": {
            "dismiss_stale_reviews": True,
            "require_code_owner_reviews": False,
            "required_approving_review_count": 0,
        },
        "restrictions": None,
        "required_linear_history": False,
        "allow_force_pushes": False,
        "allow_deletions": False,
        "block_creations": False,
        "required_conversation_resolution": True,
        "lock_branch": False,
        "allow_fork_syncing": False,
    }


def verify_rule(state, expected):
    if not isinstance(state, dict):
        return ["GitHub protection response missing"]
    problems = []
    checks = state.get("required_status_checks")
    if not isinstance(checks, dict):
        problems.append("required status checks absent")
    else:
        contexts = checks.get("contexts")
        if not isinstance(contexts, list) or not set(expected).issubset(set(contexts)):
            problems.append("required CI contexts missing")
        if checks.get("strict") is not False:
            problems.append("strict mode may deadlock bot PRs")
    if state.get("required_pull_request_reviews") is None:
        problems.append("PR-before-merge rule absent")
    if (state.get("enforce_admins") or {}).get("enabled") is not True:
        problems.append("admin bypass allowed")
    for flag in ("allow_force_pushes", "allow_deletions"):
        if (state.get(flag) or {}).get("enabled") is not False:
            problems.append(flag + " not explicitly disabled")
    return problems


def gh(path, method="GET", payload=None):
    cmd = ["gh", "api", "--method", method, path]
    if payload is not None:
        cmd.extend(["--input", "-"])
    result = subprocess.run(cmd, input=json.dumps(payload) if payload is not None else None,
                            text=True, capture_output=True, check=False)
    if result.returncode:
        raise RuntimeError(f"GitHub API {method} {path} failed; verify local gh "
                           "authentication and Administration permissions")
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        raise RuntimeError(f"GitHub returned invalid JSON for {path}") from exc


def run(apply=False, verify=False):
    records = []
    for repo, branch, checks in BRANCHES:
        item = gh(endpoint(repo, branch))
        if item.get("name") != branch or not isinstance(item.get("protected"), bool):
            raise RuntimeError(f"Wrong or incomplete branch response: {repo}:{branch}")
        records.append((repo, branch, checks, item["protected"]))

    if verify:
        issues = []
        for repo, branch, checks, protected in records:
            if not protected:
                issues.append(f"{repo}:{branch}: not protected")
            else:
                details = gh(endpoint(repo, branch, True))
                issues.extend(f"{repo}:{branch}: {problem}"
                              for problem in verify_rule(details, checks))
        for issue in issues:
            print("BLOCKED: " + issue, file=sys.stderr)
        if not issues:
            print("PASS: three branches have expected status checks, required PRs and protections")
        return 1 if issues else 0

    for repo, branch, checks, protected in records:
        print(f"{repo}:{branch}: protected={protected}; checks={','.join(checks)}")
    if not apply:
        print("DRY RUN: no writes. Use --apply, then --verify.")
        return 0
    if any(protected for _, _, _, protected in records):
        print("BLOCKED: existing protected branch found; refusing overwrite. Use --verify.",
              file=sys.stderr)
        return 1
    for repo, branch, checks, _ in records:
        # Recheck before each write: fail closed if protection was added since
        # the initial read. No best-effort replacement of another admin rule.
        latest = gh(endpoint(repo, branch))
        if latest.get("name") != branch or latest.get("protected") is not False:
            raise RuntimeError(f"Branch changed during apply: {repo}:{branch}")
        path = endpoint(repo, branch, True)
        gh(path, method="PUT", payload=desired_rule(checks))
        violations = verify_rule(gh(path), checks)
        if violations:
            raise RuntimeError(f"Post-write verification failed {repo}:{branch}: "
                               + ", ".join(violations))
        print(f"OK: protected {repo}:{branch}")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mutually_exclusive = parser.add_mutually_exclusive_group()
    mutually_exclusive.add_argument("--apply", action="store_true")
    mutually_exclusive.add_argument("--verify", action="store_true")
    args = parser.parse_args()
    try:
        return run(apply=args.apply, verify=args.verify)
    except (RuntimeError, ValueError) as exc:
        print("BLOCKED: " + str(exc), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
