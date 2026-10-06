import sys
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "performance_profile_repro.py"
sys.path.insert(0, str(SCRIPT.parent))
import performance_profile_repro as repro  # noqa: E402


class PerformanceProfileReproTests(unittest.TestCase):
    def setUp(self):
        self.fields = [
            "case",
            "input_sha256",
            "repetition",
            "run_order",
            "baseline_summary_api_pipeline_wall_ns",
            "candidate_summary_api_pipeline_wall_ns",
            "status",
        ]
        self.rows = [
            {
                "case": "stereo-2.0",
                "input_sha256": "a" * 64,
                "repetition": "1",
                "run_order": "baseline-candidate",
                "baseline_summary_api_pipeline_wall_ns": "10",
                "candidate_summary_api_pipeline_wall_ns": "11",
                "status": "pass",
            },
            {
                "case": "stereo-2.0",
                "input_sha256": "a" * 64,
                "repetition": "2",
                "run_order": "candidate-baseline",
                "baseline_summary_api_pipeline_wall_ns": "20",
                "candidate_summary_api_pipeline_wall_ns": "18",
                "status": "pass",
            },
            {
                "case": "stereo-2.0",
                "input_sha256": "a" * 64,
                "repetition": "3",
                "run_order": "baseline-candidate",
                "baseline_summary_api_pipeline_wall_ns": "30",
                "candidate_summary_api_pipeline_wall_ns": "45",
                "status": "pass",
            },
            {
                "case": "stereo-2.0",
                "input_sha256": "a" * 64,
                "repetition": "4",
                "run_order": "candidate-baseline",
                "baseline_summary_api_pipeline_wall_ns": "40",
                "candidate_summary_api_pipeline_wall_ns": "42",
                "status": "fail",
            },
            {
                "case": "stereo-2.0",
                "input_sha256": "a" * 64,
                "repetition": "5",
                "run_order": "baseline-candidate",
                "baseline_summary_api_pipeline_wall_ns": "nan",
                "candidate_summary_api_pipeline_wall_ns": "7",
                "status": "pass",
            },
        ]

    def test_reports_self_repeat_and_paired_relative_statistics_from_valid_pairs(self):
        excluded, reports = repro.analyze_rows(self.rows, self.fields)
        self.assertEqual(len(excluded), 2)
        self.assertEqual(len(reports), 1)
        report = reports[0]
        self.assertEqual(report["valid_pairs"], 3)
        self.assertEqual(report["excluded_inputs"], 2)
        baseline = report["baseline_self_repeat"]
        self.assertEqual(baseline["median"], 20)
        self.assertEqual(baseline["range"], [10, 30])
        self.assertEqual(baseline["mad"], 10)
        delta = report["paired_relative_delta_percent"]
        self.assertEqual(delta["median"], 10)
        self.assertEqual(delta["range"], [-10, 50])
        self.assertEqual(delta["mad"], 20)

    def test_rejects_shape_mismatch_and_duplicate_repetitions(self):
        fields = self.fields + ["baseline_summary_output_sample_count", "candidate_summary_output_sample_count"]
        rows = [dict(self.rows[0]), dict(self.rows[0])]
        rows[0].update(
            baseline_summary_output_sample_count="100",
            candidate_summary_output_sample_count="101",
        )
        rows[1]["candidate_summary_output_sample_count"] = "102"
        with self.assertRaisesRegex(repro.AnalysisError, "workload shape differs"):
            repro.analyze_rows(rows, fields)

    def test_rejects_paired_descriptor_changes_across_repetitions(self):
        for suffix in ("config_fingerprint", "config_descriptor_hex", "latency_samples", "output_frames"):
            with self.subTest(suffix=suffix):
                fields = self.fields + [f"{side}_summary_{suffix}" for side in ("baseline", "candidate")]
                rows = [dict(row) for row in self.rows[:3]]
                for index, row in enumerate(rows):
                    for side in ("baseline", "candidate"):
                        row[f"{side}_summary_{suffix}"] = "200" if index == 1 else "100"
                excluded, reports = repro.analyze_rows(rows, fields)
                self.assertEqual(len(excluded), 1)
                self.assertIn(f"baseline_summary_{suffix} changed within case", excluded[0])
                self.assertEqual(reports[0]["valid_pairs"], 2)
                self.assertEqual(reports[0]["baseline_self_repeat"]["range"], [10, 30])

    def test_tracks_one_sided_exported_descriptors_across_repetitions(self):
        for side in ("baseline", "candidate"):
            with self.subTest(side=side):
                column = f"{side}_summary_config_descriptor_hex"
                rows = [dict(row) for row in self.rows[:2]]
                rows[0][column] = "aa"
                rows[1][column] = "bb"
                excluded, reports = repro.analyze_rows(rows, self.fields + [column])
                self.assertIn(f"{column} changed within case", excluded[0])
                self.assertEqual(reports[0]["valid_pairs"], 1)

    def test_report_disclaims_speedup_and_labels_same_product_only_when_requested(self):
        excluded, reports = repro.analyze_rows(self.rows[:3], self.fields)
        rendered = repro.render_report(
            Path("measurements.csv"), excluded, reports, same_product_control=True
        )
        self.assertIn("caller labels this as a same-product control", rendered)
        self.assertIn("binary identity is not established by this flag", rendered)
        self.assertIn("do not imply speedup", rendered)
        self.assertIn("paired relative delta", rendered)

    def test_binary_identity_is_reported_only_when_sha256_columns_are_present(self):
        fields = self.fields + [
            "baseline_api_probe_binary_sha256",
            "candidate_api_probe_binary_sha256",
        ]
        rows = [dict(self.rows[0])]
        rows[0]["baseline_api_probe_binary_sha256"] = "b" * 64
        rows[0]["candidate_api_probe_binary_sha256"] = "b" * 64
        _, reports = repro.analyze_rows(rows, fields)
        self.assertEqual(reports[0]["probe_binary_relationship"], "identical SHA-256")

    def test_requested_metric_must_be_a_present_timing_pair(self):
        with self.assertRaisesRegex(repro.AnalysisError, "requested timing metric pair is missing"):
            repro.discover_metrics(self.fields, ["summary_missing_wall_ns"])


if __name__ == "__main__":
    unittest.main()
