import unittest
from unittest.mock import patch

from configure_branch_protection import BRANCHES, desired_rule, endpoint, run, verify_rule


def protected_response(checks):
    return {
        "required_status_checks": {"strict": False, "contexts": list(checks)},
        "required_pull_request_reviews": {"required_approving_review_count": 0},
        "enforce_admins": {"enabled": True},
        "allow_force_pushes": {"enabled": False},
        "allow_deletions": {"enabled": False},
    }


def api_unprotected(path, method="GET", payload=None):
    name = path.split("/branches/")[-1].replace("%2F", "/")
    if method == "PUT":
        raise AssertionError("unexpected write")
    return {"name": name, "protected": False}


class BranchSetupTests(unittest.TestCase):
    def test_scopes_and_proposed_review_policy(self):
        self.assertEqual(len(BRANCHES), 3)
        self.assertEqual(BRANCHES[0][2], ("portable-tests", "macos-native-tests"))
        self.assertEqual(BRANCHES[1][2], ("test",))
        self.assertEqual(BRANCHES[2][2], ("test", "Browser CI required check"))
        self.assertTrue(endpoint(BRANCHES[2][0], BRANCHES[2][1])
                        .endswith("feature%2Fnative-browser-tabs"))
        payload = desired_rule(BRANCHES[2][2])
        self.assertEqual(payload["required_pull_request_reviews"]["required_approving_review_count"], 0)
        self.assertFalse(payload["allow_force_pushes"])
        self.assertFalse(payload["allow_deletions"])

    def test_dry_run_is_read_only(self):
        with patch("configure_branch_protection.gh", side_effect=api_unprotected) as mock:
            self.assertEqual(run(), 0)
        self.assertEqual(mock.call_count, 3)

    def test_apply_creates_exact_three_protection_rules(self):
        saved = {}
        def mocked(path, method="GET", payload=None):
            if path.endswith("/protection"):
                if method == "PUT":
                    saved[path] = payload
                return protected_response(saved[path]["required_status_checks"]["contexts"])
            return api_unprotected(path)
        with patch("configure_branch_protection.gh", side_effect=mocked) as mock:
            self.assertEqual(run(apply=True), 0)
        self.assertEqual(len(saved), 3)
        self.assertEqual(mock.call_count, 12)
        self.assertTrue(all(not r["required_status_checks"]["strict"] for r in saved.values()))

    def test_protection_not_overwritten(self):
        def preprotected(path, method="GET", payload=None):
            if method != "GET":
                raise AssertionError("unexpected protected write")
            return {"name": path.split("/branches/")[-1].replace("%2F", "/"),
                    "protected": True}
        with patch("configure_branch_protection.gh", side_effect=preprotected) as mock:
            self.assertEqual(run(apply=True), 1)
        self.assertEqual(mock.call_count, 3)

    def test_missing_checks_and_approval_rule_are_blocked(self):
        checks = BRANCHES[-1][2]
        self.assertEqual(verify_rule(protected_response(checks), checks), [])
        for change in ("checks", "review", "admin", "force"):
            data = protected_response(checks)
            if change == "checks":
                data["required_status_checks"]["contexts"].pop()
            elif change == "review":
                data["required_pull_request_reviews"] = None
            elif change == "admin":
                data["enforce_admins"]["enabled"] = False
            else:
                data["allow_force_pushes"]["enabled"] = True
            self.assertTrue(verify_rule(data, checks), change)


if __name__ == "__main__":
    unittest.main()
