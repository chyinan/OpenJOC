# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

# pattern: Functional Core

from __future__ import annotations

import ntpath
import re
from typing import Iterable, Mapping, Optional, Sequence


CANONICAL_LAYOUTS = {
    "Stereo": {
        "channel_count": 2,
        "channel_mask": "0x00000003",
        "logical_lfe_channels": 0,
    },
    "5.1": {
        "channel_count": 6,
        "channel_mask": "0x0000060f",
        "logical_lfe_channels": 1,
    },
    "7.1": {
        "channel_count": 8,
        "channel_mask": "0x0000063f",
        "logical_lfe_channels": 1,
    },
    "5.1.2": {
        "channel_count": 8,
        "channel_mask": "0x0000560f",
        "logical_lfe_channels": 1,
    },
    "5.1.4": {
        "channel_count": 10,
        "channel_mask": "0x0002d60f",
        "logical_lfe_channels": 1,
    },
    "7.1.2": {
        "channel_count": 10,
        "channel_mask": "0x0000563f",
        "logical_lfe_channels": 1,
    },
    "7.1.4": {
        "channel_count": 12,
        "channel_mask": "0x0002d63f",
        "logical_lfe_channels": 1,
    },
}

MANDATORY_LAYOUTS = ("Stereo", "5.1", "7.1")
EVIDENCE_STATES = {"STREAM_PROVEN", "UNSUPPORTED", "UNVERIFIED"}
_FORBIDDEN_SEMANTIC_FIELDS = {
    "physical_subwoofer_count",
    "friendly_name",
    "endpoint_name",
    "product_name",
    "consumer_notation",
}
_SHA256_RE = re.compile(r"^[0-9a-fA-F]{64}$")
_HEAD_RE = re.compile(r"^[0-9a-fA-F]{40}$")
_HRESULT_RE = re.compile(r"^0x[0-9a-fA-F]{8}$")
_FFMPEG_LAV_RE = re.compile(r"^[a-z0-9_]+-lav-[a-z0-9_.-]+\.dll$", re.IGNORECASE)
_REQUIRED_RUNTIME_BASENAMES = {
    "lavaudio.ax",
    "lavsplitter.ax",
    "openjoc_capi.dll",
    "libbluray.dll",
}


def _error(errors: list[str], code: str, detail: str) -> None:
    errors.append(f"{code}: {detail}")


def _is_sha256(value: object) -> bool:
    return isinstance(value, str) and _SHA256_RE.fullmatch(value) is not None


def _is_hresult(value: object) -> bool:
    return isinstance(value, str) and _HRESULT_RE.fullmatch(value) is not None


def _is_absolute_windows_path(value: object) -> bool:
    return isinstance(value, str) and ntpath.isabs(value)


def _normalized_windows_path(value: str) -> str:
    return ntpath.normcase(ntpath.normpath(value))


def _find_forbidden_fields(value: object, location: str = "document") -> tuple[str, ...]:
    found: list[str] = []
    if isinstance(value, Mapping):
        for key, child in value.items():
            child_location = f"{location}.{key}"
            if key in _FORBIDDEN_SEMANTIC_FIELDS:
                found.append(child_location)
            found.extend(_find_forbidden_fields(child, child_location))
    elif isinstance(value, list):
        for index, child in enumerate(value):
            found.extend(_find_forbidden_fields(child, f"{location}[{index}]"))
    return tuple(found)


def _validate_binary_identity(
    identity: object, location: str, errors: list[str]
) -> None:
    if not isinstance(identity, Mapping):
        _error(errors, "BINARY_IDENTITY_REQUIRED", location)
        return
    if not _is_absolute_windows_path(identity.get("path")):
        _error(errors, "BINARY_PATH_INVALID", location)
    if not _is_sha256(identity.get("sha256")):
        _error(errors, "BINARY_HASH_INVALID", location)


def _validate_builds(document: Mapping[str, object], errors: list[str]) -> None:
    builds = document.get("builds")
    if not isinstance(builds, Mapping):
        _error(errors, "BUILD_IDENTITIES_REQUIRED", "builds")
        return
    for repository in ("openjoc", "lav"):
        identity = builds.get(repository)
        if not isinstance(identity, Mapping):
            _error(errors, "BUILD_IDENTITY_REQUIRED", repository)
            continue
        if not isinstance(identity.get("head"), str) or not _HEAD_RE.fullmatch(
            identity["head"]
        ):
            _error(errors, "BUILD_HEAD_INVALID", repository)
        binaries = identity.get("binaries")
        if not isinstance(binaries, list) or not binaries:
            _error(errors, "BUILD_BINARIES_REQUIRED", repository)
            continue
        for index, binary in enumerate(binaries):
            _validate_binary_identity(binary, f"{repository}.binaries[{index}]", errors)


def _validate_environment(document: Mapping[str, object], errors: list[str]) -> None:
    environment = document.get("environment")
    if not isinstance(environment, Mapping):
        _error(errors, "ENVIRONMENT_REQUIRED", "environment")
        return
    os_identity = environment.get("os")
    if not isinstance(os_identity, Mapping) or any(
        not isinstance(os_identity.get(field), str) or not os_identity[field]
        for field in ("name", "version", "build")
    ):
        _error(errors, "OS_IDENTITY_REQUIRED", "environment.os")
    host = environment.get("host")
    if not isinstance(host, Mapping) or any(
        not isinstance(host.get(field), str) or not host[field]
        for field in ("name", "version")
    ):
        _error(errors, "HOST_IDENTITY_REQUIRED", "environment.host")
        return
    _validate_binary_identity(host, "environment.host", errors)


def _validate_fixture(fixture: object, location: str, errors: list[str]) -> None:
    if not isinstance(fixture, Mapping):
        _error(errors, "FIXTURE_IDENTITY_REQUIRED", location)
        return
    if fixture.get("kind") not in {"raw", "mp4"}:
        _error(errors, "FIXTURE_KIND_INVALID", location)
    if not _is_absolute_windows_path(fixture.get("path")):
        _error(errors, "FIXTURE_PATH_INVALID", location)
    if not _is_sha256(fixture.get("sha256")):
        _error(errors, "FIXTURE_HASH_INVALID", location)


def _validate_renderer(renderer: object, location: str, errors: list[str]) -> None:
    if not isinstance(renderer, Mapping) or any(
        not isinstance(renderer.get(field), str) or not renderer[field]
        for field in ("moniker", "endpoint_id")
    ):
        _error(errors, "RENDERER_IDENTITY_REQUIRED", location)


def _module_index(
    modules: object, location: str, errors: list[str]
) -> dict[str, Mapping[str, object]]:
    if not isinstance(modules, list):
        _error(errors, "RUNTIME_MODULE_LIST_REQUIRED", location)
        return {}
    result: dict[str, Mapping[str, object]] = {}
    for index, module in enumerate(modules):
        module_location = f"{location}[{index}]"
        if not isinstance(module, Mapping):
            _error(errors, "RUNTIME_MODULE_IDENTITY_REQUIRED", module_location)
            continue
        basename = module.get("basename")
        if not isinstance(basename, str) or not basename or basename != ntpath.basename(basename):
            _error(errors, "RUNTIME_MODULE_BASENAME_INVALID", module_location)
            continue
        key = basename.casefold()
        if key in result:
            _error(errors, "RUNTIME_MODULE_DUPLICATE_BASENAME", basename)
            continue
        result[key] = module
        if not _is_absolute_windows_path(module.get("path")):
            _error(errors, "RUNTIME_MODULE_PATH_INVALID", module_location)
        if not _is_sha256(module.get("sha256")):
            _error(errors, "RUNTIME_MODULE_HASH_INVALID", module_location)
    return result


def _validate_runtime_verification(
    runtime: object, host: str, location: str, errors: list[str]
) -> None:
    if not isinstance(runtime, Mapping):
        _error(errors, "RUNTIME_VERIFICATION_REQUIRED", location)
        return
    expected_kind = (
        "PRIVATE_COM_AND_PROCESS_ENUMERATION"
        if host == "native"
        else "POTPLAYER_IN_PROCESS_ENUMERATION_AFTER_GRAPH"
    )
    if runtime.get("kind") != expected_kind:
        _error(errors, "RUNTIME_VERIFICATION_KIND_INVALID", location)
    if host == "potplayer" and runtime.get("observed_after_graph_creation") is not True:
        _error(errors, "POTPLAYER_RUNTIME_NOT_OBSERVED_AFTER_GRAPH", location)

    manifest = runtime.get("manifest")
    if not isinstance(manifest, Mapping):
        _error(errors, "DEPENDENCY_MANIFEST_REQUIRED", location)
        return
    if not _is_absolute_windows_path(manifest.get("path")) or not _is_sha256(
        manifest.get("sha256")
    ):
        _error(errors, "DEPENDENCY_MANIFEST_IDENTITY_INVALID", location)
    manifest_index = _module_index(
        manifest.get("modules"), f"{location}.manifest.modules", errors
    )
    process_index = _module_index(
        runtime.get("process_modules"), f"{location}.process_modules", errors
    )

    ffmpeg_names = runtime.get("loaded_ffmpeg_lav_basenames")
    if not isinstance(ffmpeg_names, list) or not ffmpeg_names:
        _error(errors, "LOADED_FFMPEG_MODULES_REQUIRED", location)
        ffmpeg_keys: set[str] = set()
    else:
        ffmpeg_keys = set()
        for name in ffmpeg_names:
            if not isinstance(name, str) or _FFMPEG_LAV_RE.fullmatch(name) is None:
                _error(errors, "LOADED_FFMPEG_MODULE_NAME_INVALID", location)
                continue
            key = name.casefold()
            if key in ffmpeg_keys:
                _error(errors, "RUNTIME_MODULE_DUPLICATE_BASENAME", name)
            ffmpeg_keys.add(key)

    required_keys = _REQUIRED_RUNTIME_BASENAMES | ffmpeg_keys
    for basename in sorted(required_keys):
        if basename not in manifest_index:
            _error(errors, "MANIFEST_MODULE_MISSING", basename)
        if basename not in process_index:
            _error(errors, "RUNTIME_MODULE_MISSING", basename)
            continue
        if basename not in manifest_index:
            continue
        manifest_module = manifest_index[basename]
        process_module = process_index[basename]
        manifest_path = manifest_module.get("path")
        process_path = process_module.get("path")
        if (
            isinstance(manifest_path, str)
            and isinstance(process_path, str)
            and _normalized_windows_path(manifest_path)
            != _normalized_windows_path(process_path)
        ):
            _error(errors, "RUNTIME_MODULE_PATH_MISMATCH", basename)
        manifest_hash = manifest_module.get("sha256")
        process_hash = process_module.get("sha256")
        if (
            isinstance(manifest_hash, str)
            and isinstance(process_hash, str)
            and manifest_hash.casefold() != process_hash.casefold()
        ):
            _error(errors, "RUNTIME_MODULE_HASH_MISMATCH", basename)

    if host == "native":
        private_index = _module_index(
            runtime.get("private_com_modules"),
            f"{location}.private_com_modules",
            errors,
        )
        for basename in ("lavaudio.ax", "lavsplitter.ax"):
            if basename not in private_index:
                _error(errors, "PRIVATE_COM_MODULE_MISSING", basename)
                continue
            if basename not in manifest_index:
                continue
            private_module = private_index[basename]
            manifest_module = manifest_index[basename]
            if _normalized_windows_path(str(private_module.get("path", ""))) != (
                _normalized_windows_path(str(manifest_module.get("path", "")))
            ):
                _error(errors, "RUNTIME_MODULE_PATH_MISMATCH", basename)
            if str(private_module.get("sha256", "")).casefold() != str(
                manifest_module.get("sha256", "")
            ).casefold():
                _error(errors, "RUNTIME_MODULE_HASH_MISMATCH", basename)


def _expected_media_type(contract: Mapping[str, object]) -> dict[str, object]:
    channels = int(contract["channel_count"])
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
    }


def _validate_media_type(
    media_type: object,
    contract: Mapping[str, object],
    location: str,
    errors: list[str],
) -> None:
    if not isinstance(media_type, Mapping):
        _error(errors, "EXACT_MEDIA_TYPE_REQUIRED", location)
        return
    expected = _expected_media_type(contract)
    if any(media_type.get(key) != value for key, value in expected.items()) or not _is_sha256(
        media_type.get("format_bytes_sha256")
    ):
        _error(errors, "EXACT_MEDIA_TYPE_INVALID", location)


def _validate_positive_counters(
    counters: object, location: str, errors: list[str]
) -> None:
    if not isinstance(counters, Mapping):
        _error(errors, "SAMPLES_NOT_DELIVERED", location)
        return
    if not isinstance(counters.get("samples"), int) or counters["samples"] <= 0:
        _error(errors, "SAMPLES_NOT_DELIVERED", location)
    if not isinstance(counters.get("bytes"), int) or counters["bytes"] <= 0:
        _error(errors, "BYTES_NOT_DELIVERED", location)
    if counters.get("eos") is not True:
        _error(errors, "EOS_NOT_OBSERVED", location)
    if not isinstance(counters.get("lifecycle_events"), int) or counters[
        "lifecycle_events"
    ] <= 0:
        _error(errors, "LIFECYCLE_COUNTERS_REQUIRED", location)


def _runs_by_fixture_kind(
    runs: object, location: str, errors: list[str]
) -> dict[str, Mapping[str, object]]:
    if not isinstance(runs, list):
        _error(errors, "STREAM_PROOF_INCOMPLETE", location)
        return {}
    result: dict[str, Mapping[str, object]] = {}
    for index, run in enumerate(runs):
        if not isinstance(run, Mapping):
            _error(errors, "STREAM_RUN_INVALID", f"{location}[{index}]")
            continue
        fixture = run.get("fixture")
        kind = fixture.get("kind") if isinstance(fixture, Mapping) else None
        if kind in result:
            _error(errors, "DUPLICATE_FIXTURE_KIND", f"{location}.{kind}")
        elif isinstance(kind, str):
            result[kind] = run
    for kind in ("raw", "mp4"):
        if kind not in result:
            _error(errors, "STREAM_PROOF_INCOMPLETE", f"{location}.{kind}")
    return result


def _validate_native_run(
    run: Mapping[str, object],
    layout: str,
    contract: Mapping[str, object],
    renderer: object,
    location: str,
    errors: list[str],
) -> None:
    _validate_fixture(run.get("fixture"), f"{location}.fixture", errors)
    _validate_renderer(run.get("renderer"), f"{location}.renderer", errors)
    if run.get("renderer") != renderer:
        _error(errors, "RENDERER_INSTANCE_MISMATCH", location)
    if run.get("policy_selection") != "FIXED_POLICY_ENUM":
        _error(errors, "FIXED_POLICY_SELECTION_REQUIRED", location)
    connect = run.get("connect_direct")
    if not isinstance(connect, Mapping) or any(
        (
            connect.get("attempted") is not True,
            connect.get("exact") is not True,
            connect.get("hresult") != "0x00000000",
            connect.get("proposal_count") != 1,
        )
    ):
        _error(errors, "EXACT_CONNECT_DIRECT_REQUIRED", location)
    requested = run.get("requested_type")
    pre_stream = run.get("pre_stream_connection_type")
    post_stream = run.get("post_stream_connection_type")
    _validate_media_type(requested, contract, f"{location}.requested_type", errors)
    _validate_media_type(pre_stream, contract, f"{location}.pre_stream_type", errors)
    _validate_media_type(post_stream, contract, f"{location}.post_stream_type", errors)
    if requested != pre_stream or requested != post_stream:
        _error(errors, "CONNECTION_TYPE_CHANGED", location)
    states = run.get("states")
    if not isinstance(states, Mapping) or states.get("pause_hresult") != "0x00000000":
        _error(errors, "PAUSE_STATE_NOT_PROVEN", location)
    if not isinstance(states, Mapping) or states.get("run_hresult") != "0x00000000":
        _error(errors, "RUN_STATE_NOT_PROVEN", location)
    _validate_positive_counters(run.get("counters"), f"{location}.counters", errors)
    if run.get("fallback_proposals") != 0:
        _error(errors, "FALLBACK_OBSERVED", location)
    if run.get("type_mutations") != 0:
        _error(errors, "TYPE_MUTATION_OBSERVED", location)
    if run.get("graph_errors") != 0:
        _error(errors, "GRAPH_ERROR_OBSERVED", location)
    _validate_runtime_verification(
        run.get("runtime_verification"), "native", f"{location}.runtime", errors
    )


def _validate_potplayer_run(
    run: Mapping[str, object],
    layout: str,
    contract: Mapping[str, object],
    renderer: object,
    location: str,
    errors: list[str],
) -> None:
    _validate_fixture(run.get("fixture"), f"{location}.fixture", errors)
    _validate_renderer(run.get("renderer"), f"{location}.renderer", errors)
    if run.get("renderer") != renderer:
        _error(errors, "RENDERER_INSTANCE_MISMATCH", location)
    if run.get("policy_selection") != "FIXED_POLICY_ENUM":
        _error(errors, "FIXED_POLICY_SELECTION_REQUIRED", location)
    source_as_output = run.get("source_as_output")
    if not isinstance(source_as_output, Mapping) or source_as_output.get(
        "confirmed_in_visible_ui"
    ) is not True:
        _error(errors, "POTPLAYER_SOURCE_AS_OUTPUT_NOT_CONFIRMED", location)
    same_instance = run.get("same_instance")
    if not isinstance(same_instance, Mapping) or same_instance.get(
        "observation"
    ) != "CONNECTED_LAV_AUDIO_STATUS_PAGE" or not isinstance(
        same_instance.get("graph_instance_id"), str
    ) or not same_instance.get("graph_instance_id"):
        _error(errors, "POTPLAYER_SAME_INSTANCE_REQUIRED", location)
    else:
        if same_instance.get("policy") != layout:
            _error(errors, "POTPLAYER_POLICY_MISMATCH", location)
        if same_instance.get("admission") != "OpenJoc":
            _error(errors, "POTPLAYER_ADMISSION_MISMATCH", location)
        expected_output = {
            "format": "float32",
            "sample_rate": 48000,
            "channel_count": contract["channel_count"],
            "channel_mask": contract["channel_mask"],
        }
        if same_instance.get("output") != expected_output:
            _error(errors, "POTPLAYER_OUTPUT_MISMATCH", location)
    playback = run.get("playback")
    if not isinstance(playback, Mapping) or any(
        (
            playback.get("graph_created") is not True,
            playback.get("progressed") is not True,
            playback.get("eos") is not True,
            playback.get("graph_errors") != 0,
        )
    ):
        _error(errors, "POTPLAYER_PLAYBACK_NOT_PROVEN", location)
    _validate_positive_counters(run.get("counters"), f"{location}.counters", errors)
    _validate_runtime_verification(
        run.get("runtime_verification"), "potplayer", f"{location}.runtime", errors
    )


def _validate_stream_proven_row(
    row: Mapping[str, object],
    layout: str,
    contract: Mapping[str, object],
    location: str,
    errors: list[str],
) -> None:
    if row.get("reason") is not None or row.get("failure") is not None:
        _error(errors, "STREAM_PROVEN_HAS_FAILURE_METADATA", location)
    renderer = row.get("renderer")
    _validate_renderer(renderer, f"{location}.renderer", errors)
    native_runs = _runs_by_fixture_kind(
        row.get("native_runs"), f"{location}.native_runs", errors
    )
    potplayer_runs = _runs_by_fixture_kind(
        row.get("potplayer_runs"), f"{location}.potplayer_runs", errors
    )
    if not native_runs or not potplayer_runs:
        _error(errors, "STREAM_PROOF_INCOMPLETE", location)
    for kind in ("raw", "mp4"):
        if kind in native_runs:
            _validate_native_run(
                native_runs[kind],
                layout,
                contract,
                renderer,
                f"{location}.native_runs.{kind}",
                errors,
            )
        if kind in potplayer_runs:
            _validate_potplayer_run(
                potplayer_runs[kind],
                layout,
                contract,
                renderer,
                f"{location}.potplayer_runs.{kind}",
                errors,
            )


def _validate_unsupported_row(
    row: Mapping[str, object],
    contract: Mapping[str, object],
    location: str,
    errors: list[str],
) -> None:
    failure = row.get("failure")
    if not isinstance(failure, Mapping):
        _error(errors, "UNSUPPORTED_REQUIRES_MEASURED_FAILURE", location)
        return
    if failure.get("measured") is not True:
        _error(errors, "UNSUPPORTED_REQUIRES_MEASURED_FAILURE", location)
    if not isinstance(failure.get("stage"), str) or not failure.get("stage") or not _is_hresult(
        failure.get("hresult")
    ):
        _error(errors, "UNSUPPORTED_FAILURE_DETAILS_REQUIRED", location)
    kind = failure.get("kind")
    if kind not in {"EXACT_REJECTION", "TYPE_MUTATION"}:
        _error(errors, "UNSUPPORTED_FAILURE_KIND_INVALID", location)
    if kind == "EXACT_REJECTION" and failure.get("hresult") == "0x00000000":
        _error(errors, "UNSUPPORTED_REJECTION_HRESULT_INVALID", location)
    if failure.get("proposal_count") != 1 or failure.get("fallback_proposals") != 0:
        _error(errors, "UNSUPPORTED_EXACT_ATTEMPT_INVALID", location)
    _validate_renderer(row.get("renderer"), f"{location}.renderer", errors)
    _validate_renderer(failure.get("renderer"), f"{location}.failure.renderer", errors)
    if failure.get("renderer") != row.get("renderer"):
        _error(errors, "RENDERER_INSTANCE_MISMATCH", location)
    _validate_fixture(failure.get("fixture"), f"{location}.failure.fixture", errors)
    requested = failure.get("requested_type")
    actual = failure.get("actual_type")
    _validate_media_type(requested, contract, f"{location}.failure.requested_type", errors)
    if kind == "EXACT_REJECTION" and actual is not None:
        _error(errors, "UNSUPPORTED_REJECTION_HAS_ACTUAL_TYPE", location)
    if kind == "TYPE_MUTATION":
        if not isinstance(actual, Mapping) or actual == requested:
            _error(errors, "UNSUPPORTED_MUTATION_NOT_PROVEN", location)
    _validate_runtime_verification(
        failure.get("runtime_verification"),
        "native",
        f"{location}.failure.runtime",
        errors,
    )


def _validate_candidate(
    row: Mapping[str, object], location: str, errors: list[str]
) -> Optional[str]:
    layout = row.get("layout")
    if isinstance(layout, str) and layout.casefold() == "auto":
        _error(errors, "AUTO_LAYOUT_FORBIDDEN", location)
        return None
    if layout not in CANONICAL_LAYOUTS:
        _error(errors, "UNKNOWN_LAYOUT", f"{location}.{layout}")
        return None
    contract = CANONICAL_LAYOUTS[layout]
    if row.get("channel_count") != contract["channel_count"]:
        _error(errors, "CHANNEL_COUNT_MISMATCH", layout)
    if row.get("channel_mask") != contract["channel_mask"]:
        _error(errors, "CHANNEL_MASK_MISMATCH", layout)
    if row.get("logical_lfe_channels") != contract["logical_lfe_channels"]:
        _error(errors, "LOGICAL_LFE_MISMATCH", layout)
    status = row.get("status")
    if status not in EVIDENCE_STATES:
        _error(errors, "EVIDENCE_STATE_INVALID", layout)
    elif status == "STREAM_PROVEN":
        _validate_stream_proven_row(row, layout, contract, location, errors)
    elif status == "UNSUPPORTED":
        _validate_unsupported_row(row, contract, location, errors)
    elif not isinstance(row.get("reason"), str) or not row.get("reason"):
        _error(errors, "UNVERIFIED_REASON_REQUIRED", layout)
    return layout


def stream_proven_layouts(document: object) -> tuple[str, ...]:
    if not isinstance(document, Mapping) or not isinstance(document.get("candidates"), list):
        return ()
    statuses = {
        row.get("layout"): row.get("status")
        for row in document["candidates"]
        if isinstance(row, Mapping)
    }
    return tuple(
        layout
        for layout in CANONICAL_LAYOUTS
        if statuses.get(layout) == "STREAM_PROVEN"
    )


def validate_evidence(
    document: object, shipped_layouts: Optional[Sequence[str]] = None
) -> tuple[str, ...]:
    """Return deterministic fail-closed evidence validation errors."""
    errors: list[str] = []
    if not isinstance(document, Mapping) or not document:
        return ("EVIDENCE_DOCUMENT_REQUIRED: expected a non-empty JSON object",)
    for forbidden_location in _find_forbidden_fields(document):
        _error(errors, "FORBIDDEN_SEMANTIC_FIELD", forbidden_location)
    if document.get("schema_version") != 1:
        _error(errors, "SCHEMA_VERSION_INVALID", "expected 1")
    if document.get("automatic_layout_selection") != "AUTO_NOT_RELIABLE":
        _error(errors, "AUTO_RELIABILITY_MARKER_REQUIRED", "automatic_layout_selection")
    _validate_builds(document, errors)
    _validate_environment(document, errors)

    candidates = document.get("candidates")
    rows_by_layout: dict[str, Mapping[str, object]] = {}
    if not isinstance(candidates, list) or not candidates:
        _error(errors, "EVIDENCE_ROWS_REQUIRED", "candidates")
    else:
        for index, row in enumerate(candidates):
            if not isinstance(row, Mapping):
                _error(errors, "EVIDENCE_ROW_INVALID", f"candidates[{index}]")
                continue
            layout = _validate_candidate(row, f"candidates[{index}]", errors)
            if layout is None:
                continue
            if layout in rows_by_layout:
                _error(errors, "DUPLICATE_LAYOUT_ROW", layout)
            else:
                rows_by_layout[layout] = row
    for layout in CANONICAL_LAYOUTS:
        if layout not in rows_by_layout:
            _error(errors, "LAYOUT_ROW_MISSING", layout)
    for layout in MANDATORY_LAYOUTS:
        row = rows_by_layout.get(layout)
        if row is None or row.get("status") != "STREAM_PROVEN":
            _error(errors, "MANDATORY_LAYOUT_NOT_STREAM_PROVEN", layout)

    if shipped_layouts is not None:
        proven = stream_proven_layouts(document)
        shipped = tuple(shipped_layouts)
        if shipped != proven:
            _error(
                errors,
                "SHIPPED_LAYOUT_MISMATCH",
                f"shipped={list(shipped)!r} stream_proven={list(proven)!r}",
            )
    return tuple(errors)
