from __future__ import annotations

import hashlib
import json
import os
import unittest
from pathlib import Path


SCENE_VECTOR_SCHEMA = "openjoc.public_decoded_joc_scene_vectors.v1"
RB_VECTOR_SCHEMA = "openjoc.public_rb_to_object_vectors.v1"
REQUIRED_SCENE_CASES = (
    "C00",
    "C01",
    "C02",
    "C04",
    "C05",
    "C06",
    "C07",
    "C06-static-center",
    "C06-static-frontleft",
)


def _source_path() -> Path:
    configured = os.environ.get("OPENJOC_DECODED_JOC_SCENE_VECTORS")
    candidates = (
        Path(configured) if configured else None,
        Path(r"C:\Users\chyin\Downloads\public_decoded_joc_scene_vectors.json"),
    )
    for candidate in candidates:
        if candidate is not None and candidate.is_file():
            return candidate
    raise unittest.SkipTest(
        "set OPENJOC_DECODED_JOC_SCENE_VECTORS or provide the sanitized public scene vectors"
    )


class DecodedJocSceneVectorTests(unittest.TestCase):
    def test_sanitized_vectors_preserve_all_three_typed_bridges(self) -> None:
        source = _source_path()
        payload = json.loads(source.read_text(encoding="utf-8"))

        self.assertEqual(payload["schema"], SCENE_VECTOR_SCHEMA)
        self.assertEqual(payload["measurement"]["full_vector_rows"], 15)
        self.assertEqual(payload["measurement"]["raw_pcm_exported"], False)
        self.assertTrue(set(REQUIRED_SCENE_CASES) <= set(payload["cases"]))

        for case_id in REQUIRED_SCENE_CASES:
            case = payload["cases"][case_id]
            self.assertEqual(case["row_count"], 15, case_id)
            profile = case["binding_profile"]
            self.assertEqual(profile["base_lfe_total_index"], 0, case_id)
            self.assertEqual(profile["dynamic_count"], 15, case_id)
            self.assertEqual(
                profile["decoded_joc_ordinal_to_reconstruction_row"], "j -> j", case_id
            )
            self.assertEqual(
                profile["decoded_joc_ordinal_to_oamd_dynamic_ordinal"], "j -> j", case_id
            )
            self.assertEqual(
                profile["decoded_joc_ordinal_to_oamd_total_index"], "j -> j+1", case_id
            )
            self.assertFalse(profile["authored_identity_asserted"], case_id)

            for frequency in case["frequencies"].values():
                for observation in frequency["observations"]:
                    bindings = observation["oamd_by_decoded_row"]
                    self.assertEqual(len(bindings), 15, case_id)
                    for ordinal, binding in enumerate(bindings):
                        self.assertEqual(binding["decoded_joc_ordinal"], ordinal, case_id)
                        self.assertEqual(binding["reconstruction_row"], ordinal, case_id)
                        self.assertEqual(binding["oamd_dynamic_ordinal"], ordinal, case_id)
                        self.assertEqual(binding["oamd_total_index"], ordinal + 1, case_id)

        moving_rows = {
            observation["dominant_row"]
            for observation in payload["cases"]["C06"]["frequencies"]["997"]["observations"]
        }
        self.assertTrue({0, 1, 3, 9}.issubset(moving_rows))

    def test_public_rb_vectors_are_independent_input_to_output_order_controls(self) -> None:
        vector_path = Path(__file__).parents[2] / "analysis-output" / "public_rb_to_object_vectors.json"
        if not vector_path.is_file():
            raise unittest.SkipTest("public RB-to-object vectors are not present")
        payload = json.loads(vector_path.read_text(encoding="utf-8"))
        self.assertEqual(payload["schema"], RB_VECTOR_SCHEMA)
        permutation = next(
            vector for vector in payload["vectors"] if vector["id"] == "row_permutation_sensitivity"
        )
        self.assertEqual(permutation["original_expected_qout_by_obj"], {"0": 1.0, "1": 2.0})
        self.assertEqual(permutation["swapped_expected_qout_by_obj"], {"0": 2.0, "1": 1.0})

    def test_scene_vector_provenance_is_stable(self) -> None:
        source = _source_path()
        digest = hashlib.sha256(source.read_bytes()).hexdigest()
        self.assertEqual(
            digest,
            "5c5763a2587f07304ea762b49f267901b94eace6638cbf905450282cc99a14f3",
        )


if __name__ == "__main__":
    unittest.main()
