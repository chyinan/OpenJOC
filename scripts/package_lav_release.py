#!/usr/bin/env python3
"""Build the portable OpenJOC LAV Windows x64 package from release outputs."""

# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0
# pattern: Imperative Shell

from __future__ import annotations

import argparse
import json
import pathlib
import shutil
import tempfile

from release_packaging_core import deterministic_zip, render_sha256_manifest, sha256_file


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
            _write_text(path, text.replace("0.15.0", version))


def _required_runtime_files() -> tuple[str, ...]:
    return (
        "LAVAudio.ax",
        "LAVAudio.ax.manifest",
        "LAVFilters.Dependencies.manifest",
        "openjoc_capi.dll",
        *LAV_SUPPORT_DLLS,
        *FFMPEG_DLLS,
        *CRT_DLLS,
    )


def build_package(arguments: argparse.Namespace) -> int:
    version = arguments.release_version
    if version != "0.15.0":
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
    if not dependency_dir.is_dir():
        raise FileNotFoundError(f"runtime dependency directory is missing: {dependency_dir}")
    output_dir.mkdir(parents=True, exist_ok=True)
    output = output_dir / f"openjoc-lav-{version}-windows-x64.zip"
    if output.exists():
        raise FileExistsError(f"refusing to replace existing package: {output}")

    runtime_names = _required_runtime_files()
    with tempfile.TemporaryDirectory(prefix="openjoc-lav-package-") as temporary:
        staging = pathlib.Path(temporary) / "package"
        shutil.copytree(template, staging)
        _replace_release_version(staging, version)
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
        dependency_lines = "\n".join(DEPENDENCY_FILE.format(name=name) for name in FFMPEG_DLLS)
        _write_text(
            runtime / "LAVFilters.Dependencies.manifest",
            DEPENDENCY_MANIFEST.format(files=dependency_lines),
        )
        for name in CRT_DLLS:
            _copy_file(dependency_dir / name, runtime / name)

        profile = {
            "version": version,
            "architecture": "x64",
            "required_runtime_files": list(runtime_names),
        }
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
    parser.add_argument("--release-version", required=True)
    parser.add_argument("--lav-root", type=pathlib.Path, required=True)
    parser.add_argument("--capi-dll", type=pathlib.Path, required=True)
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
