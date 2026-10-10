#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0
"""Prepare the known FFmpeg MSVC dependency forms used by LAV packaging."""

from __future__ import annotations

import argparse
from pathlib import Path
import sys


# Exact legacy awk command from FFmpeg configure; do not patch unrelated awk.
LEGACY_COMMAND = br"""_DEPCMD='$(DEP$(1)) $(DEP$(1)FLAGS) $($(1)DEP_FLAGS) $< 2>&1 | awk '\''/including/ { sub(/^.*file: */, ""); gsub(/\\/, "/"); if (!match($$0, / /)) print "$@:", $$0 }'\'' > $(@:.o=.d)'"""
WSL_COMMAND = br"""_DEPCMD='$(DEP$(1)) $(DEP$(1)FLAGS) $($(1)DEP_FLAGS) $< 2>&1 | awk '\''/including/ { sub(/^.*file: */, ""); if (!match($$0, / /)) { print $$0 } }'\'' | xargs -r -d\\n -n1 wslpath -u | awk '\''BEGIN { printf "%s:", "$@" }; { sub(/\r/,""); printf " %s", $$0 }; END { print "" }'\'' > $(@:.o=.d)'"""
BACKSLASH_PATTERN = br'gsub(/\\/, "/")'
ESCAPED_PATTERN = br'gsub(/\\\\/, "/")'
PATCHED_COMMAND = LEGACY_COMMAND.replace(BACKSLASH_PATTERN, ESCAPED_PATTERN)
LEGACY_FLAGS = b"_DEPFLAGS='$(CPPFLAGS) $(CFLAGS) -showIncludes -Zs'"
NATIVE_FLAGS = b"_depflags='-showIncludes'"
NATIVE_EVAL = br'eval "${1}_DEPFLAGS=\"\$_depflags\""'
NATIVE_EXPORT = b"CC_DEPFLAGS=$CC_DEPFLAGS"


def prepare_configure(content: bytes) -> tuple[bytes, str]:
    """Patch only the legacy command; leave recognized no-op inputs byte-exact.

    Recognition is limited to the observed MSVC dependency snippets, not a
    validation of all configure logic. New dependency implementations require
    review rather than silently skipping a now-inapplicable legacy patch.
    """
    lines = content.splitlines()
    starts = [i for i, line in enumerate(lines) if line == b"        _type=msvc"]
    if len(starts) != 1:
        raise ValueError("Unrecognized FFmpeg MSVC compiler-detection block.")
    start = starts[0]
    end = next(
        (i for i in range(start + 1, len(lines))
         if lines[i].startswith((b"    elif ", b"    else", b"    fi"))),
        len(lines),
    )
    msvc = [line.strip() for line in lines[start:end]]
    commands = [line for line in msvc if line.startswith(b"_DEPCMD=")]
    legacy_flags = [line for line in msvc if line.startswith(b"_DEPFLAGS=")]
    native_flags = [line for line in msvc if line.startswith(b"_depflags=")]
    legacy = commands.count(LEGACY_COMMAND)
    patched = commands.count(PATCHED_COMMAND)
    if (
        legacy + patched == 1
        and commands.count(WSL_COMMAND) <= 1
        and all(line in (LEGACY_COMMAND, PATCHED_COMMAND, WSL_COMMAND) for line in commands)
        and legacy_flags == [LEGACY_FLAGS]
        and not native_flags
    ):
        expected = BACKSLASH_PATTERN if legacy else ESCAPED_PATTERN
        if content.count(expected) != 1:
            raise ValueError("Ambiguous FFmpeg legacy dependency command.")
        if legacy:
            return content.replace(LEGACY_COMMAND, PATCHED_COMMAND, 1), "legacy-patched"
        return content, "legacy-already-patched"
    active_lines = [line.strip() for line in lines]
    native_evals = [line for line in active_lines if line.startswith(b'eval "${1}_DEPFLAGS=')]
    native_exports = [line for line in active_lines if line.startswith(b"CC_DEPFLAGS=")]
    if (
        native_flags == [NATIVE_FLAGS]
        and not commands
        and not legacy_flags
        and native_evals == [NATIVE_EVAL]
        and native_exports == [NATIVE_EXPORT]
    ):
        return content, "native-show-includes"
    raise ValueError("Unrecognized FFmpeg MSVC dependency-generation implementation.")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("configure", type=Path, help="Path to LAV's ffmpeg/configure")
    args = parser.parse_args()
    try:
        original = args.configure.read_bytes()
        prepared, mode = prepare_configure(original)
        if prepared != original:
            args.configure.write_bytes(prepared)
    except (OSError, ValueError) as error:
        print(f"FFmpeg dependency preparation failed: {error}", file=sys.stderr)
        return 1
    print(f"FFmpeg MSVC dependencies: {mode}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
