"""Relocate the staged FFmpeg Mach-O closure; requires native Apple tools."""
from __future__ import annotations

import os
import pathlib
import subprocess


def run(*args: str) -> str:
    return subprocess.check_output(args, text=True).strip()


def system_dependency(value: str) -> bool:
    return value.startswith(("/usr/lib/", "/System/Library/",
                             "/System/Volumes/Preboot/Cryptexes/OS/usr/lib/", "/Library/Apple/"))


def images(root: pathlib.Path) -> list[pathlib.Path]:
    return [root / "bin/openjoc-ffmpeg", root / "bin/openjoc-ffprobe",
            *sorted((root / "lib").rglob("*.dylib"))]


def dependencies(path: pathlib.Path) -> list[str]:
    # otool -L includes LC_ID_DYLIB for libraries; that is not a dependency.
    identity = [line.strip() for line in run("otool", "-D", str(path)).splitlines()[1:]]
    return [line.strip().split(" (compatibility version ", 1)[0]
            for line in run("otool", "-L", str(path)).splitlines()[1:]
            if line.strip().split(" (compatibility version ", 1)[0] not in identity]


def rpaths(path: pathlib.Path) -> list[str]:
    lines = run("otool", "-l", str(path)).splitlines()
    result = []
    for index, line in enumerate(lines):
        if line.strip() == "cmd LC_RPATH":
            for candidate in lines[index + 1:index + 8]:
                if candidate.strip().startswith("path "):
                    result.append(candidate.strip()[5:].rsplit(" (offset ", 1)[0])
                    break
    return result


def relocate(root: pathlib.Path) -> None:
    owners = images(root)
    libraries: dict[str, pathlib.Path] = {}
    for path in owners[2:]:
        if path.name in libraries:
            raise RuntimeError(f"ambiguous bundled Mach-O library: {path.name}")
        libraries[path.name] = path
    for owner in owners:
        changes = []
        for raw in dependencies(owner):
            if system_dependency(raw):
                continue
            target = libraries.get(pathlib.PurePosixPath(raw).name)
            if target is None:
                raise RuntimeError(f"unbundled Mach-O dependency {raw!r} from {owner}")
            # Direct loader-relative edges avoid build-prefix rpaths entirely.
            relative = os.path.relpath(target, owner.parent)
            changes.extend(["-change", raw, f"@loader_path/{relative}"])
        if owner.suffix == ".dylib":
            changes.extend(["-id", f"@rpath/{owner.name}"])
        for value in dict.fromkeys(rpaths(owner)):
            changes.extend(["-delete_rpath", value])
        if changes:
            run("install_name_tool", *changes, str(owner))
    verify(root)


def verify(root: pathlib.Path) -> None:
    allowed = {path.resolve() for path in images(root)[2:]}
    for owner in images(root):
        for raw in dependencies(owner):
            if system_dependency(raw):
                continue
            if not raw.startswith("@loader_path/"):
                raise RuntimeError(f"non-relocatable Mach-O dependency {raw!r} from {owner}")
            target = (owner.parent / raw.removeprefix("@loader_path/")).resolve()
            if target not in allowed or not target.is_file():
                raise RuntimeError(f"missing bundled Mach-O dependency {raw!r} from {owner}")
        if rpaths(owner):
            raise RuntimeError(f"unexpected runtime search paths in {owner}")


def sign(root: pathlib.Path) -> None:
    # Mutation invalidates existing signatures. Only claim ad-hoc signing;
    # Developer ID signing/notarization must happen separately after packaging.
    owners = images(root)
    for owner in [*owners[2:], *owners[:2]]:
        run("codesign", "--force", "--sign", "-", "--timestamp=none", str(owner))
        run("codesign", "--verify", "--strict", str(owner))
