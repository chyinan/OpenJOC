from pathlib import Path
import sys
import subprocess
import tempfile
import unittest
from unittest import mock


SCRIPT = Path(__file__).resolve().parents[1] / "verify_pcm_bitexact.py"
sys.path.insert(0, str(SCRIPT.parent))
import verify_pcm_bitexact as gate  # noqa: E402
import verify_partitioned_fft_scratch as partitioned_gate  # noqa: E402


class PcmBitexactGateTests(unittest.TestCase):
    def test_probe_binary_path_suffix_is_platform_aware(self):
        target = Path("/tmp/openjoc-target")
        self.assertEqual(
            gate.probe_binary_path(target, "pcm_regression_probe", platform="win32"),
            target / "release/examples/pcm_regression_probe.exe",
        )
        self.assertEqual(
            gate.probe_binary_path(target, "pcm_regression_probe", platform="linux"),
            target / "release/examples/pcm_regression_probe",
        )

    def test_toolchain_defaults_and_override_are_explicit(self):
        parser = gate.argument_parser()
        defaults = parser.parse_args([])
        override = parser.parse_args(["--toolchain", "1.98.1"])
        self.assertEqual(defaults.toolchain, "1.89.0")
        self.assertEqual(override.toolchain, "1.98.1")
        self.assertEqual(
            gate.toolchain_commands(override.toolchain),
            (["rustc", "+1.98.1", "-vV"], ["cargo", "+1.98.1", "-Vv"]),
        )

    def test_build_uses_selected_toolchain(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "repo"
            target = Path(temporary) / "target"
            logs = Path(temporary) / "logs"
            binary = gate.probe_binary_path(target, "pcm_regression_probe")
            binary.parent.mkdir(parents=True)
            binary.write_bytes(b"mock binary")
            with mock.patch.object(gate, "run_checked") as run_checked:
                result = gate.build_revision(
                    root,
                    target,
                    label="candidate",
                    log_dir=logs,
                    env={},
                    needs_api=True,
                    needs_core=False,
                    allocation_profile=False,
                    online=False,
                    toolchain="1.98.1",
                    verbose=True,
                )
            self.assertEqual(result["api"], binary)
            self.assertEqual(run_checked.call_args.args[0][:3], ["cargo", "+1.98.1", "build"])
            self.assertIn("--verbose", run_checked.call_args.args[0])

    def test_comparator_negative_sensitivity_suite(self):
        gate.run_self_tests()

    def test_f64_manifest_claim_fails_closed_if_stream_is_missing(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifests = [root / "base.tsv", root / "candidate.tsv"]
            pcm_files = [root / "base.pcm32le", root / "candidate.pcm32le"]
            manifest = gate._manifest_for_test(frames=[("programme", 48_000, 2, 0), ("drain", 48_000, 1, 2)])
            for path in manifests:
                path.write_bytes(manifest + b"R\tcore_f64_bytes\t48\n")
            for path in pcm_files:
                path.write_bytes(bytes(24))
            with self.assertRaisesRegex(gate.GateError, "f64 output stream/manifest is missing"):
                gate.compare_runs(manifests[0], pcm_files[0], manifests[1], pcm_files[1])

    def test_keep_handles_api_case_without_f64_artifact(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            prefixes = {name: root / f"case.{name}" for name in ("baseline", "candidate")}
            for prefix in prefixes.values():
                for suffix in (".manifest.tsv", ".pcm32le", ".timing.tsv"):
                    gate.artifact(prefix, suffix).write_bytes(b"present")
            destination = root / "retained"
            gate.retain_artifacts(prefixes, destination)
            for prefix in prefixes.values():
                for suffix in (".manifest.tsv", ".pcm32le", ".timing.tsv"):
                    self.assertTrue((destination / (prefix.name + suffix)).is_file())
                self.assertFalse((destination / (prefix.name + ".pcm64le")).exists())

    def _make_validator_worktrees(self, root: Path) -> tuple[Path, Path]:
        # Match the real gate's canonical-root contract, including Windows
        # temporary-directory aliases. Keep fixture manifests LF-exact rather
        # than letting platform text I/O or Git convert their bytes to CRLF.
        baseline = (root / "baseline").resolve()
        candidate = (root / "candidate").resolve()
        for repo in (baseline, candidate):
            (repo / "crates/openjoc-api/examples").mkdir(parents=True)
            (repo / "crates/openjoc-api/src").mkdir(parents=True)
            (repo / "crates/openjoc-eac3/examples").mkdir(parents=True)
            (repo / "crates/openjoc-eac3/src").mkdir(parents=True)
            (repo / "crates/openjoc-render/examples").mkdir(parents=True)
            (repo / "crates/openjoc-api/Cargo.toml").write_bytes(
                b'[package]\nname = "openjoc-api"\norientation-profile = ["openjoc-sofa/orientation-profile"]\n',
            )
            (repo / "crates/openjoc-eac3/Cargo.toml").write_bytes(
                b'[package]\nname = "openjoc-eac3"\nrust-version.workspace = true\n',
            )
            (repo / "crates/openjoc-api/src/lib.rs").write_text("// frozen API source\n", encoding="ascii")
            (repo / "crates/openjoc-eac3/src/lib.rs").write_text("// frozen core source\n", encoding="ascii")
            subprocess.run(["git", "init", "-q", str(repo)], check=True)
            subprocess.run(["git", "-C", str(repo), "config", "core.autocrlf", "false"], check=True)
            subprocess.run(["git", "-C", str(repo), "config", "user.email", "gate-test@example.invalid"], check=True)
            subprocess.run(["git", "-C", str(repo), "config", "user.name", "Gate Test"], check=True)
            subprocess.run(["git", "-C", str(repo), "add", "."], check=True)
            subprocess.run(["git", "-C", str(repo), "commit", "-qm", "fixture"], check=True)
            (repo / gate.PROBE).write_bytes(b"identical API probe harness\n")
            (repo / gate.CORE_PROBE).write_bytes(b"identical core probe harness\n")
            (repo / gate.PARTITIONED_PROBE).write_bytes(b"identical partitioned probe harness\n")
        return baseline, candidate

    def test_baseline_validator_allows_only_exact_harness_overlays(self):
        with tempfile.TemporaryDirectory() as temporary:
            baseline, candidate = self._make_validator_worktrees(Path(temporary))
            api_manifest = baseline / "crates/openjoc-api/Cargo.toml"
            api_manifest.write_bytes(gate._expected_feature_manifest(api_manifest.read_bytes(), "openjoc-api"))
            core_manifest = baseline / "crates/openjoc-eac3/Cargo.toml"
            core_manifest.write_bytes(gate._expected_feature_manifest(core_manifest.read_bytes(), "openjoc-eac3"))
            gate.validate_baseline_worktree(baseline, candidate)
            (baseline / "crates/openjoc-api/src/extra.rs").write_text("// product change\n", encoding="ascii")
            with self.assertRaisesRegex(gate.GateError, "untracked non-harness"):
                gate.validate_baseline_worktree(baseline, candidate)

    def test_baseline_validator_rejects_tracked_product_changes_and_overlapping_roots(self):
        with tempfile.TemporaryDirectory() as temporary:
            baseline, candidate = self._make_validator_worktrees(Path(temporary))
            source = baseline / "crates/openjoc-api/src/lib.rs"
            source.write_text("// altered product source\n", encoding="ascii")
            with self.assertRaisesRegex(gate.GateError, "outside allowlisted feature manifests"):
                gate.validate_baseline_worktree(baseline, candidate)
            with self.assertRaisesRegex(gate.GateError, "distinct, non-overlapping"):
                gate.validate_baseline_worktree(baseline, baseline)

    def test_partitioned_pcm_comparator_rejects_sample_bit_and_signed_zero_mutations(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            baseline = root / "baseline.pcm64le"
            candidate = root / "candidate.pcm64le"
            positive_zero = bytes.fromhex("0000000000000000")
            positive_one = bytes.fromhex("000000000000f03f")
            control = positive_zero + positive_one
            baseline.write_bytes(control)
            candidate.write_bytes(control)
            partitioned_gate.compare_pcm_streams(baseline, candidate, len(control))

            candidate.write_bytes(bytes.fromhex("0000000000000080") + positive_one)
            with self.assertRaisesRegex(gate.GateError, "partitioned PCM bit mismatch"):
                partitioned_gate.compare_pcm_streams(baseline, candidate, len(control))

            candidate.write_bytes(positive_zero + bytes.fromhex("010000000000f03f"))
            with self.assertRaisesRegex(gate.GateError, "partitioned PCM bit mismatch"):
                partitioned_gate.compare_pcm_streams(baseline, candidate, len(control))

            candidate.write_bytes(control[:-1])
            with self.assertRaisesRegex(gate.GateError, "PCM byte length"):
                partitioned_gate.compare_pcm_streams(baseline, candidate, len(control))


if __name__ == "__main__":
    unittest.main()
