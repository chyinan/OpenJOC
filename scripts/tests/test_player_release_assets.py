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

    def test_windows_luajit_is_pinned_without_changing_abi_or_skipping_io(self) -> None:
        windows = PLAYER_WORKFLOW.read_text(encoding="utf-8").split("  windows-x64:", 1)[1]
        self.assertIn("msystem: MINGW64", windows)
        self.assertNotIn("            mingw-w64-x86_64-luajit\n", windows)
        self.assertNotIn("mingw-w64-ucrt-x86_64-luajit", windows)
        self.assertIn("package=mingw-w64-x86_64-luajit-2.1.1791988261-1-any.pkg.tar.zst", windows)
        self.assertIn("https://repo.msys2.org/mingw/mingw64/$package", windows)
        self.assertIn("d559881bffc105af4482eab52a34fa57963047cf45e735a5c0f8daca605c68a1", windows)
        stages = [
            "sha256sum --check --strict",
            "LocalFileSigLevel = Required TrustedOnly",
            'pacman --config "$package_dir/pacman.conf" --noconfirm -U --print "$package"',
            "for signature_case in missing invalid; do",
            'pacman --config "$package_dir/pacman.conf" --noconfirm -U "$package"',
            '/mingw64/bin/luajit.exe -e',
            "luajit integrations/mpv/test-openjoc-settings.lua",
            "luajit integrations/mpv/test-openjoc-settings-io.lua",
            "scripts/build-openjoc-player-windows.sh",
        ]
        positions = [windows.index(stage) for stage in stages]
        self.assertEqual(positions, sorted(positions))
        self.assertIn('assert(jit.os == "Windows" and jit.arch == "x64")', windows)
        self.assertIn("OpenJOC Settings 日本語", windows)
        self.assertIn('cat /etc/pacman.conf > "$package_dir/pacman.conf"', windows)
        self.assertNotIn('pacman-key --verify "$package.sig"', windows)
        self.assertNotIn("pacman-key --init", windows)
        self.assertNotIn("chmod", windows)

    @unittest.skipIf(os.name == "nt", "POSIX command doubles run on Unix runners")
    def test_windows_luajit_install_fails_closed(self) -> None:
        # Exercise the real provisioning shell up to checksum/signature failures.
        # Command doubles test control flow, not Windows libalpm cryptography.
        workflow = PLAYER_WORKFLOW.read_text(encoding="utf-8")
        step = workflow.split("      - name: Install pinned native MINGW64 LuaJIT\n", 1)[1]
        block = step.split("        run: |\n", 1)[1].split("\n      - name:", 1)[0]
        script = "\n".join(line[10:] for line in block.splitlines())
        if shutil.which("bash") is None:
            self.skipTest("bash is unavailable")
        with tempfile.TemporaryDirectory() as temporary_name:
            temporary = Path(temporary_name)
            tools = temporary / "tools"
            tools.mkdir()
            commands = {
                "curl": "exit 0\n",
                "sha256sum": 'cat >/dev/null\nexit "$CHECKSUM_STATUS"\n',
                "cat": 'if [ "$1" = /etc/pacman.conf ]; then printf "[options]\\nLocalFileSigLevel = Optional\\n"; else /bin/cat "$@"; fi\n',
                "pacman": 'printf "%s\\n" "$*" >> "$INSTALL_LOG"\ncp "$2" "$CONFIG_COPY"\nexit 41\n',
            }
            for name, body in commands.items():
                tool = tools / name
                tool.write_text("#!/bin/sh\n" + body, encoding="utf-8")
                tool.chmod(0o755)
            log = temporary / "install.log"
            config_copy = temporary / "effective.conf"
            for checksum_status in (19, 0):
                with self.subTest(checksum_status=checksum_status):
                    result = subprocess.run(
                        ["bash", "-c", script], text=True, capture_output=True,
                        env={**os.environ, "PATH": f"{tools}:{os.environ['PATH']}",
                             "CHECKSUM_STATUS": str(checksum_status),
                             "INSTALL_LOG": str(log), "CONFIG_COPY": str(config_copy)},
                    )
                    self.assertEqual(result.returncode, checksum_status or 41, result.stderr)
                    if checksum_status:
                        self.assertFalse(log.exists(), "must not install a checksum mismatch")
                    else:
                        self.assertEqual(len(log.read_text().splitlines()), 1)
                        self.assertTrue(config_copy.read_text().endswith(
                            "[options]\nLocalFileSigLevel = Required TrustedOnly\n"))

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
