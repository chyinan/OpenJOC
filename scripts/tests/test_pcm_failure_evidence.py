"""Failure diagnostics regressions; no Git writes, builds, or external processes."""

from contextlib import ExitStack, redirect_stderr, redirect_stdout
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import verify_pcm_bitexact as gate


class PcmFailureEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="openjoc-evidence-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.output = self.root / "output"
        self.fixture = self.root / "fixture.ec3"
        self.fixture.write_bytes(b"input")
        self.input_hash = gate.hash_file(self.fixture)
        self.manifest = gate._manifest_for_test(
            frames=[("programme", 48_000, 2, 0), ("drain", 48_000, 1, 2)]
        ).replace(b"a" * 64, self.input_hash.encode("ascii"))
        self.pcm = bytes(range(24))
        self.args = []
        for flag in ("baseline-root", "candidate-root", "baseline-target", "candidate-target"):
            path = self.root / flag
            path.mkdir()
            self.args.extend([f"--{flag}", str(path)])
            if flag.endswith("-root"):
                probe = path / gate.PROBE
                probe.parent.mkdir(parents=True)
                probe.write_bytes(b"identical harness")
                (path / "Cargo.lock").write_bytes(b"lock")
        self.args.extend(["--output-dir", str(self.output), "--case",
                          f"../../case,{self.fixture},speaker,2.0"])
        self.stack = self.enterContext(ExitStack())
        self.stack.enter_context(redirect_stdout(io.StringIO()))
        self.stderr = self.stack.enter_context(redirect_stderr(io.StringIO()))
        self.stack.enter_context(mock.patch.object(gate, "git_head", return_value=gate.BASELINE_REVISION))
        self.stack.enter_context(mock.patch.object(gate, "validate_baseline_worktree"))
        self.stack.enter_context(mock.patch.object(gate, "validate_versioned_dependencies"))
        real_hash = gate.hash_file
        self.stack.enter_context(mock.patch.object(
            gate, "hash_file", side_effect=lambda path: gate.BASELINE_LOCK_SHA256
            if path.name == "Cargo.lock" else real_hash(path)))
        def process_output(command, **kwargs):
            if command[0] != "git":
                return "mock toolchain\n"
            if "--cached" in command:
                return b"Cargo.lock\0" + gate.PROBE.encode("ascii") + b"\0"
            return b""
        self.stack.enter_context(mock.patch.object(gate.subprocess, "check_output", side_effect=process_output))
        # Fail closed if a regression unexpectedly tries to launch anything.
        self.stack.enter_context(mock.patch.object(gate.subprocess, "run", side_effect=AssertionError("unexpected process")))
        self.build = self.stack.enter_context(mock.patch.object(gate, "build_revision", side_effect=self.build_probe))
        self.probe = self.stack.enter_context(mock.patch.object(gate, "run_probe", side_effect=self.write_probe))

    def build_probe(self, root, target, **kwargs):
        binary = target / "mock-probe"
        binary.write_bytes(b"mock binary")
        return {"api": binary}

    def write_probe(self, binary, case, prefix, **kwargs):
        gate.artifact(prefix, ".manifest.tsv").write_bytes(self.manifest)
        gate.artifact(prefix, ".pcm32le").write_bytes(self.pcm)
        gate.artifact(prefix, ".timing.tsv").write_bytes(b"timing\nSUMMARY\twall_ns\t1\n")
        return 1

    def read_report(self):
        reports = list(self.output.glob("evidence-*/report.json"))
        self.assertEqual(len(reports), 1)
        self.evidence = reports[0].parent
        return json.loads(reports[0].read_text(encoding="utf-8"))

    def assert_scratch_removed(self):
        self.assertEqual(list(self.output.glob("openjoc-pcm-pair-*")), [])

    def test_success_has_small_report_and_provenance_without_pcm(self):
        self.assertEqual(gate.main(self.args), 0)
        report = self.read_report()
        self.assertEqual(report["status"], "passed")
        self.assertEqual(len(report["completed_pairs"]), 1)
        self.assertEqual([item["path"] for item in report["artifacts"]], ["toolchain.txt"])
        self.assertEqual(report["sources"]["candidate"]["tracked_files"], 2)
        self.assertEqual(list(self.evidence.glob("*.pcm*")), [])
        provenance = (self.output / "toolchain.txt").read_text()
        for expected in ("mock toolchain", "baseline_head=", "candidate_head=", "candidate_lock_sha256=",
                         "api_probe_sha256=", "baseline_api_probe_binary_sha256=", "rustflags="):
            self.assertIn(expected, provenance)
        self.assertTrue((self.output / "measurements.csv").is_file())
        self.assert_scratch_removed()

    def test_success_keep_still_keeps_full_streams(self):
        self.assertEqual(gate.main(self.args + ["--keep"]), 0)
        self.assertEqual(len(list(self.output.glob("00-*/*.pcm32le"))), 2)
        for path in self.output.glob("00-*/*.pcm32le"):
            self.assertEqual(path.read_bytes(), self.pcm)

    def test_explicit_keep_failure_preserves_existing_failure_semantics(self):
        original = OSError("requested full retention failed")
        with mock.patch.object(gate, "retain_artifacts", side_effect=original):
            with self.assertRaises(OSError) as caught:
                gate.main(self.args + ["--keep"])
        self.assertIs(caught.exception, original)
        self.assertEqual(self.read_report()["error"], str(original))

    def test_comparison_failure_keeps_exact_error_and_both_manifests(self):
        def mutate(*args, **kwargs):
            elapsed = self.write_probe(*args, **kwargs)
            prefix = args[2]
            if prefix.name.endswith("candidate"):
                gate.artifact(prefix, ".pcm32le").write_bytes(self.pcm[:-1] + b"x")
            return elapsed
        self.probe.side_effect = mutate
        with self.assertRaises(gate.GateError) as caught:
            gate.main(self.args + ["--failure-pcm-bytes", "16"])
        report = self.read_report()
        self.assertEqual(report["error"], str(caught.exception))
        self.assertIn("PCM bit mismatch at byte 23", report["error"])
        self.assertEqual(report["stage"], "comparison")
        for variant in ("baseline", "candidate"):
            self.assertEqual((self.evidence / f"{variant}.manifest.tsv").read_bytes(), self.manifest)
            self.assertTrue((self.evidence / f"{variant}.timing.tsv").is_file())
        samples = [item for item in report["artifacts"] if item["path"].endswith(".pcm32le")]
        self.assertEqual(sum(item["retained_bytes"] for item in samples), 16)
        self.assertTrue(all(item["offset_bytes"] <= 23 < item["offset_bytes"] + item["retained_bytes"] for item in samples))
        self.assertNotEqual((self.evidence / "baseline.pcm32le").read_bytes(),
                            (self.evidence / "candidate.pcm32le").read_bytes())
        self.assert_scratch_removed()

    def test_partial_probe_failure_preserves_only_existing_files(self):
        original = gate.GateError("probe failed for case (9): original process stderr")
        def fail_probe(*args, **kwargs):
            prefix = args[2]
            if prefix.name.endswith("candidate"):
                gate.artifact(prefix, ".manifest.tsv").write_bytes(b"partial manifest\n")
                gate.artifact(prefix, ".pcm32le").write_bytes(b"partial PCM")
                raise original
            return self.write_probe(*args, **kwargs)
        self.probe.side_effect = fail_probe
        with self.assertRaises(gate.GateError) as caught:
            gate.main(self.args)
        self.assertIs(caught.exception, original)
        report = self.read_report()
        self.assertEqual(report["stage"], "probe-candidate")
        self.assertEqual((self.evidence / "baseline.manifest.tsv").read_bytes(), self.manifest)
        self.assertEqual((self.evidence / "candidate.manifest.tsv").read_bytes(), b"partial manifest\n")
        self.assertFalse((self.evidence / "candidate.timing.tsv").exists())
        self.assertTrue((self.output / "toolchain.txt").is_file())
        self.assert_scratch_removed()

    def test_first_probe_failure_does_not_invent_second_variant(self):
        def fail_probe(*args, **kwargs):
            self.write_probe(*args, **kwargs)
            raise gate.GateError("first probe failed")
        self.probe.side_effect = fail_probe
        with self.assertRaisesRegex(gate.GateError, "first probe failed"):
            gate.main(self.args)
        self.read_report()
        self.assertTrue((self.evidence / "baseline.manifest.tsv").exists())
        self.assertFalse((self.evidence / "candidate.manifest.tsv").exists())

    def test_interrupt_preserves_partial_outputs_without_changing_exception(self):
        original = KeyboardInterrupt()
        def interrupt_probe(*args, **kwargs):
            self.write_probe(*args, **kwargs)
            raise original
        self.probe.side_effect = interrupt_probe
        with self.assertRaises(KeyboardInterrupt) as caught:
            gate.main(self.args)
        self.assertIs(caught.exception, original)
        self.assertEqual(self.read_report()["error_type"], "KeyboardInterrupt")
        self.assertTrue((self.evidence / "baseline.manifest.tsv").exists())
        self.assert_scratch_removed()

    def test_retention_errors_do_not_replace_original_failure(self):
        original = gate.GateError("original comparator first difference")
        with mock.patch.object(gate, "compare_runs", side_effect=original), \
                mock.patch.object(gate.RunEvidence, "copy_artifact", side_effect=OSError("disk full")):
            with self.assertRaises(gate.GateError) as caught:
                gate.main(self.args)
        self.assertIs(caught.exception, original)
        report = self.read_report()
        self.assertEqual(report["error"], str(original))
        self.assertTrue(report["retention_errors"])
        self.assertIn("disk full", self.stderr.getvalue())
        self.assert_scratch_removed()

    def test_report_write_failure_does_not_change_success_or_failure(self):
        with mock.patch.object(gate.json, "dump", side_effect=OSError("report disk full")):
            self.assertEqual(gate.main(self.args), 0)
            original = gate.GateError("original probe failure")
            self.probe.side_effect = original
            with self.assertRaises(gate.GateError) as caught:
                gate.main(self.args)
            self.assertIs(caught.exception, original)
        self.assertIn("report disk full", self.stderr.getvalue())

    def test_evidence_destination_open_failure_never_changes_gate_outcome(self):
        original_open = Path.open
        def deny_evidence(path, *args, **kwargs):
            if path.parent.name.startswith("evidence-"):
                raise OSError("evidence destination is unwritable")
            return original_open(path, *args, **kwargs)
        with mock.patch.object(Path, "open", deny_evidence):
            self.assertEqual(gate.main(self.args), 0)
            original = gate.GateError("original comparator failure")
            with mock.patch.object(gate, "compare_runs", side_effect=original):
                with self.assertRaises(gate.GateError) as caught:
                    gate.main(self.args)
            self.assertIs(caught.exception, original)
        self.assertIn("evidence destination is unwritable", self.stderr.getvalue())
        self.assertTrue((self.output / "toolchain.txt").is_file())

    def test_source_provenance_failure_is_diagnostic_only(self):
        with mock.patch.object(gate, "source_provenance", side_effect=OSError("cannot inspect source")):
            self.assertEqual(gate.main(self.args), 0)
        report = self.read_report()
        self.assertEqual(report["status"], "passed")
        self.assertEqual(report["sources"], {})
        self.assertIn("cannot inspect source", " ".join(report["retention_errors"]))

    def test_build_failure_has_selected_toolchain_and_source_provenance(self):
        original = gate.GateError("original build failure")
        self.build.side_effect = original
        with self.assertRaises(gate.GateError) as caught:
            gate.main(self.args)
        self.assertIs(caught.exception, original)
        report = self.read_report()
        self.assertEqual(report["stage"], "build-baseline")
        self.assertEqual(report["context"]["baseline_head"], gate.BASELINE_REVISION)
        self.assertIn("mock toolchain", (self.output / "toolchain.txt").read_text())
        self.probe.assert_not_called()

    def test_failure_budget_zero_and_hard_limit(self):
        for budget in ("-1", str(gate.MAX_FAILURE_PCM_BYTES + 1)):
            with self.assertRaises(SystemExit) as caught:
                gate.main(self.args + ["--failure-pcm-bytes", budget])
            self.assertEqual(caught.exception.code, 2)
        self.build.assert_not_called()
        with mock.patch.object(gate, "compare_runs", side_effect=gate.GateError("difference")):
            with self.assertRaises(gate.GateError):
                gate.main(self.args + ["--failure-pcm-bytes", "0"])
        self.read_report()
        self.assertEqual(list(self.evidence.glob("*.pcm*")), [])
        self.assertEqual(len(list(self.evidence.glob("*.manifest.tsv"))), 2)

    def test_repeated_pair_failure_retains_current_ba_order(self):
        calls = 0
        def fail_later(*args, **kwargs):
            nonlocal calls
            calls += 1
            elapsed = self.write_probe(*args, **kwargs)
            if calls == 4:
                raise gate.GateError("second repetition baseline failed")
            return elapsed
        self.probe.side_effect = fail_later
        with self.assertRaises(gate.GateError):
            gate.main(self.args + ["--repeats", "2"])
        report = self.read_report()
        self.assertEqual(len(report["completed_pairs"]), 1)
        self.assertEqual(report["active_pair"]["repetition"], 2)
        self.assertEqual(report["stage"], "probe-baseline")
        self.assertEqual(len(list(self.evidence.glob("*.manifest.tsv"))), 2)

    def test_output_overlap_is_rejected_before_writes(self):
        args = self.args.copy()
        args[args.index("--output-dir") + 1] = str(self.root / "baseline-root" / "unsafe")
        with self.assertRaises(SystemExit) as caught:
            gate.main(args)
        self.assertEqual(caught.exception.code, 2)
        self.assertFalse((self.root / "baseline-root" / "unsafe").exists())
        self.build.assert_not_called()


class EvidenceCopySafetyTests(unittest.TestCase):
    def setUp(self):
        temporary = self.enterContext(tempfile.TemporaryDirectory(prefix="openjoc-copy-test-"))
        self.root = Path(temporary)
        self.scratch = self.root / "scratch"
        self.scratch.mkdir()
        self.stderr = self.enterContext(redirect_stderr(io.StringIO()))

    def test_all_four_pcm_streams_share_total_budget_and_keep_f64_difference(self):
        prefixes = {name: self.scratch / name for name in ("baseline", "candidate")}
        data = bytes(range(256)) * 16
        for prefix in prefixes.values():
            for suffix in (".pcm32le", ".pcm64le"):
                gate.artifact(prefix, suffix).write_bytes(data)
        evidence = gate.RunEvidence(self.root, 128, {})
        error = gate.GateError("core f64 PCM bit mismatch at byte 3000 (frame=1 sample=1 channel=0)")
        evidence.retain_failure(prefixes, self.scratch, error)
        records = evidence.report["artifacts"]
        self.assertEqual(sum(item["retained_bytes"] for item in records), 128)
        for item in records:
            actual = (evidence.directory / item["path"]).read_bytes()
            self.assertEqual(actual, data[item["offset_bytes"]:item["offset_bytes"] + 32])
            if item["path"].endswith(".pcm64le"):
                self.assertLessEqual(item["offset_bytes"], 3000)
                self.assertGreater(item["offset_bytes"] + len(actual), 3000)

    def test_tiny_and_unaligned_budgets_still_include_first_differing_byte(self):
        prefixes = {name: self.scratch / name for name in ("baseline", "candidate")}
        for prefix in prefixes.values():
            gate.artifact(prefix, ".pcm32le").write_bytes(bytes(range(24)))
        for budget in (2, 3, 6, 10, 14, 18):
            evidence = gate.RunEvidence(self.root, budget, {})
            evidence.retain_failure(prefixes, self.scratch, gate.GateError("PCM bit mismatch at byte 23"))
            self.assertLessEqual(sum(item["retained_bytes"] for item in evidence.report["artifacts"]), budget)
            for item in evidence.report["artifacts"]:
                self.assertLessEqual(item["offset_bytes"], 23)
                self.assertGreater(item["offset_bytes"] + item["retained_bytes"], 23)

    def test_source_symlink_and_outside_scratch_are_rejected(self):
        outside = self.root / "outside.manifest.tsv"
        outside.write_bytes(b"private bytes")
        link = self.scratch / "baseline.manifest.tsv"
        try:
            link.symlink_to(outside)
        except OSError as error:
            self.skipTest(f"symlinks unavailable: {error}")
        evidence = gate.RunEvidence(self.root, 16, {})
        evidence.retain_failure({"baseline": self.scratch / "baseline", "candidate": self.root / "outside"},
                                self.scratch, gate.GateError("original"))
        self.assertEqual(evidence.report["artifacts"], [])
        self.assertEqual(len(evidence.report["retention_errors"]), 2)
        self.assertEqual(outside.read_bytes(), b"private bytes")

    def test_existing_destination_and_traversal_are_never_overwritten(self):
        source = self.scratch / "source"
        source.write_bytes(b"new")
        evidence = gate.RunEvidence(self.root, 16, {})
        target = evidence.directory / "baseline.manifest.tsv"
        target.write_bytes(b"existing")
        with self.assertRaises(FileExistsError):
            evidence.copy_artifact(source, target.name)
        with self.assertRaises(OSError):
            evidence.copy_artifact(source, "../escape")
        self.assertEqual(target.read_bytes(), b"existing")
        self.assertFalse((self.root / "escape").exists())

    def test_fresh_run_directory_avoids_stale_evidence(self):
        first = gate.RunEvidence(self.root, 16, {})
        (first.directory / "baseline.pcm32le").write_bytes(b"stale")
        second = gate.RunEvidence(self.root, 16, {})
        self.assertNotEqual(first.directory, second.directory)
        self.assertFalse((second.directory / "baseline.pcm32le").exists())

    def test_source_digest_distinguishes_dirty_content_and_untracked_files(self):
        source = self.scratch / "source.rs"
        source.write_bytes(b"first source")
        untracked = self.scratch / "new.rs"
        untracked.write_bytes(b"new source")
        def git_output(command, **kwargs):
            if "--cached" in command:
                return b"source.rs\0"
            if "--others" in command:
                return b"new.rs\0"
            return b" M source.rs\0?? new.rs\0"
        with mock.patch.object(gate.subprocess, "check_output", side_effect=git_output):
            first = gate.source_provenance(self.scratch)
            source.write_bytes(b"different source, same HEAD")
            second = gate.source_provenance(self.scratch)
            untracked.write_bytes(b"changed untracked source")
            third = gate.source_provenance(self.scratch)
        self.assertTrue(first["dirty"])
        self.assertEqual(first["untracked_files"], ["new.rs"])
        self.assertNotEqual(first["working_tree_sha256"], second["working_tree_sha256"])
        self.assertNotEqual(second["working_tree_sha256"], third["working_tree_sha256"])


class PcmEvidenceWorkflowTests(unittest.TestCase):
    def test_both_pcm_jobs_always_upload_small_reports_and_bounded_samples(self):
        workflow = (Path(__file__).resolve().parents[2] / ".github/workflows/ci.yml").read_text()
        for job, next_job in (("pcm-bitexact", "pcm-bitexact-release"),
                              ("pcm-bitexact-release", "platform-windows")):
            block = workflow.split(f"  {job}:\n", 1)[1].split(f"  {next_job}:\n", 1)[0]
            self.assertIn("--failure-pcm-bytes 8388608", block)
            outcome = block.split("- name: Record PCM job outcome", 1)[1].split("- name:", 1)[0]
            self.assertIn("if: always()", outcome)
            upload = block.split("- name: Upload PCM reports and bounded failure samples", 1)[1]
            self.assertIn("if: always()", upload)
            self.assertIn("uses: actions/upload-artifact@v4", upload)
            self.assertIn("retention-days: 7", upload)
            self.assertIn("/job-status.txt", upload)
            self.assertIn("/toolchain.txt", upload)
            self.assertIn("/measurements.csv", upload)
            self.assertIn("/evidence-*/**", upload)
            self.assertNotIn("/logs", upload)
            self.assertNotIn("/openjoc-pcm-gate/**", upload)


if __name__ == "__main__":
    unittest.main()
