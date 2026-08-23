#!/usr/bin/env python3
"""Stage, audit, and deterministically archive OpenJOC LAV release candidates."""

# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0
# pattern: Imperative Shell

from __future__ import annotations

import argparse
import pathlib
import shutil
import subprocess
import sys
from collections.abc import Iterable

from release_packaging_core import (
    archive_files,
    classify_dependency,
    deterministic_zip,
    render_sha256_manifest,
    sha256_file,
)


LAV_MODIFIED_FILES = (
    "common/includes/common_defines.h",
    "decoder/LAVAudio/AudioSettingsProp.cpp",
    "decoder/LAVAudio/LAVAudio.cpp",
    "decoder/LAVAudio/LAVAudio.h",
    "decoder/LAVAudio/LAVAudio.vcxproj",
    "decoder/LAVAudio/dllmain.cpp",
    "include/LAVAudioSettings.h",
)
LAV_NEW_FILES = (
    "decoder/LAVAudio/LAVAudioIdentitySmoke.cpp",
    "decoder/LAVAudio/OpenJocAdmission.cpp",
    "decoder/LAVAudio/OpenJocAdmission.h",
    "decoder/LAVAudio/OpenJocAdmissionTests.cpp",
    "decoder/LAVAudio/OpenJocDecoder.cpp",
    "decoder/LAVAudio/OpenJocDecoder.h",
    "decoder/LAVAudio/OpenJocDecoderSmoke.cpp",
)
FFMPEG_CONFIGURATION_FILES = (
    "config.h",
    "ffbuild/config.log",
    "ffbuild/config.mak",
    "ffbuild/config.out",
)
RELEASE_SCRIPTS = (
    "release_security_core.py",
    "release_security.py",
    "release_lav_msbuild.cmd",
    "release_lav_smokes.cmd",
    "release_packaging_core.py",
    "release_packaging.py",
)
RELEASE_DOCUMENTS = (
    "DISTRIBUTION_REVIEW.md",
    "FORK-RELEASE-METADATA.yml",
    "GCC_RUNTIME_SOURCE.md",
    "SOURCE_AND_LICENSES.md",
    "SOURCE-STATE.md",
    "THIRD_PARTY_COMPONENT_MATRIX.md",
    "THIRD_PARTY_NOTICES.md",
)
RUNTIME_REPLACEMENTS = (
    "avcodec-lav-63.dll",
    "avfilter-lav-12.dll",
    "avformat-lav-63.dll",
    "avutil-lav-61.dll",
    "swresample-lav-7.dll",
    "swscale-lav-10.dll",
    "LAVAudio.ax",
)
LEGACY_ROOT_INSTALLERS = ("install.ps1", "verify.ps1", "uninstall.ps1")


def ensure_new_directory(path: pathlib.Path) -> None:
    """Create one staging root without ever replacing an existing target."""

    if path.exists():
        raise FileExistsError(f"staging target already exists: {path}")
    path.mkdir(parents=True)


def _copy(source: pathlib.Path, destination: pathlib.Path) -> None:
    if not source.is_file():
        raise FileNotFoundError(f"required release input is missing: {source}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, destination)


def _overlay_tree(source: pathlib.Path, destination: pathlib.Path) -> None:
    if not source.is_dir():
        raise FileNotFoundError(f"required release input directory is missing: {source}")
    shutil.copytree(source, destination, dirs_exist_ok=True)


def _prepare_binary_base(
    binary_base: pathlib.Path,
    onboarding_template: pathlib.Path,
    destination: pathlib.Path,
) -> None:
    """Copy a binary base and replace its installer surface atomically in staging."""

    if destination.exists():
        raise FileExistsError(f"binary staging target already exists: {destination}")
    if not binary_base.is_dir():
        raise FileNotFoundError(f"binary base is missing: {binary_base}")
    if not onboarding_template.is_dir():
        raise FileNotFoundError(f"onboarding template is missing: {onboarding_template}")
    shutil.copytree(binary_base, destination)
    for name in LEGACY_ROOT_INSTALLERS:
        (destination / name).unlink(missing_ok=True)
    _overlay_tree(onboarding_template, destination)


def _copy_onboarding_source(
    onboarding_template: pathlib.Path, openjoc_source: pathlib.Path
) -> None:
    """Include the canonical installer sources needed to reproduce the binary overlay."""

    _overlay_tree(onboarding_template, openjoc_source / "packaging" / "windows-lav")


def _write_text(path: pathlib.Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="\n") as stream:
        stream.write(text.replace("\r\n", "\n"))


def _git_output(git: pathlib.Path, repository: pathlib.Path, arguments: Iterable[str]) -> str:
    completed = subprocess.run(
        [str(git), "-C", str(repository), *arguments],
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="strict",
    )
    return completed.stdout


def _copy_release_documents(
    workspace: pathlib.Path, destination: pathlib.Path
) -> None:
    release_docs = workspace / "docs" / "release"
    for name in RELEASE_DOCUMENTS:
        _copy(release_docs / name, destination / name)


def stage_candidates(arguments: argparse.Namespace) -> int:
    workspace = arguments.workspace.resolve()
    lav = arguments.lav_root.resolve()
    binary_base = arguments.binary_base.resolve()
    source_base = arguments.source_base.resolve()
    gcc_source = arguments.gcc_source.resolve()
    output_root = arguments.output_root.resolve()
    git = arguments.git_executable.resolve()
    onboarding_template = arguments.onboarding_template.resolve()
    release_version = arguments.release_version

    if release_version != "0.11.0":
        raise ValueError(
            "the current canonical onboarding template is for release 0.11.0; "
            f"refusing mismatched version {release_version}"
        )

    ensure_new_directory(output_root)
    binary = output_root / "binary-staging"
    source = output_root / "source-staging"
    _prepare_binary_base(binary_base, onboarding_template, binary)
    shutil.copytree(source_base, source)

    for name in RUNTIME_REPLACEMENTS:
        _copy(lav / "bin_x64" / name, binary / "runtime" / name)
    _copy(
        workspace
        / "audit"
        / "remediation-build"
        / "openjoc-target"
        / "release"
        / "openjoc_capi.dll",
        binary / "runtime" / "openjoc_capi.dll",
    )

    _copy_release_documents(workspace, binary)
    _copy(
        workspace / "docs" / "release" / "DISTRIBUTION_REVIEW.md",
        binary / "licenses" / "DISTRIBUTION-REVIEW.md",
    )
    _copy(
        lav / "docs" / "openjoc" / "DIRECTSHOW_BASECLASSES_PROVENANCE.md",
        binary / "DIRECTSHOW_BASECLASSES_PROVENANCE.md",
    )
    _copy(
        lav / "docs" / "openjoc" / "LAV_SOURCE_LICENSE_CENSUS.md",
        binary / "LAV_SOURCE_LICENSE_CENSUS.md",
    )
    _copy(
        lav / "docs" / "openjoc" / "LAV_SOURCE_LICENSE_CENSUS.json",
        binary / "LAV_SOURCE_LICENSE_CENSUS.json",
    )
    _copy(
        workspace / "audit" / "provenance" / "windows-classic-samples" / "LICENSE",
        binary / "licenses" / "DirectShow-Baseclasses-MIT.txt",
    )
    gcc_licenses = (
        workspace
        / "audit"
        / "remediation-inputs"
        / "msys2-gcc-16.2.0-3"
        / "binary-package-extract"
        / "mingw64"
        / "share"
        / "licenses"
        / "gcc-libs"
    )
    for source_name, destination_name in (
        ("COPYING3", "GCC-COPYING3"),
        ("COPYING.LIB", "GCC-COPYING.LIB"),
        ("COPYING.RUNTIME", "GCC-COPYING.RUNTIME"),
        ("README", "GCC-LIBS-README"),
    ):
        _copy(gcc_licenses / source_name, binary / "licenses" / destination_name)
    mpc_license = _git_output(
        git,
        workspace / "audit" / "provenance" / "mpc-hc-history",
        ("show", "dcbf6bf36a37438f8eb25536aafe419c835fcd1c:COPYING.txt"),
    )
    _write_text(binary / "licenses" / "MPC-HC-COPYING.GPLv3", mpc_license)

    openjoc_source = source / "OpenJOC"
    _overlay_tree(workspace / "tools" / "import-etsi-tables", openjoc_source / "tools" / "import-etsi-tables")
    _overlay_tree(workspace / "docs" / "release", openjoc_source / "docs" / "release")
    _copy_onboarding_source(onboarding_template, openjoc_source)
    for name in RELEASE_SCRIPTS:
        _copy(workspace / "scripts" / name, openjoc_source / "scripts" / name)
    _overlay_tree(
        workspace / "scripts" / "msys2-cross-tools",
        openjoc_source / "scripts" / "msys2-cross-tools",
    )
    for test in sorted((workspace / "scripts" / "tests").glob("test_*.py")):
        _copy(test, openjoc_source / "scripts" / "tests" / test.name)

    lav_source = source / "LAVFilters-OpenJOC"
    for relative in (*LAV_MODIFIED_FILES, *LAV_NEW_FILES):
        _copy(lav / relative, lav_source / relative)
    _overlay_tree(lav / "docs" / "openjoc", lav_source / "docs" / "openjoc")
    for relative in FFMPEG_CONFIGURATION_FILES:
        _copy(lav / "ffmpeg" / relative, lav_source / "ffmpeg" / relative)

    patch = _git_output(git, lav, ("diff", "--binary", "HEAD", "--"))
    _write_text(source / "distribution" / "openjoc-lav-working-tree.diff", patch)
    _copy_release_documents(workspace, source / "distribution")
    _copy(
        lav / "docs" / "openjoc" / "DIRECTSHOW_BASECLASSES_PROVENANCE.md",
        source / "distribution" / "DIRECTSHOW_BASECLASSES_PROVENANCE.md",
    )
    _copy(
        lav / "docs" / "openjoc" / "LAV_SOURCE_LICENSE_CENSUS.md",
        source / "distribution" / "LAV_SOURCE_LICENSE_CENSUS.md",
    )
    _copy(
        lav / "docs" / "openjoc" / "LAV_SOURCE_LICENSE_CENSUS.json",
        source / "distribution" / "LAV_SOURCE_LICENSE_CENSUS.json",
    )
    _copy(
        workspace / "audit" / "provenance" / "windows-classic-samples" / "LICENSE",
        source / "licenses" / "DirectShow-Baseclasses-MIT.txt",
    )
    _write_text(source / "licenses" / "MPC-HC-COPYING.GPLv3", mpc_license)
    _copy(
        gcc_source,
        source / "third_party_sources" / "msys2" / gcc_source.name,
    )
    _copy(
        workspace / "docs" / "release" / "GCC_RUNTIME_SOURCE.md",
        source / "third_party_sources" / "msys2" / "GCC_RUNTIME_SOURCE.md",
    )

    print(f"binary_staging={binary}")
    print(f"source_staging={source}")
    print(f"release_version={release_version}")
    print(f"onboarding_template={onboarding_template}")
    return 0


def _pe_audit(runtime: pathlib.Path, release_version: str) -> tuple[str, tuple[str, ...]]:
    try:
        import pefile
    except ImportError as error:  # pragma: no cover - release host dependency
        raise RuntimeError("pefile is required for the PE dependency audit") from error

    payload_names = {path.name.casefold() for path in runtime.iterdir() if path.is_file()}
    missing: list[str] = []
    lines = [
        "# SPDX-FileCopyrightText: 2026 OpenJOC contributors",
        "# SPDX-License-Identifier: Apache-2.0",
        f"OPENJOC_LAV_{release_version.replace('.', '_')}_PE_DEPENDENCY_AUDIT",
        "candidate_root = <candidate-root>",
        "",
    ]
    pe_paths = sorted(
        (
            path
            for path in runtime.iterdir()
            if path.is_file() and path.suffix.casefold() in {".dll", ".ax"}
        ),
        key=lambda path: path.name.casefold(),
    )
    for path in pe_paths:
        pe = pefile.PE(str(path), fast_load=True)
        pe.parse_data_directories(
            directories=[pefile.DIRECTORY_ENTRY["IMAGE_DIRECTORY_ENTRY_IMPORT"]]
        )
        machine = int(pe.FILE_HEADER.Machine)
        lines.append(f"file = runtime/{path.name}")
        lines.append(f"machine = 0x{machine:04X}")
        if machine != 0x8664:
            missing.append(f"runtime/{path.name}:non-x64-machine")
        for entry in getattr(pe, "DIRECTORY_ENTRY_IMPORT", ()):  # API-set stubs have none
            dependency = entry.dll.decode("ascii", errors="replace")
            classification = classify_dependency(dependency, payload_names)
            lines.append(f"import = {dependency} [{classification}]")
            if classification == "MISSING":
                missing.append(f"runtime/{path.name}:{dependency}")
        lines.append("")
        pe.close()
    lines.append(f"pe_dependency_closure = {'PASS' if not missing else 'FAIL'}")
    return "\n".join(lines) + "\n", tuple(missing)


def _render_reproducibility_manifest(stage: pathlib.Path, release_version: str) -> str:
    runtime = stage / "runtime"
    names = (*RUNTIME_REPLACEMENTS, "openjoc_capi.dll", "libgcc_s_seh-1.dll")
    lines = [
        "# SPDX-FileCopyrightText: 2026 OpenJOC contributors",
        "# SPDX-License-Identifier: Apache-2.0",
        f"OPENJOC_LAV_{release_version.replace('.', '_')}_REPRODUCIBILITY_MANIFEST",
        f"artifact_name = openjoc-lav-{release_version}-windows-x64.zip",
        "artifact_status = unpublished local candidate",
        "openjoc_revision = a4e5964eec42eb41b9e7ca0ffd82c03903bfe4be",
        "openjoc_capi_abi = 1.3-experimental",
        "openjoc_rustc = 1.90.0 (1159e78c4 2025-09-14)",
        "openjoc_path_remap = release roots mapped to /openjoc, /cargo, and /rust",
        "lav_revision = fefb6987994ed56e4525e8a125f5fbb53707bc52",
        "lav_build = LAVAudio Rebuild; Release|x64; OpenJOC=true; side-by-side=true",
        "lav_effective_combined_license = GPL-3.0-only",
        "ffmpeg_revision = 599d3a140460e1b57c234fe064db5185fb76ee5b",
        "ffmpeg_configuration = GPL=1; VERSION3=1; NONFREE=0; sanitized authentic log retained",
        "gcc_runtime_package = mingw-w64-x86_64-gcc-libs 16.2.0-3",
        "gcc_source_archive = mingw-w64-gcc-16.2.0-3.src.tar.zst",
        "gcc_source_sha256 = EB3479A8B0B23810FBBBC25EF76879E867E88D09960A40145D73F5505FDA4DA0",
        "",
    ]
    for name in names:
        lines.append(f"sha256.runtime/{name} = {sha256_file(runtime / name)}")
    return "\n".join(lines) + "\n"


def finalize_source(arguments: argparse.Namespace) -> int:
    stage = arguments.stage.resolve()
    destination = arguments.output.resolve()
    if not stage.is_dir():
        raise FileNotFoundError(f"source staging directory is missing: {stage}")
    _write_text(
        stage / "SHA256SUMS.txt",
        render_sha256_manifest(stage, excluded={"SHA256SUMS.txt"}),
    )
    deterministic_zip(stage, destination)
    print(f"source_archive={destination}")
    print(f"source_sha256={sha256_file(destination)}")
    print(f"source_entries={len(archive_files(stage))}")
    return 0


def _source_availability_text(release_version: str, source_hash: str) -> str:
    return """<!--
SPDX-FileCopyrightText: 2026 OpenJOC contributors
SPDX-License-Identifier: Apache-2.0
-->

# Corresponding source availability

This binary candidate corresponds exactly to
`openjoc-lav-{release_version}-corresponding-source.zip`.

SHA-256: `{source_hash}`

The release must publish this source archive together with the binary archive
and preserve the immutable downstream LAV tag `openjoc-{release_version}`.
""".format(release_version=release_version, source_hash=source_hash)


def finalize_binary(arguments: argparse.Namespace) -> int:
    stage = arguments.stage.resolve()
    source_archive = arguments.source_archive.resolve()
    destination = arguments.output.resolve()
    if not stage.is_dir():
        raise FileNotFoundError(f"binary staging directory is missing: {stage}")
    if not source_archive.is_file():
        raise FileNotFoundError(f"source archive is missing: {source_archive}")

    release_version = arguments.release_version
    if release_version != "0.11.0":
        raise ValueError(f"unsupported release version: {release_version}")
    audit, missing = _pe_audit(stage / "runtime", release_version)
    _write_text(stage / "tools" / "PE-DEPENDENCY-AUDIT.txt", audit)
    if missing:
        for item in missing:
            print(f"unresolved_pe_dependency={item}", file=sys.stderr)
        return 1
    source_hash = sha256_file(source_archive)
    _write_text(
        stage / "SOURCE-AVAILABILITY.md",
        _source_availability_text(release_version, source_hash),
    )
    _write_text(
        stage / "REPRODUCIBILITY-MANIFEST.txt",
        _render_reproducibility_manifest(stage, release_version),
    )
    _write_text(
        stage / "tools" / "SHA256SUMS.txt",
        render_sha256_manifest(stage, excluded={"tools/SHA256SUMS.txt"}),
    )
    deterministic_zip(stage, destination)
    print(f"binary_archive={destination}")
    print(f"binary_sha256={sha256_file(destination)}")
    print(f"binary_entries={len(archive_files(stage))}")
    return 0


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="operation", required=True)

    stage = subparsers.add_parser("stage", help="create fresh binary and source staging trees")
    stage.add_argument("--workspace", type=pathlib.Path, required=True)
    stage.add_argument("--lav-root", type=pathlib.Path, required=True)
    stage.add_argument("--binary-base", type=pathlib.Path, required=True)
    stage.add_argument("--source-base", type=pathlib.Path, required=True)
    stage.add_argument("--gcc-source", type=pathlib.Path, required=True)
    stage.add_argument("--git-executable", type=pathlib.Path, required=True)
    stage.add_argument("--output-root", type=pathlib.Path, required=True)
    stage.add_argument("--onboarding-template", type=pathlib.Path, required=True)
    stage.add_argument("--release-version", required=True)
    stage.set_defaults(handler=stage_candidates)

    source = subparsers.add_parser("finalize-source", help="write manifest and source ZIP")
    source.add_argument("--stage", type=pathlib.Path, required=True)
    source.add_argument("--output", type=pathlib.Path, required=True)
    source.set_defaults(handler=finalize_source)

    binary = subparsers.add_parser("finalize-binary", help="audit PE closure and write binary ZIP")
    binary.add_argument("--stage", type=pathlib.Path, required=True)
    binary.add_argument("--source-archive", type=pathlib.Path, required=True)
    binary.add_argument("--output", type=pathlib.Path, required=True)
    binary.add_argument("--release-version", required=True)
    binary.set_defaults(handler=finalize_binary)
    return parser


def main() -> int:
    arguments = _parser().parse_args()
    return int(arguments.handler(arguments))


if __name__ == "__main__":
    raise SystemExit(main())
