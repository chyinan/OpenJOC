#!/usr/bin/env python3
# pattern: Imperative Shell
"""Run OpenJOC documentation and repository-hygiene checks."""

from __future__ import annotations

import pathlib
import subprocess
import sys

from repository_hygiene_core import (
    documentation_consistency_errors,
    markdown_link_errors,
    root_artifact_errors,
)


REPOSITORY = pathlib.Path(__file__).resolve().parents[1]
CONSISTENCY_INPUTS = {
    "Cargo.toml",
    "CHANGELOG.md",
    "README.md",
    "crates/openjoc-capi/include/openjoc.h",
    "crates/openjoc-scene/src/speaker_layouts.rs",
    "packaging/player/PLAYER_PACKAGE_MANIFEST.json",
    "docs/C_API.md",
    "docs/CAPABILITIES.md",
    "docs/CUSTOM_SPEAKER_LAYOUTS.md",
    "docs/KNOWN_LIMITATIONS.md",
    "docs/README.md",
    "docs/integration/LAV_FILTERS_OPENJOC.md",
}


def candidate_paths() -> set[str]:
    """Gather tracked and non-ignored candidate files from Git."""

    completed = subprocess.run(
        [
            "git",
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
        cwd=REPOSITORY,
        check=True,
        stdout=subprocess.PIPE,
    )
    paths = {
        value.decode("utf-8")
        for value in completed.stdout.split(b"\0")
        if value
    }
    return {path for path in paths if (REPOSITORY / path).is_file()}


def read_utf8(path: str) -> str:
    return (REPOSITORY / path).read_text(encoding="utf-8")


def main() -> int:
    try:
        paths = candidate_paths()
        documents = {path: read_utf8(path) for path in paths if path.endswith(".md")}
        consistency_files = dict(documents)
        consistency_files.update({
            path: read_utf8(path)
            for path in CONSISTENCY_INPUTS
            if (REPOSITORY / path).is_file()
        })
    except (OSError, subprocess.CalledProcessError, UnicodeError) as error:
        print(f"repository hygiene check could not read inputs: {error}", file=sys.stderr)
        return 2

    errors = []
    errors.extend(root_artifact_errors(paths))
    errors.extend(markdown_link_errors(documents, paths))
    errors.extend(documentation_consistency_errors(consistency_files))

    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        print(f"repository hygiene check failed: {len(errors)} error(s)", file=sys.stderr)
        return 1

    print(
        "repository hygiene check passed: "
        f"{len(documents)} Markdown files, {len(paths)} candidate files"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
