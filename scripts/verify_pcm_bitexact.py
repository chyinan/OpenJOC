#!/usr/bin/env python3
"""Build paired OpenJOC revisions and compare API PCM byte-for-byte."""

from __future__ import annotations

import argparse
import csv
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass

BASELINE_REVISION = "15aefe1baa7b40f37950df252b6dbf6179894d6d"
BASELINE_LOCK_SHA256 = "40f4a91c662652bef309f2bf57d01cfa811c5d004ebf9d194bf9e688d4ab45e7"
PROBE = "crates/openjoc-api/examples/pcm_regression_probe.rs"
CORE_PROBE = "crates/openjoc-eac3/examples/eac3_pcm_regression_probe.rs"
PARTITIONED_PROBE = "crates/openjoc-render/examples/partitioned_fft_scratch_probe.rs"
CHUNK_BYTES = 1024 * 1024
PINNED_CORPUS_HASHES = {
    "joc.lifecycle.ec3": "b860509a1613134931e1e39b9d2b6d4d31687b1a6586d8e2bcf5fe99e7da14f8",
    "joc.lifecycle.30s.ec3": "a44fc36470d07f98c68053c9015e3cb169a21af927b1b3b93bd5712a42234671",
    "joc.lifecycle.60s.ec3": "8202e69a5cd13c9142165315f7640d2d7c3cf554b6de0bba8b2c5c40cc499cf2",
    "ordinary.multitone.30s.eac3": "a6a22c1e5735bc5674734dc1ab40db735f69aff9fe202439b54a9aeaaf9d3fd9",
    "ordinary.multitone.60s.eac3": "0b98eb8c92ad542cbda37c402e420e74f8a9ddd25e72cd440788be110ce4a136",
}


class GateError(RuntimeError):
    pass


@dataclass(frozen=True)
class Case:
    name: str
    input_path: Path
    mode: str
    layout: str


@dataclass
class Manifest:
    raw_lines: list[str]
    headers: dict[str, str]
    access_units: list[list[str]]
    frames: list[list[str]]
    channels: list[list[str]]
    results: dict[str, str]


def read_manifest(path: Path) -> Manifest:
    try:
        lines = path.read_text(encoding="ascii").splitlines()
    except OSError as error:
        raise GateError(f"cannot read manifest {path}: {error}") from error
    if not lines or lines[0] != "openjoc-pcm-probe\t1":
        raise GateError(f"{path}: unsupported or missing manifest header")
    headers: dict[str, str] = {}
    access_units: list[list[str]] = []
    frames: list[list[str]] = []
    channels: list[list[str]] = []
    results: dict[str, str] = {}
    for number, line in enumerate(lines[1:], start=2):
        parts = line.split("\t")
        if parts[0] == "H" and len(parts) == 3:
            if parts[1] in headers:
                raise GateError(f"{path}:{number}: duplicate header {parts[1]}")
            headers[parts[1]] = parts[2]
        elif parts[0] == "AU" and len(parts) == 9:
            access_units.append(parts[1:])
        elif parts[0] == "F" and len(parts) == 12:
            frames.append(parts[1:])
        elif parts[0] == "C" and len(parts) == 9:
            channels.append(parts[1:])
        elif parts[0] == "R" and len(parts) == 3:
            if parts[1] in results:
                raise GateError(f"{path}:{number}: duplicate result {parts[1]}")
            results[parts[1]] = parts[2]
        elif parts[0] == "O" and len(parts) == 4:
            # Accepted/applied orientation receipts are deterministic and are
            # retained in raw-line equality but do not need separate parsing.
            continue
        else:
            raise GateError(f"{path}:{number}: malformed manifest record {line!r}")

    required_headers = {
        "input_sha256",
        "mode",
        "layout",
        "config_fingerprint",
        "config_descriptor_hex",
        "input_unit_count",
    }
    if not required_headers.issubset(headers):
        missing = ", ".join(sorted(required_headers - headers.keys()))
        raise GateError(f"{path}: missing required headers: {missing}")
    required_results = {
        "input_units",
        "output_frames",
        "pcm_bytes",
        "sample_count",
        "tail_samples",
        "channel_count",
        "distinct_channel_fingerprints",
        "diagnostics_hex",
        "terminal_state",
    }
    if not required_results.issubset(results):
        missing = ", ".join(sorted(required_results - results.keys()))
        raise GateError(f"{path}: missing required results: {missing}")
    if int(headers["input_unit_count"]) != len(access_units):
        raise GateError(f"{path}: access-unit count does not match manifest header")
    if int(results["input_units"]) != len(access_units):
        raise GateError(f"{path}: result access-unit count mismatch")
    if int(results["output_frames"]) != len(frames):
        raise GateError(f"{path}: output-frame count does not match frame descriptors")
    if "core_f64_bytes" in results:
        expected_f64_bytes = sum(int(frame[4]) * int(frame[3]) * 8 for frame in frames)
        if int(results["core_f64_bytes"]) != expected_f64_bytes:
            raise GateError(f"{path}: core f64 byte count does not match frame descriptors")
    if int(results["channel_count"]) != len(channels):
        raise GateError(f"{path}: channel count does not match channel audit")
    if not access_units or not frames:
        raise GateError(f"{path}: empty input corpus or output")
    if int(results["sample_count"]) <= 0 or int(results["pcm_bytes"]) <= 0:
        raise GateError(f"{path}: empty PCM output")
    if headers["mode"] != "ordinary-eac3-core" and int(results["tail_samples"]) <= 0:
        raise GateError(f"{path}: missing delayed/tail PCM coverage")
    if not results["terminal_state"]:
        raise GateError(f"{path}: empty terminal state")
    if int(results["distinct_channel_fingerprints"]) < 1:
        raise GateError(f"{path}: missing channel fingerprint audit")
    if sum(int(channel[1]) for channel in channels) <= 0:
        raise GateError(f"{path}: corpus produced only zero samples")
    if sum(int(channel[3]) for channel in channels) != 0:
        raise GateError(f"{path}: non-finite output PCM in benchmark corpus")
    return Manifest(lines, headers, access_units, frames, channels, results)


def _frame_location(
    frames: list[list[str]], byte_offset: int, bytes_per_sample: int = 4
) -> tuple[int, int, int] | None:
    running_offset = 0
    for frame in frames:
        # F fields after the tag: id, phase, rate, channels, samples, pts,
        # layout, labels, mode, format, offset, byte length.
        start = int(frame[10]) if bytes_per_sample == 4 else running_offset
        count = int(frame[4])
        channels = int(frame[3])
        end = start + count * channels * bytes_per_sample
        if start <= byte_offset < end:
            scalar_index = (byte_offset - start) // bytes_per_sample
            sample_index, channel_index = divmod(scalar_index, channels)
            return int(frame[0]), sample_index, channel_index
        running_offset = end
    return None


def artifact(prefix: Path, suffix: str) -> Path:
    return Path(f"{prefix}{suffix}")


def retain_artifacts(prefixes: dict[str, Path], destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    for prefix in prefixes.values():
        for suffix in (".manifest.tsv", ".pcm32le", ".pcm64le", ".timing.tsv"):
            source = artifact(prefix, suffix)
            if source.is_file():
                shutil.copyfile(source, destination / (prefix.name + suffix))


def compare_runs(
    baseline_manifest_path: Path,
    baseline_pcm_path: Path,
    candidate_manifest_path: Path,
    candidate_pcm_path: Path,
) -> None:
    baseline = read_manifest(baseline_manifest_path)
    candidate = read_manifest(candidate_manifest_path)
    if baseline.raw_lines != candidate.raw_lines:
        limit = min(len(baseline.raw_lines), len(candidate.raw_lines))
        line_index = next(
            (i for i in range(limit) if baseline.raw_lines[i] != candidate.raw_lines[i]),
            limit,
        )
        expected = baseline.raw_lines[line_index] if line_index < len(baseline.raw_lines) else "<EOF>"
        actual = candidate.raw_lines[line_index] if line_index < len(candidate.raw_lines) else "<EOF>"
        raise GateError(
            f"PCM descriptor mismatch at manifest line {line_index + 1}: "
            f"baseline={expected!r}; candidate={actual!r}"
        )
    if baseline.headers["input_sha256"] != candidate.headers["input_sha256"]:
        raise GateError("baseline/candidate input hashes differ")

    expected_length = int(baseline.results["pcm_bytes"])
    if expected_length != int(candidate.results["pcm_bytes"]):
        raise GateError("manifest PCM byte lengths differ")
    for path in (baseline_pcm_path, candidate_pcm_path):
        try:
            actual_size = path.stat().st_size
        except OSError as error:
            raise GateError(f"cannot stat PCM stream {path}: {error}") from error
        if actual_size != expected_length:
            raise GateError(
                f"{path}: PCM file size {actual_size} differs from manifest {expected_length}"
            )

    _compare_stream_bytes(
        "PCM",
        baseline_pcm_path,
        candidate_pcm_path,
        expected_length,
        baseline.frames,
        bytes_per_sample=4,
    )

    baseline_pcm64 = Path(str(baseline_pcm_path).removesuffix(".pcm32le") + ".pcm64le")
    candidate_pcm64 = Path(str(candidate_pcm_path).removesuffix(".pcm32le") + ".pcm64le")
    baseline_has_f64 = "core_f64_bytes" in baseline.results
    candidate_has_f64 = "core_f64_bytes" in candidate.results
    baseline_file_f64 = baseline_pcm64.is_file()
    candidate_file_f64 = candidate_pcm64.is_file()
    if baseline_has_f64 != candidate_has_f64 or baseline_has_f64 != baseline_file_f64 or candidate_has_f64 != candidate_file_f64:
        raise GateError("ordinary-core f64 output stream/manifest is missing or inconsistent")
    if baseline_has_f64:
        baseline_length = int(baseline.results["core_f64_bytes"])
        candidate_length = int(candidate.results["core_f64_bytes"])
        if baseline_length <= 0 or baseline_length != candidate_length:
            raise GateError("ordinary-core f64 byte lengths differ or are empty")
        _compare_stream_bytes(
            "core f64 PCM",
            baseline_pcm64,
            candidate_pcm64,
            baseline_length,
            baseline.frames,
            bytes_per_sample=8,
        )


def _compare_stream_bytes(
    label: str,
    baseline_path: Path,
    candidate_path: Path,
    expected_length: int,
    frames: list[list[str]],
    *,
    bytes_per_sample: int,
) -> None:
    for path in (baseline_path, candidate_path):
        try:
            actual_size = path.stat().st_size
        except OSError as error:
            raise GateError(f"cannot stat {label} stream {path}: {error}") from error
        if actual_size != expected_length:
            raise GateError(
                f"{path}: {label} file size {actual_size} differs from manifest {expected_length}"
            )
    offset = 0
    with baseline_path.open("rb") as expected, candidate_path.open("rb") as actual:
        while True:
            left = expected.read(CHUNK_BYTES)
            right = actual.read(CHUNK_BYTES)
            common = min(len(left), len(right))
            if left[:common] != right[:common]:
                local = next(i for i, (a, b) in enumerate(zip(left, right)) if a != b)
                byte_offset = offset + local
                aligned = byte_offset - (byte_offset % bytes_per_sample)
                with baseline_path.open("rb") as expected_bits, candidate_path.open("rb") as actual_bits:
                    expected_bits.seek(aligned)
                    actual_bits.seek(aligned)
                    expected_word = expected_bits.read(bytes_per_sample).ljust(bytes_per_sample, b"\0")
                    actual_word = actual_bits.read(bytes_per_sample).ljust(bytes_per_sample, b"\0")
                location = _frame_location(frames, aligned, bytes_per_sample)
                where = (
                    f"frame={location[0]} sample={location[1]} channel={location[2]}"
                    if location is not None
                    else "outside-described-frames"
                )
                digits = bytes_per_sample * 2
                raise GateError(
                    f"{label} bit mismatch at byte {byte_offset} ({where}): "
                    f"baseline=0x{int.from_bytes(expected_word, 'little'):0{digits}x}; "
                    f"candidate=0x{int.from_bytes(actual_word, 'little'):0{digits}x}"
                )
            if len(left) != len(right):
                raise GateError(
                    f"{label} truncation/trailing-data mismatch at byte {offset + common}: "
                    f"baseline chunk={len(left)} candidate chunk={len(right)}"
                )
            if not left:
                break
            offset += len(left)
    if offset != expected_length:
        raise GateError(f"{label} stream ended at {offset} bytes, expected {expected_length}")


def _manifest_for_test(*, frames: list[tuple[str, int, int, int]]) -> bytes:
    lines = [
        "openjoc-pcm-probe\t1",
        "H\tinput_sha256\t" + ("a" * 64),
        "H\tmode\tspeaker",
        "H\tlayout\t322e30",
        "H\tconfig_fingerprint\t" + ("b" * 64),
        "H\tconfig_descriptor_hex\t636f6e666967",
        "H\tinput_unit_count\t1",
        "AU\t0\t0\t4\t1\t48000\t0\t1\t0",
    ]
    offset = 0
    channels = 2
    total_samples = 0
    tails = 0
    for index, (phase, rate, count, pts) in enumerate(frames):
        length = channels * count * 4
        lines.append(
            f"F\t{index}\t{phase}\t{rate}\t{channels}\t{count}\t{pts}\t6c61796f7574\t6c,72\tSpeaker\t0\t{offset}"
        )
        offset += length
        total_samples += count
        if phase == "drain":
            tails += count
    for channel in range(channels):
        # C field 1 is nonzero count, field 3 is non-finite count.
        lines.append(f"C\t{channel}\t1\t2\t0\t1\t00000000\t3f800000\t{channel + 1:016x}")
    lines.extend(
        [
            "R\tinput_units\t1",
            f"R\toutput_frames\t{len(frames)}",
            f"R\tpcm_bytes\t{offset}",
            f"R\tsample_count\t{total_samples}",
            f"R\ttail_samples\t{tails}",
            f"R\tchannel_count\t{channels}",
            "R\tdistinct_channel_fingerprints\t2",
            "R\tdiagnostics_hex\t70726f66696c65",
            "R\tterminal_state\tdrained-repeated-drain-eos",
        ]
    )
    return ("\n".join(lines) + "\n").encode("ascii")


def run_self_tests() -> None:
    """Negative sensitivity tests for descriptors, bits, lengths, and tails."""
    import tempfile

    def write_pair(root: Path, *, frames: list[tuple[str, int, int, int]], pcm: bytes):
        baseline_manifest = root / "baseline.manifest.tsv"
        candidate_manifest = root / "candidate.manifest.tsv"
        baseline_pcm = root / "baseline.pcm32le"
        candidate_pcm = root / "candidate.pcm32le"
        Path(str(baseline_pcm).removesuffix(".pcm32le") + ".pcm64le").unlink(missing_ok=True)
        Path(str(candidate_pcm).removesuffix(".pcm32le") + ".pcm64le").unlink(missing_ok=True)
        content = _manifest_for_test(frames=frames)
        baseline_manifest.write_bytes(content)
        candidate_manifest.write_bytes(content)
        baseline_pcm.write_bytes(pcm)
        candidate_pcm.write_bytes(pcm)
        return baseline_manifest, baseline_pcm, candidate_manifest, candidate_pcm

    with tempfile.TemporaryDirectory(prefix="openjoc-pcm-gate-selftest-") as temp:
        root = Path(temp)
        frames = [("programme", 48_000, 2, 0), ("drain", 48_000, 1, 2)]
        # Exact copies pass; the first channel sample is +0 and the last is a tail sample.
        pcm = bytes.fromhex("00000000 00000000 0000803f 00000000 00000000 00000000")
        pair = write_pair(root, frames=frames, pcm=pcm)
        compare_runs(*pair)

        # The ordinary-E-AC-3 core path has a second full-precision f64 oracle.
        pair = write_pair(root, frames=frames, pcm=pcm)
        for manifest in (pair[0], pair[2]):
            manifest.write_text(
                manifest.read_text(encoding="ascii") + "R\tcore_f64_bytes\t48\n",
                encoding="ascii",
            )
        baseline_f64 = Path(str(pair[1]).removesuffix(".pcm32le") + ".pcm64le")
        candidate_f64 = Path(str(pair[3]).removesuffix(".pcm32le") + ".pcm64le")
        baseline_f64.write_bytes(bytes(48))
        candidate_f64.write_bytes(bytes(48))
        compare_runs(*pair)
        candidate_f64.write_bytes(bytes.fromhex("0000000000000080") + bytes(40))
        try:
            compare_runs(*pair)
        except GateError as error:
            if "core f64 PCM bit mismatch" not in str(error):
                raise AssertionError(f"f64 mutation failed for the wrong reason: {error}") from error
        else:
            raise AssertionError("sensitivity test failed to reject a changed core f64 sample bit")

        negative_cases: list[tuple[str, bytes, str]] = [
            ("signed-zero", bytes.fromhex("00000080") + pcm[4:], "PCM bit mismatch"),
            ("sample-bit", pcm[:8] + bytes([pcm[8] ^ 1]) + pcm[9:], "PCM bit mismatch"),
            ("tail-byte", pcm[:-1] + bytes([pcm[-1] ^ 1]), "PCM bit mismatch"),
            ("truncated", pcm[:-1], "PCM file size"),
            ("trailing", pcm + b"x", "PCM file size"),
        ]
        for name, candidate_bytes, expected_reason in negative_cases:
            pair = write_pair(root, frames=frames, pcm=pcm)
            compare_runs(*pair)
            pair[3].write_bytes(candidate_bytes)
            try:
                compare_runs(*pair)
            except GateError as error:
                if expected_reason not in str(error):
                    raise AssertionError(f"{name} failed for the wrong reason: {error}") from error
            else:
                raise AssertionError(f"sensitivity test failed to reject {name}")

        # Alter a bit-exactly significant descriptor while leaving PCM intact.
        for name, old, new in [
            ("sample-count", "F\t0\tprogramme\t48000\t2\t2", "F\t0\tprogramme\t48000\t2\t3"),
            ("channel-count", "F\t0\tprogramme\t48000\t2\t2", "F\t0\tprogramme\t48000\t1\t2"),
            ("sample-rate", "F\t0\tprogramme\t48000\t2\t2", "F\t0\tprogramme\t44100\t2\t2"),
            ("pts", "F\t0\tprogramme\t48000\t2\t2\t0", "F\t0\tprogramme\t48000\t2\t2\t1"),
            ("config", "H\tconfig_fingerprint\t" + ("b" * 64), "H\tconfig_fingerprint\t" + ("c" * 64)),
        ]:
            pair = write_pair(root, frames=frames, pcm=pcm)
            compare_runs(*pair)
            content = pair[2].read_text(encoding="ascii")
            if old not in content:
                raise AssertionError(f"self-test manifest fixture missing {name} target")
            pair[2].write_text(content.replace(old, new, 1), encoding="ascii")
            try:
                compare_runs(*pair)
            except GateError as error:
                if "PCM descriptor mismatch" not in str(error):
                    raise AssertionError(f"{name} failed for the wrong reason: {error}") from error
            else:
                raise AssertionError(f"sensitivity test failed to reject {name}")
    print("PCM gate self-tests passed: exact copies pass; signed zero, sample bit, descriptors, truncation, append, and tail mutations fail")


def parse_case(value: str) -> Case:
    try:
        name, path, mode, layout = value.split(",", 3)
    except ValueError as error:
        raise argparse.ArgumentTypeError("case must be NAME,INPUT,MODE,LAYOUT") from error
    return Case(name, Path(path).resolve(), mode, layout)


def hash_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(CHUNK_BYTES), b""):
            digest.update(block)
    return digest.hexdigest()


def git_head(root: Path) -> str:
    return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()


def paths_overlap(left: Path, right: Path) -> bool:
    return left == right or left in right.parents or right in left.parents


def _expected_feature_manifest(head_bytes: bytes, package: str) -> bytes:
    if package == "openjoc-api":
        marker = b'orientation-profile = ["openjoc-sofa/orientation-profile"]\n'
        replacement = marker + b"allocation-profile = []\n"
    elif package == "openjoc-eac3":
        marker = b"rust-version.workspace = true\n"
        replacement = marker + b"\n[features]\nallocation-profile = []\n"
    else:
        raise GateError(f"unexpected baseline feature overlay package: {package}")
    if head_bytes.count(marker) != 1:
        raise GateError(f"cannot validate expected one-line feature overlay in {package}/Cargo.toml")
    return head_bytes.replace(marker, replacement, 1)


def validate_baseline_worktree(baseline_root: Path, candidate_root: Path) -> None:
    if paths_overlap(baseline_root, candidate_root):
        raise GateError("baseline and candidate repository roots must be distinct, non-overlapping worktrees")
    baseline_top = Path(subprocess.check_output(["git", "rev-parse", "--show-toplevel"], cwd=baseline_root, text=True).strip()).resolve()
    candidate_top = Path(subprocess.check_output(["git", "rev-parse", "--show-toplevel"], cwd=candidate_root, text=True).strip()).resolve()
    if baseline_top != baseline_root or candidate_top != candidate_root:
        raise GateError("repository roots must be the canonical worktree roots")
    tracked_changes = set(
        subprocess.check_output(
            ["git", "diff", "--name-only", "HEAD"], cwd=baseline_root, text=True
        ).splitlines()
    )
    staged_changes = set(
        subprocess.check_output(
            ["git", "diff", "--cached", "--name-only"], cwd=baseline_root, text=True
        ).splitlines()
    )
    allowed_manifests = {"crates/openjoc-api/Cargo.toml", "crates/openjoc-eac3/Cargo.toml"}
    if staged_changes or not tracked_changes.issubset(allowed_manifests):
        raise GateError(
            f"frozen baseline contains changes outside allowlisted feature manifests: "
            f"staged={sorted(staged_changes)}, tracked={sorted(tracked_changes)}"
        )
    for package_path, package in (
        ("crates/openjoc-api/Cargo.toml", "openjoc-api"),
        ("crates/openjoc-eac3/Cargo.toml", "openjoc-eac3"),
    ):
        current = (baseline_root / package_path).read_bytes()
        committed = subprocess.check_output(
            ["git", "show", f"HEAD:{package_path}"], cwd=baseline_root
        )
        expected = _expected_feature_manifest(committed, package)
        if current not in (committed, expected):
            raise GateError(f"frozen baseline {package_path} differs beyond the one-line allocator feature overlay")
    untracked = set(
        subprocess.check_output(
            ["git", "ls-files", "--others", "--exclude-standard"], cwd=baseline_root, text=True
        ).splitlines()
    )
    allowed_harnesses = {PROBE, CORE_PROBE, PARTITIONED_PROBE}
    if not untracked.issubset(allowed_harnesses):
        raise GateError(f"frozen baseline contains untracked non-harness files: {sorted(untracked - allowed_harnesses)}")
    for harness in untracked:
        if not (baseline_root / harness).is_file() or not (candidate_root / harness).is_file():
            raise GateError(f"allowlisted baseline harness is missing: {harness}")
        if (baseline_root / harness).read_bytes() != (candidate_root / harness).read_bytes():
            raise GateError(f"baseline harness overlay differs from candidate: {harness}")


def run_checked(command: list[str], *, cwd: Path, env: dict[str, str], log: Path) -> None:
    log.parent.mkdir(parents=True, exist_ok=True)
    with log.open("wb") as output:
        result = subprocess.run(command, cwd=cwd, env=env, stdout=output, stderr=subprocess.STDOUT, check=False)
    if result.returncode != 0:
        tail = log.read_text(encoding="utf-8", errors="replace")[-6000:]
        raise GateError(f"command failed ({result.returncode}): {' '.join(command)}\n{tail}")


def build_revision(
    root: Path,
    target: Path,
    *,
    label: str,
    log_dir: Path,
    env: dict[str, str],
    needs_api: bool,
    needs_core: bool,
    allocation_profile: bool,
    online: bool,
) -> dict[str, Path]:
    cargo_env = env.copy()
    cargo_env.update(
        {
            "CARGO_TARGET_DIR": str(target),
            "CARGO_INCREMENTAL": "0",
            "CARGO_BUILD_JOBS": "1",
            "CARGO_TERM_COLOR": "never",
        }
    )
    binaries: dict[str, Path] = {}
    builds = []
    if needs_api:
        builds.append(("openjoc-api", "pcm_regression_probe", "api"))
    if needs_core:
        builds.append(("openjoc-eac3", "eac3_pcm_regression_probe", "core"))
    for package, example, key in builds:
        command = ["cargo", "+1.89.0", "build", "--release", "--locked"]
        if not online:
            command.append("--offline")
        command.extend(["-p", package, "--example", example])
        if allocation_profile:
            command.extend(["--features", "allocation-profile"])
        run_checked(
            command,
            cwd=root,
            env=cargo_env,
            log=log_dir / f"build-{label}-{package}.log",
        )
        binary = target / "release" / "examples" / example
        if not binary.is_file():
            raise GateError(f"build reported success but probe binary is missing: {binary}")
        binaries[key] = binary
    return binaries


def run_probe(
    binary: Path,
    case: Case,
    prefix: Path,
    *,
    stage: bool,
    cwd: Path,
    log: Path,
    input_sha256: str,
    allocation_profile: bool,
    timing_only: bool,
) -> int:
    log.parent.mkdir(parents=True, exist_ok=True)
    if case.mode == "ordinary-eac3-core":
        command = [str(binary), str(case.input_path), case.layout, str(prefix)]
        process_env = os.environ.copy()
        process_env["OPENJOC_INPUT_SHA256"] = input_sha256
    else:
        command = [str(binary), str(case.input_path), case.mode, case.layout, str(prefix)]
        process_env = os.environ.copy()
    if stage:
        command.append("--stage")
    if allocation_profile:
        command.append("--alloc")
    if timing_only:
        command.append("--timing-only")
    start = time.perf_counter_ns()
    with log.open("wb") as output:
        result = subprocess.run(command, cwd=cwd, env=process_env, stdout=output, stderr=subprocess.STDOUT, check=False)
    elapsed = time.perf_counter_ns() - start
    if result.returncode != 0:
        tail = log.read_text(encoding="utf-8", errors="replace")[-6000:]
        raise GateError(f"probe failed for {case.name} ({result.returncode}): {' '.join(command)}\n{tail}")
    return elapsed


def read_timing(path: Path) -> dict[str, str]:
    result: dict[str, str] = {}
    per_au_allocations: list[tuple[int, int, int, int]] = []
    orientation_allocations: list[tuple[int, int, int, int]] = []
    for line in path.read_text(encoding="ascii").splitlines()[1:]:
        parts = line.split("\t")
        if len(parts) == 3 and parts[0] in {"SUMMARY", "STAGE"}:
            result[f"{parts[0].lower()}_{parts[1]}"] = parts[2]
        elif parts[0] == "AU" and len(parts) >= 11:
            per_au_allocations.append(tuple(int(value) for value in parts[7:11]))
        elif parts[0] == "AU" and len(parts) == 9:
            per_au_allocations.append(tuple(int(value) for value in parts[5:9]))
        elif parts[0] == "ORIENT" and len(parts) == 7:
            orientation_allocations.append(tuple(int(value) for value in parts[3:7]))
    for label, observations in (("au", per_au_allocations), ("orientation", orientation_allocations)):
        if observations:
            totals = [sum(observation[column] for observation in observations) for column in range(4)]
            suffixes = ("alloc_calls", "realloc_calls", "dealloc_calls", "requested_bytes")
            for suffix, total in zip(suffixes, totals):
                result[f"allocation_{label}_{suffix}_total"] = str(total)
                result[f"allocation_{label}_{suffix}_mean"] = f"{total / len(observations):.6f}"
    return result


def mutation_sensitivity(
    baseline_manifest: Path,
    baseline_pcm: Path,
    candidate_manifest: Path,
    candidate_pcm: Path,
    scratch: Path,
) -> None:
    mutated = scratch / "candidate-mutated.pcm32le"
    shutil.copyfile(candidate_pcm, mutated)
    if mutated.stat().st_size == 0:
        raise GateError("integrated sensitivity test has empty candidate PCM")
    with mutated.open("r+b") as stream:
        byte = stream.read(1)
        stream.seek(0)
        stream.write(bytes([byte[0] ^ 1]))
    try:
        compare_runs(baseline_manifest, baseline_pcm, candidate_manifest, mutated)
    except GateError as error:
        if "PCM bit mismatch" not in str(error):
            raise GateError(f"integrated output mutation was rejected for the wrong reason: {error}") from error
        print("integrated PCM mutation sensitivity passed: a one-bit output mutation was rejected")
    else:
        raise GateError("integrated PCM mutation sensitivity failed to detect a changed output bit")
    finally:
        mutated.unlink(missing_ok=True)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run comparator negative-sensitivity tests")
    parser.add_argument("--baseline-root", type=Path, help="clean frozen baseline worktree")
    parser.add_argument("--candidate-root", type=Path, help="candidate worktree")
    parser.add_argument("--baseline-target", type=Path, help="dedicated baseline Cargo target directory")
    parser.add_argument("--candidate-target", type=Path, help="dedicated candidate Cargo target directory")
    parser.add_argument("--output-dir", type=Path, help="owned scratch output directory (deleted per-case unless --keep)")
    parser.add_argument("--case", action="append", type=parse_case, default=[], help="NAME,INPUT,MODE,LAYOUT; repeatable")
    parser.add_argument("--repeats", type=int, default=1, help="sequential baseline/candidate pairs; order alternates AB, then BA")
    parser.add_argument("--stage", action="store_true", help="record separate instrumented stage timings")
    parser.add_argument("--allocations", action="store_true", help="run a separate counting-allocator build; its timings are instrumentation-only")
    parser.add_argument("--timing-only", action="store_true", help="plain-allocator timing run; consume/drop PCM in scope and compare output-shape summaries, not PCM capture")
    parser.add_argument("--online", action="store_true", help="allow Cargo to fetch locked dependencies when CI has no local crate cache")
    parser.add_argument("--keep", action="store_true", help="retain per-case PCM files after comparison")
    parser.add_argument("--selftest-integrated", action="store_true", help="mutate first candidate PCM output copy and prove the gate rejects it")
    args = parser.parse_args(argv)
    if args.self_test:
        run_self_tests()
        return 0
    if not args.baseline_root or not args.candidate_root or not args.baseline_target or not args.candidate_target or not args.output_dir:
        parser.error("paired gate requires baseline/candidate roots, targets, and output directory")
    if not args.case:
        parser.error("provide at least one --case")
    if args.repeats < 1:
        parser.error("--repeats must be at least one")
    if args.stage and args.allocations:
        parser.error("stage timing and allocation counting are separate instrumented runs")
    if args.timing_only and (args.stage or args.allocations or args.selftest_integrated):
        parser.error("timing-only runs are plain-allocator runs and cannot use stage/allocation instrumentation or PCM mutation tests")
    baseline_root = args.baseline_root.resolve()
    candidate_root = args.candidate_root.resolve()
    baseline_target = args.baseline_target.resolve()
    candidate_target = args.candidate_target.resolve()
    output_dir = args.output_dir.resolve()
    if paths_overlap(baseline_target, candidate_target):
        parser.error("baseline and candidate Cargo target directories must be distinct and non-overlapping")
    for path, name in ((baseline_target, "baseline target"), (candidate_target, "candidate target"), (output_dir, "output directory")):
        for repo, repo_name in ((baseline_root, "baseline worktree"), (candidate_root, "candidate worktree")):
            if paths_overlap(path, repo):
                parser.error(f"{name} must not overlap the {repo_name}")
    if paths_overlap(output_dir, baseline_target) or paths_overlap(output_dir, candidate_target):
        parser.error("output directory must not overlap either Cargo target directory")
    output_dir.mkdir(parents=True, exist_ok=True)
    logs = output_dir / "logs"
    baseline_head = git_head(baseline_root)
    candidate_head = git_head(candidate_root)
    if baseline_head != BASELINE_REVISION:
        raise GateError(
            f"baseline root HEAD {baseline_head} differs from pinned oracle {BASELINE_REVISION}"
        )
    validate_baseline_worktree(baseline_root, candidate_root)
    env = os.environ.copy()
    if env.get("RUSTFLAGS") or env.get("CARGO_ENCODED_RUSTFLAGS"):
        print("paired gate note: nonempty RUSTFLAGS/CARGO_ENCODED_RUSTFLAGS are active and will be shared by both builds", file=sys.stderr)
    baseline_lock_hash = hash_file(baseline_root / "Cargo.lock")
    candidate_lock_hash = hash_file(candidate_root / "Cargo.lock")
    if baseline_lock_hash != BASELINE_LOCK_SHA256:
        raise GateError(f"baseline Cargo.lock SHA-256 {baseline_lock_hash} differs from pinned lock {BASELINE_LOCK_SHA256}")
    if baseline_lock_hash != candidate_lock_hash:
        raise GateError("baseline and candidate Cargo.lock files differ")
    needs_api = any(case.mode != "ordinary-eac3-core" for case in args.case)
    needs_core = any(case.mode == "ordinary-eac3-core" for case in args.case)
    if needs_api and (baseline_root / PROBE).read_bytes() != (candidate_root / PROBE).read_bytes():
        raise GateError("baseline API probe harness overlay differs from candidate probe source")
    if needs_api and (baseline_root / "crates/openjoc-api/Cargo.toml").read_bytes() != (candidate_root / "crates/openjoc-api/Cargo.toml").read_bytes():
        raise GateError("baseline API probe feature manifest differs from candidate")
    if needs_core and (baseline_root / CORE_PROBE).read_bytes() != (candidate_root / CORE_PROBE).read_bytes():
        raise GateError("baseline ordinary-core probe harness overlay differs from candidate probe source")
    if needs_core and (baseline_root / "crates/openjoc-eac3/Cargo.toml").read_bytes() != (candidate_root / "crates/openjoc-eac3/Cargo.toml").read_bytes():
        raise GateError("baseline E-AC-3 probe feature manifest differs from candidate")
    baseline_binaries = build_revision(
        baseline_root,
        baseline_target,
        label="baseline",
        log_dir=logs,
        env=env,
        needs_api=needs_api,
        needs_core=needs_core,
        allocation_profile=args.allocations,
        online=args.online,
    )
    candidate_binaries = build_revision(
        candidate_root,
        candidate_target,
        label="candidate",
        log_dir=logs,
        env=env,
        needs_api=needs_api,
        needs_core=needs_core,
        allocation_profile=args.allocations,
        online=args.online,
    )
    for key in baseline_binaries.keys() & candidate_binaries.keys():
        if baseline_binaries[key].read_bytes() == candidate_binaries[key].read_bytes():
            print(f"{key} probe binaries are byte-identical", file=sys.stderr)
    toolchain_log = output_dir / "toolchain.txt"
    with toolchain_log.open("w", encoding="utf-8") as stream:
        for command in (["rustc", "+1.89.0", "-vV"], ["cargo", "+1.89.0", "-Vv"]):
            stream.write("$ " + " ".join(command) + "\n")
            stream.write(subprocess.check_output(command, text=True, env=env))
            stream.write("\n")
        stream.write(f"baseline_revision={BASELINE_REVISION}\n")
        stream.write(f"baseline_head={baseline_head}\n")
        stream.write(f"candidate_head={candidate_head}\n")
        stream.write(f"baseline_lock_sha256={baseline_lock_hash}\n")
        if needs_api:
            stream.write(f"api_probe_sha256={hash_file(candidate_root / PROBE)}\n")
        if needs_core:
            stream.write(f"core_probe_sha256={hash_file(candidate_root / CORE_PROBE)}\n")
        stream.write(f"rustflags={env.get('RUSTFLAGS', '')}\n")
        stream.write(f"cargo_encoded_rustflags={env.get('CARGO_ENCODED_RUSTFLAGS', '')}\n")
        stream.write(f"allocation_profile={args.allocations}\n")
        stream.write(f"cargo_offline={not args.online}\n")
        for key in sorted(key for key in env if key.startswith("CARGO_PROFILE_")):
            stream.write(f"{key}={env[key]}\n")

    with tempfile.TemporaryDirectory(prefix="openjoc-pcm-pair-", dir=output_dir) as temp:
        per_case = Path(temp)
        summary_path = output_dir / "measurements.csv"
        summary_rows: list[dict[str, str]] = []
        for index, case in enumerate(args.case):
            if not case.input_path.is_file():
                raise GateError(f"case input does not exist: {case.input_path}")
            expected_hash = hash_file(case.input_path)
            pinned_hash = PINNED_CORPUS_HASHES.get(case.input_path.name)
            if pinned_hash is not None and expected_hash != pinned_hash:
                raise GateError(
                    f"case {case.name} input SHA-256 {expected_hash} differs from pinned corpus {pinned_hash}"
                )
            safe_name = "".join(char if char.isalnum() or char in "-_" else "_" for char in case.name)
            label = f"{index:02d}-{safe_name}"
            for repetition in range(args.repeats):
                order = ["baseline", "candidate"] if repetition % 2 == 0 else ["candidate", "baseline"]
                prefixes: dict[str, Path] = {}
                timings: dict[str, dict[str, str]] = {}
                external_walls: dict[str, int] = {}
                for variant in order:
                    prefix = per_case / f"{label}.rep{repetition:02d}.{variant}"
                    prefixes[variant] = prefix
                    binaries = baseline_binaries if variant == "baseline" else candidate_binaries
                    binary = binaries["core" if case.mode == "ordinary-eac3-core" else "api"]
                    root = baseline_root if variant == "baseline" else candidate_root
                    external_walls[variant] = run_probe(
                        binary,
                        case,
                        prefix,
                        stage=args.stage,
                        cwd=root,
                        log=logs / f"{label}-rep{repetition:02d}-{variant}.log",
                        input_sha256=expected_hash,
                        allocation_profile=args.allocations,
                        timing_only=args.timing_only,
                    )
                    timings[variant] = read_timing(artifact(prefix, ".timing.tsv"))
                base_prefix = prefixes["baseline"]
                candidate_prefix = prefixes["candidate"]
                shape: dict[str, str] = {}
                parsed: Manifest | None = None
                if args.timing_only:
                    shape_keys = (
                        "input_sha256", "config_fingerprint", "config_descriptor_hex", "latency_samples",
                        "mode", "layout", "input_access_units", "output_frames", "output_sample_count",
                        "tail_samples", "channel_count",
                    )
                    for key in shape_keys:
                        baseline_value = timings["baseline"].get(f"summary_{key}")
                        candidate_value = timings["candidate"].get(f"summary_{key}")
                        if baseline_value is None or candidate_value is None:
                            raise GateError(f"{case.name}: timing-only run omitted output-shape field {key}")
                        if baseline_value != candidate_value:
                            raise GateError(f"{case.name}: baseline/candidate timing output shape differs at {key}")
                        shape[key] = baseline_value
                    if shape["input_sha256"] != expected_hash:
                        raise GateError(f"{case.name}: timing-only probe input hash differs from input file")
                    print(
                        f"PASS timing-only {case.name} replicate={repetition + 1}/{args.repeats}: "
                        f"input_sha256={expected_hash} mode={shape['mode']} layout={case.layout} "
                        f"aus={shape['input_access_units']} frames={shape['output_frames']} "
                        f"samples={shape['output_sample_count']} channels={shape['channel_count']}"
                    )
                else:
                    compare_runs(
                        artifact(base_prefix, ".manifest.tsv"),
                        artifact(base_prefix, ".pcm32le"),
                        artifact(candidate_prefix, ".manifest.tsv"),
                        artifact(candidate_prefix, ".pcm32le"),
                    )
                    parsed = read_manifest(artifact(base_prefix, ".manifest.tsv"))
                    if parsed.headers["input_sha256"] != expected_hash:
                        raise GateError(f"{case.name}: probe input hash differs from input file")
                    print(
                        f"PASS {case.name} replicate={repetition + 1}/{args.repeats}: "
                        f"input_sha256={expected_hash} mode={case.mode} layout={case.layout} "
                        f"aus={len(parsed.access_units)} frames={len(parsed.frames)} samples={parsed.results['sample_count']} "
                        f"channels={parsed.results['channel_count']} pcm_bytes={parsed.results['pcm_bytes']}"
                    )
                if index == 0 and repetition == 0 and args.selftest_integrated:
                    mutation_sensitivity(
                        artifact(base_prefix, ".manifest.tsv"),
                        artifact(base_prefix, ".pcm32le"),
                        artifact(candidate_prefix, ".manifest.tsv"),
                        artifact(candidate_prefix, ".pcm32le"),
                        per_case,
                    )
                row = {
                    "case": case.name,
                    "input_sha256": expected_hash,
                    "mode": case.mode,
                    "layout": case.layout,
                    "repetition": str(repetition + 1),
                    "run_order": "-".join(order),
                    "input_access_units": shape.get("input_access_units", str(len(parsed.access_units) if parsed else 0)),
                    "output_samples": shape.get("output_sample_count", parsed.results["sample_count"] if parsed else "0"),
                    "tail_samples": shape.get("tail_samples", parsed.results["tail_samples"] if parsed else "0"),
                    "channels": shape.get("channel_count", parsed.results["channel_count"] if parsed else "0"),
                }
                for variant in ("baseline", "candidate"):
                    row[f"{variant}_external_process_wall_ns"] = str(external_walls[variant])
                    for key, value in timings[variant].items():
                        row[f"{variant}_{key}"] = value
                summary_rows.append(row)
                if args.keep and repetition == 0:
                    retain_artifacts(prefixes, output_dir / label)
        if summary_rows:
            fieldnames = list(dict.fromkeys(key for row in summary_rows for key in row))
            with summary_path.open("w", newline="", encoding="utf-8") as output:
                writer = csv.DictWriter(output, fieldnames=fieldnames)
                writer.writeheader()
                writer.writerows(summary_rows)
    print(f"paired PCM bit-exact gate passed for {len(args.case)} case(s) × {args.repeats} paired replicate(s); reports: {logs}, {toolchain_log}, and {output_dir / 'measurements.csv'}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (GateError, OSError, subprocess.SubprocessError) as error:
        print(f"PCM BIT-EXACT GATE FAILED: {error}", file=sys.stderr)
        raise SystemExit(1)
