from __future__ import annotations

import hashlib
import json
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MENU = ROOT / "integrations/mpv/openjoc-settings.lua"
MENU_TEST = ROOT / "integrations/mpv/test-openjoc-settings.lua"
FFMPEG_PATCH = ROOT / "integrations/ffmpeg/native/patches/0001-avcodec-add-experimental-libopenjoc-decoder-wrapper.patch"
MANIFEST = ROOT / "packaging/player/PLAYER_PACKAGE_MANIFEST.json"


class MpvSettingsMenuTests(unittest.TestCase):
    def test_manifest_and_patch_pin_the_cabi_16_hrtf_option(self) -> None:
        manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
        ffmpeg = manifest["pinned_stack"]["ffmpeg"]
        patch_sha = hashlib.sha256(FFMPEG_PATCH.read_bytes()).hexdigest()
        baseline = dict(
            line.split("=", 1)
            for line in (ROOT / "integrations/ffmpeg/native/BASELINES").read_text().splitlines()
            if "=" in line
        )

        self.assertEqual(ffmpeg["patch_sha256"], patch_sha)
        self.assertEqual(baseline["PATCH_SHA256"], patch_sha)
        self.assertEqual(ffmpeg["minimum_openjoc_c_abi"], "1.6")
        self.assertEqual(baseline["OPENJOC_C_ABI"], "1.6-experimental")
        self.assertEqual(manifest["openjoc"]["native_ffmpeg_minimum_c_abi"]["minor"], 6)
        patch = FFMPEG_PATCH.read_text(encoding="utf-8")
        self.assertIn("openjoc_decoder_config_init_v1_6", patch)
        self.assertIn('OFFSET(hrtf_preset)', patch)
        self.assertIn("OPENJOC_HRTF_SADIE_D2_KEMAR", patch)
        self.assertIn("s->render_mode == OPENJOC_RENDER_BINAURAL && s->sofa", patch)

    def test_menu_contains_lav_controls_and_uses_per_file_decoder_options(self) -> None:
        source = MENU.read_text(encoding="utf-8")
        for label in (
            "Stereo (Speakers)", "Binaural (Headphones)", "5.1", "7.1",
            "5.1.2", "5.1.4", "7.1.2", "7.1.4", "Calibrated",
            "Unity / Compatibility", "SADIE II D1 / KU100",
            "SADIE II D2 / KEMAR", "Custom SOFA", "9.1.6",
        ):
            self.assertIn(label, source)
        self.assertIn("file-local-options/ad-lavc-o", source)
        self.assertNotIn("file-local-options/ad-lavc-o-append", source)
        self.assertNotIn("file-local-options/audio-channels", source)
        self.assertIn("track.codec == 'eac3'", source)
        self.assertIn("mp.set_property_native", source)
        self.assertNotIn("mp.commandv('seek'", source)
        self.assertNotIn("loadfile", source)
        self.assertIn("Live rows are mpv properties, not JOC diagnostics", source)
        self.assertIn("mp.add_hook('on_preloaded'", source)
        self.assertIn("OpenJOC saved decoder options applied for an E-AC-3 file", source)
        self.assertIn("LAV output gain", source)
        self.assertIn("Best-effort E-AC-3 options; mpv audio routing is unchanged", source)
        self.assertTrue(MENU_TEST.is_file())
        self.assertTrue((ROOT / "scripts/tests/render_mpv_settings_preview.py").is_file())

    def test_settings_panel_has_mouse_keyboard_and_resize_interaction(self) -> None:
        source = MENU.read_text(encoding="utf-8")
        self.assertIn("mp.create_osd_overlay('ass-events')", source)
        self.assertIn("MBTN_LEFT", source)
        self.assertIn("mp.get_mouse_pos()", source)
        self.assertIn("osd-dimensions", source)
        self.assertIn("Shift+TAB", source)
        self.assertIn("Set path...", source)
        self.assertIn("overlay.res_x", source)
        self.assertIn("overlay.res_y", source)
        self.assertNotIn("file-local-options/audio-channels", source)
        self.assertNotIn("gain slider", source.lower())

    def test_bundle_copies_menu_and_ci_installs_lua_runtime(self) -> None:
        packager = (ROOT / "scripts/player-package.py").read_text(encoding="utf-8")
        self.assertIn("bin/portable_config/scripts/openjoc-settings.lua", packager)
        self.assertIn('"bin/portable_config/scripts/openjoc-settings.lua"', packager)
        self.assertIn(r'--config-dir=%OPENJOC_PLAYER_ROOT%\\bin\\portable_config', packager)
        self.assertIn('--config-dir=$here/bin/portable_config', packager)
        self.assertIn('menu_executable = executable if arguments.platform == "windows-x64"', packager)
        self.assertIn('mpv.exe directly with no --config-dir', packager)
        self.assertIn('"bin/portable_config"', packager)
        workflow = (ROOT / ".github/workflows/player-packaging.yml").read_text(encoding="utf-8")
        self.assertIn("luajit integrations/mpv/test-openjoc-settings.lua", workflow)
        self.assertIn("luajit\n", workflow)
        self.assertIn("libluajit-5.1-dev", workflow)
        self.assertIn("mingw-w64-x86_64-luajit", workflow)

    def test_packaging_preflights_and_smokes_luajit_menu_runtime(self) -> None:
        build_scripts = [
            (ROOT / "scripts/build-openjoc-player.sh").read_text(encoding="utf-8"),
            (ROOT / "scripts/build-openjoc-player-windows.sh").read_text(encoding="utf-8"),
        ]
        for script in build_scripts:
            self.assertIn("pkg-config --exists luajit", script)
            self.assertIn("-Dlua=luajit", script)

        package_script = (ROOT / "scripts/player-package.py").read_text(encoding="utf-8")
        self.assertIn('"-Dlua=luajit"', package_script)
        self.assertIn('"--idle=yes"', package_script)
        self.assertIn('"--load-scripts=yes"', package_script)
        self.assertIn('fixture_scripts.mkdir()', package_script)
        self.assertNotIn('f"--script={menu_script}"', package_script)
        self.assertNotIn('"--no-config"', package_script)
        self.assertIn('f"--config-dir={fixture_config}"', package_script)
        self.assertNotIn('f"--config-dir={temporary}"', package_script)
        self.assertIn('"--vo=null"', package_script)
        self.assertIn('"--ao=null"', package_script)
        self.assertIn("OPENJOC_SETTINGS_IDLE_SMOKE_QUIT", package_script)
        self.assertIn("saved decoder options applied for an E-AC-3 file", package_script)
        qualifier = (ROOT / "scripts/qualify-player-package.py").read_text(encoding="utf-8")
        self.assertIn('"extracted bundle with spaces — 日本語"', qualifier)
        self.assertIn('env.pop("MPV_HOME", None)', qualifier)
        self.assertIn("DIRECT_GUI_CONFIG_AUTOLOAD", qualifier)
        self.assertIn("and direct_gui_config_ok", qualifier)
        self.assertIn('verifier.extend(["--fixture", str(fixtures / "joc.single.ec3")])', qualifier)


if __name__ == "__main__":
    unittest.main()
