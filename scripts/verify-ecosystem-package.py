#!/usr/bin/env python3
"""Extract and smoke-test one OpenJOC ecosystem package archive."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import pathlib
import re
import shutil
import subprocess
import tarfile
import tempfile
import zipfile


_PLAYER_PACKAGE_SPEC = importlib.util.spec_from_file_location(
    "openjoc_player_package", pathlib.Path(__file__).with_name("player-package.py")
)
if _PLAYER_PACKAGE_SPEC is None or _PLAYER_PACKAGE_SPEC.loader is None:
    raise RuntimeError("unable to load shared player package closure implementation")
_PLAYER_PACKAGE = importlib.util.module_from_spec(_PLAYER_PACKAGE_SPEC)
_PLAYER_PACKAGE_SPEC.loader.exec_module(_PLAYER_PACKAGE)
pe_imports = _PLAYER_PACKAGE.pe_imports
verify_windows_dependency_closure_from_roots = _PLAYER_PACKAGE.verify_windows_dependency_closure_from_roots
windows_system_dependency = _PLAYER_PACKAGE.windows_system_dependency


FORBIDDEN = (b"/Users/chyinan", b"/home/runner", b"C:\\Users\\runneradmin", b"D:\\a\\")


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def extract(archive: pathlib.Path, destination: pathlib.Path) -> pathlib.Path:
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive) as package:
            package.extractall(destination)
    else:
        with tarfile.open(archive, "r:gz") as package:
            package.extractall(destination, filter="data")
    roots = [path for path in destination.iterdir() if path.is_dir()]
    if len(roots) != 1:
        raise SystemExit(f"archive must contain one package root, found {roots}")
    return roots[0]


def verify_checksums(root: pathlib.Path) -> None:
    checksum_file = root / "SHA256SUMS"
    lines = checksum_file.read_text(encoding="utf-8").splitlines()
    for line in lines:
        digest, relative = line.split("  ", 1)
        path = root / relative
        if not path.is_file() or path.is_symlink() or sha256(path) != digest:
            raise SystemExit(f"package checksum mismatch: {relative}")


def scan_private(root: pathlib.Path) -> None:
    for path in root.rglob("*"):
        if not path.is_file():
            continue
        data = path.read_bytes()
        if any(marker in data for marker in FORBIDDEN):
            raise SystemExit(f"private/runner path found in {path}")


def hermetic_environment(
    root: pathlib.Path,
    platform_name: str,
    tool_parent: pathlib.Path | None = None,
) -> dict[str, str]:
    """Build a runtime environment without inherited loader/search paths."""
    if platform_name == "windows-x64":
        system_root = os.environ.get("SystemRoot") or os.environ.get("WINDIR") or r"C:\Windows"
        entries = [str(root / "bin")]
        if tool_parent is not None:
            entries.append(str(tool_parent))
        entries.extend([f"{system_root}\\System32", f"{system_root}\\System32\\Wbem"])
        return {
            "PATH": ";".join(dict.fromkeys(entries)),
            "SystemRoot": system_root,
            "WINDIR": system_root,
            "TEMP": str(root.parent),
            "TMP": str(root.parent),
            "LC_ALL": "C",
        }
    entries = [str(root / "bin")]
    if tool_parent is not None:
        entries.append(str(tool_parent))
    entries.extend(["/usr/bin", "/bin"])
    return {
        "PATH": ":".join(dict.fromkeys(entries)),
        "HOME": str(root.parent),
        "LC_ALL": "C",
        "LD_LIBRARY_PATH": str(root / "lib"),
        "DYLD_LIBRARY_PATH": str(root / "lib"),
    }


def run_binary(
    path: pathlib.Path,
    root: pathlib.Path,
    platform_name: str,
    arguments: tuple[str, ...] = ("-version",),
    check: bool = True,
) -> tuple[int, str]:
    completed = subprocess.run(
        [str(path), *arguments],
        cwd=root,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        env=hermetic_environment(root, platform_name),
    )
    output = completed.stdout.decode("utf-8", errors="replace")
    if check and completed.returncode != 0:
        raise SystemExit(
            f"FFmpeg runtime smoke failed for {path.name} (exit {completed.returncode}): {output}"
        )
    return completed.returncode, output


def verify_ffmpeg_runtime(root: pathlib.Path, platform_name: str) -> None:
    suffix = ".exe" if platform_name == "windows-x64" else ""
    ffmpeg = root / f"bin/openjoc-ffmpeg{suffix}"
    ffprobe = root / f"bin/openjoc-ffprobe{suffix}"
    _, ffmpeg_version = run_binary(ffmpeg, root, platform_name)
    _, ffprobe_version = run_binary(ffprobe, root, platform_name)
    if not re.search(r"(?im)^ffmpeg version\b", ffmpeg_version):
        raise SystemExit(f"FFmpeg banner missing from {ffmpeg.name}: {ffmpeg_version}")
    if not re.search(r"(?im)^ffprobe version\b", ffprobe_version):
        raise SystemExit(f"FFprobe banner missing from {ffprobe.name}: {ffprobe_version}")
    _, decoder_inventory = run_binary(ffmpeg, root, platform_name, ("-decoders",))
    for decoder in ("eac3", "libopenjoc"):
        if not re.search(rf"(?im)^\s*[A-Z.]+\s+{re.escape(decoder)}\b", decoder_inventory):
            raise SystemExit(f"decoder inventory missing {decoder}: {decoder_inventory}")
    print(f"{ffmpeg.name} hermetic -version: {ffmpeg_version.splitlines()[0]}")
    print(f"{ffprobe.name} hermetic -version: {ffprobe_version.splitlines()[0]}")
    print("decoder inventory: eac3=PASS libopenjoc=PASS")


def verify_windows_hermetic_closure(
    root: pathlib.Path,
    roots: list[pathlib.Path],
    build_info: dict[str, object],
) -> None:
    bundled, _external = verify_windows_dependency_closure_from_roots(root, roots)
    closure = build_info.get("runtime_dependency_closure", {})
    if not isinstance(closure, dict) or closure.get("missing") != 0:
        raise SystemExit("Windows ecosystem package did not record a zero-missing DLL closure")
    recorded = {str(name).lower() for name in closure.get("non_system_dlls", [])}
    actual = {name.lower() for name in bundled}
    if recorded != actual:
        raise SystemExit(
            f"Windows ecosystem DLL closure mismatch: recorded={sorted(recorded)} actual={sorted(actual)}"
        )
    print(f"Windows non-system DLL closure: {len(actual)} bundled, missing=0")


def negative_windows_missing_dll_smoke(
    root: pathlib.Path,
    ffmpeg: pathlib.Path,
    ffprobe: pathlib.Path,
) -> None:
    capi = root / "bin/openjoc_capi.dll"
    roots = [ffmpeg, ffprobe]
    if capi.is_file():
        roots.append(capi)
    bundled, _external = verify_windows_dependency_closure_from_roots(root, roots)
    candidates = [root / "bin" / name for name in bundled if (root / "bin" / name).is_file()]
    if not candidates:
        raise SystemExit("Windows negative smoke found no non-system packaged DLL")
    missing = sorted(candidates, key=lambda path: path.name.lower())[0]
    renamed = missing.with_name(missing.name + ".missing")
    missing.rename(renamed)
    try:
        return_code, output = run_binary(ffmpeg, root, "windows-x64", check=False)
        if return_code == 0:
            raise SystemExit(
                f"negative missing-DLL smoke unexpectedly passed after removing {missing.name}"
            )
        print(
            f"negative missing-DLL smoke: PASS removed={missing.name} exit={return_code} output={output.strip()!r}"
        )
    finally:
        renamed.rename(missing)
    run_binary(ffmpeg, root, "windows-x64")
    print(f"negative missing-DLL smoke: restored={missing.name} PASS")


def build_environment(
    root: pathlib.Path,
    platform_name: str,
    tool_dirs: list[pathlib.Path],
) -> dict[str, str]:
    environment = hermetic_environment(root, platform_name)
    separator = ";" if platform_name == "windows-x64" else ":"
    entries = [str(path) for path in tool_dirs if path.is_dir()]
    entries.extend(environment["PATH"].split(separator))
    environment["PATH"] = separator.join(dict.fromkeys(entries))
    return environment


def run_consumer(executable: pathlib.Path, root: pathlib.Path, platform_name: str, label: str) -> None:
    completed = subprocess.run(
        [str(executable)],
        cwd=root,
        env=hermetic_environment(root, platform_name),
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        check=False,
    )
    if completed.returncode != 0:
        raise SystemExit(f"{label} consumer failed (exit {completed.returncode}): {completed.stdout}")
    if platform_name == "windows-x64":
        verify_windows_dependency_closure_from_roots(root, [executable])
    print(f"SDK {label} consumer: PASS")


def write_consumer_source(path: pathlib.Path, template: pathlib.Path) -> None:
    shutil.copyfile(template, path)


def verify_sdk_consumers(root: pathlib.Path, platform_name: str) -> None:
    compiler_name = shutil.which("cc") or shutil.which("clang") or shutil.which("gcc")
    if compiler_name is None:
        raise SystemExit("SDK qualification requires a C compiler")
    compiler = pathlib.Path(compiler_name).resolve()
    temporary = root.parent / "openjoc-sdk-consumers"
    temporary.mkdir()
    source = temporary / "consumer.c"
    write_consumer_source(source, root / "examples/c_api_example.c")
    tool_dirs = [compiler.parent]
    build_env = build_environment(root, platform_name, tool_dirs)

    direct = temporary / ("direct-consumer.exe" if platform_name == "windows-x64" else "direct-consumer")
    subprocess.run(
        [
            str(compiler),
            str(source),
            "-I",
            str(root / "include"),
            "-L",
            str(root / "lib"),
            "-lopenjoc_capi",
            "-o",
            str(direct),
        ],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        env=build_env,
    )
    run_consumer(direct, root, platform_name, "direct compiler")

    pkg_config_name = shutil.which("pkg-config") or shutil.which("pkgconf")
    if pkg_config_name is None:
        raise SystemExit("SDK qualification requires pkg-config/pkgconf")
    pkg_config = pathlib.Path(pkg_config_name).resolve()
    pkg_env = build_environment(root, platform_name, [compiler.parent, pkg_config.parent])
    pkg_env["PKG_CONFIG_PATH"] = str(root / "lib/pkgconfig")
    flags = subprocess.check_output(
        [str(pkg_config), "--cflags", "--libs", "openjoc"],
        text=True,
        env=pkg_env,
    ).split()
    pkg_consumer = temporary / ("pkg-config-consumer.exe" if platform_name == "windows-x64" else "pkg-config-consumer")
    subprocess.run(
        [str(compiler), str(source), *flags, "-o", str(pkg_consumer)],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        env=pkg_env,
    )
    run_consumer(pkg_consumer, root, platform_name, "pkg-config")

    cmake_name = shutil.which("cmake")
    if cmake_name is None:
        raise SystemExit("SDK qualification requires CMake")
    cmake = pathlib.Path(cmake_name).resolve()
    cmake_source = temporary / "cmake-source"
    cmake_source.mkdir()
    (cmake_source / "CMakeLists.txt").write_text(
        "cmake_minimum_required(VERSION 3.16)\n"
        "project(openjoc_consumer C)\n"
        "find_package(OpenJOC CONFIG REQUIRED)\n"
        "add_executable(cmake-consumer consumer.c)\n"
        "target_link_libraries(cmake-consumer PRIVATE OpenJOC::openjoc_capi)\n",
        encoding="utf-8",
    )
    shutil.copyfile(source, cmake_source / "consumer.c")
    cmake_build = temporary / "cmake-build"
    cmake_env = build_environment(root, platform_name, [compiler.parent, cmake.parent])
    configure = [
        str(cmake),
        "-S",
        str(cmake_source),
        "-B",
        str(cmake_build),
        "-DCMAKE_BUILD_TYPE=Release",
        f"-DCMAKE_PREFIX_PATH={root}",
        f"-DCMAKE_C_COMPILER={compiler}",
    ]
    subprocess.run(configure, check=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, env=cmake_env)
    build = subprocess.run(
        [str(cmake), "--build", str(cmake_build)],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        env=cmake_env,
    )
    if build.returncode != 0:
        raise SystemExit(
            "CMake CONFIG consumer build failed "
            f"(exit {build.returncode}):\n{build.stdout}"
        )
    cmake_consumer = cmake_build / ("cmake-consumer.exe" if platform_name == "windows-x64" else "cmake-consumer")
    run_consumer(cmake_consumer, root, platform_name, "CMake CONFIG")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", required=True, type=pathlib.Path)
    parser.add_argument("--kind", choices=("sdk", "ffmpeg", "gstreamer-plugin"), required=True)
    parser.add_argument("--platform", choices=("macos-arm64", "linux-x86_64", "windows-x64"), required=True)
    parser.add_argument("--run-gstreamer", action="store_true")
    arguments = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="openjoc-ecosystem-verify-") as temporary:
        root = extract(arguments.archive.resolve(), pathlib.Path(temporary))
        verify_checksums(root)
        scan_private(root)
        for required in ("LICENSE", "THIRD_PARTY_NOTICES.md", "DEPENDENCIES", "BUILD_INFO", "SHA256SUMS", "QUICKSTART.md"):
            if not (root / required).is_file():
                raise SystemExit(f"required package file is missing: {required}")
        build_info = json.loads((root / "BUILD_INFO").read_text(encoding="utf-8"))
        if arguments.kind == "ffmpeg":
            suffix = ".exe" if arguments.platform == "windows-x64" else ""
            ffmpeg = root / f"bin/openjoc-ffmpeg{suffix}"
            ffprobe = root / f"bin/openjoc-ffprobe{suffix}"
            verify_ffmpeg_runtime(root, arguments.platform)
            if arguments.platform == "windows-x64":
                roots = [ffmpeg, ffprobe]
                capi = root / "bin/openjoc_capi.dll"
                if capi.is_file():
                    roots.append(capi)
                verify_windows_hermetic_closure(root, roots, build_info)
                negative_windows_missing_dll_smoke(root, ffmpeg, ffprobe)
        elif arguments.kind == "gstreamer" and arguments.run_gstreamer:
            gst_tool = shutil.which("gst-inspect-1.0")
            if gst_tool is None:
                raise SystemExit("GStreamer qualification requires gst-inspect-1.0")
            environment = hermetic_environment(
                root,
                arguments.platform,
                pathlib.Path(gst_tool).resolve().parent,
            )
            environment["GST_PLUGIN_PATH"] = str(root / "lib/gstreamer-1.0")
            environment["GST_REGISTRY_1_0"] = str(root.parent / "gst-registry.bin")
            for element in ("openjocclassify", "openjocdec"):
                subprocess.run(
                    [gst_tool, element],
                    check=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                    env=environment,
                )
            print(
                "GStreamer verifier isolation: package plugin path plus explicit host gst-inspect directory; inherited loader paths removed"
            )
        elif arguments.kind == "sdk":
            if not (root / "include/openjoc.h").is_file() or not (root / "examples/c_api_example.c").is_file():
                raise SystemExit("SDK header/example surface is incomplete")
            if arguments.platform == "windows-x64":
                capi = root / "bin/openjoc_capi.dll"
                if not capi.is_file():
                    raise SystemExit("Windows SDK package is missing bin/openjoc_capi.dll")
                verify_windows_hermetic_closure(root, [capi], build_info)
            verify_sdk_consumers(root, arguments.platform)
    print(f"ecosystem package PASS: {arguments.archive}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
