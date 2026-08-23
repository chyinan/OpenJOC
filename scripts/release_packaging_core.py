#!/usr/bin/env python3
"""Pure helpers for deterministic OpenJOC release packaging and PE closure."""

# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0
# pattern: Functional Core

from __future__ import annotations

import hashlib
import pathlib
import zipfile
from collections.abc import Iterable


FIXED_ZIP_TIMESTAMP = (2026, 8, 22, 0, 0, 0)
FORBIDDEN_PARTS = frozenset(
    {
        ".git",
        "__pycache__",
        ".pytest_cache",
        ".mypy_cache",
    }
)
FORBIDDEN_SUFFIXES = frozenset(
    {
        ".dmp",
        ".eac3",
        ".ec3",
        ".exp",
        ".ilk",
        ".iobj",
        ".ipdb",
        ".mkv",
        ".mp4",
        ".obj",
        ".pdb",
        ".pyc",
        ".tlog",
        ".wav",
    }
)
SYSTEM_DLLS = frozenset(
    name.casefold()
    for name in {
        "advapi32.dll",
        "bcrypt.dll",
        "bcryptprimitives.dll",
        "cfgmgr32.dll",
        "comctl32.dll",
        "crypt32.dll",
        "d3d11.dll",
        "dxgi.dll",
        "gdi32.dll",
        "kernel32.dll",
        "msvcrt.dll",
        "ncrypt.dll",
        "ntdll.dll",
        "ole32.dll",
        "oleaut32.dll",
        "rpcrt4.dll",
        "setupapi.dll",
        "shell32.dll",
        "shlwapi.dll",
        "user32.dll",
        "version.dll",
        "winmm.dll",
        "ws2_32.dll",
    }
)


def is_release_file(relative: pathlib.PurePath) -> bool:
    """Return whether a relative path is safe for either release archive."""

    parts = {part.casefold() for part in relative.parts}
    if parts.intersection(FORBIDDEN_PARTS):
        return False
    normalized = tuple(part.casefold() for part in relative.parts)
    if normalized[:1] and "ffmpeg" in normalized and "tests" in normalized and "ref" in normalized:
        return True
    return relative.suffix.casefold() not in FORBIDDEN_SUFFIXES


def archive_files(root: pathlib.Path) -> tuple[pathlib.Path, ...]:
    """Return sorted safe archive-relative files."""

    return tuple(
        sorted(
            (
                path.relative_to(root)
                for path in root.rglob("*")
                if path.is_file() and is_release_file(path.relative_to(root))
            ),
            key=lambda path: path.as_posix(),
        )
    )


def sha256_file(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest().upper()


def render_sha256_manifest(
    root: pathlib.Path, *, excluded: Iterable[str] = ()
) -> str:
    """Render stable SHA-256 lines without hashing the manifest itself."""

    excluded_names = {name.replace("\\", "/").casefold() for name in excluded}
    lines = []
    for relative in archive_files(root):
        name = relative.as_posix()
        if name.casefold() in excluded_names:
            continue
        lines.append(f"{sha256_file(root / relative)}  {name}")
    return "\n".join(lines) + "\n"


def deterministic_zip(root: pathlib.Path, destination: pathlib.Path) -> None:
    """Create a byte-stable ZIP from the release-safe file set."""

    if destination.exists():
        raise FileExistsError(f"archive output already exists: {destination}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(
        destination,
        "w",
        compression=zipfile.ZIP_DEFLATED,
        compresslevel=9,
    ) as archive:
        for relative in archive_files(root):
            info = zipfile.ZipInfo(relative.as_posix(), FIXED_ZIP_TIMESTAMP)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            archive.writestr(info, (root / relative).read_bytes(), compresslevel=9)


def classify_dependency(dependency: str, payload_names: Iterable[str]) -> str:
    """Classify one PE import as package-local, Windows, or unresolved."""

    normalized = dependency.casefold()
    payload = {name.casefold() for name in payload_names}
    if normalized in payload:
        return "LOCAL"
    if (
        normalized in SYSTEM_DLLS
        or normalized.startswith("api-ms-win-")
        or normalized.startswith("ext-ms-win-")
    ):
        return "OS"
    return "MISSING"
