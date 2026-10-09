#!/usr/bin/env python3
"""Check package-level float-WAVE gain and exact-unity PCM evidence."""
from __future__ import annotations

import argparse
import math
from pathlib import Path
import struct
import sys


def read_float_wave(path: Path) -> tuple[tuple[int, int, int], bytes]:
    raw = path.read_bytes()
    if len(raw) < 12 or raw[:4] not in (b"RIFF", b"RF64") or raw[8:12] != b"WAVE":
        raise ValueError(f"{path}: not a RIFF/RF64 WAVE file")
    offset = 12
    format_info = None
    payload = None
    while offset + 8 <= len(raw):
        chunk_id = raw[offset:offset + 4]
        size = struct.unpack_from("<I", raw, offset + 4)[0]
        start = offset + 8
        end = start + size
        if end > len(raw):
            raise ValueError(f"{path}: truncated {chunk_id!r} chunk")
        chunk = raw[start:end]
        if chunk_id == b"fmt ":
            if size < 16:
                raise ValueError(f"{path}: short fmt chunk")
            code, channels, rate, _byte_rate, align, bits = struct.unpack_from("<HHIIHH", chunk)
            if code == 0xFFFE:
                if size < 40:
                    raise ValueError(f"{path}: short WAVE_FORMAT_EXTENSIBLE fmt chunk")
                code = struct.unpack_from("<I", chunk, 24)[0]
                if chunk[28:40] != bytes.fromhex("00001000800000aa00389b71"):
                    raise ValueError(f"{path}: unknown extensible WAVE subformat GUID")
            if code != 3 or bits != 32 or align != channels * 4:
                raise ValueError(f"{path}: expected 32-bit IEEE-float WAVE, got code={code}, bits={bits}, align={align}")
            format_info = (channels, rate, bits)
        elif chunk_id == b"data":
            payload = chunk
        offset = end + (size & 1)
    if format_info is None or payload is None:
        raise ValueError(f"{path}: missing fmt or data chunk")
    channels, _rate, _bits = format_info
    if channels <= 0 or len(payload) % (channels * 4):
        raise ValueError(f"{path}: data size is not a whole number of frames")
    return format_info, payload


def exact(reference: Path, candidate: Path, channels: int, rate: int) -> None:
    reference_format, reference_data = read_float_wave(reference)
    candidate_format, candidate_data = read_float_wave(candidate)
    expected = (channels, rate, 32)
    if reference_format != expected or candidate_format != expected:
        raise ValueError(f"expected both formats to be {expected}; got {reference_format} and {candidate_format}")
    frames = len(reference_data) // (channels * 4)
    if len(candidate_data) != len(reference_data) or candidate_data != reference_data:
        raise ValueError("unity filter changed final float PCM bytes or sample count")
    print(f"PCM_EXACT:PASS channels={channels} rate={rate} frames={frames}")


def gain(reference: Path, candidate: Path, tenths_db: int, channels: int, rate: int) -> None:
    if tenths_db == 0 or not -200 <= tenths_db <= 200:
        raise ValueError("gain comparison requires nonzero tenths of dB in [-200, 200]")
    reference_format, reference_data = read_float_wave(reference)
    candidate_format, candidate_data = read_float_wave(candidate)
    expected_format = (channels, rate, 32)
    if reference_format != expected_format or candidate_format != expected_format:
        raise ValueError(f"expected both formats to be {expected_format}; got {reference_format} and {candidate_format}")
    if len(candidate_data) != len(reference_data):
        raise ValueError("gain changed the final float PCM sample count")
    factor = math.pow(10.0, tenths_db / 200.0)
    changed = 0
    tested = 0
    max_relative_error = 0.0
    for (source,), (actual,) in zip(struct.iter_unpack("<f", reference_data), struct.iter_unpack("<f", candidate_data)):
        if not math.isfinite(source) or not math.isfinite(actual):
            raise ValueError("non-finite float sample in gain PCM")
        expected = struct.unpack("<f", struct.pack("<f", source * factor))[0]
        if abs(source) > 1e-5:
            tested += 1
            error = abs(actual - expected)
            tolerance = max(2e-7, abs(expected) * 2e-6)
            if error > tolerance:
                raise ValueError(f"sample gain mismatch at sample {tested}: actual={actual!r}, expected≈{expected!r}, error={error:g}")
            max_relative_error = max(max_relative_error, error / max(abs(expected), 1e-12))
            if actual != source:
                changed += 1
    if tested < 100 or changed < max(50, tested // 2):
        raise ValueError(f"insufficient nonzero/gain-changed samples: tested={tested}, changed={changed}")
    frames = len(reference_data) // (channels * 4)
    print(f"PCM_GAIN:PASS tenths_db={tenths_db} factor={factor:.17g} channels={channels} rate={rate} frames={frames} compared={tested} changed={changed} max_rel_error={max_relative_error:.3g}")


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="mode", required=True)
    exact_parser = subparsers.add_parser("exact")
    exact_parser.add_argument("reference", type=Path)
    exact_parser.add_argument("candidate", type=Path)
    exact_parser.add_argument("--channels", type=int, required=True)
    exact_parser.add_argument("--rate", type=int, required=True)
    gain_parser = subparsers.add_parser("gain")
    gain_parser.add_argument("reference", type=Path)
    gain_parser.add_argument("candidate", type=Path)
    gain_parser.add_argument("--tenths-db", type=int, required=True)
    gain_parser.add_argument("--channels", type=int, required=True)
    gain_parser.add_argument("--rate", type=int, required=True)
    args = parser.parse_args()
    try:
        if args.mode == "exact":
            exact(args.reference, args.candidate, args.channels, args.rate)
        else:
            gain(args.reference, args.candidate, args.tenths_db, args.channels, args.rate)
    except (OSError, ValueError, struct.error) as error:
        print(f"PCM_CHECK:FAIL {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
