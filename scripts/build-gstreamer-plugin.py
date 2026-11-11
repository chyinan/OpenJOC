#!/usr/bin/env python3
"""Build the local GStreamer plugin and print its actual artifact directory."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest-path", type=Path, required=True)
    parser.add_argument("--target-dir", required=True)
    args = parser.parse_args()
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = args.target_dir
    build = subprocess.run(
        ["cargo", "build", "--manifest-path", str(args.manifest_path),
         "-p", "gst-plugin-openjoc", "--release", "--features", "gstreamer",
         "--message-format=json-render-diagnostics"],
        env=environment, stdout=subprocess.PIPE, text=True,
    )
    if build.returncode:
        return build.returncode

    plugin_manifest = args.manifest_path.resolve().parent / "crates/gst-plugin-openjoc/Cargo.toml"
    artifacts: set[Path] = set()
    try:
        for line in build.stdout.splitlines():
            message = json.loads(line)
            if message.get("reason") != "compiler-artifact":
                continue
            if Path(message.get("manifest_path", "")).resolve() != plugin_manifest:
                continue
            target = message.get("target", {})
            if target.get("name") != "gstopenjoc" or "cdylib" not in target.get("crate_types", []):
                continue
            for filename in message.get("filenames", []):
                path = Path(filename)
                if path.suffix in {".so", ".dylib", ".dll"}:
                    artifacts.add(path.resolve())
    except (ValueError, TypeError, AttributeError) as error:
        print(f"invalid Cargo artifact output: {error}", file=sys.stderr)
        return 1
    if len(artifacts) != 1:
        print("expected exactly one gstopenjoc cdylib from the Cargo build", file=sys.stderr)
        return 1
    artifact = artifacts.pop()
    if not artifact.is_file():
        print(f"Cargo plugin artifact does not exist: {artifact}", file=sys.stderr)
        return 1
    print(artifact.parent)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
