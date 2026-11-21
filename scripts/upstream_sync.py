#!/usr/bin/env python3
"""Propose upstream Herdr stable-tag merges; no direct release or force pushes.

The workflow containing this script MUST live on the GitHub default branch.
The supplied worktree MUST be the maintained WebView integration branch.
All merge results are proposed as PRs and require the real browser CI gate.
"""
from __future__ import annotations

import json
import os
import re
import subprocess
import sys
from pathlib import Path

UPSTREAM = "https://github.com/herdrdev/herdr-app.git"
REPO = "herdrdev/herdr-app"
STABLE = re.compile(r"^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$")
TAG_REF = "refs/tags/"
MAX_LOG = 6000


def command(args: list[str], *, cwd: Path | None = None, check: bool = True) -> subprocess.CompletedProcess[str]:
    proc = subprocess.run(args, cwd=cwd, text=True, capture_output=True, check=False)
    if check and proc.returncode:
        detail = (proc.stdout + "\n" + proc.stderr)[-MAX_LOG:]
        raise RuntimeError(f"{' '.join(args)} failed (exit {proc.returncode}):\n{detail}")
    return proc


def choose_tag(ls_remote: str) -> tuple[str, str]:
    candidates: list[tuple[tuple[int, int, int], str, str]] = []
    for line in ls_remote.splitlines():
        fields = line.split("\t", 1)
        if len(fields) != 2 or not fields[1].startswith(TAG_REF):
            continue
        tag = fields[1][len(TAG_REF):]
        match = STABLE.fullmatch(tag)
        if match and re.fullmatch(r"[0-9a-fA-F]{40}", fields[0]):
            candidates.append((tuple(map(int, match.groups())), tag, fields[0]))
    if not candidates:
        raise RuntimeError("no eligible stable vX.Y.Z Git tags in upstream; refusing prerelease")
    _, tag, object_sha = max(candidates, key=lambda c: c[0])
    return tag, object_sha.lower()


def branch_for(tag: str, commit_sha: str) -> str:
    if not STABLE.fullmatch(tag) or not re.fullmatch(r"[0-9a-f]{40}", commit_sha):
        raise ValueError("invalid immutable upstream tag or commit")
    return f"upstream-sync/{tag}-{commit_sha[:12]}"


def report_failure(tag: str, message: str) -> None:
    """Deduplicate open issues by exact upstream tag, with actionable run logs."""
    repository = os.environ["GH_REPO"]
    run_url = os.environ.get("GITHUB_RUN_URL", "unknown run")
    title = f"Upstream WebView integration failed: {tag}"
    content = ("Automated weekly sync could not safely integrate the new upstream tag.\n\n"
               f"Source: {REPO}@{tag}\n"
               f"Target: {os.environ.get('INTEGRATION_BRANCH', 'feature/native-browser-tabs')}\n"
               f"Action logs: {run_url}\n\n"
               "### Error details\n\n```text\n"
               + message[-MAX_LOG:].replace("```", "[code fence]") + "\n```\n\n"
               "Do not publish. Fix the conflict/dependency and real dual-engine E2E tests, "
               "then rerun the upstream-sync workflow.")
    listing = json.loads(command(["gh", "issue", "list", "--repo", repository,
        "--state", "open", "--limit", "100", "--json", "number,title"]).stdout)
    match = next((item for item in listing if item["title"] == title), None)
    if match:
        command(["gh", "issue", "comment", str(match["number"]), "--repo", repository, "--body", content])
    else:
        command(["gh", "issue", "create", "--repo", repository, "--title", title, "--body", content])


def sync(worktree: Path) -> None:
    worktree = worktree.resolve()
    expected = os.environ.get("INTEGRATION_BRANCH", "feature/native-browser-tabs")
    repository = os.environ["GH_REPO"]
    actual = command(["git", "rev-parse", "--abbrev-ref", "HEAD"], cwd=worktree).stdout.strip()
    if actual != expected:
        raise RuntimeError(f"refusing to sync wrong branch {actual!r}; expected {expected!r}")
    output = command(["git", "ls-remote", "--tags", "--refs", UPSTREAM]).stdout
    tag, object_sha = choose_tag(output)
    print(f"Latest stable upstream tag: {tag} ({object_sha})", flush=True)
    try:
        command(["git", "fetch", "--no-tags", UPSTREAM, f"refs/tags/{tag}"], cwd=worktree)
        # Lightweight and annotated tags both resolve to an immutable commit.
        ref_sha = command(["git", "rev-parse", "FETCH_HEAD"], cwd=worktree).stdout.strip().lower()
        if ref_sha != object_sha:
            raise RuntimeError("upstream tag moved during inspection; refusing moving target")
        upstream_sha = command(["git", "rev-parse", "FETCH_HEAD^{commit}"], cwd=worktree).stdout.strip().lower()
        ancestor = command(["git", "merge-base", "--is-ancestor", upstream_sha, "HEAD"],
                           cwd=worktree, check=False)
        if ancestor.returncode == 0:
            print(f"Already integrated {tag} ({upstream_sha}); nothing to change.")
            return
        if ancestor.returncode != 1:
            raise RuntimeError(f"cannot check integration ancestry: {ancestor.stderr}")
        proposed = branch_for(tag, upstream_sha)
        old = command(["git", "ls-remote", "--heads", "origin", f"refs/heads/{proposed}"], cwd=worktree)
        if old.stdout.strip():
            print(f"Existing proposal branch {proposed}; no duplicate or force push.")
            return
        command(["git", "config", "user.name", "github-actions[bot]"], cwd=worktree)
        command(["git", "config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com"], cwd=worktree)
        command(["git", "checkout", "-b", proposed], cwd=worktree)
        result = command(["git", "merge", "--no-commit", "--no-ff", upstream_sha], cwd=worktree, check=False)
        if result.returncode:
            command(["git", "merge", "--abort"], cwd=worktree, check=False)
            raise RuntimeError("upstream merge conflict or incompatibility:\n" + result.stdout + result.stderr)
        state = worktree / ".github" / "upstream-sync-state.json"
        state.parent.mkdir(parents=True, exist_ok=True)
        state.write_text(json.dumps({"source": REPO, "tag": tag, "commit": upstream_sha},
                                    indent=2) + "\n", encoding="utf-8")
        command(["git", "add", ".github/upstream-sync-state.json"], cwd=worktree)
        command(["git", "commit", "-m", f"chore: merge {REPO}@{tag} into WebView integration"], cwd=worktree)
        # No force. Existing branch was checked above and the remote rejects races.
        command(["git", "push", "origin", f"HEAD:refs/heads/{proposed}"], cwd=worktree)
        body = (f"Automated upstream proposal from `{REPO}@{tag}` (`{upstream_sha}`).\n\n"
                "Preserves the browser integration branch and original icon by using a normal merge.\n\n"
                "**Release gate:** manual/automated review and all native WKWebView + Chrome CDP "
                "GUI E2E, packaging and updater tests must pass before merging or publishing. "
                "This workflow never releases or force-pushes.\n\n"
                f"Source workflow: {os.environ.get('GITHUB_RUN_URL', '')}.\n\nTracking #1.")
        created = command(["gh", "pr", "create", "--repo", repository, "--base", expected,
            "--head", proposed, "--title", f"chore(upstream): integrate {tag}", "--body", body])
        print(created.stdout.strip())
    except Exception as error:
        command(["git", "merge", "--abort"], cwd=worktree, check=False)
        report_failure(tag, str(error))
        raise


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: upstream_sync.py <browser-integration-worktree>")
    sync(Path(sys.argv[1]))
