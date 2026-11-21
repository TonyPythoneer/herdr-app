#!/usr/bin/env python3
"""Verify a macOS WebView bundle and prepare a version-pinned update manifest.

CI artifact creation is deliberately NOT a GitHub Release. Publishing latest.json
must remain blocked until real WKWebView + Chrome GUI E2E passes.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import plistlib
import re
import tarfile
import tomllib
from pathlib import Path, PurePosixPath

REPOSITORY = "TonyPythoneer/herdr-webview"
ROOT = "Herdr WebView.app"
TARGET = "macos-aarch64"
ASSET = "herdr-app-macos-aarch64.tar.gz"
VERSION = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+-webview\.[0-9]+$")


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def validate(archive: Path, dmg: Path, icon: Path, version: str) -> dict:
    if not VERSION.fullmatch(version):
        raise ValueError(f"not a fork-specific stable SemVer: {version!r}")
    if archive.name != ASSET:
        raise ValueError(f"unexpected updater archive name: {archive.name}")
    if not archive.is_file() or archive.stat().st_size == 0:
        raise ValueError("missing or empty updater tar.gz")
    if not dmg.is_file() or dmg.stat().st_size == 0:
        raise ValueError("missing or empty installer DMG")
    with tarfile.open(archive, "r:gz") as tar:
        members = {m.name: m for m in tar.getmembers()}
        for member in members.values():
            p = PurePosixPath(member.name)
            if p.is_absolute() or ".." in p.parts or p.parts[0] != ROOT:
                raise ValueError(f"unsafe or unexpected archive path: {member.name}")
            if member.issym() or member.islnk():
                target = PurePosixPath(member.linkname)
                if target.is_absolute() or ".." in target.parts:
                    raise ValueError(f"unsafe archive link: {member.name}")
        def read(path: str) -> bytes:
            name = f"{ROOT}/{path}"
            if name not in members or not members[name].isfile():
                raise ValueError(f"missing mandatory bundle file: {name}")
            extracted = tar.extractfile(members[name])
            if extracted is None:
                raise ValueError(f"cannot read bundle member: {name}")
            return extracted.read()
        plist = plistlib.loads(read("Contents/Info.plist"))
        if plist.get("CFBundleDisplayName") != "Herdr WebView":
            raise ValueError("incorrect visible app identity")
        if plist.get("CFBundleIdentifier") != "dev.herdr.webview":
            raise ValueError("wrong bundle ID (cannot overwrite official Herdr)")
        if plist.get("HerdrWebViewVersion") != version:
            raise ValueError("Info.plist fork release version differs from Cargo")
        core, build = version.split("-webview.", 1)
        if plist.get("CFBundleShortVersionString") != core or plist.get("CFBundleVersion") != f"{core}.{build}":
            raise ValueError("invalid Apple numeric bundle version")
        binary = read("Contents/MacOS/Herdr")
        if not binary:
            raise ValueError("empty desktop executable")
        for file in ("LICENSE", "THIRD-PARTY-NOTICES"):
            if not read(f"Contents/Resources/{file}"):
                raise ValueError(f"empty {file}")
        if read("Contents/Resources/Herdr.icns") != icon.read_bytes():
            raise ValueError("the original Herdr icon was changed")
    return {
        "version": version,
        "targets": {
            TARGET: {
                "url": f"https://github.com/{REPOSITORY}/releases/download/v{version}/{ASSET}",
                "sha256": sha256(archive),
            }
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--archive", required=True, type=Path)
    parser.add_argument("--dmg", required=True, type=Path)
    parser.add_argument("--original-icon", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--cargo", default="Cargo.toml", type=Path)
    args = parser.parse_args()
    version = tomllib.loads(args.cargo.read_text(encoding="utf-8"))["package"]["version"]
    manifest = validate(args.archive, args.dmg, args.original_icon, version)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"Verified fork release {version}, created {args.output}")


if __name__ == "__main__":
    main()
