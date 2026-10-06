#!/usr/bin/env python3
"""Paired same-backend exactness/allocation/timing gate for FFT scratch reuse."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import statistics
import struct
import subprocess
import sys

from verify_pcm_bitexact import (
    BASELINE_LOCK_SHA256,
    BASELINE_REVISION,
    GateError,
    PARTITIONED_PROBE,
    git_head,
    hash_file,
    paths_overlap,
    validate_baseline_worktree,
    validate_versioned_dependencies,
)


PROBE_NAME = "partitioned_fft_scratch_probe"
PACKAGE = "openjoc-render"
SAMPLE_BYTES = 8
FRAME_BYTES = 16  # interleaved stereo f64le


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def fnv1a64(data: bytes) -> int:
    value = 0xCBF29CE484222325
    for byte in data:
        value = ((value ^ byte) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return value


def validate_input_fixture(path: Path) -> dict[str, object]:
    raw = path.read_bytes()
    if raw[:8] != b"OJPTD001" or len(raw) < 12:
        raise GateError("unsupported or truncated shared partitioned input fixture")
    scenario_count = struct.unpack_from("<I", raw, 8)[0]
    if scenario_count != 4:
        raise GateError(f"shared input fixture has {scenario_count} scenarios, expected 4")
    cursor = 12
    scenarios: list[tuple[list[float], list[float], bytes]] = []
    for _ in range(scenario_count):
        if cursor + 8 > len(raw):
            raise GateError("truncated shared partitioned input fixture header")
        sample_count = struct.unpack_from("<Q", raw, cursor)[0]
        cursor += 8
        payload_bytes = sample_count * 2 * SAMPLE_BYTES
        end = cursor + payload_bytes
        if end > len(raw):
            raise GateError("truncated shared partitioned input samples")
        payload = raw[cursor:end]
        values = [value for (value,) in struct.iter_unpack("<d", payload)]
        if any(not math.isfinite(value) for value in values):
            raise GateError("shared partitioned input fixture contains non-finite samples")
        scenarios.append((values[:sample_count], values[sample_count:], payload))
        cursor = end
    if cursor != len(raw):
        raise GateError("shared partitioned input fixture has trailing bytes")
    if [len(scenario[0]) for scenario in scenarios] != [791, 77, 613, 791]:
        raise GateError("shared partitioned input scenario lengths differ from the intended corpus")
    if scenarios[0][2] != scenarios[3][2]:
        raise GateError("replayed nonzero input fixture differs")
    if any(value != 0.0 for value in scenarios[1][0] + scenarios[1][1]):
        raise GateError("intended silence fixture contains nonzero input")
    if not any(value != 0.0 for value in scenarios[0][0] + scenarios[0][1]):
        raise GateError("first input scenario is silent")
    if not any(value != 0.0 for value in scenarios[2][0] + scenarios[2][1]):
        raise GateError("third input scenario is silent")
    return {
        "sha256": hashlib.sha256(raw).hexdigest(),
        "fnv1a64": f"{fnv1a64(raw):016x}",
        "byte_count": len(raw),
        "scenario_sample_counts": [len(scenario[0]) for scenario in scenarios],
        "silence_scenario_index": 1,
    }


def find_difference(expected: bytes, actual: bytes) -> int:
    return next(
        (index for index, (left, right) in enumerate(zip(expected, actual)) if left != right),
        min(len(expected), len(actual)),
    )


def validate_capture(
    manifest_path: Path,
    pcm_path: Path,
    partition_size: int,
    input_fixture_fnv1a64: str,
) -> dict[str, object]:
    try:
        lines = manifest_path.read_text(encoding="ascii").splitlines()
    except OSError as error:
        raise GateError(f"cannot read partitioned manifest {manifest_path}: {error}") from error
    if not lines or lines[0] != "openjoc-partitioned-pcm-probe\t1":
        raise GateError(f"{manifest_path}: unsupported partitioned probe manifest")
    headers: dict[str, str] = {}
    results: dict[str, str] = {}
    input_records: list[list[str]] = []
    frames: list[list[str]] = []
    scenarios: list[list[str]] = []
    for number, line in enumerate(lines[1:], start=2):
        fields = line.split("\t")
        if fields[0] == "H" and len(fields) == 3:
            if fields[1] in headers:
                raise GateError(f"{manifest_path}:{number}: duplicate header {fields[1]}")
            headers[fields[1]] = fields[2]
        elif fields[0] == "I" and len(fields) == 5:
            input_records.append(fields[1:])
        elif fields[0] == "F" and len(fields) == 8:
            frames.append(fields[1:])
        elif fields[0] == "S" and len(fields) == 6:
            scenarios.append(fields[1:])
        elif fields[0] == "R" and len(fields) == 3:
            if fields[1] in results:
                raise GateError(f"{manifest_path}:{number}: duplicate result {fields[1]}")
            results[fields[1]] = fields[2]
        else:
            raise GateError(f"{manifest_path}:{number}: malformed manifest record")

    required_headers = {
        "sample_rate_hz",
        "partition_size",
        "fft_size",
        "source_count",
        "hrir_tap_counts",
        "scenario_count",
        "pcm_format",
        "input_fixture_fnv1a64",
    }
    if not required_headers.issubset(headers):
        raise GateError(f"{manifest_path}: missing a required header")
    if (
        headers["sample_rate_hz"] != "48000"
        or int(headers["partition_size"]) != partition_size
        or int(headers["fft_size"]) != partition_size * 2
    ):
        raise GateError(f"{manifest_path}: unexpected partitioned configuration")
    if headers["source_count"] != "2" or headers["hrir_tap_counts"] != "300,533":
        raise GateError(f"{manifest_path}: expected two active sources with unequal HRIR lengths")
    if headers["input_fixture_fnv1a64"] != input_fixture_fnv1a64:
        raise GateError(f"{manifest_path}: input-fixture digest differs from the shared sidecar")
    if headers["scenario_count"] != "4" or headers["pcm_format"] != "f64le-interleaved-stereo":
        raise GateError(f"{manifest_path}: unexpected capture shape")
    required_results = {"frame_count", "pcm_bytes", "input_f64le_fnv1a64", "reset_replay", "terminal_state"}
    if not required_results.issubset(results):
        raise GateError(f"{manifest_path}: missing a required result")
    if len(input_records) != 4 or len(scenarios) != 4 or not frames:
        raise GateError(f"{manifest_path}: expected 4 input/output scenarios and frame records")
    expected_input_lengths = [791, 77, 613, 791]
    for index, input_record in enumerate(input_records):
        if input_record[0] != str(index) or int(input_record[1]) != expected_input_lengths[index] or int(input_record[2]) != expected_input_lengths[index]:
            raise GateError(f"{manifest_path}: input scenario shape differs from the shared corpus")
    if int(results["frame_count"]) != len(frames):
        raise GateError(f"{manifest_path}: frame descriptor count differs from frame_count")
    if results["reset_replay"] != "bit-identical" or results["terminal_state"] != "finished-after-each-tail":
        raise GateError(f"{manifest_path}: missing reset/tail lifecycle audit")
    if input_records[0][1] != input_records[3][1] or input_records[0][2] != input_records[3][2]:
        raise GateError(f"{manifest_path}: reset replay input shape differs")

    input_phases = {frame[2] for frame in frames}
    if not {"input", "finish", "drain"}.issubset(input_phases):
        raise GateError(f"{manifest_path}: missing full-input, partial-final, or tail-drain coverage")
    orders = {frame[6] for frame in frames if frame[2] in {"input", "finish"}}
    if orders != {"10,20", "20,10"}:
        raise GateError(f"{manifest_path}: source-block permutation coverage is incomplete")
    drain_sizes = {int(frame[5]) for frame in frames if frame[2] == "drain"}
    if len(drain_sizes) < 4 or 1 not in drain_sizes:
        raise GateError(f"{manifest_path}: tail was not drained in varied chunk sizes")

    pcm_size = int(results["pcm_bytes"])
    try:
        file_size = pcm_path.stat().st_size
    except OSError as error:
        raise GateError(f"cannot stat partitioned PCM stream {pcm_path}: {error}") from error
    if pcm_size <= 0 or file_size != pcm_size or pcm_size % FRAME_BYTES:
        raise GateError(f"{pcm_path}: empty, truncated, or misaligned PCM stream")
    described_samples = sum(int(frame[5]) for frame in frames)
    if described_samples * FRAME_BYTES != pcm_size:
        raise GateError(f"{manifest_path}: descriptors do not account for every PCM byte")

    scenario_bytes: list[int] = []
    for expected_index, record in enumerate(scenarios):
        index, sample_count, byte_count, _digest, nonzero = record
        if int(index) != expected_index or int(byte_count) != int(sample_count) * FRAME_BYTES:
            raise GateError(f"{manifest_path}: malformed scenario output shape")
        left_nonzero, right_nonzero = (int(value) for value in nonzero.split(","))
        if expected_index == 1 and (left_nonzero != 0 or right_nonzero != 0):
            raise GateError(f"{manifest_path}: explicit silence scenario is not silent")
        if expected_index != 1 and (left_nonzero <= 0 or right_nonzero <= 0):
            raise GateError(f"{manifest_path}: a scenario/channel produced no nonzero PCM")
        scenario_bytes.append(int(byte_count))
    if sum(scenario_bytes) != pcm_size:
        raise GateError(f"{manifest_path}: scenario byte counts do not cover PCM")

    with pcm_path.open("rb") as stream:
        for index, (value,) in enumerate(struct.iter_unpack("<d", stream.read())):
            if not math.isfinite(value):
                raise GateError(f"{pcm_path}: non-finite PCM scalar at index {index}")

    offset = 0
    if scenario_bytes[0] != scenario_bytes[3]:
        raise GateError(f"{manifest_path}: replayed scenario byte counts differ")
    with pcm_path.open("rb") as stream:
        first = stream.read(scenario_bytes[0])
        silence = stream.read(scenario_bytes[1])
        offset += scenario_bytes[0] + scenario_bytes[1] + scenario_bytes[2]
        stream.seek(offset)
        replay = stream.read(scenario_bytes[3])
    if first != replay:
        raise GateError(f"{pcm_path}: reset/source-order replay is not bit-identical")
    if any(value != 0.0 for (value,) in struct.iter_unpack("<d", silence)):
        raise GateError(f"{pcm_path}: zero-input scenario emitted nonzero PCM")

    return {
        "headers": headers,
        "results": results,
        "input_records": input_records,
        "frames": frames,
        "scenario_bytes": scenario_bytes,
        "pcm_bytes": pcm_size,
        "pcm_sha256": sha256_file(pcm_path),
    }


def compare_capture(
    baseline_manifest: Path,
    baseline_pcm: Path,
    candidate_manifest: Path,
    candidate_pcm: Path,
    partition_size: int,
    input_fixture_fnv1a64: str,
) -> tuple[dict[str, object], dict[str, object]]:
    baseline = validate_capture(baseline_manifest, baseline_pcm, partition_size, input_fixture_fnv1a64)
    candidate = validate_capture(candidate_manifest, candidate_pcm, partition_size, input_fixture_fnv1a64)
    expected = baseline_manifest.read_bytes()
    actual = candidate_manifest.read_bytes()
    if expected != actual:
        raise GateError(f"partitioned descriptor mismatch at byte {find_difference(expected, actual)}")
    expected_length = int(baseline["pcm_bytes"])
    if expected_length != int(candidate["pcm_bytes"]):
        raise GateError("partitioned PCM byte counts differ")
    compare_pcm_streams(baseline_pcm, candidate_pcm, expected_length)
    return baseline, candidate


def compare_pcm_streams(baseline_pcm: Path, candidate_pcm: Path, expected_length: int) -> None:
    for path in (baseline_pcm, candidate_pcm):
        try:
            size = path.stat().st_size
        except OSError as error:
            raise GateError(f"cannot stat partitioned PCM stream {path}: {error}") from error
        if size != expected_length:
            raise GateError(f"{path}: PCM byte length {size} differs from expected {expected_length}")
    offset = 0
    with baseline_pcm.open("rb") as expected_stream, candidate_pcm.open("rb") as actual_stream:
        while True:
            left = expected_stream.read(1024 * 1024)
            right = actual_stream.read(1024 * 1024)
            if left != right:
                byte_offset = offset + find_difference(left, right)
                word_offset = byte_offset - byte_offset % SAMPLE_BYTES
                with baseline_pcm.open("rb") as expected_bits, candidate_pcm.open("rb") as actual_bits:
                    expected_bits.seek(word_offset)
                    actual_bits.seek(word_offset)
                    expected_word = expected_bits.read(SAMPLE_BYTES).ljust(SAMPLE_BYTES, b"\0")
                    actual_word = actual_bits.read(SAMPLE_BYTES).ljust(SAMPLE_BYTES, b"\0")
                raise GateError(
                    f"partitioned PCM bit mismatch at byte {byte_offset}: "
                    f"baseline=0x{int.from_bytes(expected_word, 'little'):016x}; "
                    f"candidate=0x{int.from_bytes(actual_word, 'little'):016x}"
                )
            if not left:
                break
            offset += len(left)
    if offset != expected_length:
        raise GateError(f"partitioned PCM streams ended at {offset}, expected {expected_length}")


def run_command(command: list[str], *, cwd: Path, env: dict[str, str], log_path: Path) -> None:
    log_path.parent.mkdir(parents=True, exist_ok=True)
    with log_path.open("wb") as output:
        process = subprocess.run(command, cwd=cwd, env=env, stdout=output, stderr=subprocess.STDOUT, check=False)
    if process.returncode != 0:
        tail = log_path.read_text(encoding="utf-8", errors="replace")[-6000:]
        raise GateError(f"command failed ({process.returncode}): {' '.join(command)}\n{tail}")


def build_probe(
    root: Path,
    target: Path,
    *,
    label: str,
    log_dir: Path,
    env: dict[str, str],
    online: bool,
    allocation: bool = False,
) -> Path:
    cargo_env = env.copy()
    cargo_env.update(
        {
            "CARGO_TARGET_DIR": str(target),
            "CARGO_BUILD_JOBS": "1",
            "CARGO_INCREMENTAL": "0",
            "CARGO_TERM_COLOR": "never",
        }
    )
    common = ["cargo", "+1.89.0"]
    if allocation:
        command = common + ["rustc", "--release", "--locked"]
    else:
        command = common + ["build", "--release", "--locked"]
    if not online:
        command.append("--offline")
    command.extend(["-p", PACKAGE, "--example", PROBE_NAME])
    if allocation:
        command.extend(["--", "--cfg", "openjoc_alloc_probe"])
    run_command(
        command,
        cwd=root,
        env=cargo_env,
        log_path=log_dir / f"build-{label}-{'alloc' if allocation else 'plain'}.log",
    )
    binary = target / "release" / "examples" / PROBE_NAME
    if not binary.is_file():
        raise GateError(f"build reported success but probe binary is missing: {binary}")
    return binary


def run_probe(binary: Path, args: list[str], *, cwd: Path, env: dict[str, str], log_path: Path) -> str:
    log_path.parent.mkdir(parents=True, exist_ok=True)
    process = subprocess.run(
        [str(binary), *args], cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False
    )
    output = process.stdout.decode("utf-8", errors="replace")
    log_path.write_text(output, encoding="utf-8")
    if process.returncode != 0:
        raise GateError(f"probe failed ({process.returncode}): {binary.name} {' '.join(args)}\n{output[-6000:]}")
    return output.strip()


def parse_alloc(output: str) -> dict[str, int | str]:
    fields = output.split(",")
    if len(fields) != 7 or fields[0] != "alloc":
        raise GateError(f"malformed allocation probe output: {output!r}")
    return {
        "calls": int(fields[1]),
        "allocations": int(fields[2]),
        "reallocations": int(fields[3]),
        "deallocations": int(fields[4]),
        "allocated_bytes": int(fields[5]),
        "output_digest": fields[6],
    }


def parse_timing(output: str) -> dict[str, int | float | str]:
    fields = output.split(",")
    if len(fields) != 5 or fields[0] != "timing":
        raise GateError(f"malformed timing probe output: {output!r}")
    return {
        "calls": int(fields[1]),
        "elapsed_ns": int(fields[2]),
        "ns_per_block": float(fields[3]),
        "output_digest": fields[4],
    }


def run_integrated_mutation_test(
    baseline_manifest: Path,
    baseline_pcm: Path,
    candidate_manifest: Path,
    candidate_pcm: Path,
    temp_root: Path,
    partition_size: int,
    input_fixture_fnv1a64: str,
) -> None:
    mutated = temp_root / "mutated-candidate.pcm64le"
    baseline = validate_capture(baseline_manifest, baseline_pcm, partition_size, input_fixture_fnv1a64)
    mutation_offset = int(baseline["scenario_bytes"][0]) + int(baseline["scenario_bytes"][1])
    shutil.copyfile(candidate_pcm, mutated)
    with mutated.open("r+b") as stream:
        stream.seek(mutation_offset)
        first = stream.read(1)
        stream.seek(mutation_offset)
        stream.write(bytes([first[0] ^ 1]))
    try:
        compare_capture(
            baseline_manifest,
            baseline_pcm,
            candidate_manifest,
            mutated,
            partition_size,
            input_fixture_fnv1a64,
        )
    except GateError as error:
        if "partitioned PCM bit mismatch" not in str(error):
            raise GateError(f"partitioned bit-mutation self-test rejected for the wrong reason: {error}") from error
    else:
        raise GateError("partitioned bit-mutation self-test failed to reject changed PCM")
    mutated.unlink(missing_ok=True)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-root", type=Path)
    parser.add_argument("--candidate-root", type=Path)
    parser.add_argument("--baseline-target", type=Path)
    parser.add_argument("--candidate-target", type=Path)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--timing-pairs", type=int, default=5)
    parser.add_argument("--online", action="store_true", help="allow Cargo to fetch locked dependencies")
    args = parser.parse_args(argv)
    if args.timing_pairs < 5:
        parser.error("--timing-pairs must be at least five")
    required = (args.baseline_root, args.candidate_root, args.baseline_target, args.candidate_target, args.output_dir)
    if any(value is None for value in required):
        parser.error("provide baseline/candidate roots, targets, and output directory")
    baseline_root = args.baseline_root.resolve()
    candidate_root = args.candidate_root.resolve()
    baseline_target = args.baseline_target.resolve()
    candidate_target = args.candidate_target.resolve()
    output_dir = args.output_dir.resolve()
    if baseline_root == candidate_root or paths_overlap(baseline_root, candidate_root):
        parser.error("baseline/candidate roots must be distinct and non-overlapping")
    if paths_overlap(baseline_target, candidate_target):
        parser.error("baseline/candidate target directories must be distinct and non-overlapping")
    for label, path in (("baseline target", baseline_target), ("candidate target", candidate_target), ("output directory", output_dir)):
        if paths_overlap(path, baseline_root) or paths_overlap(path, candidate_root):
            parser.error(f"{label} must be outside both repository roots")
    if paths_overlap(output_dir, baseline_target) or paths_overlap(output_dir, candidate_target):
        parser.error("output directory must be outside both Cargo target directories")
    if git_head(baseline_root) != BASELINE_REVISION:
        raise GateError(f"baseline HEAD must be pinned at {BASELINE_REVISION}")
    if hash_file(baseline_root / "Cargo.lock") != BASELINE_LOCK_SHA256:
        raise GateError("baseline Cargo.lock differs from the frozen oracle")

    candidate_harness = candidate_root / PARTITIONED_PROBE
    baseline_harness = baseline_root / PARTITIONED_PROBE
    if not candidate_harness.is_file():
        raise GateError(f"candidate partitioned harness is missing: {candidate_harness}")
    baseline_harness.parent.mkdir(parents=True, exist_ok=True)
    tracked_harness = subprocess.run(
        ["git", "ls-files", "--error-unmatch", PARTITIONED_PROBE],
        cwd=baseline_root,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    ).returncode == 0
    if tracked_harness:
        raise GateError("frozen baseline unexpectedly tracks the partitioned probe path")
    if not baseline_harness.exists() or baseline_harness.read_bytes() != candidate_harness.read_bytes():
        shutil.copyfile(candidate_harness, baseline_harness)
    validate_baseline_worktree(baseline_root, candidate_root)

    output_dir.mkdir(parents=True, exist_ok=True)
    logs = output_dir / "logs"
    env = os.environ.copy()
    base_lock = hash_file(baseline_root / "Cargo.lock")
    candidate_lock = hash_file(candidate_root / "Cargo.lock")
    validate_versioned_dependencies(baseline_root, candidate_root)

    baseline_plain = build_probe(
        baseline_root,
        baseline_target,
        label="baseline",
        log_dir=logs,
        env=env,
        online=args.online,
    )
    candidate_plain = build_probe(
        candidate_root,
        candidate_target,
        label="candidate",
        log_dir=logs,
        env=env,
        online=args.online,
    )
    input_fixture = output_dir / "partitioned.inputs.f64le"
    run_probe(
        candidate_plain,
        ["generate-inputs", str(input_fixture)],
        cwd=candidate_root,
        env=env,
        log_path=logs / "generate-shared-input.log",
    )
    input_metadata = validate_input_fixture(input_fixture)
    input_sha256 = str(input_metadata["sha256"])

    captures = output_dir / "captures"
    captures.mkdir(exist_ok=True)
    exact_rows: list[dict[str, object]] = []
    captures_by_partition: dict[int, tuple[dict[str, object], dict[str, object]]] = {}
    for partition_size in (1, 64, 128, 256):
        prefixes = {
            "baseline": captures / f"p{partition_size}-baseline",
            "candidate": captures / f"p{partition_size}-candidate",
        }
        for label in ("baseline", "candidate"):
            binary = baseline_plain if label == "baseline" else candidate_plain
            root = baseline_root if label == "baseline" else candidate_root
            run_probe(
                binary,
                ["capture", str(input_fixture), str(prefixes[label]), str(partition_size)],
                cwd=root,
                env=env,
                log_path=logs / f"capture-p{partition_size}-{label}.log",
            )
        if sha256_file(input_fixture) != input_sha256:
            raise GateError("shared input fixture changed during baseline/candidate capture")
        baseline_capture, candidate_capture = compare_capture(
            Path(f"{prefixes['baseline']}.manifest.tsv"),
            Path(f"{prefixes['baseline']}.pcm64le"),
            Path(f"{prefixes['candidate']}.manifest.tsv"),
            Path(f"{prefixes['candidate']}.pcm64le"),
            partition_size,
            str(input_metadata["fnv1a64"]),
        )
        captures_by_partition[partition_size] = (baseline_capture, candidate_capture)
        exact_rows.append(
            {
                "partition_size": partition_size,
                "fft_size": partition_size * 2,
                "scenario_count": 4,
                "input_sha256": input_sha256,
                "baseline_pcm_sha256": baseline_capture["pcm_sha256"],
                "candidate_pcm_sha256": candidate_capture["pcm_sha256"],
                "pcm_bytes": baseline_capture["pcm_bytes"],
            }
        )
        print(
            f"PASS partitioned exact gate P={partition_size}: shared_input_sha256={input_sha256} "
            f"scenarios=4 pcm_bytes={baseline_capture['pcm_bytes']} f64_bits=identical "
            "silence/reset/source-permutation/tail=covered"
        )
        if partition_size == 256:
            run_integrated_mutation_test(
                Path(f"{prefixes['baseline']}.manifest.tsv"),
                Path(f"{prefixes['baseline']}.pcm64le"),
                Path(f"{prefixes['candidate']}.manifest.tsv"),
                Path(f"{prefixes['candidate']}.pcm64le"),
                output_dir,
                partition_size,
                str(input_metadata["fnv1a64"]),
            )
            print("PASS integrated one-bit f64 PCM mutation was rejected")
    baseline_capture, candidate_capture = captures_by_partition[256]

    unsupported_rows: list[dict[str, str]] = []
    for partition_size in (7, 257):
        outputs = {}
        for label in ("baseline", "candidate"):
            binary = baseline_plain if label == "baseline" else candidate_plain
            root = baseline_root if label == "baseline" else candidate_root
            outputs[label] = run_probe(
                binary,
                ["config", str(partition_size)],
                cwd=root,
                env=env,
                log_path=logs / f"config-p{partition_size}-{label}.log",
            )
        expected = f"unsupported,{partition_size},InvalidPartitionSize"
        if outputs["baseline"] != expected or outputs["candidate"] != expected:
            raise GateError(f"unsupported partition {partition_size} changed behavior: {outputs}")
        unsupported_rows.append({"partition_size": str(partition_size), "status": outputs["baseline"]})
        print(f"PASS unsupported partition size {partition_size}: baseline and candidate return InvalidPartitionSize")

    with (output_dir / "exact-gates.csv").open("w", newline="", encoding="utf-8") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(exact_rows[0]))
        writer.writeheader()
        writer.writerows(exact_rows)
    with (output_dir / "unsupported-partitions.csv").open("w", newline="", encoding="utf-8") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(unsupported_rows[0]))
        writer.writeheader()
        writer.writerows(unsupported_rows)

    # Allocation counting is a separate instrumented build/run from plain timing.
    baseline_alloc_binary = build_probe(
        baseline_root,
        baseline_target,
        label="baseline",
        log_dir=logs,
        env=env,
        online=args.online,
        allocation=True,
    )
    candidate_alloc_binary = build_probe(
        candidate_root,
        candidate_target,
        label="candidate",
        log_dir=logs,
        env=env,
        online=args.online,
        allocation=True,
    )
    baseline_alloc = parse_alloc(
        run_probe(baseline_alloc_binary, ["alloc"], cwd=baseline_root, env=env, log_path=logs / "alloc-baseline.log")
    )
    candidate_alloc = parse_alloc(
        run_probe(candidate_alloc_binary, ["alloc"], cwd=candidate_root, env=env, log_path=logs / "alloc-candidate.log")
    )
    if baseline_alloc["calls"] != candidate_alloc["calls"] or baseline_alloc["output_digest"] != candidate_alloc["output_digest"]:
        raise GateError("allocation probes used different call counts or produced different outputs")
    calls = int(candidate_alloc["calls"])
    alloc_rows = []
    for label, record in (("baseline", baseline_alloc), ("candidate", candidate_alloc)):
        alloc_rows.append(
            {
                "revision": label,
                "calls": calls,
                "allocations_per_block": int(record["allocations"]) / calls,
                "reallocations_per_block": int(record["reallocations"]) / calls,
                "deallocations_per_block": int(record["deallocations"]) / calls,
                "requested_bytes_per_block": int(record["allocated_bytes"]) / calls,
                "output_digest": record["output_digest"],
            }
        )
    with (output_dir / "allocations.csv").open("w", newline="", encoding="utf-8") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(alloc_rows[0]))
        writer.writeheader()
        writer.writerows(alloc_rows)
    print(
        "Allocation diagnostic (not a qualification gate): baseline={:.3f} allocs/{:.3f} bytes per block; "
        "candidate={:.3f} allocs/{:.3f} bytes per block".format(
            alloc_rows[0]["allocations_per_block"],
            alloc_rows[0]["requested_bytes_per_block"],
            alloc_rows[1]["allocations_per_block"],
            alloc_rows[1]["requested_bytes_per_block"],
        )
    )

    timing_rows: list[dict[str, object]] = []
    for repetition in range(args.timing_pairs):
        order = ["baseline", "candidate"] if repetition % 2 == 0 else ["candidate", "baseline"]
        record: dict[str, dict[str, int | float | str]] = {}
        for label in order:
            binary = baseline_plain if label == "baseline" else candidate_plain
            root = baseline_root if label == "baseline" else candidate_root
            record[label] = parse_timing(
                run_probe(
                    binary,
                    ["timing"],
                    cwd=root,
                    env=env,
                    log_path=logs / f"timing-{repetition + 1:02d}-{label}.log",
                )
            )
        if record["baseline"]["calls"] != record["candidate"]["calls"]:
            raise GateError("paired timing call counts differ")
        if record["baseline"]["output_digest"] != record["candidate"]["output_digest"]:
            raise GateError("paired timing runs produced different output digests")
        timing_rows.append(
            {
                "pair": repetition + 1,
                "run_order": "-".join(order),
                "calls": record["baseline"]["calls"],
                "baseline_elapsed_ns": record["baseline"]["elapsed_ns"],
                "candidate_elapsed_ns": record["candidate"]["elapsed_ns"],
                "baseline_ns_per_block": record["baseline"]["ns_per_block"],
                "candidate_ns_per_block": record["candidate"]["ns_per_block"],
                "candidate_over_baseline": float(record["candidate"]["ns_per_block"])
                / float(record["baseline"]["ns_per_block"]),
                "output_digest": record["baseline"]["output_digest"],
            }
        )
        print(
            "PASS timing pair {}/{} order={} baseline={:.3f} ns/block candidate={:.3f} ns/block".format(
                repetition + 1,
                args.timing_pairs,
                "-".join(order),
                record["baseline"]["ns_per_block"],
                record["candidate"]["ns_per_block"],
            )
        )
    with (output_dir / "timings.csv").open("w", newline="", encoding="utf-8") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(timing_rows[0]))
        writer.writeheader()
        writer.writerows(timing_rows)
    baseline_median = statistics.median(float(row["baseline_ns_per_block"]) for row in timing_rows)
    candidate_median = statistics.median(float(row["candidate_ns_per_block"]) for row in timing_rows)
    faster_pairs = sum(float(row["candidate_ns_per_block"]) < float(row["baseline_ns_per_block"]) for row in timing_rows)
    gain = candidate_median < baseline_median and faster_pairs >= math.ceil(args.timing_pairs * 0.8)

    provenance = {
        "baseline_revision": git_head(baseline_root),
        "candidate_revision": git_head(candidate_root),
        "baseline_lock_sha256": base_lock,
        "candidate_lock_sha256": candidate_lock,
        "baseline_partitioned_source_sha256": sha256_file(baseline_root / "crates/openjoc-render/src/partitioned.rs"),
        "candidate_partitioned_source_sha256": sha256_file(candidate_root / "crates/openjoc-render/src/partitioned.rs"),
        "probe_sha256": sha256_file(candidate_harness),
        "shared_input_sha256": input_sha256,
        "shared_input_metadata": input_metadata,
        "same_backend_exact_matrix": exact_rows,
        "explicitly_unsupported_partition_sizes": unsupported_rows,
        "baseline_pcm_sha256": baseline_capture["pcm_sha256"],
        "candidate_pcm_sha256": candidate_capture["pcm_sha256"],
        "pcm_bytes": baseline_capture["pcm_bytes"],
        "allocation_probe": {
            "rows": alloc_rows,
            "allocation_reduction_observed": alloc_rows[1]["allocations_per_block"]
            < alloc_rows[0]["allocations_per_block"],
        },
        "timing_probe": {
            "workload": "11 sources x 256 taps x 256 samples, 512-point FFT; 64 warmups + 1024 timed blocks",
            "pairs": timing_rows,
            "baseline_median_ns_per_block": baseline_median,
            "candidate_median_ns_per_block": candidate_median,
            "candidate_faster_pairs": faster_pairs,
            "cpu_gain_demonstrated_by_rule": gain,
            "rule": "candidate median lower and faster in at least 80% of alternating paired runs",
        },
        "limitations": [
            "Synthetic deterministic mono source blocks and synthetic unequal-length HRIRs; not programme audio.",
            "Partitioned renderer only; this does not change or improve the default direct FIR/LAV path.",
            "Timed region covers warmed preallocated render_partition calls; constructor/FFT planning and file I/O are excluded.",
            "The allocator probe is a separate instrumented process and is not used for timing.",
        ],
    }
    (output_dir / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n", encoding="utf-8")
    print(
        f"TIMING SUMMARY baseline_median={baseline_median:.3f} ns/block candidate_median={candidate_median:.3f} ns/block "
        f"candidate_faster_pairs={faster_pairs}/{args.timing_pairs} cpu_gain_demonstrated={gain}"
    )
    print(f"Partitioned FFT scratch gate results: {output_dir}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (GateError, OSError, subprocess.SubprocessError) as error:
        print(f"PARTITIONED FFT SCRATCH GATE FAILED: {error}", file=sys.stderr)
        raise SystemExit(1)
