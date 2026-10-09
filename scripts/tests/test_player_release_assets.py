from __future__ import annotations

import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
PLAYER_MANIFEST = ROOT / "packaging" / "player" / "PLAYER_PACKAGE_MANIFEST.json"
PLAYER_WORKFLOW = ROOT / ".github" / "workflows" / "player-packaging.yml"


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

    def test_windows_fixture_environment_installs_cmp_provider(self) -> None:
        workflow = PLAYER_WORKFLOW.read_text(encoding="utf-8")

        self.assertIn("            diffutils\n", workflow)

    def test_player_harness_preserves_executable_paths_with_spaces_and_unicode(self) -> None:
        if os.name == "nt":
            self.skipTest("POSIX fake executable smoke is covered on Unix runners")
        shell = shutil.which("sh")
        if shell is None:
            self.skipTest("POSIX shell is unavailable")

        with tempfile.TemporaryDirectory(prefix="openjoc-harness-test-") as temporary_name:
            temporary = Path(temporary_name)
            package_root = temporary / "OpenJOC harness with spaces — 日本語"
            fixtures = package_root / "fixtures with spaces — 日本語"
            fixtures.mkdir(parents=True)
            for name in ("joc.single.ec3", "joc.multi.ec3", "joc.mp4", "ordinary.eac3"):
                (fixtures / name).touch()

            fake_mpv = package_root / "fake mpv executable"
            fake_mpv.write_text(
                "#!/bin/sh\n"
                'printf \'%s\\n\' "$*" >> "$VERIFY_TEST_LOG"\n'
                'if [ "$1" = "--no-config" ] && [ "$2" = "--ad=help" ]; then\n'
                "  printf 'libopenjoc (eac3)\\neac3 - test\\n'\n"
                "  exit 0\n"
                "fi\n"
                "exit 91\n",
                encoding="utf-8",
            )
            fake_mpv.chmod(0o755)
            call_log = temporary / "fake-mpv-calls.log"
            env = dict(os.environ, VERIFY_TEST_LOG=str(call_log))
            result = subprocess.run(
                [shell, str(ROOT / "integrations/mpv/verify-player.sh"), str(fake_mpv), str(fixtures)],
                env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                text=True, encoding="utf-8", errors="replace", check=False,
            )

            self.assertEqual(result.returncode, 91, result.stdout)
            calls = call_log.read_text(encoding="utf-8").splitlines()
            self.assertGreaterEqual(len(calls), 2)
            self.assertEqual(calls[0], "--no-config --ad=help")
            self.assertTrue(calls[1].startswith(str(fixtures / "ordinary.eac3")))

if __name__ == "__main__":
    unittest.main()
