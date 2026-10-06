#!/usr/bin/env python3
"""Generate reusable synthetic 30s/60s JOC and changing-tone E-AC-3 inputs."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import subprocess
import sys


EXPORT_TEST = "tests::export_synthetic_joc_lifecycle_fixture_when_requested"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def checked(command: list[str], *, cwd: Path, env: dict[str, str]) -> str:
    result = subprocess.run(command, cwd=cwd, env=env, text=True, capture_output=True, check=False)
    if result.returncode:
        raise RuntimeError(
            f"command failed ({result.returncode}): {' '.join(command)}\n"
            f"stdout:\n{result.stdout[-4000:]}\nstderr:\n{result.stderr[-8000:]}"
        )
    return result.stdout + result.stderr


def generate_joc(root: Path, target: Path, output: Path, access_units: int, env: dict[str, str]) -> dict[str, object]:
    output.parent.mkdir(parents=True, exist_ok=True)
    output.unlink(missing_ok=True)
    cargo_env = env.copy()
    cargo_env.update(
        {
            "CARGO_TARGET_DIR": str(target),
            "CARGO_INCREMENTAL": "0",
            "CARGO_BUILD_JOBS": "1",
            "OPENJOC_LIFECYCLE_JOC_PATH": str(output),
            "OPENJOC_LIFECYCLE_JOC_FRAME_COUNT": str(access_units),
        }
    )
    command = [
        "cargo",
        "+1.89.0",
        "test",
        "--release",
        "--locked",
        "--offline",
        "-p",
        "openjoc-ffmpeg",
        "--lib",
        EXPORT_TEST,
        "--",
        "--exact",
        "--nocapture",
    ]
    evidence = checked(command, cwd=root, env=cargo_env)
    expected_bytes = access_units * 4096
    if output.stat().st_size != expected_bytes:
        raise RuntimeError(f"JOC fixture size {output.stat().st_size} != {expected_bytes}")
    audit_lines = [line for line in evidence.splitlines() if "lifecycle corpus audit:" in line]
    if len(audit_lines) != 1:
        raise RuntimeError("lifecycle exporter did not produce exactly one synthetic metadata audit")
    return {
        "path": str(output),
        "sha256": sha256(output),
        "bytes": expected_bytes,
        "access_units": access_units,
        "sample_rate_hz": 48_000,
        "programme_samples": access_units * 1536,
        "programme_seconds": access_units * 1536 / 48_000,
        "provenance": "OpenJOC test-only lifecycle builder; synthetic, not real programme media",
        "export_command": command,
        "exporter_audit": audit_lines[0],
        "decode_validation": "test exporter validated two 2.0 OpenJocSession decode/reset/drain passes and sample/PTS conservation before writing",
    }


def generate_ordinary_eac3(path: Path, duration_seconds: float, *, ffmpeg: str, ffprobe: str) -> dict[str, object]:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.unlink(missing_ok=True)
    expressions = [
        "0.22*sin(2*PI*220*t)*(0.65+0.35*sin(2*PI*0.07*t))",
        "0.20*sin(2*PI*311*t+0.3)*(0.60+0.40*sin(2*PI*0.11*t))",
        "0.18*sin(2*PI*417*t+0.6)*(0.55+0.45*sin(2*PI*0.05*t))",
        "0.10*sin(2*PI*65*t)*(0.70+0.30*sin(2*PI*0.13*t))",
        "0.15*sin(2*PI*523*t+0.8)*(0.60+0.40*sin(2*PI*0.09*t))",
        "0.13*sin(2*PI*659*t+0.9)*(0.50+0.50*sin(2*PI*0.03*t))",
    ]
    source = (
        "aevalsrc="
        + "|".join(expressions)
        + f":s=48000:d={duration_seconds:.6f}:channel_layout=5.1"
    )
    command = [
        ffmpeg,
        "-hide_banner",
        "-v",
        "error",
        "-f",
        "lavfi",
        "-i",
        source,
        "-c:a",
        "eac3",
        "-b:a",
        "640k",
        "-f",
        "eac3",
        "-y",
        str(path),
    ]
    version = checked([ffmpeg, "-version"], cwd=path.parent, env=os.environ.copy()).splitlines()[0]
    checked(command, cwd=path.parent, env=os.environ.copy())
    stream_info = checked(
        [
            ffprobe,
            "-v",
            "error",
            "-f",
            "eac3",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=codec_name,sample_rate,channels,channel_layout",
            "-of",
            "default=nw=1",
            str(path),
        ],
        cwd=path.parent,
        env=os.environ.copy(),
    ).splitlines()
    frame_samples = checked(
        [
            ffprobe,
            "-v",
            "error",
            "-f",
            "eac3",
            "-select_streams",
            "a:0",
            "-show_entries",
            "frame=nb_samples",
            "-of",
            "csv=p=0",
            str(path),
        ],
        cwd=path.parent,
        env=os.environ.copy(),
    )
    sample_counts = []
    for line in frame_samples.splitlines():
        value = line.strip().split(",", 1)[0].strip()
        if value.isdigit():
            sample_counts.append(int(value))
    frames = len(sample_counts)
    total_samples = sum(sample_counts)
    expected_aus = math.ceil(duration_seconds * 48_000 / 1536)
    if frames != expected_aus or total_samples != expected_aus * 1536:
        raise RuntimeError(
            f"E-AC-3 encoder duration mismatch: {frames} frames / {total_samples} samples; "
            f"expected {expected_aus} × 1536"
        )
    if "sample_rate=48000" not in stream_info or "channels=6" not in stream_info:
        raise RuntimeError(f"unexpected encoded E-AC-3 stream: {stream_info}")
    return {
        "path": str(path),
        "sha256": sha256(path),
        "bytes": path.stat().st_size,
        "syncframes": frames,
        "sample_rate_hz": 48_000,
        "channels": 6,
        "channel_layout": "5.1(side) as reported by ffprobe",
        "programme_samples": total_samples,
        "programme_seconds": total_samples / 48_000,
        "provenance": "locally encoded synthetic channel-distinct multitone with independent amplitude modulation; not real programme media",
        "encoder": version,
        "encode_command": command,
        "encoded_stream_info": stream_info,
        "channel_tones_hz": [220, 311, 417, 65, 523, 659],
        "amplitude_modulation_hz": [0.07, 0.11, 0.05, 0.13, 0.09, 0.03],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True, help="candidate repository worktree")
    parser.add_argument("--target", type=Path, required=True, help="candidate Cargo target directory")
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--durations", nargs="+", choices=["30", "60"], default=["30", "60"])
    parser.add_argument("--ffmpeg", default="ffmpeg")
    parser.add_argument("--ffprobe", default="ffprobe")
    args = parser.parse_args()
    root = args.repo.resolve()
    target = args.target.resolve()
    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    if shutil.which(args.ffmpeg) is None or shutil.which(args.ffprobe) is None:
        raise RuntimeError("ffmpeg and ffprobe must already be available; this script does not install software")

    corpus: dict[str, object] = {
        "schema": "openjoc-performance-corpus-v1",
        "generated_utc_date": "2026-10-05",
        "synthetic_only": True,
        "real_programme_media_available": False,
        "cases": [],
    }
    cases = []
    for seconds_text in args.durations:
        seconds = int(seconds_text)
        access_units = math.ceil(seconds * 48_000 / 1536)
        actual_seconds = access_units * 1536 / 48_000
        key = f"{seconds}s"
        joc = generate_joc(
            root,
            target,
            output_dir / f"joc.lifecycle.{key}.ec3",
            access_units,
            os.environ.copy(),
        )
        eac3 = generate_ordinary_eac3(
            output_dir / f"ordinary.multitone.{key}.eac3",
            actual_seconds,
            ffmpeg=args.ffmpeg,
            ffprobe=args.ffprobe,
        )
        cases.append({"requested_minimum_seconds": seconds, "actual_seconds": actual_seconds, "joc": joc, "ordinary_eac3": eac3})
        print(
            f"{key}: JOC aus={access_units} sha256={joc['sha256']} metadata={joc['exporter_audit']}\n"
            f"{key}: E-AC-3 syncframes={eac3['syncframes']} samples={eac3['programme_samples']} "
            f"sha256={eac3['sha256']} encoder={eac3['encoder']}"
        )
    corpus["cases"] = cases
    manifest_path = output_dir / "corpus.json"
    manifest_path.write_text(json.dumps(corpus, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"corpus manifest: {manifest_path}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"corpus generation failed: {error}", file=sys.stderr)
        raise SystemExit(1)
