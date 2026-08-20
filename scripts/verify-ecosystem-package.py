#!/usr/bin/env python3
"""Extract and smoke-test one OpenJOC ecosystem package archive."""

from __future__ import annotations

import argparse
import hashlib
import os
import pathlib
import shutil
import subprocess
import tarfile
import tempfile
import zipfile


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


def run_binary(path: pathlib.Path, root: pathlib.Path) -> None:
    environment = os.environ.copy()
    library = root / "lib"
    if path.suffix == ".exe":
        environment["PATH"] = f"{root / 'bin'};{library};{environment.get('PATH', '')}"
    else:
        environment["PATH"] = f"{root / 'bin'}:{environment.get('PATH', '')}"
        environment["LD_LIBRARY_PATH"] = f"{library}:{environment.get('LD_LIBRARY_PATH', '')}"
        environment["DYLD_LIBRARY_PATH"] = f"{library}:{environment.get('DYLD_LIBRARY_PATH', '')}"
    subprocess.run([str(path), "-version"], check=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, env=environment)


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
        if arguments.kind == "ffmpeg":
            suffix = ".exe" if arguments.platform == "windows-x64" else ""
            run_binary(root / f"bin/openjoc-ffmpeg{suffix}", root)
            run_binary(root / f"bin/openjoc-ffprobe{suffix}", root)
        elif arguments.kind == "gstreamer" and arguments.run_gstreamer:
            environment = os.environ.copy()
            environment["GST_PLUGIN_PATH"] = str(root / "lib/gstreamer-1.0")
            for element in ("openjocclassify", "openjocdec"):
                subprocess.run(["gst-inspect-1.0", element], check=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, env=environment)
        elif arguments.kind == "sdk":
            if not (root / "include/openjoc.h").is_file() or not (root / "examples/c_api_example.c").is_file():
                raise SystemExit("SDK header/example surface is incomplete")
            compiler = shutil.which("cc") or shutil.which("clang") or shutil.which("gcc")
            if compiler is None:
                raise SystemExit("SDK qualification requires a C compiler")
            consumer = root.parent / "openjoc-sdk-consumer"
            command = [
                compiler,
                str(root / "examples/c_api_example.c"),
                "-I",
                str(root / "include"),
                "-L",
                str(root / "lib"),
                "-L",
                str(root / "bin"),
                "-lopenjoc_capi",
                "-o",
                str(consumer),
            ]
            subprocess.run(command, check=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            executable = consumer.with_suffix(".exe") if arguments.platform == "windows-x64" else consumer
            environment = os.environ.copy()
            if arguments.platform == "windows-x64":
                environment["PATH"] = f"{root / 'bin'};{root / 'lib'};{environment.get('PATH', '')}"
            else:
                environment["LD_LIBRARY_PATH"] = f"{root / 'lib'}:{environment.get('LD_LIBRARY_PATH', '')}"
                environment["DYLD_LIBRARY_PATH"] = f"{root / 'lib'}:{environment.get('DYLD_LIBRARY_PATH', '')}"
            subprocess.run([str(executable)], check=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, env=environment)
    print(f"ecosystem package PASS: {arguments.archive}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
