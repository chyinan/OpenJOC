import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO = Path(__file__).resolve().parents[2]
DOWNLOADS = Path(r"C:\Users\chyin\Downloads")
JOC = DOWNLOADS / "C06_STATIC_CENTER_997_JOC_ec3.mp4"


class RealC06DecodedBindingTests(unittest.TestCase):
    def test_exact_observed_raw3_capture_admits_decoded_object_scene(self):
        if not JOC.is_file():
            self.skipTest("user-owned C06 fixture is not available")

        with tempfile.TemporaryDirectory(prefix="openjoc-c06-binding-") as temp:
            root = Path(temp)
            internal = root / "internal-base"

            subprocess.run(
                [
                    "cargo",
                    "run",
                    "--quiet",
                    "-p",
                    "openjoc-cli",
                    "--",
                    "decode",
                    str(JOC),
                    "--internal-base",
                    "--validation-profile",
                    "observed-vendor-compat",
                    "--reference-f64",
                    "-o",
                    str(internal),
                ],
                cwd=REPO,
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            scene = json.loads((internal / "scene.json").read_text())
            basis = json.loads(
                (internal / "diagnostics" / "reconstruction_basis.json").read_text()
            )
            first_profile = json.loads(
                (internal / "debug" / "frame_000" / "profile_validation.json").read_text()
            )
            opaque = json.loads(
                (internal / "debug" / "frame_000" / "oamd_partial_status.json").read_text()
            )

            self.assertEqual(scene["semantic_binding"], "resolved_within_carrier")
            self.assertEqual(len(scene["objects"]), 16)
            self.assertEqual(len(basis["rows"]), 15)
            self.assertEqual(first_profile["profile"], "OBSERVED_VENDOR_COMPAT")
            self.assertEqual(opaque["opaque_elements"][0]["raw_warp"], 3)


if __name__ == "__main__":
    unittest.main()
