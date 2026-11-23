import unittest
from webview_release_identity import assess

SHA = "a" * 40
OTHER = "b" * 40
TAG = "v0.1.16-webview.2"


def release(sha=SHA, draft=False):
    return {"tag_name": TAG, "target_commitish": sha, "draft": draft}


def ref(sha=SHA):
    return {"ref": "refs/tags/" + TAG, "object": {"sha": sha, "type": "commit"}}


class ReleaseIdentityTests(unittest.TestCase):
    def test_new_version_is_eligible(self):
        self.assertEqual(assess(SHA, TAG, None, None), (True, "eligible"))

    def test_existing_published_version_same_sha_is_idempotent(self):
        self.assertEqual(assess(SHA, TAG, release(), ref()), (True, "already-published"))

    def test_draft_must_match_exact_sha(self):
        self.assertEqual(assess(SHA, TAG, release(draft=True), ref()), (True, "eligible"))
        self.assertFalse(assess(SHA, TAG, release(OTHER, draft=True), ref())[0])

    def test_published_version_other_commit_is_blocked(self):
        okay, reason = assess(SHA, TAG, release(OTHER), ref(OTHER))
        self.assertFalse(okay)
        self.assertIn("DIFFERENT", reason)

    def test_preexisting_tag_on_other_commit_blocks_new_release(self):
        self.assertFalse(assess(SHA, TAG, None, ref(OTHER))[0])

    def test_release_without_matching_tag_blocks(self):
        self.assertFalse(assess(SHA, TAG, release(), None)[0])

    def test_annotated_tag_requires_manual_review(self):
        tagged = ref()
        tagged["object"]["type"] = "tag"
        self.assertFalse(assess(SHA, TAG, None, tagged)[0])

    def test_malformed_metadata_is_blocked(self):
        for invalid in (release("branch"), {"tag_name": TAG, "draft": True}, {"tag_name": TAG}):
            self.assertFalse(assess(SHA, TAG, invalid, ref())[0])
        self.assertFalse(assess("xyz", TAG, None, None)[0])
        self.assertFalse(assess(SHA, "v0.1.16-rc.1", None, None)[0])


if __name__ == "__main__":
    unittest.main()
