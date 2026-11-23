import unittest
from webview_release_gate import REQUIRED, assess


def checks(*, omit=(), failed=(), pending=()):
    return {"check_runs": [
        {"name":name, "status":"in_progress" if name in pending else "completed",
         "conclusion":None if name in pending else ("failure" if name in failed else "success"),
         "started_at":"2026-11-22T00:00:00Z"}
        for name in REQUIRED if name not in omit
    ]}


class ReleaseGateTests(unittest.TestCase):
    def test_all_eight_real_checks_are_required(self):
        self.assertEqual(len(REQUIRED),8)
        self.assertTrue(assess(checks())[0])

    def test_no_release_if_native_browser_e2e_missing(self):
        okay,reasons=assess(checks(omit={"wkwebview-native-e2e"}))
        self.assertFalse(okay)
        self.assertTrue(any("wkwebview-native-e2e" in reason for reason in reasons))

    def test_no_release_if_approval_or_general_platform_failed(self):
        okay,reasons=assess(checks(failed={"browser-gpui-approval-e2e"}))
        self.assertFalse(okay)
        self.assertTrue(any("not successful" in r for r in reasons))
        self.assertFalse(assess(checks(pending={"platform-tests (windows-x86_64, windows-2022)"}))[0])

    def test_latest_rerun_failure_must_not_be_overridden_by_old_success(self):
        payload=checks()
        payload["check_runs"].append({"name":"browser-macos","status":"completed","conclusion":"failure",
            "started_at":"2026-11-22T00:10:00Z"})
        self.assertFalse(assess(payload)[0])


if __name__ == "__main__":
    unittest.main()
