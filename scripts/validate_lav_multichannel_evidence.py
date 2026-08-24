#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

# pattern: Imperative Shell

from __future__ import annotations

import argparse
import json
import pathlib
import subprocess
import sys

from lav_multichannel_evidence_core import validate_evidence


def _arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Validate measured Windows LAV multichannel support evidence."
    )
    parser.add_argument("evidence", type=pathlib.Path)
    parser.add_argument(
        "--shipped-layouts-exe",
        type=pathlib.Path,
        help="OpenJocShippedLayoutsTests.exe to invoke with --list-shipped",
    )
    return parser.parse_args()


def _read_shipped_layouts(executable: pathlib.Path) -> tuple[str, ...]:
    result = subprocess.run(
        [str(executable.resolve()), "--list-shipped"],
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    return tuple(line.strip() for line in result.stdout.splitlines() if line.strip())


def main() -> int:
    arguments = _arguments()
    try:
        document = json.loads(arguments.evidence.read_text(encoding="utf-8"))
        shipped_layouts = (
            _read_shipped_layouts(arguments.shipped_layouts_exe)
            if arguments.shipped_layouts_exe is not None
            else None
        )
    except (OSError, UnicodeError, json.JSONDecodeError, subprocess.SubprocessError) as error:
        print(f"EVIDENCE_INPUT_ERROR: {error}", file=sys.stderr)
        return 2

    errors = validate_evidence(document, shipped_layouts=shipped_layouts)
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print("LAV_MULTICHANNEL_EVIDENCE_VALID")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
