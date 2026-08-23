#!/usr/bin/env python3
# pattern: Imperative Shell
"""Package a checked-out OpenJOC platform build for release aggregation."""

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
TARGETS = {
    "x86_64-pc-windows-msvc": ("zip", "openjoc.exe"),
    "x86_64-unknown-linux-gnu": ("tar.gz", "openjoc"),
}
C_API_ARTIFACTS = {
    "x86_64-pc-windows-msvc": (
        "openjoc_capi.dll",
        "openjoc_capi.dll.lib",
        "openjoc_capi.lib",
    ),
    "x86_64-unknown-linux-gnu": (
        "libopenjoc_capi.a",
        "libopenjoc_capi.so",
    ),
}
PUBLIC_PATHS = (
    "LICENSE",
    "README.md",
    "docs/ADM_EXPORT.md",
    "docs/ARCHITECTURE.md",
    "docs/CAPABILITIES.md",
    "docs/CUSTOM_SPEAKER_LAYOUTS.md",
    "docs/KNOWN_LIMITATIONS.md",
    "docs/JOC_RENDER.md",
    "docs/LIBRARY_API.md",
    "docs/C_API.md",
    "docs/README.md",
    "docs/PUBLIC_SMOKE_FIXTURE.md",
    "docs/integration/ECOSYSTEM_PACKAGING.md",
)


def run(arguments: list[str], *, cwd: pathlib.Path = REPOSITORY) -> str:
    completed = subprocess.run(
        arguments,
        cwd=cwd,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    )
    return completed.stdout.strip()


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def write_json(path: pathlib.Path, value: object) -> None:
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )


def record(path: pathlib.Path, root: pathlib.Path) -> dict[str, object]:
    return {
        "path": path.relative_to(root).as_posix(),
        "sha256": sha256(path),
        "size": path.stat().st_size,
    }


def add_tar_entry(
    archive: tarfile.TarFile, source: pathlib.Path, archive_name: str
) -> None:
    info = archive.gettarinfo(str(source), archive_name)
    info.uid = 0
    info.gid = 0
    info.uname = "root"
    info.gname = "wheel"
    info.mtime = 0
    if source.is_dir():
        info.mode = 0o755
        archive.addfile(info)
        return
    info.mode = 0o755 if os.access(source, os.X_OK) else 0o644
    with source.open("rb") as stream:
        archive.addfile(info, stream)


def write_tar_gz(source_root: pathlib.Path, output: pathlib.Path) -> None:
    with output.open("wb") as raw:
        with gzip.GzipFile(
            filename="", mode="wb", fileobj=raw, mtime=0, compresslevel=9
        ) as compressed:
            with tarfile.open(
                fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT
            ) as archive:
                add_tar_entry(archive, source_root, source_root.name)
                directories = sorted(
                    path for path in source_root.rglob("*") if path.is_dir()
                )
                files = sorted(
                    path for path in source_root.rglob("*") if path.is_file()
                )
                for path in directories + files:
                    archive_name = (
                        f"{source_root.name}/"
                        f"{path.relative_to(source_root).as_posix()}"
                    )
                    add_tar_entry(archive, path, archive_name)


def write_zip(source_root: pathlib.Path, output: pathlib.Path) -> None:
    with zipfile.ZipFile(
        output, mode="w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
    ) as archive:
        for path in sorted(path for path in source_root.rglob("*") if path.is_file()):
            relative = f"{source_root.name}/{path.relative_to(source_root).as_posix()}"
            info = zipfile.ZipInfo(relative, date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            mode = 0o755 if os.access(path, os.X_OK) else 0o644
            info.external_attr = mode << 16
            archive.writestr(info, path.read_bytes())


def current_host() -> str:
    for line in run(["rustc", "-vV"]).splitlines():
        if line.startswith("host: "):
            return line.removeprefix("host: ")
    raise SystemExit("rustc -vV did not report a host target")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True, choices=sorted(TARGETS))
    parser.add_argument(
        "--output", required=True, type=pathlib.Path, help="new or empty output directory"
    )
    parser.add_argument(
        "--binary",
        type=pathlib.Path,
        help="release executable to package; defaults to target/release",
    )
    arguments = parser.parse_args()
    target = arguments.target
    archive_kind, executable_name = TARGETS[target]
    output = arguments.output.expanduser().resolve()
    if output == REPOSITORY or REPOSITORY in output.parents:
        parser.error("output directory must be outside the source repository")
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        parser.error(f"output directory must be empty: {output}")

    host = current_host()
    if host != target:
        raise SystemExit(f"release target not admitted: expected {target}, got {host}")

    metadata = json.loads(
        run(
            [
                "cargo",
                "metadata",
                "--format-version",
                "1",
                "--locked",
                "--no-deps",
            ]
        )
    )
    cli = next(
        package for package in metadata["packages"] if package["name"] == "openjoc-cli"
    )
    version = cli["version"]
    base_name = f"openjoc-{version}-{target}"
    archive_name = f"{base_name}.{archive_kind}"
    manifest_name = f"{base_name}.manifest.json"
    executable = (
        arguments.binary.expanduser().resolve()
        if arguments.binary is not None
        else REPOSITORY / "target" / "release" / executable_name
    )
    if not executable.is_file() or executable.is_symlink():
        raise SystemExit(f"release executable is missing or invalid: {executable}")

    commit = run(["git", "rev-parse", "HEAD"])
    rustc_version = run(["rustc", "--version"])
    cargo_version = run(["cargo", "--version"])
    with tempfile.TemporaryDirectory(prefix="openjoc-platform-release-") as temporary:
        source_root = pathlib.Path(temporary) / base_name
        (source_root / "bin").mkdir(parents=True)
        (source_root / "include").mkdir(parents=True)
        (source_root / "lib").mkdir(parents=True)
        for relative in PUBLIC_PATHS:
            source = REPOSITORY / relative
            if not source.is_file():
                raise SystemExit(f"required release file is missing: {relative}")
            destination = source_root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)
        shutil.copy2(
            REPOSITORY / "crates/openjoc-capi/include/openjoc.h",
            source_root / "include/openjoc.h",
        )
        for library in C_API_ARTIFACTS[target]:
            source = REPOSITORY / "target" / "release" / library
            if not source.is_file() or source.is_symlink():
                raise SystemExit(f"required C ABI artifact is missing or invalid: {source}")
            shutil.copy2(source, source_root / "lib" / library)
        shutil.copy2(executable, source_root / "bin" / executable_name)
        (source_root / "bin" / executable_name).chmod(0o755)

        archive_path = output / archive_name
        if archive_kind == "zip":
            write_zip(source_root, archive_path)
        else:
            write_tar_gz(source_root, archive_path)

        manifest = {
            "artifact_filename": archive_name,
            "artifact_sha256": sha256(archive_path),
            "artifact_size": archive_path.stat().st_size,
            "archive_format": archive_kind,
            "build_source": {
                "cargo_version": cargo_version,
                "declared_commit": commit,
                "rustc_version": rustc_version,
            },
            "files": [
                record(path, source_root)
                for path in sorted(path for path in source_root.rglob("*") if path.is_file())
            ],
            "schema": "openjoc.platform-release-manifest.v1",
            "target": target,
            "version": version,
        }
        write_json(output / manifest_name, manifest)

    print(
        json.dumps(
            {
                "artifact": archive_name,
                "manifest": manifest_name,
                "sha256": manifest["artifact_sha256"],
                "target": target,
                "version": version,
            },
            indent=2,
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
