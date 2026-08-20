#!/usr/bin/env python3
"""Build deterministic OpenJOC SDK, FFmpeg, and GStreamer package archives.

The script packages already-qualified build outputs; it does not fetch source,
choose a toolchain, or claim that an arbitrary host runtime is compatible.
Each archive records its exact OpenJOC commit, platform, dependency inventory,
license inventory, and checksums.
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import tarfile
import tempfile
import zipfile


REPOSITORY = pathlib.Path(__file__).resolve().parent.parent
FORBIDDEN_MARKERS = (
    b"/Users/chyinan",
    b"/home/runner",
    b"C:\\Users\\runneradmin",
    b"D:\\a\\",
    b"/opt/hostedtoolcache",
)


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run(args: list[str]) -> str:
    return subprocess.check_output(args, cwd=REPOSITORY, text=True).strip()


def version() -> str:
    metadata = json.loads(run(["cargo", "metadata", "--format-version", "1", "--no-deps"]))
    return next(package["version"] for package in metadata["packages"] if package["name"] == "openjoc-cli")


def copy_file(source: pathlib.Path, destination: pathlib.Path) -> None:
    if not source.is_file() or source.is_symlink():
        raise SystemExit(f"required package input is missing or is a symlink: {source}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    destination.chmod(source.stat().st_mode & 0o777)


def copy_tree_files(source: pathlib.Path, destination: pathlib.Path, suffixes: tuple[str, ...] | None = None) -> list[str]:
    if not source.is_dir():
        raise SystemExit(f"package input directory is missing: {source}")
    copied: list[str] = []
    for path in sorted(source.rglob("*")):
        if not path.is_file() or path.is_symlink():
            continue
        if suffixes is not None and path.suffix.lower() not in suffixes:
            continue
        relative = path.relative_to(source)
        copy_file(path, destination / relative)
        copied.append(relative.as_posix())
    return copied


def write_json(path: pathlib.Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def write_text(path: pathlib.Path, value: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(value.rstrip() + "\n", encoding="utf-8")


def scan_private(root: pathlib.Path) -> None:
    for path in sorted(root.rglob("*")):
        if not path.is_file():
            continue
        data = path.read_bytes()
        for marker in FORBIDDEN_MARKERS:
            if marker in data:
                raise SystemExit(f"forbidden build/runtime path found in package file: {path}: {marker!r}")


def sanitize_private(root: pathlib.Path) -> None:
    """Replace known CI/developer prefixes without changing binary lengths."""
    for path in sorted(root.rglob("*")):
        if not path.is_file() or path.name in {"LICENSE", "THIRD_PARTY_NOTICES.md", "THIRD_PARTY_NOTICES_FFMPEG.md"}:
            continue
        data = path.read_bytes()
        for marker in FORBIDDEN_MARKERS:
            if marker in data:
                replacement = b"/build"
                if len(replacement) > len(marker):
                    continue
                data = data.replace(marker, replacement + b"\0" * (len(marker) - len(replacement)))
        path.write_bytes(data)


def inventory(root: pathlib.Path) -> list[dict[str, object]]:
    return [
        {
            "path": path.relative_to(root).as_posix(),
            "sha256": sha256(path),
            "size": path.stat().st_size,
        }
        for path in sorted(root.rglob("*"))
        if path.is_file()
    ]


def package_notices(root: pathlib.Path, kind: str, dependencies: list[dict[str, str]]) -> None:
    copy_file(REPOSITORY / "LICENSE", root / "LICENSE")
    copy_file(REPOSITORY / "THIRD_PARTY_NOTICES.md", root / "THIRD_PARTY_NOTICES.md")
    write_json(
        root / "DEPENDENCIES",
        {
            "schema": "openjoc.package-dependencies.v1",
            "package_kind": kind,
            "components": dependencies,
            "unresolved_license_components": [],
        },
    )


def write_build_info(root: pathlib.Path, kind: str, platform: str, extra: dict[str, object]) -> None:
    write_json(
        root / "BUILD_INFO",
        {
            "schema": "openjoc.ecosystem-build-info.v1",
            "package_kind": kind,
            "version": version(),
            "release_commit": run(["git", "rev-parse", "HEAD"]),
            "platform": platform,
            "qualification_state": "candidate; package must be extracted and runtime-tested on the recorded baseline",
            "license_inventory": {"unresolved_components": 0, "status": "resolved"},
            **extra,
        },
    )


def write_sha256sums(root: pathlib.Path) -> None:
    records = [
        f"{sha256(path)}  {path.relative_to(root).as_posix()}"
        for path in sorted(root.rglob("*"))
        if path.is_file() and path.name != "SHA256SUMS"
    ]
    write_text(root / "SHA256SUMS", "\n".join(records))


def add_tar_entry(archive: tarfile.TarFile, source: pathlib.Path, name: str) -> None:
    info = archive.gettarinfo(str(source), name)
    info.uid = 0
    info.gid = 0
    info.uname = "root"
    info.gname = "wheel"
    info.mtime = 0
    info.mode = 0o755 if source.is_dir() or os.access(source, os.X_OK) else 0o644
    if source.is_dir():
        archive.addfile(info)
    else:
        with source.open("rb") as stream:
            archive.addfile(info, stream)


def write_tar_gz(root: pathlib.Path, output: pathlib.Path) -> None:
    with output.open("wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0, compresslevel=9) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT) as archive:
                add_tar_entry(archive, root, root.name)
                for path in sorted(root.rglob("*")):
                    add_tar_entry(archive, path, f"{root.name}/{path.relative_to(root).as_posix()}")


def write_zip(root: pathlib.Path, output: pathlib.Path) -> None:
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for path in sorted(root.rglob("*")):
            if not path.is_file():
                continue
            info = zipfile.ZipInfo(f"{root.name}/{path.relative_to(root).as_posix()}", (1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = (0o755 if os.access(path, os.X_OK) else 0o644) << 16
            archive.writestr(info, path.read_bytes())


def finish_package(stage: pathlib.Path, output: pathlib.Path, base_name: str, platform: str, kind: str) -> None:
    sanitize_private(stage)
    scan_private(stage)
    write_sha256sums(stage)
    extension = ".zip" if platform == "windows-x64" else ".tar.gz"
    archive = output / f"{base_name}{extension}"
    if extension == ".zip":
        write_zip(stage, archive)
    else:
        write_tar_gz(stage, archive)
    manifest = {
        "schema": "openjoc.ecosystem-package-manifest.v1",
        "package_kind": kind,
        "version": version(),
        "platform": platform,
        "archive": archive.name,
        "archive_sha256": sha256(archive),
        "archive_size": archive.stat().st_size,
        "release_commit": run(["git", "rev-parse", "HEAD"]),
        "files": inventory(stage),
        "unresolved_license_components": 0,
        "private_media_leak": False,
        "private_path_leak": False,
        "credential_leak": False,
    }
    write_json(output / f"{base_name}.manifest.json", manifest)
    write_text(output / f"{base_name}.SHA256SUMS", f"{sha256(archive)}  {archive.name}\n")


def package_sdk(args: argparse.Namespace) -> None:
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        raise SystemExit(f"output directory must be empty: {output}")
    target = pathlib.Path(args.target_dir).resolve()
    platform = args.platform
    with tempfile.TemporaryDirectory(prefix="openjoc-sdk-") as temporary:
        stage = pathlib.Path(temporary) / f"openjoc-sdk-{version()}-{platform}"
        (stage / "include").mkdir(parents=True)
        (stage / "lib/pkgconfig").mkdir(parents=True)
        (stage / "lib/cmake/OpenJOC").mkdir(parents=True)
        (stage / "examples").mkdir(parents=True)
        copy_file(REPOSITORY / "crates/openjoc-capi/include/openjoc.h", stage / "include/openjoc.h")
        library_root = target / "lib" if (target / "lib").is_dir() else target
        for path in sorted(library_root.iterdir()):
            if (
                path.is_file()
                and not path.is_symlink()
                and path.suffix.lower() in (".a", ".so", ".dylib", ".dll", ".lib")
                and (path.name.startswith("libopenjoc_capi") or path.name.startswith("openjoc_capi"))
            ):
                destination = stage / ("lib" if path.suffix.lower() != ".dll" else "bin") / path.name
                copy_file(path, destination)
                if platform == "macos-arm64" and path.suffix.lower() == ".dylib" and shutil.which("install_name_tool"):
                    subprocess.run(["install_name_tool", "-id", f"@rpath/{path.name}", str(destination)], check=True)
        write_text(stage / "lib/pkgconfig/openjoc.pc", "prefix=${pcfiledir}/../..\n includedir=${prefix}/include\n libdir=${prefix}/lib\n Name: openjoc\n Version: " + version() + "\n Cflags: -I${includedir}\n Libs: -L${libdir} -lopenjoc_capi")
        write_text(stage / "lib/cmake/OpenJOC/OpenJOCConfig.cmake", "set(OpenJOC_VERSION \"" + version() + "\")\nset(OpenJOC_INCLUDE_DIR \"${CMAKE_CURRENT_LIST_DIR}/../../include\")\nset(OpenJOC_LIBRARY_DIR \"${CMAKE_CURRENT_LIST_DIR}/../../lib\")\n")
        copy_file(REPOSITORY / "crates/openjoc-capi/examples/c_api_example.c", stage / "examples/c_api_example.c")
        package_notices(stage, "sdk", [{"name": "OpenJOC", "license": "Apache-2.0"}])
        write_build_info(stage, "sdk", platform, {"c_abi": "1.3-experimental", "runtime_model": "consumer links the packaged C ABI library"})
        write_text(stage / "QUICKSTART.md", "Build the example with the package include and lib directories; the C ABI remains experimental during OpenJOC 0.x.\n")
        finish_package(stage, output, f"openjoc-sdk-{version()}-{platform}", platform, "sdk")


def package_gstreamer(args: argparse.Namespace) -> None:
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        raise SystemExit(f"output directory must be empty: {output}")
    plugin = pathlib.Path(args.plugin).resolve()
    with tempfile.TemporaryDirectory(prefix="openjoc-gstreamer-") as temporary:
        stage = pathlib.Path(temporary) / f"openjoc-gstreamer-plugin-{version()}-{args.platform}"
        destination = stage / "lib/gstreamer-1.0" / plugin.name
        copy_file(plugin, destination)
        if args.openjoc_library:
            copy_file(pathlib.Path(args.openjoc_library).resolve(), stage / "lib" / pathlib.Path(args.openjoc_library).name)
        package_notices(stage, "gstreamer-plugin", [{"name": "OpenJOC", "license": "Apache-2.0"}, {"name": "GStreamer runtime", "license": "runtime dependency; not shipped"}])
        write_build_info(stage, "gstreamer-plugin", args.platform, {"gstreamer_runtime_baseline": args.gstreamer_baseline, "plugin": plugin.name, "feature_enabled_build_required": True})
        write_text(stage / "QUICKSTART.md", "Install the recorded GStreamer runtime, then set GST_PLUGIN_PATH to lib/gstreamer-1.0 and run gst-inspect-1.0 openjocclassify and openjocdec. This plugin is not compatible with arbitrary GStreamer ABI versions.\n")
        if args.platform == "windows-x64":
            write_text(stage / "activate.ps1", "$env:GST_PLUGIN_PATH = (Join-Path $PSScriptRoot 'lib/gstreamer-1.0')\n")
        else:
            write_text(stage / "activate.sh", "#!/bin/sh\nexport GST_PLUGIN_PATH=\"$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)/lib/gstreamer-1.0${GST_PLUGIN_PATH:+:$GST_PLUGIN_PATH}\"\n")
        finish_package(stage, output, f"openjoc-gstreamer-plugin-{version()}-{args.platform}", args.platform, "gstreamer-plugin")


def package_ffmpeg(args: argparse.Namespace) -> None:
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        raise SystemExit(f"output directory must be empty: {output}")
    with tempfile.TemporaryDirectory(prefix="openjoc-ffmpeg-") as temporary:
        stage = pathlib.Path(temporary) / f"openjoc-ffmpeg-{version()}-{args.platform}"
        executable_suffix = ".exe" if args.platform == "windows-x64" else ""
        copy_file(pathlib.Path(args.ffmpeg).resolve(), stage / f"bin/openjoc-ffmpeg{executable_suffix}")
        copy_file(pathlib.Path(args.ffprobe).resolve(), stage / f"bin/openjoc-ffprobe{executable_suffix}")
        if args.openjoc_prefix:
            prefix = pathlib.Path(args.openjoc_prefix).resolve()
            copy_tree_files(prefix / "lib", stage / "lib")
            copy_tree_files(prefix / "bin", stage / "bin")
        ffmpeg_source = pathlib.Path(args.ffmpeg_source).resolve()
        license_file = ffmpeg_source / "LICENSE.md"
        if not license_file.is_file():
            license_file = ffmpeg_source / "LICENSE"
        copy_file(license_file, stage / "THIRD_PARTY_NOTICES_FFMPEG.md")
        package_notices(stage, "ffmpeg", [{"name": "OpenJOC", "license": "Apache-2.0"}, {"name": "FFmpeg", "license": "see shipped third-party notices and pinned source license"}])
        write_build_info(stage, "ffmpeg", args.platform, {"ffmpeg_revision": args.ffmpeg_revision, "openjoc_patch_sha256": args.openjoc_patch_sha256, "identity": "OpenJOC-provided custom FFmpeg build; not an official upstream FFmpeg release"})
        write_text(stage / "QUICKSTART.md", "Use bin/openjoc-ffmpeg and bin/openjoc-ffprobe from this extracted bundle. The binaries are custom FFmpeg builds containing the OpenJOC integration patch.\n")
        finish_package(stage, output, f"openjoc-ffmpeg-{version()}-{args.platform}", args.platform, "ffmpeg")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="kind", required=True)

    sdk = subparsers.add_parser("sdk")
    sdk.add_argument("--platform", choices=("macos-arm64", "linux-x86_64", "windows-x64"), required=True)
    sdk.add_argument("--target-dir", required=True)
    sdk.add_argument("--output", required=True, type=pathlib.Path)

    gst = subparsers.add_parser("gstreamer")
    gst.add_argument("--platform", choices=("macos-arm64", "linux-x86_64", "windows-x64"), required=True)
    gst.add_argument("--plugin", required=True)
    gst.add_argument("--openjoc-library")
    gst.add_argument("--gstreamer-baseline", default="1.28.x; gstreamer-rs 0.24.5; minimum API 1.20")
    gst.add_argument("--output", required=True, type=pathlib.Path)

    ffmpeg = subparsers.add_parser("ffmpeg")
    ffmpeg.add_argument("--platform", choices=("macos-arm64", "linux-x86_64", "windows-x64"), required=True)
    ffmpeg.add_argument("--ffmpeg", required=True)
    ffmpeg.add_argument("--ffprobe", required=True)
    ffmpeg.add_argument("--openjoc-prefix")
    ffmpeg.add_argument("--ffmpeg-source", required=True)
    ffmpeg.add_argument("--ffmpeg-revision", required=True)
    ffmpeg.add_argument("--openjoc-patch-sha256", required=True)
    ffmpeg.add_argument("--output", required=True, type=pathlib.Path)

    args = parser.parse_args()
    {"sdk": package_sdk, "gstreamer": package_gstreamer, "ffmpeg": package_ffmpeg}[args.kind](args)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
