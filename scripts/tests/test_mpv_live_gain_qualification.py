from __future__ import annotations

import importlib.util
import json
import math
from pathlib import Path
import struct
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
CHECKER_PATH = ROOT / "scripts" / "compare-player-gain-pcm.py"
VERIFY_PATH = ROOT / "integrations" / "mpv" / "verify-player.sh"
DRIVER_PATH = ROOT / "integrations" / "mpv" / "test-openjoc-live-gain-driver.lua"
MANIFEST_PATH = ROOT / "packaging" / "player" / "PLAYER_PACKAGE_MANIFEST.json"
QUALIFIER_PATH = ROOT / "scripts" / "qualify-player-package.py"


def load_checker():
    spec = importlib.util.spec_from_file_location("gain_pcm_checker", CHECKER_PATH)
    if spec is None or spec.loader is None:
        raise AssertionError("could not load gain PCM checker")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_qualifier():
    spec = importlib.util.spec_from_file_location("player_qualifier", QUALIFIER_PATH)
    if spec is None or spec.loader is None:
        raise AssertionError("could not load player package qualifier")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def float_wave(samples: list[float], channels: int = 2, rate: int = 48000) -> bytes:
    payload = struct.pack("<" + "f" * len(samples), *samples)
    fmt = struct.pack("<HHIIHH", 3, channels, rate, rate * channels * 4, channels * 4, 32)
    chunks = b"fmt " + struct.pack("<I", len(fmt)) + fmt
    chunks += b"data" + struct.pack("<I", len(payload)) + payload
    return b"RIFF" + struct.pack("<I", len(chunks) + 4) + b"WAVE" + chunks


class MpvLiveGainQualificationTests(unittest.TestCase):
    def test_exact_pcm_checker_compares_shape_and_complete_samples(self) -> None:
        checker = load_checker()
        samples = [((index % 31) - 15) / 100.0 for index in range(400)]
        baseline = float_wave(samples)
        with tempfile.TemporaryDirectory() as temporary_name:
            temporary = Path(temporary_name)
            source = temporary / "source.wav"
            unity = temporary / "unity.wav"
            source.write_bytes(baseline)
            unity.write_bytes(baseline)
            checker.exact(source, unity, channels=2, rate=48000)
            unity.write_bytes(float_wave(samples[:-2]))
            with self.assertRaisesRegex(ValueError, "bytes or sample count"):
                checker.exact(source, unity, channels=2, rate=48000)

    def test_float_pcm_checker_validates_representative_lav_gain_samples(self) -> None:
        checker = load_checker()
        source_samples = [((index % 53) - 26) / 150.0 for index in range(800)]
        with tempfile.TemporaryDirectory() as temporary_name:
            temporary = Path(temporary_name)
            baseline = temporary / "baseline.wav"
            baseline.write_bytes(float_wave(source_samples))
            for tenths_db in (-200, -60, -1, 1, 60, 200):
                factor = math.pow(10.0, tenths_db / 200.0)
                f32_factor = struct.unpack("<f", struct.pack("<f", factor))[0]
                f32_sources = [
                    struct.unpack("<f", struct.pack("<f", sample))[0]
                    for sample in source_samples
                ]
                output = temporary / f"gain-{tenths_db}.wav"
                output.write_bytes(float_wave([
                    struct.unpack("<f", struct.pack("<f", sample * f32_factor))[0]
                    for sample in f32_sources
                ]))
                checker.gain(baseline, output, tenths_db, channels=2, rate=48000)

    def test_all_401_gain_steps_are_monotone_and_unity_is_exact(self) -> None:
        factors = [math.pow(10.0, value / 200.0) for value in range(-200, 201)]
        self.assertEqual(factors[200], 1.0)
        self.assertTrue(all(math.isfinite(value) and value > 0.0 for value in factors))
        self.assertTrue(all(left < right for left, right in zip(factors, factors[1:])))

    def test_qualifier_requires_live_gain_evidence_on_every_platform(self) -> None:
        manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
        required = {
            "GAIN_PCM_BITEXACT", "GAIN_SAMPLES", "LIVE_GAIN_NO_RESTART",
            "APPLYCURRENT_REINIT",
        }
        self.assertTrue(required.issubset(manifest["qualification"]["status_fields"]))
        qualifier = load_qualifier()
        self.assertTrue(required.issubset(qualifier.FIELDS))

    def test_missing_gain_markers_fail_the_status_contract_closed(self) -> None:
        qualifier = load_qualifier()
        statuses = {field: "PASS" for field in qualifier.GAIN_HARNESS_MARKERS}
        missing = qualifier.apply_gain_harness_statuses(statuses, "JOC:PASS\\nEOS:PASS")
        self.assertEqual(set(missing), set(qualifier.GAIN_HARNESS_MARKERS.values()))
        self.assertEqual(set(statuses.values()), {"FAIL"})

        complete_output = "\\n".join(qualifier.GAIN_HARNESS_MARKERS.values())
        statuses = {field: "FAIL" for field in qualifier.GAIN_HARNESS_MARKERS}
        self.assertEqual(qualifier.apply_gain_harness_statuses(statuses, complete_output), [])
        self.assertEqual(set(statuses.values()), {"PASS"})

    def test_runtime_harness_checks_no_restart_then_exactly_one_apply_reopen(self) -> None:
        source = VERIFY_PATH.read_text(encoding="utf-8")
        self.assertIn("runtime_opens != 0", source)
        self.assertIn("apply_opens != 1", source)
        self.assertIn(r'command=\"volume\", argument=\"', source)
        self.assertIn(r"volume_dB:0\.100000", source)
        self.assertIn("LIVE_GAIN_NO_RESTART:PASS", source)
        self.assertIn("APPLYCURRENT_REINIT:PASS", source)
        driver = DRIVER_PATH.read_text(encoding="utf-8")
        self.assertIn("LIVE_GAIN_PHASE:PRE_RUNTIME", driver)
        self.assertIn("LIVE_GAIN_PHASE:PRE_APPLYCURRENT", driver)
        self.assertIn("string.format('%.17g', math.pow(10.0, 1 / 200.0))", driver)
        self.assertIn("gain_filter_exists()", driver)
        self.assertIn("output_channels=8 samplerate=48000 gain=", driver)

    def test_full_pipeline_unity_qualification_covers_required_modes(self) -> None:
        source = VERIFY_PATH.read_text(encoding="utf-8")
        self.assertIn("'render_mode=binaural,hrtf=d1,virtual_layout=7.1.4' none", source)
        self.assertIn("'render_mode=binaural,hrtf=d1,virtual_layout=7.1.4' 0", source)
        self.assertIn("'render_mode=binaural,hrtf=d2,virtual_layout=7.1.4' none", source)
        self.assertIn("'render_mode=binaural,hrtf=d2,virtual_layout=7.1.4' 0", source)
        self.assertIn("'render_mode=speaker,speaker_layout=2.0' none", source)
        self.assertIn("'render_mode=speaker,speaker_layout=7.1.4' none", source)
        self.assertIn(":fix-pts=yes", source)
        self.assertIn("--end=4", source)
        self.assertIn("GAIN_CUTOFF_PCM_BITEXACT:PASS", source)
        self.assertIn("GAIN_PCM_BITEXACT:PASS", source)
        self.assertIn("for gain_tenths in -200 -60 -1 1 60 200", source)
        checker = CHECKER_PATH.read_text(encoding="utf-8")
        self.assertIn("f32_factor = struct.unpack", checker)
        self.assertIn("candidate_data[index * 4:index * 4 + 4]", checker)
        self.assertIn("PCM_GAIN_F32:PASS", checker)
        self.assertIn("--tenths-db \"$gain_tenths\"", source)


if __name__ == "__main__":
    unittest.main()
