import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


class WorkflowWiringTests(unittest.TestCase):
    def test_both_dispatch_targets_are_on_the_default_branch(self):
        for name in ("ci.yml", "browser-macos.yml"):
            text = (ROOT / ".github" / "workflows" / name).read_text()
            self.assertIn("  workflow_dispatch:", text, name)
            if name == "browser-macos.yml":
                self.assertIn("feature/native-browser-tabs", text, name)
            else:
                self.assertIn("branches: [master]", text, name)

    def test_bot_calls_both_host_checks(self):
        text = (ROOT / ".github/workflows/adapter-auto-update.yml").read_text()
        self.assertIn('gh workflow run ci.yml', text)
        self.assertIn('gh workflow run browser-macos.yml', text)
        self.assertIn("persist-credentials: false", text)

    def test_release_guard_cannot_be_bypassed_by_existing_tag(self):
        text = (ROOT / ".github/workflows/webview-auto-release.yml").read_text()
        self.assertIn("scripts/webview_release_identity.py", text)
        self.assertIn('if [[ "$outcome" == already-published ]]; then', text)


if __name__ == "__main__":
    unittest.main()
