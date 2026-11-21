import unittest

from upstream_sync import choose_tag, branch_for


class StableTagSelectionTest(unittest.TestCase):
    def test_numeric_semver_wins_over_lexical_order(self):
        data = ("a" * 40 + "\trefs/tags/v0.1.9\n" +
                "b" * 40 + "\trefs/tags/v0.1.10\n" +
                "c" * 40 + "\trefs/tags/v0.2.0-rc.1\n")
        self.assertEqual(choose_tag(data), ("v0.1.10", "b" * 40))

    def test_excludes_prereleases_malformed_and_peeled_refs(self):
        data = ("a" * 40 + "\trefs/tags/v0.1.16\n" +
                "b" * 40 + "\trefs/tags/v0.1.17-beta.1\n" +
                "c" * 40 + "\trefs/tags/v0.1.16^{}\n" +
                "d" * 40 + "\trefs/tags/1.2.3\n")
        self.assertEqual(choose_tag(data), ("v0.1.16", "a" * 40))

    def test_empty_stable_set_is_error(self):
        with self.assertRaisesRegex(RuntimeError, "no eligible stable"):
            choose_tag("a" * 40 + "\trefs/tags/v0.1.17-rc.1\n")

    def test_branch_name_includes_immutable_commit(self):
        self.assertEqual(branch_for("v0.1.17", "d" * 40),
                         "upstream-sync/v0.1.17-dddddddddddd")
        with self.assertRaises(ValueError):
            branch_for("v0.1.17-rc.1", "d" * 40)


if __name__ == "__main__":
    unittest.main()
