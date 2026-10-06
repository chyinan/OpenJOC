# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import json
import pathlib
import tomllib
import unittest


WORKSPACE = pathlib.Path(__file__).resolve().parents[2]


class ReleaseVersionConsistencyTests(unittest.TestCase):
    def test_workspace_and_current_packages_share_release_version(self) -> None:
        manifest = tomllib.loads((WORKSPACE / 'Cargo.toml').read_text())
        version = manifest['workspace']['package']['version']
        members = [
            tomllib.loads((member / 'Cargo.toml').read_text())
            for pattern in manifest['workspace']['members']
            for member in WORKSPACE.glob(pattern)
        ]
        names = {member['package']['name'] for member in members}
        lock = tomllib.loads((WORKSPACE / 'Cargo.lock').read_text())
        locked = {package['name']: package['version'] for package in lock['package'] if package['name'] in names}
        self.assertEqual(locked, dict.fromkeys(names, version))
        for member in members:
            self.assertEqual(member['package']['version'], {'workspace': True})
            for section in ('dependencies', 'dev-dependencies', 'build-dependencies'):
                for name, dependency in member.get(section, {}).items():
                    if name in names and isinstance(dependency, dict) and 'path' in dependency:
                        self.assertEqual(dependency['version'], version)
        player = json.loads((WORKSPACE / 'packaging/player/PLAYER_PACKAGE_MANIFEST.json').read_text())
        self.assertEqual(player['openjoc']['version'], version)
        for name in ('package_lav_release.py', 'release_packaging.py'):
            self.assertIn(f'CANONICAL_RELEASE_VERSION = "{version}"', (WORKSPACE / 'scripts' / name).read_text())
        self.assertIn(f'default: v{version}', (WORKSPACE / '.github/workflows/lav-release.yml').read_text())
        self.assertIn(f'## [{version}]', (WORKSPACE / 'CHANGELOG.md').read_text())


if __name__ == '__main__':
    unittest.main()
