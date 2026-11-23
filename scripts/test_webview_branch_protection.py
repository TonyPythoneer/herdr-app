import unittest
from webview_branch_protection import BRANCHES, assess


def fixtures():
    return {key: {"name": branch, "protected": True}
            for key, branch in BRANCHES}


class BranchSafetyTests(unittest.TestCase):
    def test_only_all_three_protected_branches_pass(self):
        self.assertEqual(assess(fixtures()), [])

    def test_unprotected_any_branch_blocks_auto_update_or_release(self):
        for key, _ in BRANCHES:
            data = fixtures()
            data[key]["protected"] = False
            self.assertTrue(any(key in reason for reason in assess(data)))

    def test_missing_data_and_wrong_branch_are_fail_closed(self):
        for key, _ in BRANCHES:
            missing = fixtures()
            del missing[key]
            self.assertTrue(assess(missing))
            wrong = fixtures()
            wrong[key]["name"] = "untrusted"
            self.assertTrue(assess(wrong))
            no_flag = fixtures()
            del no_flag[key]["protected"]
            self.assertTrue(assess(no_flag))


if __name__ == "__main__":
    unittest.main()
