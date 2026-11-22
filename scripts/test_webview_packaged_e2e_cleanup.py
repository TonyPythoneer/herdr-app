"""Safety regression for the packaged browser E2E process cleanup."""
import unittest
from pathlib import Path
from webview_packaged_e2e import isolated_chrome_processes


class ChromeCleanupTests(unittest.TestCase):
    def test_only_private_profile_chrome_and_descendants(self):
        profile = Path("/tmp/hv-e2e-abc/herdr-home/chrome-profile")
        listing = "\n".join([
            "100 1 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome --headless=new --remote-debugging-pipe --user-data-dir=/tmp/hv-e2e-abc/herdr-home/chrome-profile",
            "101 100 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome Helper --type=renderer",
            "102 101 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome Helper --type=utility",
            "200 1 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome --user-data-dir=/Users/person/Library/Application Support/Google/Chrome",
            "300 1 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome --headless=new --remote-debugging-pipe --user-data-dir=/tmp/hv-e2e-abc/herdr-home/chrome-profile-extra",
            "400 1 python3 my_test.py --user-data-dir=/tmp/hv-e2e-abc/herdr-home/chrome-profile",
        ])
        self.assertEqual(isolated_chrome_processes(listing, profile), [101, 102, 100])

    def test_requires_all_exact_cdp_arguments(self):
        profile = Path("/tmp/isolated/chrome-profile")
        listing = "\n".join([
            "100 1 chrome --remote-debugging-pipe --user-data-dir=/tmp/isolated/chrome-profile",
            "101 1 chrome --headless=new --user-data-dir=/tmp/isolated/chrome-profile",
            "102 1 chrome --headless=new --remote-debugging-pipe --user-data-dir=/tmp/isolated/chrome-profile",
        ])
        self.assertEqual(isolated_chrome_processes(listing, profile), [102])

    def test_empty_listing_never_kills_anything(self):
        self.assertEqual(isolated_chrome_processes("", Path("/tmp/isolated/chrome-profile")), [])


if __name__ == "__main__":
    unittest.main()
