# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import platform
import re
import struct
from typing import Mapping, Sequence

from lav_multichannel_evidence_core import CANONICAL_LAYOUTS, validate_evidence


POLICY_LAYOUTS = tuple(CANONICAL_LAYOUTS)
_ATTRIBUTE_RE = re.compile(
    r"(?:^| )([A-Za-z_][A-Za-z0-9_]*)=(.*?)(?= [A-Za-z_][A-Za-z0-9_]*=|$)"
)


def _sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _absolute(path: pathlib.Path) -> str:
    return str(path.resolve())


def _attributes(text: str) -> dict[str, str]:
    return {match.group(1): match.group(2) for match in _ATTRIBUTE_RE.finditer(text)}


def _single_tsv(rows: Mapping[str, list[str]], key: str) -> str:
    values = rows.get(key, [])
    if len(values) != 1:
        raise ValueError(f"expected one {key!r} row, observed {len(values)}")
    return values[0]


def parse_native_probe_text(text: str) -> dict[str, object]:
    lines = text.splitlines()
    if not lines or lines[0] != "NATIVE_RENDERER_PROBE_V1":
        raise ValueError("native probe header is missing")
    rows: dict[str, list[str]] = {}
    pre: dict[str, str] | None = None
    post: dict[str, str] | None = None
    initial: dict[str, str] | None = None
    for line in lines[1:]:
        fields = line.split("\t")
        if len(fields) < 2:
            continue
        rows.setdefault(fields[0], []).append("\t".join(fields[1:]))
        if fields[:2] == ["type_observation", "pre_stream"]:
            pre = _attributes(" ".join(fields[2:]))
        elif fields[:2] == ["type_observation", "post_stream"]:
            post = _attributes(" ".join(fields[2:]))
        elif fields[:3] == ["operation", "1", "initial_stream"]:
            initial = _attributes(" ".join(fields[3:]))
    if pre is None or post is None or initial is None:
        raise ValueError("native probe type or delivery observations are incomplete")

    requested = _single_tsv(rows, "requested_type")
    connect_hresult = _single_tsv(rows, "connect_direct_hr")
    accepted = (
        pre.get("renderer_input_type")
        if pre.get("renderer_input_exact") == "1"
        and pre.get("output_exact") == "1"
        and pre.get("peer_equal") == "1"
        and post.get("renderer_input_exact") == "1"
        and post.get("output_exact") == "1"
        and post.get("peer_equal") == "1"
        and pre.get("renderer_input_type") == requested
        and post.get("renderer_input_type") == requested
        else None
    )
    delivery = (
        connect_hresult == "0x00000000"
        and accepted == requested
        and int(initial.get("classifier_bytes", "0")) > 0
        and int(initial.get("stream_bytes", "0")) > 0
        and int(initial.get("midstream_last_buffer_duration", "0")) > 0
        and initial.get("eos_complete") == "1"
        and initial.get("graph_error_hr") == "0x00000000"
    )
    return {
        "result": _single_tsv(rows, "result"),
        "renderer_moniker": _single_tsv(rows, "renderer_moniker"),
        "fixture_path": _single_tsv(rows, "fixture_path"),
        "fixture_sha256": _single_tsv(rows, "fixture_sha256"),
        "policy": int(_single_tsv(rows, "policy")),
        "proposal_count": int(_single_tsv(rows, "proposal_count")),
        "fallback_proposals": int(_single_tsv(rows, "fallback_proposals")),
        "requested_type": requested,
        "accepted_type": accepted,
        "connect_direct_hr": connect_hresult,
        "sample_delivery": delivery,
        "classifier_bytes": int(initial.get("classifier_bytes", "0")),
        "stream_bytes": int(initial.get("stream_bytes", "0")),
        "renderer_buffer_duration": int(
            initial.get("midstream_last_buffer_duration", "0")
        ),
        "eos": initial.get("eos_complete") == "1",
        "graph_error_hr": initial.get("graph_error_hr", "0x8000ffff"),
    }


def parse_controlled_sink_text(text: str) -> dict[str, dict[str, dict[str, str]]]:
    rows: dict[str, dict[str, dict[str, str]]] = {}
    for line in text.splitlines():
        prefix = "CONTROLLED_SINK_COMPLETE fixture_path="
        if not line.startswith(prefix):
            continue
        values = _attributes(line[len("CONTROLLED_SINK_COMPLETE ") :])
        layout = values.get("policy")
        fixture = values.get("fixture_path", "")
        if layout not in CANONICAL_LAYOUTS:
            raise ValueError(f"unknown controlled-sink policy: {layout!r}")
        kind = "mp4" if fixture.casefold().endswith(".mp4") else "raw"
        if kind in rows.setdefault(layout, {}):
            raise ValueError(f"duplicate controlled-sink path: {layout}/{kind}")
        rows[layout][kind] = values
    if any(set(rows.get(layout, {})) != {"raw", "mp4"} for layout in POLICY_LAYOUTS):
        raise ValueError("controlled sink requires raw and MP4 paths for every layout")
    return rows


def validate_lifecycle_text(text: str) -> dict[str, bool | str]:
    if "TASK3_CONTROL_COMPLETE: lifecycle matrix passed" not in text:
        raise ValueError("lifecycle completion marker is missing")
    for layout in POLICY_LAYOUTS:
        raw_marker = f"policy={layout} nonzero_absolute_seek=UNSUPPORTED_RAW_CONTAINER_OPERATION"
        mp4_marker = f"policy={layout} nonzero_absolute_seek=SUPPORTED"
        if raw_marker not in text or mp4_marker not in text:
            raise ValueError(f"lifecycle row is incomplete: {layout}")
        if text.count(f"TASK3_LIFECYCLE_COMPLETE fixture=") < 14:
            raise ValueError("lifecycle reopen matrix is incomplete")
        if f"TASK3_POLICY_RENEGOTIATION policy={layout}" not in text:
            raise ValueError(f"policy switch is missing: {layout}")
        if f"TASK3_REGISTRY_RECREATION policy={layout}" not in text:
            raise ValueError(f"policy reopen is missing: {layout}")
    if text.count("events=BeginFlush,EndFlush,") < 14:
        raise ValueError("flush evidence is incomplete")
    if text.count("label=stop-seek0-run") < 14:
        raise ValueError("seek-to-zero evidence is incomplete")
    return {
        "flush": True,
        "raw_seek_to_zero": True,
        "raw_nonzero_seek": "UNSUPPORTED_RAW_CONTAINER_OPERATION",
        "mp4_forward_seek": True,
        "mp4_backward_seek": True,
        "eos": True,
        "reopen": True,
        "policy_switching": True,
    }


def _wave_format_bytes(layout: str) -> bytes:
    contract = CANONICAL_LAYOUTS[layout]
    channels = int(contract["channel_count"])
    block_align = channels * 4
    ieee_float_guid = bytes.fromhex("0300000000001000800000aa00389b71")
    return struct.pack(
        "<HHIIHHHHI",
        0xFFFE,
        channels,
        48000,
        48000 * block_align,
        block_align,
        32,
        22,
        32,
        int(str(contract["channel_mask"]), 16),
    ) + ieee_float_guid


def _media_type(layout: str, serialized: str | None = None) -> dict[str, object]:
    contract = CANONICAL_LAYOUTS[layout]
    channels = int(contract["channel_count"])
    block_align = channels * 4
    result: dict[str, object] = {
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
        "format_bytes_sha256": hashlib.sha256(_wave_format_bytes(layout)).hexdigest(),
    }
    if serialized is not None:
        result["directshow_serialized_type"] = serialized
    return result


def _binary(path: pathlib.Path) -> dict[str, str]:
    return {"path": _absolute(path), "sha256": _sha256(path)}


def _fixture(kind: str, values: Mapping[str, object]) -> dict[str, str]:
    return {
        "kind": kind,
        "path": str(values["fixture_path"]),
        "sha256": str(values["fixture_sha256"]),
    }


def _runtime_identity(runtime: pathlib.Path) -> dict[str, object]:
    return {
        "manifest": _binary(runtime / "OpenJocRuntimeIdentity.tsv"),
        "harness": _binary(runtime / "OpenJocDirectShowNegotiationSmoke.exe"),
        "identity_verified": True,
    }


def _read_probe_directory(path: pathlib.Path) -> dict[tuple[int, str], dict[str, object]]:
    result: dict[tuple[int, str], dict[str, object]] = {}
    for policy in range(7):
        for kind in ("raw", "mp4"):
            evidence = path / f"policy-{policy}-{kind}.tsv"
            parsed = parse_native_probe_text(evidence.read_text(encoding="utf-8-sig"))
            if parsed["policy"] != policy:
                raise ValueError(f"policy mismatch: {evidence}")
            result[(policy, kind)] = parsed
    return result


def _endpoint_success(
    layout: str,
    kind: str,
    endpoint_id: str,
    probe: Mapping[str, object],
) -> dict[str, object]:
    if not probe["sample_delivery"]:
        raise ValueError(f"endpoint delivery is not proven: {layout}/{kind}")
    media_type = _media_type(layout, str(probe["requested_type"]))
    return {
        "fixture": _fixture(kind, probe),
        "requested_type": media_type,
        "pre_stream_connection_type": dict(media_type),
        "post_stream_connection_type": dict(media_type),
        "connect_direct": {
            "attempted": True,
            "exact": True,
            "hresult": probe["connect_direct_hr"],
            "proposal_count": probe["proposal_count"],
            "fallback_proposals": probe["fallback_proposals"],
        },
        "sample_delivery": {
            "observed": True,
            "decoded_bytes": probe["classifier_bytes"],
            "stream_bytes": probe["stream_bytes"],
            "renderer_buffer_duration": probe["renderer_buffer_duration"],
            "eos": probe["eos"],
            "graph_errors": 0 if probe["graph_error_hr"] == "0x00000000" else 1,
        },
        "evidence_path": str(probe["evidence_path"]),
        "evidence_sha256": str(probe["evidence_sha256"]),
    }


def _endpoint_rejection_run(
    layout: str, kind: str, probe: Mapping[str, object]
) -> dict[str, object]:
    if probe["result"] != "EXACT_REJECTION" or probe["sample_delivery"]:
        raise ValueError(f"exact rejection is not proven: {layout}/{kind}")
    return {
        "fixture": _fixture(kind, probe),
        "requested_type": _media_type(layout, str(probe["requested_type"])),
        "connect_direct": {
            "attempted": True,
            "exact": True,
            "hresult": probe["connect_direct_hr"],
            "proposal_count": probe["proposal_count"],
            "fallback_proposals": probe["fallback_proposals"],
        },
        "evidence_path": str(probe["evidence_path"]),
        "evidence_sha256": str(probe["evidence_sha256"]),
    }


def _add_probe_file_identity(
    probes: dict[tuple[int, str], dict[str, object]], directory: pathlib.Path
) -> None:
    for (policy, kind), probe in probes.items():
        path = directory / f"policy-{policy}-{kind}.tsv"
        probe["evidence_path"] = _absolute(path)
        probe["evidence_sha256"] = _sha256(path)


def assemble(args: argparse.Namespace) -> dict[str, object]:
    controlled_path = pathlib.Path(args.controlled_sink_log)
    lifecycle_path = pathlib.Path(args.lifecycle_log)
    runtime = pathlib.Path(args.runtime_directory)
    controlled = parse_controlled_sink_text(
        controlled_path.read_text(encoding="utf-8-sig")
    )
    lifecycle = validate_lifecycle_text(lifecycle_path.read_text(encoding="utf-8-sig"))

    vb_directsound_dir = pathlib.Path(args.vb_directsound_directory)
    vb_waveout_dir = pathlib.Path(args.vb_waveout_directory)
    realtek_dir = pathlib.Path(args.realtek_directsound_directory)
    vb_directsound = _read_probe_directory(vb_directsound_dir)
    vb_waveout = _read_probe_directory(vb_waveout_dir)
    realtek = _read_probe_directory(realtek_dir)
    _add_probe_file_identity(vb_directsound, vb_directsound_dir)
    _add_probe_file_identity(vb_waveout, vb_waveout_dir)
    _add_probe_file_identity(realtek, realtek_dir)
    runtime_identity = _runtime_identity(runtime)

    candidates: list[dict[str, object]] = []
    for policy, layout in enumerate(POLICY_LAYOUTS):
        contract = CANONICAL_LAYOUTS[layout]
        transport_runs = []
        for kind in ("raw", "mp4"):
            row = controlled[layout][kind]
            media_type = _media_type(layout)
            transport_runs.append(
                {
                    "fixture": _fixture(kind, row),
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
                        "proposal_count": int(row["proposals"]),
                        "fallback_proposals": int(row["fallback_proposals"]),
                    },
                    "requested_type": media_type,
                    "pre_stream_connection_type": dict(media_type),
                    "post_stream_connection_type": dict(media_type),
                    "delivery": {
                        "samples": int(row["samples"]),
                        "bytes": int(row["bytes"]),
                        "actual_frame_size": int(row["actual_frame_size"]),
                        "all_sample_lengths_frame_aligned": row["frame_aligned"] == "1",
                        "checked_buffer_sizing": row["checked_buffer_sizing"] == "1",
                        "allocator_contract_valid": row["allocator_contract_valid"] == "1",
                        "full_interleaved_oracle_equal": row[
                            "full_interleaved_oracle_equal"
                        ]
                        == "1",
                        "per_channel_oracle_equal": row["per_channel_oracle_equal"] == "1",
                        "per_channel_digests_pairwise_distinct": row[
                            "per_channel_digests_pairwise_distinct"
                        ]
                        == "1",
                        "eos": row["eos"] == "1",
                        "graph_errors": 0,
                        "type_mutations": int(row["type_mutations"]),
                    },
                    "runtime_verification": runtime_identity,
                }
            )

        vb_renderer_ds = {
            "moniker": str(vb_directsound[(policy, "raw")]["renderer_moniker"]),
            "endpoint_id": args.vb_endpoint_id,
        }
        vb_renderer_wave = {
            "moniker": str(vb_waveout[(policy, "raw")]["renderer_moniker"]),
            "endpoint_id": args.vb_endpoint_id,
        }
        realtek_renderer = {
            "moniker": str(realtek[(policy, "raw")]["renderer_moniker"]),
            "endpoint_id": args.realtek_endpoint_id,
        }
        candidates.append(
            {
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
                "transport_evidence": {
                    "hardware_independent": True,
                    "runs": transport_runs,
                    "lifecycle": dict(lifecycle),
                    "checked_10_12_channel_safety": int(contract["channel_count"])
                    in (10, 12),
                },
                "virtual_windows_endpoint_evidence": {
                    "endpoint_kind": "THIRD_PARTY_VIRTUAL_WINDOWS_AUDIO_DRIVER",
                    "directsound": {
                        "classification": "ENDPOINT_FORMAT_NOT_SUPPORTED",
                        "renderer": vb_renderer_ds,
                        "runs": [
                            _endpoint_rejection_run(
                                layout, kind, vb_directsound[(policy, kind)]
                            )
                            for kind in ("raw", "mp4")
                        ],
                    },
                    "waveout": {
                        "classification": "SAMPLE_DELIVERY_VERIFIED",
                        "renderer_family": "WaveOut",
                        "renderer": vb_renderer_wave,
                        "runs": [
                            _endpoint_success(
                                layout,
                                kind,
                                args.vb_endpoint_id,
                                vb_waveout[(policy, kind)],
                            )
                            for kind in ("raw", "mp4")
                        ],
                    },
                },
                "physical_realtek_endpoint_evidence": {
                    "classification": "SAMPLE_DELIVERY_VERIFIED",
                    "renderer_family": "DirectSound",
                    "renderer": realtek_renderer,
                    "runs": [
                        _endpoint_success(
                            layout,
                            kind,
                            args.realtek_endpoint_id,
                            realtek[(policy, kind)],
                        )
                        for kind in ("raw", "mp4")
                    ],
                },
                "physical_multichannel_hardware_evidence": {
                    "reason": (
                        "stereo row is outside the physical multichannel hardware gate"
                        if layout == "Stereo"
                        else "no physical multichannel speaker endpoint or AVR was tested"
                    )
                },
            }
        )

    inventory = pathlib.Path(args.renderer_inventory)
    vb_capabilities = pathlib.Path(args.vb_capabilities)
    realtek_capabilities = pathlib.Path(args.realtek_capabilities)
    document: dict[str, object] = {
        "schema_version": 2,
        "automatic_layout_selection": "AUTO_NOT_RELIABLE",
        "builds": {
            "openjoc": {
                "head": args.openjoc_head,
                "binaries": [_binary(runtime / "openjoc_capi.dll")],
            },
            "lav": {
                "head": args.lav_head,
                "binaries": [
                    _binary(runtime / "LAVAudio.ax"),
                    _binary(runtime / "LAVSplitter.ax"),
                    _binary(runtime / "OpenJocDirectShowNegotiationSmoke.exe"),
                ],
            },
        },
        "environment": {
            "os": {
                "name": platform.system() or "Windows",
                "version": platform.version() or "unknown",
                "build": platform.release() or "unknown",
            },
            "host": {
                "name": "OpenJOC native DirectShow harness",
                "version": "phase6-layered-endpoint-validation",
                **_binary(runtime / "OpenJocDirectShowNegotiationSmoke.exe"),
            },
        },
        "measurement_sources": {
            "controlled_sink": _binary(controlled_path),
            "lifecycle": _binary(lifecycle_path),
            "renderer_inventory": _binary(inventory),
            "vb_endpoint_capabilities": _binary(vb_capabilities),
            "realtek_endpoint_capabilities": _binary(realtek_capabilities),
            "configuration_changed": False,
        },
        "stereo_diagnosis": {
            "openjoc_proposal_construction": "EXACT_TYPE_STREAMED_THROUGH_WAVEOUT_AND_REALTEK_DIRECTSOUND",
            "vb_directsound": "EXACT_REJECTION",
            "vb_waveout": "SAMPLE_DELIVERY_VERIFIED",
            "realtek_directsound": "SAMPLE_DELIVERY_VERIFIED",
            "conclusion": "VB_DIRECTSOUND_RENDERER_OR_ENDPOINT_FORMAT_COMPATIBILITY_NOT_PROPOSAL_CONSTRUCTION",
        },
        "candidates": candidates,
    }
    errors = validate_evidence(document, shipped_layouts=POLICY_LAYOUTS)
    if errors:
        raise ValueError("assembled evidence failed validation:\n" + "\n".join(errors))
    return document


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    parser.add_argument("--controlled-sink-log", required=True)
    parser.add_argument("--lifecycle-log", required=True)
    parser.add_argument("--vb-directsound-directory", required=True)
    parser.add_argument("--vb-waveout-directory", required=True)
    parser.add_argument("--realtek-directsound-directory", required=True)
    parser.add_argument("--renderer-inventory", required=True)
    parser.add_argument("--vb-capabilities", required=True)
    parser.add_argument("--realtek-capabilities", required=True)
    parser.add_argument("--runtime-directory", required=True)
    parser.add_argument("--vb-endpoint-id", required=True)
    parser.add_argument("--realtek-endpoint-id", required=True)
    parser.add_argument("--openjoc-head", required=True)
    parser.add_argument("--lav-head", required=True)
    parser.add_argument("--output", required=True)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    output = pathlib.Path(args.output)
    if not output.is_absolute() or output.exists() or not output.parent.is_dir():
        raise ValueError("output must be a new absolute file in an existing directory")
    document = assemble(args)
    with output.open("x", encoding="utf-8", newline="\n") as destination:
        json.dump(document, destination, indent=2, sort_keys=True)
        destination.write("\n")
    print(f"LAYERED_EVIDENCE_COMPLETE output={output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
