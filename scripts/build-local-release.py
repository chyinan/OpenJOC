#!/usr/bin/env python3
"""Build a deterministic local OpenJOC macOS-arm64 candidate."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tarfile
import tempfile


REPOSITORY = pathlib.Path(__file__).resolve().parent.parent
TARGET = "aarch64-apple-darwin"
PAYLOAD_PATHS = (
    "LICENSE",
    "README.md",
    "bin/openjoc",
    "verify.sh",
)
STATIC_BUNDLE_PATHS = (
    "LICENSE",
    "README.md",
    "RELEASE_MANIFEST.json",
    "SHA256SUMS",
    "bin/openjoc",
    "verify.sh",
)


def run(
    arguments: list[str],
    *,
    cwd: pathlib.Path = REPOSITORY,
    env: dict[str, str] | None = None,
    capture: bool = True,
) -> str:
    completed = subprocess.run(
        arguments,
        cwd=cwd,
        env=env,
        check=True,
        text=True,
        stdout=subprocess.PIPE if capture else None,
    )
    return completed.stdout.strip() if completed.stdout is not None else ""


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


def add_deterministic(
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
    else:
        info.mode = 0o755 if os.access(source, os.X_OK) else 0o644
        with source.open("rb") as stream:
            archive.addfile(info, stream)


def deterministic_bundle(source_root: pathlib.Path, output: pathlib.Path) -> None:
    with output.open("wb") as raw:
        with gzip.GzipFile(
            filename="", mode="wb", fileobj=raw, mtime=0, compresslevel=9
        ) as compressed:
            with tarfile.open(
                fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT
            ) as archive:
                add_deterministic(archive, source_root, source_root.name)
                directories = sorted(
                    path for path in source_root.rglob("*") if path.is_dir()
                )
                files = sorted(path for path in source_root.rglob("*") if path.is_file())
                for path in directories + files:
                    name = (
                        f"{source_root.name}/"
                        f"{path.relative_to(source_root).as_posix()}"
                    )
                    add_deterministic(archive, path, name)


def deterministic_gzip(source_tar: pathlib.Path, output: pathlib.Path) -> None:
    with source_tar.open("rb") as source, output.open("wb") as raw:
        with gzip.GzipFile(
            filename="", mode="wb", fileobj=raw, mtime=0, compresslevel=9
        ) as compressed:
            shutil.copyfileobj(source, compressed)


def require_clean_committed_source() -> str:
    run(["git", "diff", "--quiet"], capture=False)
    run(["git", "diff", "--cached", "--quiet"], capture=False)
    return run(["git", "rev-parse", "HEAD"])


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output", required=True, type=pathlib.Path, help="new or empty output directory"
    )
    arguments = parser.parse_args()
    output = arguments.output.expanduser().resolve()
    if output == REPOSITORY or REPOSITORY in output.parents:
        parser.error("output directory must be outside the source repository")
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        parser.error(f"output directory must be empty: {output}")

    commit = require_clean_committed_source()
    rustc = run(["rustc", "-vV"])
    cargo = run(["cargo", "-Vv"])
    host_line = next(line for line in rustc.splitlines() if line.startswith("host: "))
    host = host_line.removeprefix("host: ")
    if host != TARGET:
        raise SystemExit(f"release target not admitted: expected {TARGET}, got {host}")

    metadata = json.loads(
        run(
            [
                "cargo",
                "metadata",
                "--format-version",
                "1",
                "--locked",
                "--offline",
                "--no-deps",
            ]
        )
    )
    cli = next(
        package for package in metadata["packages"] if package["name"] == "openjoc-cli"
    )
    version = cli["version"]
    base_name = f"openjoc-{version}-{TARGET}"
    bundle_name = f"{base_name}.tar.gz"
    source_name = f"openjoc-{version}-source-{commit[:12]}.tar.gz"
    manifest_name = f"{base_name}.manifest.json"
    checksums_name = f"{base_name}.SHA256SUMS"

    with tempfile.TemporaryDirectory(prefix="openjoc-local-release-") as temporary:
        temporary_root = pathlib.Path(temporary)
        source_tar = temporary_root / "source.tar"
        with source_tar.open("wb") as stream:
            subprocess.run(
                [
                    "git",
                    "archive",
                    "--format=tar",
                    "HEAD",
                    "--",
                    ".",
                    ":(exclude)docs/PROVENANCE.md",
                    ":(exclude)docs/REQUIREMENTS_MATRIX.md",
                    ":(exclude)docs/research",
                ],
                cwd=REPOSITORY,
                check=True,
                stdout=stream,
            )
        source = temporary_root / "source"
        source.mkdir()
        with tarfile.open(source_tar, "r:") as archive:
            archive.extractall(source, filter="data")

        target = temporary_root / "target"
        environment = os.environ.copy()
        cargo_home = pathlib.Path(
            environment.get("CARGO_HOME", pathlib.Path.home() / ".cargo")
        ).expanduser().resolve()
        remap = f"--remap-path-prefix={cargo_home}=/cargo"
        existing_rustflags = environment.get("RUSTFLAGS", "").strip()
        environment.update(
            {
                "CARGO_TARGET_DIR": str(target),
                "CARGO_BUILD_JOBS": "1",
                "RUSTFLAGS": f"{existing_rustflags} {remap}".strip(),
            }
        )
        run(
            [
                "cargo",
                "build",
                "-p",
                "openjoc-cli",
                "--release",
                "--locked",
                "--offline",
            ],
            cwd=source,
            env=environment,
            capture=False,
        )

        bundle_root = temporary_root / base_name
        (bundle_root / "bin").mkdir(parents=True)
        shutil.copy2(target / "release/openjoc", bundle_root / "bin/openjoc")
        shutil.copy2(source / "LICENSE", bundle_root / "LICENSE")
        shutil.copy2(source / "README.md", bundle_root / "README.md")
        # Ship the canonical documentation tree so the standalone release
        # README keeps its source-relative links intact.  This is a copy of
        # public repository documentation only; private evidence and media
        # remain outside both the source archive and the binary bundle.
        shutil.copytree(source / "docs", bundle_root / "docs")
        shutil.copy2(
            source / "scripts/verify-release-bundle.sh", bundle_root / "verify.sh"
        )
        (bundle_root / "bin/openjoc").chmod(0o755)
        (bundle_root / "verify.sh").chmod(0o755)
        binary_bytes = (bundle_root / "bin/openjoc").read_bytes()
        for forbidden in (b"/Users/", b"OpenJOC-" + b"Private"):
            if forbidden in binary_bytes:
                raise SystemExit(
                    "release binary contains a forbidden developer path marker: "
                    f"{forbidden!r}"
                )
        signature_probe = subprocess.run(
            ["codesign", "-dv", "--verbose=4", bundle_root / "bin/openjoc"],
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        if "Signature=adhoc" not in signature_probe.stderr:
            raise SystemExit("release binary does not have the expected linker ad-hoc signature")

        lock_hash = sha256(source / "Cargo.lock")
        doc_paths = tuple(
            sorted(
                path.relative_to(bundle_root).as_posix()
                for path in (bundle_root / "docs").rglob("*")
                if path.is_file()
            )
        )
        payload_paths = PAYLOAD_PATHS + doc_paths
        payload_records = [
            record(bundle_root / relative, bundle_root) for relative in payload_paths
        ]
        inner_manifest = {
            "artifact_filename": bundle_name,
            "artifact_sha256_surface": (
                f"adjacent {checksums_name} and outer {manifest_name}"
            ),
            "build_claim": (
                "same committed source/toolchain/host; no Developer-ID identity"
            ),
            "capability_contract": "docs/CAPABILITIES.md",
            "cargo_lock_sha256": lock_hash,
            "cargo_version": cargo.splitlines()[0],
            "declared_source_commit": commit,
            "debug_path_remap": "active Cargo home -> /cargo",
            "files": payload_records,
            "known_limitations": "docs/KNOWN_LIMITATIONS.md",
            "notarized": False,
            "signing": {
                "developer_identity_signed": False,
                "linker_adhoc_signed": True,
            },
            "runtime_dependencies": {
                "ffmpeg": "capture/demux and compatible-base paths",
                "ffprobe": "supported ISO BMFF and compatible-base inspection paths",
                "raw_ec3_internal_decode": "in-process",
            },
            "rustc_version": rustc.splitlines()[0],
            "schema": "openjoc.bundle-manifest.v1",
            "target": TARGET,
            "version": version,
        }
        write_json(bundle_root / "RELEASE_MANIFEST.json", inner_manifest)
        checksum_lines = [
            f'{item["sha256"]}  {item["path"]}' for item in payload_records
        ]
        (bundle_root / "SHA256SUMS").write_text(
            "\n".join(checksum_lines) + "\n", encoding="utf-8"
        )

        actual_paths = tuple(
            sorted(
                path.relative_to(bundle_root).as_posix()
                for path in bundle_root.rglob("*")
                if path.is_file()
            )
        )
        expected_paths = tuple(sorted(STATIC_BUNDLE_PATHS + doc_paths))
        if actual_paths != expected_paths:
            raise SystemExit(f"bundle inventory mismatch: {actual_paths!r}")

        bundle_path = output / bundle_name
        deterministic_bundle(bundle_root, bundle_path)
        source_path = output / source_name
        deterministic_gzip(source_tar, source_path)

        all_bundle_records = [
            record(bundle_root / relative, bundle_root) for relative in actual_paths
        ]
        outer_manifest = {
            "artifacts": [
                {
                    "filename": bundle_name,
                    "role": "binary_bundle",
                    "sha256": sha256(bundle_path),
                    "size": bundle_path.stat().st_size,
                },
                {
                    "filename": source_name,
                    "role": "clean_source_archive",
                    "sha256": sha256(source_path),
                    "size": source_path.stat().st_size,
                },
            ],
            "binary": record(bundle_root / "bin/openjoc", bundle_root),
            "bundle_files": all_bundle_records,
            "cargo_lock_sha256": lock_hash,
            "cargo_version": cargo.splitlines()[0],
            "declared_source_commit": commit,
            "debug_path_remap": "active Cargo home -> /cargo",
            "notarized": False,
            "publication_status": "UNPUBLISHED_LOCAL_RELEASE_CANDIDATE",
            "rustc_version": rustc.splitlines()[0],
            "schema": "openjoc.release-manifest.v1",
            "signing": {
                "developer_identity_signed": False,
                "linker_adhoc_signed": True,
            },
            "target": TARGET,
            "verification": {
                "archive_checksums": checksums_name,
                "bundle_command": "extract bundle, cd into its root, run ./verify.sh",
                "network_required": False,
                "source_repository_required": False,
            },
            "version": version,
        }
        outer_manifest_path = output / manifest_name
        write_json(outer_manifest_path, outer_manifest)
        outer_checksums = [
            f"{sha256(bundle_path)}  {bundle_name}",
            f"{sha256(source_path)}  {source_name}",
            f"{sha256(outer_manifest_path)}  {manifest_name}",
        ]
        (output / checksums_name).write_text(
            "\n".join(outer_checksums) + "\n", encoding="utf-8"
        )

    result = {
        "binary_sha256": outer_manifest["binary"]["sha256"],
        "bundle": bundle_name,
        "bundle_sha256": outer_manifest["artifacts"][0]["sha256"],
        "checksums": checksums_name,
        "declared_source_commit": commit,
        "manifest": manifest_name,
        "output": str(output),
        "source_archive": source_name,
        "source_archive_sha256": outer_manifest["artifacts"][1]["sha256"],
        "target": TARGET,
        "version": version,
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
