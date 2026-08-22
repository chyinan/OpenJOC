from __future__ import annotations

import json
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
PLAYER_MANIFEST = ROOT / "packaging" / "player" / "PLAYER_PACKAGE_MANIFEST.json"
class PlayerReleaseAssetTests(unittest.TestCase):
    def test_player_manifest_uses_project_release_version_in_all_archive_names(self) -> None:
        cargo_text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
        cargo_version = next(
            line.split('"', 2)[1]
            for line in cargo_text.splitlines()
            if line.startswith('version = "')
        )
        manifest = json.loads(PLAYER_MANIFEST.read_text(encoding="utf-8"))

        self.assertEqual(manifest["openjoc"]["version"], cargo_version)
        for platform in manifest["platforms"].values():
            self.assertIn(f"openjoc-mpv-{cargo_version}-", platform["archive"])
            self.assertIn(f"openjoc-mpv-{cargo_version}-", platform["development_archive"])

if __name__ == "__main__":
    unittest.main()
