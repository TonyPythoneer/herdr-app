import io
import plistlib
import tarfile
import tempfile
import unittest
from pathlib import Path
from webview_release_manifest import validate

VERSION = "0.1.16-webview.1"


class ManifestTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.home = Path(self.tmp.name)
        self.archive = self.home / "herdr-app-macos-aarch64.tar.gz"
        self.dmg = self.home / "Herdr-Browser-macos-arm64.dmg"
        self.icon = self.home / "Herdr.icns"
        self.icon.write_bytes(b"unmodified-upstream-icon")
        self.dmg.write_bytes(b"dmg bytes")
        self.make_tar()

    def make_tar(self, *, bundle_id="dev.herdr.webview", version=VERSION, icon=None):
        plist = plistlib.dumps({
            "CFBundleDisplayName":"Herdr WebView",
            "CFBundleIdentifier":bundle_id,
            "HerdrWebViewVersion":version,
            "CFBundleShortVersionString":"0.1.16",
            "CFBundleVersion":"0.1.16001",
        })
        files = {
            "Contents/Info.plist":plist,
            "Contents/MacOS/Herdr":b"fake binary",
            "Contents/Resources/Herdr.icns":self.icon.read_bytes() if icon is None else icon,
            "Contents/Resources/LICENSE":b"license",
            "Contents/Resources/THIRD-PARTY-NOTICES":b"notices",
        }
        with tarfile.open(self.archive, "w:gz") as tar:
            for name, data in files.items():
                entry = tarfile.TarInfo("Herdr WebView.app/"+name)
                entry.size = len(data)
                tar.addfile(entry, io.BytesIO(data))

    def test_valid_bundle_generates_expected_own_github_release(self):
        manifest=validate(self.archive,self.dmg,self.icon,VERSION)
        self.assertEqual(manifest["version"],VERSION)
        target=manifest["targets"]["macos-aarch64"]
        self.assertIn("TonyPythoneer/herdr-webview/releases/download/v"+VERSION,target["url"])
        self.assertEqual(len(target["sha256"]),64)

    def test_rejects_official_herdr_bundle_id(self):
        self.make_tar(bundle_id="dev.herdr.app")
        with self.assertRaisesRegex(ValueError,"bundle ID"):
            validate(self.archive,self.dmg,self.icon,VERSION)

    def test_rejects_icon_change_and_inconsistent_version(self):
        self.make_tar(icon=b"new-icon")
        with self.assertRaisesRegex(ValueError,"original Herdr icon"):
            validate(self.archive,self.dmg,self.icon,VERSION)
        self.make_tar(version="0.1.16-webview.2")
        with self.assertRaisesRegex(ValueError,"release version"):
            validate(self.archive,self.dmg,self.icon,VERSION)

    def test_rejects_non_fork_version_and_missing_dmg(self):
        with self.assertRaisesRegex(ValueError,"fork-specific stable SemVer"):
            validate(self.archive,self.dmg,self.icon,"0.1.16")
        self.dmg.unlink()
        with self.assertRaisesRegex(ValueError,"missing or empty installer"):
            validate(self.archive,self.dmg,self.icon,VERSION)


if __name__ == "__main__":
    unittest.main()
