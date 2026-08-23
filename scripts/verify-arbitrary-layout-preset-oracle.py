#!/usr/bin/env python3
"""Compare rendered PCM from two OpenJOC CLI builds for every public preset."""

from __future__ import annotations

import argparse
import hashlib
import math
import re
import struct
import subprocess
from pathlib import Path


PRESETS = [
    "2.0",
    "5.1",
    "5.1.2",
    "5.1.4",
    "7.1",
    "7.1.2",
    "7.1.4",
    "7.1.6",
    "9.1",
    "9.1.2",
    "9.1.4",
    "9.1.6",
    "22.2",
]


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def render(binary: Path, fixture: Path, layout: str, output: Path) -> tuple[int, bytes]:
    result = subprocess.run(
        [
            str(binary),
            "render-joc",
            str(fixture),
            "--layout",
            layout,
            "--output",
            str(output),
            "--overwrite",
            "--no-progress",
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    match = re.search(r"^frames: (\d+)$", result.stdout, re.MULTILINE)
    if match is None:
        raise RuntimeError(f"renderer summary has no frame count for {layout}")
    pcm = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", str(output), "-f", "f32le", "-acodec", "pcm_f32le", "-"],
        check=True,
        capture_output=True,
    ).stdout
    return int(match.group(1)), pcm


def numerical_delta(old: bytes, new: bytes) -> tuple[float, float]:
    if len(old) != len(new) or len(old) % 4 != 0:
        return math.inf, math.inf
    old_values = struct.unpack(f"<{len(old) // 4}f", old)
    new_values = struct.unpack(f"<{len(new) // 4}f", new)
    errors = [float(b) - float(a) for a, b in zip(old_values, new_values)]
    maximum = max((abs(error) for error in errors), default=0.0)
    rms = math.sqrt(sum(error * error for error in errors) / len(errors)) if errors else 0.0
    return maximum, rms


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--old-binary", type=Path, required=True)
    parser.add_argument("--new-binary", type=Path, required=True)
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)

    print("# OpenJOC preset cross-version renderer oracle")
    print("# PCM is decoded from deterministic CAF output as interleaved float32.")
    print()
    print("| Preset | Old samples | New samples | Old file SHA-256 | New file SHA-256 | Old PCM SHA-256 | New PCM SHA-256 | Bit identical | Max abs error | RMS error |")
    print("| --- | ---: | ---: | --- | --- | --- | --- | --- | ---: | ---: |")
    for layout in PRESETS:
        old_path = args.output_dir / f"old-{layout}.caf"
        new_path = args.output_dir / f"new-{layout}.caf"
        old_samples, old_pcm = render(args.old_binary, args.fixture, layout, old_path)
        new_samples, new_pcm = render(args.new_binary, args.fixture, layout, new_path)
        identical = old_pcm == new_pcm
        maximum, rms = numerical_delta(old_pcm, new_pcm)
        print(
            f"| `{layout}` | {old_samples} | {new_samples} | `{sha256(old_path.read_bytes())}` | `{sha256(new_path.read_bytes())}` | `{sha256(old_pcm)}` | `{sha256(new_pcm)}` | {'YES' if identical else 'NO'} | {maximum:.9g} | {rms:.9g} |"
        )
        if old_samples != new_samples or not identical:
            raise SystemExit(f"preset oracle failed for {layout}")


if __name__ == "__main__":
    main()
