#!/usr/bin/env python3
"""Build the portable OpenJOC LAV Windows x64 package from release outputs."""

# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0
# pattern: Imperative Shell

from __future__ import annotations

import argparse
import json
import os
import pathlib
import re
import shutil
import tempfile

from release_packaging_core import deterministic_zip, render_sha256_manifest, sha256_file


CANONICAL_RELEASE_VERSION = "0.20.0"

FFMPEG_DLLS = (
    "avcodec-lav-63.dll",
    "avfilter-lav-12.dll",
    "avformat-lav-63.dll",
    "avutil-lav-61.dll",
    "swresample-lav-7.dll",
    "swscale-lav-10.dll",
)
LAV_SUPPORT_DLLS = (
    "libbluray.dll",
    "libgcc_s_seh-1.dll",
    "libwinpthread-1.dll",
    "zlib1.dll",
)
CRT_DLLS = (
    *(f"api-ms-win-crt-{name}-l1-1-0.dll" for name in (
        "conio", "convert", "environment", "filesystem", "heap", "locale",
        "math", "multibyte", "private", "process", "runtime", "stdio",
        "string", "time", "utility",
    )),
    "ucrtbase.dll",
    "vcruntime140.dll",
    "vcruntime140_1.dll",
    "vcruntime140_threads.dll",
    "zlibwapi.dll",
)
# The explicit full variant uses the complete x64 CRT closure verified for the
# three-filter candidate. Do not expand the default audio installer inventory.
FULL_CRT_DLLS = (
    *CRT_DLLS,
    "concrt140.dll",
    "msvcp140.dll",
    "msvcp140_1.dll",
    "msvcp140_2.dll",
    "msvcp140_atomic_wait.dll",
    "msvcp140_codecvt_ids.dll",
    "vccorlib140.dll",
)
FULL_EXTRA_BINARIES = ("LAVVideo.ax", "LAVSplitter.ax", "IntelQuickSyncDecoder.dll")
FULL_EXTRA_MANIFESTS = {
    "LAVVideo.ax.manifest": "decoder/LAVVideo/LAVVideo.manifest",
    "LAVSplitter.ax.manifest": "demuxer/LAVSplitter/LAVSplitter.manifest",
}
FULL_TEMPLATE_FILES = (
    "README.md",
    "SOURCE_AND_LICENSES.md",
    "register-all.bat",
    "unregister-all.bat",
    "scripts/manual-registration.ps1",
    "scripts/registration-classes.json",
)
FULL_LAV_LICENSE_FILES = (
    "COPYING",
    "ffmpeg/LICENSE.md",
    "ffmpeg/COPYING.GPLv2",
    "ffmpeg/COPYING.GPLv3",
    "ffmpeg/COPYING.LGPLv2.1",
    "ffmpeg/COPYING.LGPLv3",
    "libbluray/COPYING",
    "qsdecoder/license.txt",
)
FULL_CORE_LICENSE_FILES = ("LICENSE", "THIRD_PARTY_NOTICES.md")
TEXT_SUFFIXES = frozenset({".bat", ".cmd", ".json", ".md", ".ps1", ".psm1", ".txt"})
DEPENDENCY_MANIFEST = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="LAVFilters.Dependencies" version="1.0.0.0" />
{files}
</assembly>
"""
DEPENDENCY_FILE = '  <file name="{name}" />'


def _write_text(path: pathlib.Path, text: str) -> None:
    with path.open("w", encoding="utf-8", newline="\n") as stream:
        stream.write(text)


def _copy_file(source: pathlib.Path, destination: pathlib.Path) -> None:
    if not source.is_file():
        raise FileNotFoundError(f"required LAV release input is missing: {source}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, destination)


def _replace_release_version(root: pathlib.Path, version: str) -> None:
    for path in root.rglob("*"):
        if path.is_file() and path.suffix.casefold() in TEXT_SUFFIXES:
            text = path.read_text(encoding="utf-8")
            _write_text(path, text.replace(CANONICAL_RELEASE_VERSION, version))


def _copy_full_licenses(
    lav_root: pathlib.Path, core_root: pathlib.Path, staging: pathlib.Path
) -> None:
    for name in FULL_CORE_LICENSE_FILES:
        _copy_file(core_root / name, staging / "LICENSES" / "openjoc" / name)
    selected = {lav_root / name for name in FULL_LAV_LICENSE_FILES}
    for base, directories, files in os.walk(lav_root):
        directories[:] = [name for name in directories if name not in {
            ".git", "target", "bin_x64", "obj_x64", "bin", "obj", "__pycache__",
        }]
        for name in files:
            if re.match(r"^(copying|licen[cs]e|notice|copyright)([._-]|$)", name, re.I):
                selected.add(pathlib.Path(base) / name)
    for source in sorted(selected):
        _copy_file(source, staging / "LICENSES" / "lav" / source.relative_to(lav_root))


def _required_runtime_files(variant: str = "audio") -> tuple[str, ...]:
    if variant not in {"audio", "full"}:
        raise ValueError(f"unsupported LAV package variant: {variant}")
    return (
        "LAVAudio.ax",
        "LAVAudio.ax.manifest",
        "LAVFilters.Dependencies.manifest",
        "openjoc_capi.dll",
        *LAV_SUPPORT_DLLS,
        *FFMPEG_DLLS,
        *(FULL_CRT_DLLS if variant == "full" else CRT_DLLS),
        *(FULL_EXTRA_BINARIES if variant == "full" else ()),
        *(FULL_EXTRA_MANIFESTS if variant == "full" else ()),
    )


def build_package(arguments: argparse.Namespace) -> int:
    variant = getattr(arguments, "variant", "audio")
    runtime_names = _required_runtime_files(variant)
    version = arguments.release_version
    if version != CANONICAL_RELEASE_VERSION:
        raise ValueError(f"unsupported OpenJOC LAV release version: {version}")
    lav_root = arguments.lav_root.resolve()
    capi_dll = arguments.capi_dll.resolve()
    dependency_dir = arguments.dependency_dir.resolve()
    template = arguments.onboarding_template.resolve()
    output_dir = arguments.output_dir.resolve()
    if not lav_root.is_dir():
        raise FileNotFoundError(f"LAV root is missing: {lav_root}")
    if not template.is_dir():
        raise FileNotFoundError(f"onboarding template is missing: {template}")
    if variant == "full":
        core_source_root = getattr(arguments, "core_source_root", None)
        if core_source_root is None:
            raise ValueError("--core-source-root is required for full package")
        core_source_root = core_source_root.resolve()
        if not core_source_root.is_dir():
            raise FileNotFoundError(f"core source root is missing: {core_source_root}")
        for name in FULL_TEMPLATE_FILES:
            if not (template / name).is_file():
                raise FileNotFoundError(f"required full package template input is missing: {template / name}")
        if any((template / name).exists() for name in ("install.bat", "uninstall.bat")):
            raise ValueError("full package must use its separate manual-registration template")
    elif any((template / name).exists() for name in ("register-all.bat", "unregister-all.bat")):
        raise ValueError("audio package must use its existing audio onboarding template")
    if not dependency_dir.is_dir():
        raise FileNotFoundError(f"runtime dependency directory is missing: {dependency_dir}")
    output_dir.mkdir(parents=True, exist_ok=True)
    package_name = "openjoc-lav-full" if variant == "full" else "openjoc-lav"
    output = output_dir / f"{package_name}-{version}-windows-x64.zip"
    if output.exists():
        raise FileExistsError(f"refusing to replace existing package: {output}")

    with tempfile.TemporaryDirectory(prefix="openjoc-lav-package-") as temporary:
        staging = pathlib.Path(temporary) / "package"
        shutil.copytree(template, staging)
        if variant == "audio":
            _replace_release_version(staging, version)
        else:
            _copy_full_licenses(lav_root, core_source_root, staging)
        runtime = staging / "runtime"
        runtime.mkdir()

        for name in FFMPEG_DLLS:
            _copy_file(lav_root / "bin_x64" / name, runtime / name)
        for name in LAV_SUPPORT_DLLS:
            _copy_file(lav_root / "bin_x64" / name, runtime / name)
        _copy_file(lav_root / "bin_x64" / "LAVAudio.ax", runtime / "LAVAudio.ax")
        _copy_file(capi_dll, runtime / "openjoc_capi.dll")
        _copy_file(
            lav_root / "decoder" / "LAVAudio" / "LAVAudio.manifest",
            runtime / "LAVAudio.ax.manifest",
        )
        if variant == "full":
            for name in FULL_EXTRA_BINARIES:
                _copy_file(lav_root / "bin_x64" / name, runtime / name)
            for name, source in FULL_EXTRA_MANIFESTS.items():
                _copy_file(lav_root / source, runtime / name)
        for name in FULL_CRT_DLLS if variant == "full" else CRT_DLLS:
            _copy_file(dependency_dir / name, runtime / name)

        # Full filters share one assembly containing every bundled DLL, including
        # Quick Sync and the C++ runtime. Preserve the audio manifest unchanged.
        dependency_names = (
            sorted(name for name in runtime_names if name.lower().endswith(".dll"))
            if variant == "full" else FFMPEG_DLLS
        )
        dependency_lines = "\n".join(DEPENDENCY_FILE.format(name=name) for name in dependency_names)
        _write_text(
            runtime / "LAVFilters.Dependencies.manifest",
            DEPENDENCY_MANIFEST.format(files=dependency_lines),
        )
        profile = {
            "version": version,
            "architecture": "x64",
            "required_runtime_files": list(runtime_names),
        }
        if variant == "full":
            profile["variant"] = "full"
        _write_text(
            runtime / "OpenJocRuntimeProfile.json", json.dumps(profile, indent=2) + "\n"
        )
        _write_text(
            staging / "PACKAGE_SHA256SUMS.txt",
            render_sha256_manifest(staging, excluded={"PACKAGE_SHA256SUMS.txt"}),
        )
        deterministic_zip(staging, output)

    print(f"lav_package={output}")
    print(f"lav_package_sha256={sha256_file(output)}")
    return 0


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--variant", choices=("audio", "full"), default="audio",
        help="audio keeps the one-click installer (default); full is a separate opt-in bundle",
    )
    parser.add_argument("--release-version", required=True)
    parser.add_argument("--lav-root", type=pathlib.Path, required=True)
    parser.add_argument("--capi-dll", type=pathlib.Path, required=True)
    parser.add_argument(
        "--core-source-root", type=pathlib.Path,
        help="actual OpenJOC source checkout used for the CAPI (required for full notices only)",
    )
    parser.add_argument("--dependency-dir", type=pathlib.Path, required=True)
    parser.add_argument("--onboarding-template", type=pathlib.Path, required=True)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.set_defaults(handler=build_package)
    return parser


def main() -> int:
    arguments = _parser().parse_args()
    return int(arguments.handler(arguments))


if __name__ == "__main__":
    raise SystemExit(main())
