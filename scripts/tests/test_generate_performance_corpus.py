"""Check corpus manifest metadata without invoking native fixture exporters."""

import contextlib
from datetime import datetime, timezone
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import generate_performance_corpus as corpus


class CorpusManifestDateTests(unittest.TestCase):
    def generate_manifest(self, directory, durations):
        root = Path(directory)
        args = [
            "generate_performance_corpus.py",
            "--repo", str(root / "repo"),
            "--target", str(root / "target"),
            "--output-dir", str(root / "corpus"),
            "--durations", *durations,
        ]
        # The manifest is the unit under test. No audio or native success is
        # inferred from these deliberately minimal exporter return values.
        with (
            mock.patch.object(sys, "argv", args),
            mock.patch.object(corpus.shutil, "which", return_value="unused-test-tool"),
            mock.patch.object(corpus, "generate_joc", return_value={
                "sha256": "test-joc", "exporter_audit": "test audit",
            }) as joc,
            mock.patch.object(corpus, "generate_ordinary_eac3", return_value={
                "sha256": "test-eac3", "syncframes": 938,
                "programme_samples": 1440768, "encoder": "test encoder",
            }) as ordinary,
            contextlib.redirect_stdout(io.StringIO()),
        ):
            self.assertEqual(corpus.main(), 0)
        self.assertEqual(joc.call_count, len(durations))
        self.assertEqual(ordinary.call_count, len(durations))
        return json.loads((root / "corpus/corpus.json").read_text(encoding="utf-8"))

    def test_manifest_records_actual_generation_date(self):
        before = datetime.now(timezone.utc).date().isoformat()
        with tempfile.TemporaryDirectory() as directory:
            manifest = self.generate_manifest(directory, ["30"])
        after = datetime.now(timezone.utc).date().isoformat()
        self.assertIn(manifest["generated_utc_date"], {before, after})
        self.assertTrue(manifest["synthetic_only"])
        self.assertFalse(manifest["real_programme_media_available"])

    def test_generation_date_uses_utc(self):
        with (
            tempfile.TemporaryDirectory() as directory,
            mock.patch.object(corpus, "datetime", create=True) as clock,
        ):
            clock.now.return_value = datetime(2030, 1, 1, 0, 1, tzinfo=timezone.utc)
            manifest = self.generate_manifest(directory, ["30", "60"])
        self.assertEqual(manifest["generated_utc_date"], "2030-01-01")
        clock.now.assert_called_once_with(timezone.utc)
        self.assertEqual(len(manifest["cases"]), 2)

    def test_generation_date_is_recomputed_for_each_invocation(self):
        with (
            tempfile.TemporaryDirectory() as directory,
            mock.patch.object(corpus, "datetime", create=True) as clock,
        ):
            clock.now.side_effect = [
                datetime(2030, 12, 31, 23, 59, tzinfo=timezone.utc),
                datetime(2031, 1, 1, 0, 1, tzinfo=timezone.utc),
            ]
            before = self.generate_manifest(Path(directory) / "before", ["30"])
            after = self.generate_manifest(Path(directory) / "after", ["60"])
        self.assertEqual(before["generated_utc_date"], "2030-12-31")
        self.assertEqual(after["generated_utc_date"], "2031-01-01")
        self.assertEqual(clock.now.call_args_list, [mock.call(timezone.utc)] * 2)


if __name__ == "__main__":
    unittest.main()
