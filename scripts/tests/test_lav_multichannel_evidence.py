# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import copy
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPTS = ROOT / "scripts"
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

from lav_multichannel_evidence_core import (  # noqa: E402
    CANONICAL_LAYOUTS,
    validate_evidence,
)


SHA_A = "a" * 64
SHA_B = "b" * 64
SHA_C = "c" * 64
HEAD_A = "1" * 40
HEAD_B = "2" * 40


def _renderer() -> dict[str, str]:
    return {
        "moniker": "@device:cm:real-renderer-guid",
        "endpoint_id": "{0.0.0.00000000}.real-endpoint-guid",
    }


def _media_type(layout: str) -> dict[str, object]:
    contract = CANONICAL_LAYOUTS[layout]
    channels = contract["channel_count"]
    block_align = channels * 4
    return {
        "major_type": "MEDIATYPE_Audio",
        "subtype": "MEDIASUBTYPE_IEEE_FLOAT",
        "format_type": "FORMAT_WaveFormatEx",
        "format_tag": "WAVE_FORMAT_EXTENSIBLE",
        "channels": channels,
        "sample_rate": 48000,
        "bits_per_sample": 32,
        "valid_bits_per_sample": 32,
        "channel_mask": contract["channel_mask"],
        "subformat": "KSDATAFORMAT_SUBTYPE_IEEE_FLOAT",
        "block_align": block_align,
        "avg_bytes_per_sec": 48000 * block_align,
        "sample_size": block_align,
        "cb_size": 22,
        "format_bytes_sha256": SHA_C,
    }


def _manifest_modules() -> list[dict[str, str]]:
    names = (
        "LAVAudio.ax",
        "LAVSplitter.ax",
        "openjoc_capi.dll",
        "avcodec-lav-62.dll",
        "avutil-lav-60.dll",
        "libbluray.dll",
    )
    return [
        {
            "basename": name,
            "path": rf"D:\evidence-runtime\{name}",
            "sha256": f"{index + 3:064x}",
        }
        for index, name in enumerate(names)
    ]


def _runtime_verification(host: str) -> dict[str, object]:
    manifest_modules = _manifest_modules()
    runtime = {
        "kind": (
            "PRIVATE_COM_AND_PROCESS_ENUMERATION"
            if host == "native"
            else "POTPLAYER_IN_PROCESS_ENUMERATION_AFTER_GRAPH"
        ),
        "manifest": {
            "path": r"D:\evidence-runtime\LAVFilters.Dependencies.manifest",
            "sha256": SHA_B,
            "modules": copy.deepcopy(manifest_modules),
        },
        "process_modules": copy.deepcopy(manifest_modules),
        "loaded_ffmpeg_lav_basenames": [
            "avcodec-lav-62.dll",
            "avutil-lav-60.dll",
        ],
    }
    if host == "native":
        runtime["private_com_modules"] = copy.deepcopy(manifest_modules[:2])
    else:
        runtime["observed_after_graph_creation"] = True
    return runtime


def _fixture(kind: str) -> dict[str, str]:
    suffix = "eac3" if kind == "raw" else "mp4"
    return {
        "kind": kind,
        "path": rf"D:\fixtures\joc.multi.{suffix}",
        "sha256": SHA_A,
    }


def _native_run(layout: str, kind: str) -> dict[str, object]:
    media_type = _media_type(layout)
    return {
        "fixture": _fixture(kind),
        "renderer": _renderer(),
        "policy_selection": "FIXED_POLICY_ENUM",
        "connect_direct": {
            "attempted": True,
            "exact": True,
            "hresult": "0x00000000",
            "proposal_count": 1,
        },
        "query_accept": {"attempted": True, "hresult": "0x00000000"},
        "requested_type": copy.deepcopy(media_type),
        "pre_stream_connection_type": copy.deepcopy(media_type),
        "post_stream_connection_type": copy.deepcopy(media_type),
        "states": {
            "pause_hresult": "0x00000000",
            "run_hresult": "0x00000000",
        },
        "counters": {
            "samples": 4,
            "bytes": 4096,
            "eos": True,
            "lifecycle_events": 3,
        },
        "fallback_proposals": 0,
        "type_mutations": 0,
        "graph_errors": 0,
        "runtime_verification": _runtime_verification("native"),
    }


def _potplayer_run(layout: str, kind: str) -> dict[str, object]:
    contract = CANONICAL_LAYOUTS[layout]
    return {
        "fixture": _fixture(kind),
        "renderer": _renderer(),
        "policy_selection": "FIXED_POLICY_ENUM",
        "source_as_output": {"confirmed_in_visible_ui": True},
        "persistent_policy_helper": {"before": layout, "after": layout},
        "same_instance": {
            "observation": "CONNECTED_LAV_AUDIO_STATUS_PAGE",
            "graph_instance_id": f"graph-{layout}-{kind}",
            "policy": layout,
            "admission": "OpenJoc",
            "output": {
                "format": "float32",
                "sample_rate": 48000,
                "channel_count": contract["channel_count"],
                "channel_mask": contract["channel_mask"],
            },
        },
        "playback": {
            "graph_created": True,
            "progressed": True,
            "eos": True,
            "graph_errors": 0,
        },
        "counters": {
            "samples": 4,
            "bytes": 4096,
            "eos": True,
            "lifecycle_events": 3,
        },
        "runtime_verification": _runtime_verification("potplayer"),
    }


def _unverified_row(layout: str) -> dict[str, object]:
    contract = CANONICAL_LAYOUTS[layout]
    return {
        "layout": layout,
        "channel_count": contract["channel_count"],
        "channel_mask": contract["channel_mask"],
        "logical_lfe_channels": contract["logical_lfe_channels"],
        "status": "UNVERIFIED",
        "reason": "real renderer and PotPlayer evidence has not been collected",
        "failure": None,
        "native_runs": [],
        "potplayer_runs": [],
    }


def _proven_row(layout: str) -> dict[str, object]:
    row = _unverified_row(layout)
    row.update(
        {
            "status": "STREAM_PROVEN",
            "reason": None,
            "renderer": _renderer(),
            "native_runs": [_native_run(layout, kind) for kind in ("raw", "mp4")],
            "potplayer_runs": [
                _potplayer_run(layout, kind) for kind in ("raw", "mp4")
            ],
        }
    )
    return row


def _unsupported_row(layout: str) -> dict[str, object]:
    row = _unverified_row(layout)
    requested_type = _media_type(layout)
    row.update(
        {
            "status": "UNSUPPORTED",
            "reason": None,
            "renderer": _renderer(),
            "failure": {
                "kind": "EXACT_REJECTION",
                "stage": "ConnectDirect",
                "hresult": "0x8004022A",
                "measured": True,
                "requested_type": requested_type,
                "actual_type": None,
                "proposal_count": 1,
                "fallback_proposals": 0,
                "fixture": _fixture("raw"),
                "renderer": _renderer(),
                "runtime_verification": _runtime_verification("native"),
            },
        }
    )
    return row


def _document() -> dict[str, object]:
    return {
        "schema_version": 1,
        "automatic_layout_selection": "AUTO_NOT_RELIABLE",
        "builds": {
            "openjoc": {
                "head": HEAD_A,
                "binaries": [
                    {
                        "path": r"D:\build\openjoc_capi.dll",
                        "sha256": SHA_A,
                    }
                ],
            },
            "lav": {
                "head": HEAD_B,
                "binaries": [
                    {"path": r"D:\build\LAVAudio.ax", "sha256": SHA_B},
                    {"path": r"D:\build\LAVSplitter.ax", "sha256": SHA_C},
                ],
            },
        },
        "environment": {
            "os": {"name": "Windows 11", "version": "10.0.26200", "build": "26200"},
            "host": {
                "name": "PotPlayer",
                "path": r"C:\Program Files\DAUM\PotPlayer\PotPlayerMini64.exe",
                "version": "26.07.01.0",
                "sha256": SHA_C,
            },
        },
        "candidates": [_unverified_row(layout) for layout in CANONICAL_LAYOUTS],
    }


def _replace_row(document: dict[str, object], replacement: dict[str, object]) -> None:
    candidates = document["candidates"]
    assert isinstance(candidates, list)
    for index, row in enumerate(candidates):
        if row["layout"] == replacement["layout"]:
            candidates[index] = replacement
            return
    raise AssertionError("layout not found")


def _complete_document() -> dict[str, object]:
    document = _document()
    for layout in ("Stereo", "5.1", "7.1"):
        _replace_row(document, _proven_row(layout))
    _replace_row(document, _unsupported_row("7.1.4"))
    return document


class LavMultichannelEvidenceTests(unittest.TestCase):
    def assertRejected(self, document: dict[str, object], code: str) -> None:
        errors = validate_evidence(document)
        self.assertTrue(
            any(error.startswith(code) for error in errors),
            f"expected {code}, got {errors}",
        )

    def test_accepts_complete_three_state_evidence(self) -> None:
        document = _complete_document()

        errors = validate_evidence(
            document,
            shipped_layouts=("Stereo", "5.1", "7.1"),
        )

        self.assertEqual(errors, ())

    def test_rejects_empty_evidence(self) -> None:
        self.assertRejected({}, "EVIDENCE_DOCUMENT_REQUIRED")

    def test_rejects_legal_mask_only_as_stream_proven(self) -> None:
        document = _complete_document()
        row = _unverified_row("Stereo")
        row["status"] = "STREAM_PROVEN"
        row["reason"] = None
        _replace_row(document, row)

        self.assertRejected(document, "STREAM_PROOF_INCOMPLETE")

    def test_rejects_query_accept_only_as_stream_proven(self) -> None:
        document = _complete_document()
        row = _unverified_row("Stereo")
        row["status"] = "STREAM_PROVEN"
        row["reason"] = None
        row["query_accept"] = {"attempted": True, "hresult": "0x00000000"}
        _replace_row(document, row)

        self.assertRejected(document, "STREAM_PROOF_INCOMPLETE")

    def test_rejects_changed_connection_type(self) -> None:
        document = _complete_document()
        row = next(row for row in document["candidates"] if row["layout"] == "Stereo")
        row["native_runs"][0]["post_stream_connection_type"]["sample_rate"] = 44100

        self.assertRejected(document, "CONNECTION_TYPE_CHANGED")

    def test_rejects_missing_samples(self) -> None:
        document = _complete_document()
        row = next(row for row in document["candidates"] if row["layout"] == "Stereo")
        row["native_runs"][0]["counters"]["samples"] = 0

        self.assertRejected(document, "SAMPLES_NOT_DELIVERED")

    def test_rejects_missing_loaded_dependency(self) -> None:
        document = _complete_document()
        runtime = document["candidates"][0]["native_runs"][0]["runtime_verification"]
        runtime["process_modules"] = [
            module
            for module in runtime["process_modules"]
            if module["basename"] != "libbluray.dll"
        ]

        self.assertRejected(document, "RUNTIME_MODULE_MISSING")

    def test_rejects_duplicate_loaded_dependency_basename(self) -> None:
        document = _complete_document()
        runtime = document["candidates"][0]["native_runs"][0]["runtime_verification"]
        runtime["process_modules"].append(copy.deepcopy(runtime["process_modules"][0]))

        self.assertRejected(document, "RUNTIME_MODULE_DUPLICATE_BASENAME")

    def test_rejects_loaded_dependency_wrong_path(self) -> None:
        document = _complete_document()
        runtime = document["candidates"][0]["native_runs"][0]["runtime_verification"]
        runtime["process_modules"][0]["path"] = r"D:\wrong\LAVAudio.ax"

        self.assertRejected(document, "RUNTIME_MODULE_PATH_MISMATCH")

    def test_rejects_loaded_dependency_wrong_hash(self) -> None:
        document = _complete_document()
        runtime = document["candidates"][0]["native_runs"][0]["runtime_verification"]
        runtime["process_modules"][0]["sha256"] = "f" * 64

        self.assertRejected(document, "RUNTIME_MODULE_HASH_MISMATCH")

    def test_rejects_manifest_only_runtime_evidence(self) -> None:
        document = _complete_document()
        runtime = document["candidates"][0]["native_runs"][0]["runtime_verification"]
        runtime["process_modules"] = []

        self.assertRejected(document, "RUNTIME_MODULE_MISSING")

    def test_rejects_potplayer_registry_helper_only_evidence(self) -> None:
        document = _complete_document()
        run = document["candidates"][0]["potplayer_runs"][0]
        run.pop("same_instance")
        run.pop("playback")

        self.assertRejected(document, "POTPLAYER_SAME_INSTANCE_REQUIRED")

    def test_rejects_wrong_same_instance_policy(self) -> None:
        document = _complete_document()
        run = document["candidates"][0]["potplayer_runs"][0]
        run["same_instance"]["policy"] = "5.1"

        self.assertRejected(document, "POTPLAYER_POLICY_MISMATCH")

    def test_rejects_wrong_same_instance_admission(self) -> None:
        document = _complete_document()
        run = document["candidates"][0]["potplayer_runs"][0]
        run["same_instance"]["admission"] = "StockEac3"

        self.assertRejected(document, "POTPLAYER_ADMISSION_MISMATCH")

    def test_rejects_wrong_same_instance_output(self) -> None:
        document = _complete_document()
        run = document["candidates"][0]["potplayer_runs"][0]
        run["same_instance"]["output"]["channel_mask"] = "0x0000060f"

        self.assertRejected(document, "POTPLAYER_OUTPUT_MISMATCH")

    def test_rejects_mandatory_layout_not_proven(self) -> None:
        document = _complete_document()
        _replace_row(document, _unverified_row("7.1"))

        self.assertRejected(document, "MANDATORY_LAYOUT_NOT_STREAM_PROVEN")

    def test_rejects_unmeasured_unsupported_row(self) -> None:
        document = _complete_document()
        row = _unsupported_row("7.1.4")
        row["failure"]["measured"] = False
        _replace_row(document, row)

        self.assertRejected(document, "UNSUPPORTED_REQUIRES_MEASURED_FAILURE")

    def test_rejects_unsupported_without_stage_and_hresult(self) -> None:
        document = _complete_document()
        row = _unsupported_row("7.1.4")
        row["failure"]["stage"] = ""
        row["failure"]["hresult"] = ""
        _replace_row(document, row)

        self.assertRejected(document, "UNSUPPORTED_FAILURE_DETAILS_REQUIRED")

    def test_rejects_exact_rejection_with_success_hresult(self) -> None:
        document = _complete_document()
        row = _unsupported_row("7.1.4")
        row["failure"]["hresult"] = "0x00000000"
        _replace_row(document, row)

        self.assertRejected(document, "UNSUPPORTED_REJECTION_HRESULT_INVALID")

    def test_rejects_unverified_without_reason(self) -> None:
        document = _complete_document()
        row = _unverified_row("5.1.2")
        row["reason"] = ""
        _replace_row(document, row)

        self.assertRejected(document, "UNVERIFIED_REASON_REQUIRED")

    def test_rejects_wrong_logical_lfe_count(self) -> None:
        document = _complete_document()
        row = next(row for row in document["candidates"] if row["layout"] == "5.1.2")
        row["logical_lfe_channels"] = 2

        self.assertRejected(document, "LOGICAL_LFE_MISMATCH")

    def test_rejects_physical_subwoofer_field(self) -> None:
        document = _complete_document()
        document["candidates"][1]["physical_subwoofer_count"] = 2

        self.assertRejected(document, "FORBIDDEN_SEMANTIC_FIELD")

    def test_rejects_auto_candidate(self) -> None:
        document = _complete_document()
        document["candidates"][0]["layout"] = "Auto"

        self.assertRejected(document, "AUTO_LAYOUT_FORBIDDEN")

    def test_rejects_shipped_layout_mismatch(self) -> None:
        document = _complete_document()

        errors = validate_evidence(document, shipped_layouts=("Stereo",))

        self.assertTrue(
            any(error.startswith("SHIPPED_LAYOUT_MISMATCH") for error in errors),
            errors,
        )

    def test_initial_real_pre_run_matrix_fails_mandatory_gate_truthfully(self) -> None:
        path = (
            ROOT
            / "docs"
            / "integration"
            / "evidence"
            / "windows-lav-multichannel-2026-08-23.json"
        )
        document = json.loads(path.read_text(encoding="utf-8"))

        errors = validate_evidence(document)

        mandatory_errors = [
            error
            for error in errors
            if error.startswith("MANDATORY_LAYOUT_NOT_STREAM_PROVEN")
        ]
        self.assertEqual(len(mandatory_errors), 3)
        self.assertNotIn(
            "STREAM_PROVEN",
            {row["status"] for row in document["candidates"]},
        )

    def test_cli_returns_failure_for_pre_run_matrix(self) -> None:
        document = _document()
        with tempfile.TemporaryDirectory() as temporary_directory:
            evidence_path = pathlib.Path(temporary_directory) / "evidence.json"
            evidence_path.write_text(json.dumps(document), encoding="utf-8")

            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPTS / "validate_lav_multichannel_evidence.py"),
                    str(evidence_path),
                ],
                check=False,
                capture_output=True,
                text=True,
                encoding="utf-8",
            )

        self.assertEqual(result.returncode, 1)
        self.assertIn("MANDATORY_LAYOUT_NOT_STREAM_PROVEN", result.stderr)


if __name__ == "__main__":
    unittest.main()
