# pattern: Imperative Shell

"""Measure lossless HRTF packaging compression, including round-trip time."""

from __future__ import annotations

import gzip
import hashlib
import json
import time
from pathlib import Path

try:
    import zstandard
except ImportError:
    zstandard = None


ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "crates" / "openjoc-sofa" / "assets"
SHAPES = {
    "sadie-ii-d1-ku100": (8_817, 256),
    "sadie-ii-d2-kemar": (8_817, 256),
}


def measure(path: Path) -> dict[str, object]:
    raw = path.read_bytes()
    direction_count, tap_count = SHAPES[path.stem]
    result: dict[str, object] = {
        "preset": path.stem,
        "asset_bytes": len(raw),
        "raw_f32_tap_bytes": direction_count * 2 * tap_count * 4,
        "sha256": hashlib.sha256(raw).hexdigest(),
    }

    started = time.perf_counter()
    gzip_asset = gzip.compress(raw, compresslevel=9, mtime=0)
    result["gzip9_bytes"] = len(gzip_asset)
    result["gzip9_compress_ms"] = round((time.perf_counter() - started) * 1000, 1)
    started = time.perf_counter()
    gzip_roundtrip = gzip.decompress(gzip_asset)
    result["gzip9_decompress_ms"] = round((time.perf_counter() - started) * 1000, 1)
    if gzip_roundtrip != raw:
        raise RuntimeError(f"gzip round trip changed {path.name}")

    if zstandard is not None:
        compressor = zstandard.ZstdCompressor(level=1)
        started = time.perf_counter()
        zstd_asset = compressor.compress(raw)
        result["zstd1_bytes"] = len(zstd_asset)
        result["zstd1_compress_ms"] = round((time.perf_counter() - started) * 1000, 1)
        started = time.perf_counter()
        zstd_roundtrip = zstandard.ZstdDecompressor().decompress(zstd_asset)
        result["zstd1_decompress_ms"] = round((time.perf_counter() - started) * 1000, 1)
        if zstd_roundtrip != raw:
            raise RuntimeError(f"zstd round trip changed {path.name}")
    return result


def main() -> None:
    results = [measure(ASSETS / f"{preset}.ojhrtf") for preset in SHAPES]
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
