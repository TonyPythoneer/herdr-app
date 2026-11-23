# Herdr WebView — secure release bootstrap

This document applies to the **Host fork's default `master` branch** and
its maintained `feature/native-browser-tabs` branch, plus the separate
`herdr-webview-adapter:main` repository.

## Order of operations

1. Merge the green Adapter hardening PR #5, then the green Host hardening PR #24
   after its **latest SHA** checks are all passing.
2. From an updated local checkout of the Host `master` branch, authenticate
   GitHub CLI with a user token that has **Administration: write** permission
   in both repositories. Never put that token into an Actions secret or commit.
3. Preview, apply, then verify the exact three branch-protection rules:

   ```sh
   python3 scripts/configure_branch_protection.py
   python3 scripts/configure_branch_protection.py --apply
   python3 scripts/configure_branch_protection.py --verify
   ```

4. Ensure that GitHub Settings → Actions → General permits Actions to **create
   pull requests**. Only the PR-creation bot needs that setting; it does not
   approve or merge its own PRs.
5. Validate that the release and Adapter update workflows no longer fail at
   the branch-protection gate. Keep all native WKWebView, Chrome CDP, trusted
   GPUI approval, and all-platform CI checks mandatory before publishing.

## Protection semantics

| Repository / branch | Required passing checks |
| --- | --- |
| `TonyPythoneer/herdr-webview-adapter:main` | `portable-tests`, `macos-native-tests` |
| `TonyPythoneer/herdr-webview:master` | `test` |
| `TonyPythoneer/herdr-webview:feature/native-browser-tabs` | `test`, `Browser CI required check` |

All branches must reject direct pushes, force pushes, and deletions, including
admin bypasses. Pull requests remain mandatory, but the solo-maintainer
configuration does **not** require an independent reviewer. The script refuses
to overwrite a pre-existing protection rule, so any different existing policy
must be reviewed manually.

Required checks use non-strict branch-up-to-date mode deliberately: a pull
request created using GitHub Actions' default token does not automatically
receive the usual PR CI trigger and is instead explicitly dispatched against
the candidate SHA. The **post-merge release gate** still requires full native
E2E and platform checks at the exact final integration commit.

## Limits and failure recovery

- GitHub branch metadata `protected: true` alone is **not** proof that every
  required CI context or review rule is configured. The local `--verify`
  command queries actual protection settings using administrator credentials.
- Never use the setup tool from a GitHub Actions workflow. Do not grant an
  auto-release or dependency-update job repository-administration privileges.
- Applying protections can temporarily pause releases. This is intentional:
  fail closed until all three branches meet the policy.
- If applying fails partway, run `--verify` to inspect the state. The tool
  refuses to overwrite existing rules and does not attempt unsafe rollback.
- For an incompatible upstream GPUI dependency, keep the Adapter PR blocked
  until the shared dependency graph and shipped-app E2E are verified.
- For every future fork update, increment `-webview.N` before release if the
  source SHA differs from a previously published tag.

Tracking: [P0 branch security issue #25](https://github.com/TonyPythoneer/herdr-webview/issues/25).
