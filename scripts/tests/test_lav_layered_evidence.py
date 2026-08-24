# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import copy
import pathlib
import sys
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPTS = ROOT / "scripts"
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

from lav_multichannel_evidence_core import (  # noqa: E402
    CANONICAL_LAYOUTS,
    transport_verified_layouts,
    validate_evidence,
)


SHA_A = "a" * 64
SHA_B = "b" * 64
HEAD_A = "1" * 40
HEAD_B = "2" * 40


def _media_type(layout: str) -> dict[str, object]:
    contract = CANONICAL_LAYOUTS[layout]
    channels = int(contract["channel_count"])
    block_align = channels * 4
    return {
        "major_type": "MEDIATYPE_Audio",
        "subtype": "MEDIASUBTYPE_IEEE_FLOAT",
        "format_type": "FORMAT_WaveFormatEx",
        "format_tag": "WAVE_FORMAT_EXTENSIBLE",
        "channels": channels,
        "channel_order": list(contract["channel_order"]),
        "sample_rate": 48000,
        "bits_per_sample": 32,
        "valid_bits_per_sample": 32,
        "channel_mask": contract["channel_mask"],
        "subformat": "KSDATAFORMAT_SUBTYPE_IEEE_FLOAT",
        "block_align": block_align,
        "avg_bytes_per_sec": 48000 * block_align,
        "sample_size": block_align,
        "cb_size": 22,
        "format_bytes_sha256": SHA_A,
    }


def _fixture(kind: str) -> dict[str, str]:
    return {
        "kind": kind,
        "path": rf"D:\fixtures\joc.lifecycle.{ 'ec3' if kind == 'raw' else 'mp4' }",
        "sha256": SHA_A,
    }


def _runtime() -> dict[str, object]:
    return {
        "manifest": {
            "path": r"D:\runtime\LAVFilters.Dependencies.manifest",
            "sha256": SHA_A,
        },
        "harness": {
            "path": r"D:\runtime\OpenJocDirectShowNegotiationSmoke.exe",
            "sha256": SHA_B,
        },
        "identity_verified": True,
    }


def _transport_run(layout: str, kind: str) -> dict[str, object]:
    media_type = _media_type(layout)
    block_align = int(media_type["block_align"])
    return {
        "fixture": _fixture(kind),
        "policy_selection": "FIXED_POLICY_ENUM",
        "oracle": {
            "kind": "TEST_ONLY_DIRECTSHOW_STRICT_CAPTURE_SINK",
            "accepts_exact_type_only": True,
            "silently_repairs_media_type": False,
        },
        "connect_direct": {
            "attempted": True,
            "exact": True,
            "hresult": "0x00000000",
            "proposal_count": 1,
            "fallback_proposals": 0,
        },
        "requested_type": copy.deepcopy(media_type),
        "pre_stream_connection_type": copy.deepcopy(media_type),
        "post_stream_connection_type": copy.deepcopy(media_type),
        "delivery": {
            "samples": 128,
            "bytes": block_align * 1536 * 128,
            "actual_frame_size": block_align,
            "all_sample_lengths_frame_aligned": True,
            "checked_buffer_sizing": True,
            "allocator_contract_valid": True,
            "full_interleaved_oracle_equal": True,
            "per_channel_oracle_equal": True,
            "per_channel_digests_pairwise_distinct": True,
            "eos": True,
            "graph_errors": 0,
            "type_mutations": 0,
        },
        "runtime_verification": _runtime(),
    }


def _transport_evidence(layout: str) -> dict[str, object]:
    return {
        "hardware_independent": True,
        "runs": [_transport_run(layout, kind) for kind in ("raw", "mp4")],
        "lifecycle": {
            "flush": True,
            "raw_seek_to_zero": True,
            "raw_nonzero_seek": "UNSUPPORTED_RAW_CONTAINER_OPERATION",
            "mp4_forward_seek": True,
            "mp4_backward_seek": True,
            "eos": True,
            "reopen": True,
            "policy_switching": True,
        },
        "checked_10_12_channel_safety": True,
    }


def _endpoint_rejection(
    layout: str, moniker: str = "@device:cm:real-directsound-renderer"
) -> dict[str, object]:
    return {
        "classification": "ENDPOINT_FORMAT_NOT_SUPPORTED",
        "renderer": {
            "moniker": moniker,
            "endpoint_id": "DirectSound:{real-endpoint-guid}",
        },
        "runs": [
            {
                "fixture": _fixture(kind),
                "requested_type": _media_type(layout),
                "connect_direct": {
                    "attempted": True,
                    "exact": True,
                    "hresult": "0x8004025C",
                    "proposal_count": 1,
                    "fallback_proposals": 0,
                },
            }
            for kind in ("raw", "mp4")
        ],
    }


def _endpoint_success(layout: str, moniker: str, family: str) -> dict[str, object]:
    return {
        "classification": "SAMPLE_DELIVERY_VERIFIED",
        "renderer_family": family,
        "renderer": {
            "moniker": moniker,
            "endpoint_id": "{0.0.0.00000000}.{real-endpoint-guid}",
        },
        "runs": [
            {
                "fixture": _fixture(kind),
                "requested_type": _media_type(layout),
                "pre_stream_connection_type": _media_type(layout),
                "post_stream_connection_type": _media_type(layout),
                "connect_direct": {
                    "attempted": True,
                    "exact": True,
                    "hresult": "0x00000000",
                    "proposal_count": 1,
                    "fallback_proposals": 0,
                },
                "sample_delivery": {
                    "observed": True,
                    "decoded_bytes": 8192,
                    "stream_bytes": 524288,
                    "renderer_buffer_duration": 32,
                    "eos": True,
                    "graph_errors": 0,
                },
            }
            for kind in ("raw", "mp4")
        ],
    }


def _candidate(layout: str) -> dict[str, object]:
    contract = CANONICAL_LAYOUTS[layout]
    return {
        "layout": layout,
        "channel_count": contract["channel_count"],
        "channel_mask": contract["channel_mask"],
        "channel_order": list(contract["channel_order"]),
        "logical_lfe_channels": contract["logical_lfe_channels"],
        "states": {
            "transport": "TRANSPORT_VERIFIED",
            "virtual_windows_endpoint": "VIRTUAL_WINDOWS_ENDPOINT_VERIFIED",
            "physical_realtek_endpoint": "REAL_ENDPOINT_VERIFIED",
            "physical_multichannel_hardware": (
                "NOT_TESTED" if layout == "Stereo" else "HARDWARE_NOT_AVAILABLE"
            ),
        },
        "transport_evidence": _transport_evidence(layout),
        "virtual_windows_endpoint_evidence": {
            "endpoint_kind": "THIRD_PARTY_VIRTUAL_WINDOWS_AUDIO_DRIVER",
            "directsound": _endpoint_rejection(
                layout, "@device:cm:vbaudio-directsound-renderer"
            ),
            "waveout": _endpoint_success(
                layout, "@device:cm:vbaudio-waveout-renderer", "WaveOut"
            ),
        },
        "physical_realtek_endpoint_evidence": _endpoint_success(
            layout, "@device:cm:realtek-directsound-renderer", "DirectSound"
        ),
        "physical_multichannel_hardware_evidence": {
            "reason": (
                "not a multichannel policy"
                if layout == "Stereo"
                else "no physical multichannel playback endpoint is present"
            )
        },
    }


def _document() -> dict[str, object]:
    return {
        "schema_version": 2,
        "automatic_layout_selection": "AUTO_NOT_RELIABLE",
        "builds": {
            "openjoc": {
                "head": HEAD_A,
                "binaries": [{"path": r"D:\build\openjoc_capi.dll", "sha256": SHA_A}],
            },
            "lav": {
                "head": HEAD_B,
                "binaries": [{"path": r"D:\build\LAVAudio.ax", "sha256": SHA_B}],
            },
        },
        "environment": {
            "os": {"name": "Windows 11", "version": "10.0.26200", "build": "26200"},
            "host": {
                "name": "OpenJOC native DirectShow harness",
                "path": r"D:\runtime\OpenJocDirectShowNegotiationSmoke.exe",
                "version": "phase6",
                "sha256": SHA_B,
            },
        },
        "candidates": [_candidate(layout) for layout in CANONICAL_LAYOUTS],
    }


class LavLayeredEvidenceTests(unittest.TestCase):
    def assertRejected(self, document: dict[str, object], code: str) -> None:
        errors = validate_evidence(document)
        self.assertTrue(
            any(error.startswith(code) for error in errors),
            f"expected {code}, got {errors}",
        )

    def test_accepts_transport_with_separately_classified_endpoints(self) -> None:
        document = _document()

        self.assertEqual(validate_evidence(document), ())
        self.assertEqual(transport_verified_layouts(document), tuple(CANONICAL_LAYOUTS))

    def test_rejects_wrong_semantic_channel_order(self) -> None:
        document = _document()
        document["candidates"][4]["channel_order"][-1] = "TBC"

        self.assertRejected(document, "CHANNEL_ORDER_MISMATCH")

    def test_rejects_missing_raw_or_mp4_transport_path(self) -> None:
        document = _document()
        document["candidates"][0]["transport_evidence"]["runs"].pop()

        self.assertRejected(document, "TRANSPORT_PATH_INCOMPLETE")

    def test_rejects_media_type_mutation_or_non_float_representation(self) -> None:
        document = _document()
        run = document["candidates"][0]["transport_evidence"]["runs"][0]
        run["post_stream_connection_type"]["subtype"] = "MEDIASUBTYPE_PCM"

        self.assertRejected(document, "TRANSPORT_CONNECTION_TYPE_CHANGED")

    def test_rejects_unchecked_or_misaligned_delivered_frame(self) -> None:
        document = _document()
        delivery = document["candidates"][4]["transport_evidence"]["runs"][0]["delivery"]
        delivery["actual_frame_size"] = 36
        delivery["all_sample_lengths_frame_aligned"] = False

        self.assertRejected(document, "TRANSPORT_FRAME_SIZE_INVALID")

    def test_rejects_oracle_that_repairs_or_accepts_fallback(self) -> None:
        document = _document()
        run = document["candidates"][0]["transport_evidence"]["runs"][0]
        run["oracle"]["silently_repairs_media_type"] = True
        run["connect_direct"]["fallback_proposals"] = 1

        self.assertRejected(document, "STRICT_TRANSPORT_ORACLE_REQUIRED")
        self.assertRejected(document, "TRANSPORT_FALLBACK_OBSERVED")

    def test_rejects_incomplete_flush_seek_eos_reopen_or_policy_switch(self) -> None:
        for field in (
            "flush",
            "raw_seek_to_zero",
            "mp4_forward_seek",
            "mp4_backward_seek",
            "eos",
            "reopen",
            "policy_switching",
        ):
            with self.subTest(field=field):
                document = _document()
                document["candidates"][0]["transport_evidence"]["lifecycle"][field] = False
                self.assertRejected(document, "TRANSPORT_LIFECYCLE_INCOMPLETE")

    def test_rejects_missing_10_or_12_channel_safety(self) -> None:
        document = _document()
        document["candidates"][4]["transport_evidence"][
            "checked_10_12_channel_safety"
        ] = False

        self.assertRejected(document, "HIGH_CHANNEL_SAFETY_NOT_PROVEN")

    def test_hardware_not_available_does_not_invalidate_transport(self) -> None:
        document = _document()
        row = document["candidates"][2]
        row["states"]["physical_realtek_endpoint"] = "HARDWARE_NOT_AVAILABLE"
        row["physical_realtek_endpoint_evidence"] = {
            "reason": "no 7.1-capable endpoint is present",
            "endpoint_inventory_collected": True,
        }

        self.assertEqual(validate_evidence(document), ())

    def test_rejects_directsound_driver_verified_when_only_waveout_delivered(self) -> None:
        document = _document()
        row = document["candidates"][0]
        row["states"]["virtual_windows_endpoint"] = "DIRECTSOUND_DRIVER_VERIFIED"

        self.assertRejected(document, "DIRECTSOUND_DELIVERY_NOT_PROVEN")

    def test_rejects_physical_multichannel_claim_without_hardware_evidence(self) -> None:
        document = _document()
        row = document["candidates"][4]
        row["states"]["physical_multichannel_hardware"] = "REAL_ENDPOINT_VERIFIED"

        self.assertRejected(document, "PHYSICAL_MULTICHANNEL_HARDWARE_NOT_PROVEN")

    def test_rejects_transport_state_in_endpoint_columns(self) -> None:
        document = _document()
        document["candidates"][0]["states"][
            "virtual_windows_endpoint"
        ] = "TRANSPORT_VERIFIED"
        document["candidates"][1]["states"][
            "physical_realtek_endpoint"
        ] = "TRANSPORT_VERIFIED"

        self.assertRejected(document, "VIRTUAL_ENDPOINT_STATE_INVALID")
        self.assertRejected(document, "REALTEK_ENDPOINT_STATE_INVALID")

    def test_rejects_auto_proven_marker(self) -> None:
        document = _document()
        document["automatic_layout_selection"] = "AUTO_PROVEN"

        self.assertRejected(document, "AUTO_RELIABILITY_MARKER_REQUIRED")

    def test_shipped_layouts_follow_transport_not_endpoint_state(self) -> None:
        document = _document()

        self.assertEqual(
            validate_evidence(document, shipped_layouts=tuple(CANONICAL_LAYOUTS)),
            (),
        )
        errors = validate_evidence(document, shipped_layouts=("Stereo",))
        self.assertTrue(any(error.startswith("SHIPPED_LAYOUT_MISMATCH") for error in errors))


if __name__ == "__main__":
    unittest.main()
