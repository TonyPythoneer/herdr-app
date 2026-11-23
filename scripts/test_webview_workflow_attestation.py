import unittest
from webview_workflow_attestation import (
    FORK_EXPLANATION, TRUSTED_TRIGGER, FEATURE_TRIGGER, attest,
)

TRUSTED_CI = ("name: ci\non:\n  push:\n" + TRUSTED_TRIGGER +
              "  workflow_dispatch:\njobs:\n  test:\n    runs-on: ubuntu-latest\n"
              "    steps:\n      - run: cargo test --locked\n")
FEATURE_CI = TRUSTED_CI.replace(TRUSTED_TRIGGER, FORK_EXPLANATION + FEATURE_TRIGGER)
BROWSER = ("name: browser-macos\non:\n  workflow_dispatch:\n"
           "jobs:\n  wkwebview-native-e2e:\n    steps:\n      - run: cargo test --locked\n")


class ReleaseAttestationTests(unittest.TestCase):
    def test_only_two_known_non_executable_ci_differences_allowed(self):
        self.assertEqual(attest(TRUSTED_CI, FEATURE_CI, BROWSER, BROWSER), [])

    def test_silently_removed_platform_tests_blocked(self):
        changed = FEATURE_CI.replace("cargo test --locked", "echo green")
        self.assertTrue(attest(TRUSTED_CI, changed, BROWSER, BROWSER))

    def test_silently_removed_native_e2e_blocked(self):
        changed = BROWSER.replace("cargo test --locked", "echo approved")
        self.assertIn("WKWebView/Chrome/GPUI E2E workflow differs from trusted master",
                      attest(TRUSTED_CI, FEATURE_CI, BROWSER, changed))

    def test_new_unsafe_trigger_or_missing_dispatch_blocked(self):
        self.assertTrue(attest(TRUSTED_CI, FEATURE_CI.replace(
            "feature/native-browser-tabs", "some-other-branch"), BROWSER, BROWSER))
        self.assertTrue(attest(TRUSTED_CI.replace("  workflow_dispatch:\n", ""),
                               FEATURE_CI, BROWSER, BROWSER))
        self.assertTrue(attest(TRUSTED_CI, FEATURE_CI,
                               BROWSER, BROWSER.replace("  workflow_dispatch:\n", "")))

    def test_comment_changes_must_be_reviewed_on_default_branch(self):
        self.assertTrue(attest(TRUSTED_CI, FEATURE_CI + "# new untrusted rule\n",
                               BROWSER, BROWSER))
        self.assertTrue(attest(TRUSTED_CI, FEATURE_CI + FORK_EXPLANATION,
                               BROWSER, BROWSER))


if __name__ == "__main__":
    unittest.main()
