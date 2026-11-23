import unittest
from adapter_update import current_pin, patch_manifest, checks_pass, runtime_changes, lock_valid

SHA_A = "a" * 40
SHA_B = "b" * 40
URL = "https://github.com/TonyPythoneer/herdr-webview-adapter"
MANIFEST = ('[package]\nname = "herdr-app"\nversion = "0.1.16-webview.2"\n'
            '[dependencies]\nherdr-webview-adapter = { git = "' + URL +
            '", rev = "' + SHA_A + '" }\n')


class AdapterUpdateTests(unittest.TestCase):
    def test_exact_sha_and_version_bump(self):
        updated = patch_manifest(MANIFEST, SHA_B)
        self.assertEqual(current_pin(updated), SHA_B)
        self.assertIn('version = "0.1.16-webview.3"', updated)
        self.assertNotIn(SHA_A, updated)

    def test_rejects_invalid_or_duplicate_pins(self):
        for sha in ("master", "deadbeef", "z" * 40, SHA_A):
            with self.assertRaises(ValueError):
                patch_manifest(MANIFEST, sha)
        with self.assertRaises(ValueError):
            current_pin(MANIFEST + MANIFEST)

    def test_checks_are_strict_and_missing_checks_block(self):
        items = [{"name": name, "status": "completed", "conclusion": "success"}
                 for name in ("portable-tests", "macos-native-tests")]
        self.assertTrue(checks_pass({"check_runs": items}))
        self.assertFalse(checks_pass({"check_runs": items[:1]}))
        items[-1]["conclusion"] = "failure"
        self.assertFalse(checks_pass({"check_runs": items}))

    def test_only_adapter_runtime_changes_require_host_upgrade(self):
        self.assertFalse(runtime_changes({"status": "ahead", "files": [
            {"filename": "README.md"}, {"filename": ".github/workflows/ci.yml"}]}))
        self.assertTrue(runtime_changes({"status": "ahead", "files": [
            {"filename": "src/browser.rs"}]}))
        with self.assertRaises(ValueError):
            runtime_changes({"status": "diverged", "files": []})
        with self.assertRaises(ValueError):
            runtime_changes({"status": "ahead"})

    def test_lockfile_must_match_exact_revision(self):
        expected = "git+" + URL + "?rev=" + SHA_B + "#" + SHA_B
        self.assertTrue(lock_valid("source = \"" + expected + "\"\n", SHA_B))
        self.assertFalse(lock_valid("source = \"" + expected + "\"\n", SHA_A))


if __name__ == "__main__":
    unittest.main()
