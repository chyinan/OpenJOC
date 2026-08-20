#!/usr/bin/env python3
"""Verify platform release artifacts and create the public asset set."""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import shutil


TARGET_ORDER = (
    ("aarch64-apple-darwin", "tar.gz"),
    ("x86_64-pc-windows-msvc", "zip"),
    ("x86_64-unknown-linux-gnu", "tar.gz"),
)
PLAYER_TARGET_ORDER = (
    ("macos-arm64", "tar.gz"),
    ("linux-x86_64", "tar.gz"),
    ("windows-x64", "zip"),
)
ECOSYSTEM_PLATFORM_ORDER = (
    ("macos-arm64", "tar.gz"),
    ("linux-x86_64", "tar.gz"),
    ("windows-x64", "zip"),
)
ECOSYSTEM_PACKAGE_KINDS = ("sdk", "ffmpeg", "gstreamer-plugin")


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def find_unique(root: pathlib.Path, name: str, *, kind: str) -> pathlib.Path:
    matches = sorted(path for path in root.rglob(name) if path.is_file())
    if len(matches) != 1:
        raise SystemExit(
            f"expected exactly one {kind} named {name}, found {len(matches)}: {matches}"
        )
    return matches[0]


def manifest_archive_record(
    manifest: dict[str, object], archive_name: str
) -> tuple[str, int]:
    direct_name = manifest.get("artifact_filename")
    direct_hash = manifest.get("artifact_sha256")
    direct_size = manifest.get("artifact_size")
    if direct_name is not None or direct_hash is not None or direct_size is not None:
        if not isinstance(direct_name, str):
            raise SystemExit("manifest artifact_filename is missing or invalid")
        if not isinstance(direct_hash, str):
            raise SystemExit("manifest artifact_sha256 is missing or invalid")
        if not isinstance(direct_size, int):
            raise SystemExit("manifest artifact_size is missing or invalid")
        if direct_name != archive_name:
            raise SystemExit(
                f"manifest artifact filename mismatch: {direct_name!r} != {archive_name!r}"
            )
        return direct_hash, direct_size

    artifacts = manifest.get("artifacts")
    if not isinstance(artifacts, list):
        raise SystemExit("manifest has no supported archive record")
    records = [
        item
        for item in artifacts
        if isinstance(item, dict) and item.get("filename") == archive_name
    ]
    if len(records) != 1:
        raise SystemExit(
            f"manifest must contain exactly one record for {archive_name}, found {len(records)}"
        )
    record = records[0]
    digest = record.get("sha256")
    size = record.get("size")
    if not isinstance(digest, str) or not isinstance(size, int):
        raise SystemExit(f"manifest archive record is incomplete for {archive_name}")
    return digest, size


def validate_manifest(
    manifest_path: pathlib.Path,
    archive_path: pathlib.Path,
    *,
    version: str,
    target: str,
) -> None:
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"cannot read manifest {manifest_path}: {error}") from error
    if not isinstance(manifest, dict):
        raise SystemExit(f"manifest is not an object: {manifest_path}")
    if manifest.get("target") != target:
        raise SystemExit(
            f"manifest target mismatch for {archive_path.name}: "
            f"{manifest.get('target')!r} != {target!r}"
        )
    if manifest.get("version") != version:
        raise SystemExit(
            f"manifest version mismatch for {archive_path.name}: "
            f"{manifest.get('version')!r} != {version!r}"
        )
    expected_hash, expected_size = manifest_archive_record(manifest, archive_path.name)
    actual_hash = sha256(archive_path)
    actual_size = archive_path.stat().st_size
    if expected_hash.lower() != actual_hash:
        raise SystemExit(
            f"manifest SHA-256 mismatch for {archive_path.name}: "
            f"{expected_hash} != {actual_hash}"
        )
    if expected_size != actual_size:
        raise SystemExit(
            f"manifest size mismatch for {archive_path.name}: "
            f"{expected_size} != {actual_size}"
        )


def validate_public_checksums(
    checksum_path: pathlib.Path, archives: list[pathlib.Path]
) -> None:
    lines = checksum_path.read_text(encoding="utf-8").splitlines()
    expected_names = [archive.name for archive in archives]
    if len(lines) != len(expected_names):
        raise SystemExit(
            f"SHA256SUMS must contain {len(expected_names)} entries, found {len(lines)}"
        )
    seen: set[str] = set()
    for line, archive in zip(lines, archives):
        fields = line.split("  ")
        if len(fields) != 2:
            raise SystemExit(f"invalid SHA256SUMS line: {line!r}")
        digest, filename = fields
        if len(digest) != 64 or any(character not in "0123456789abcdef" for character in digest):
            raise SystemExit(f"invalid SHA-256 in SHA256SUMS: {digest!r}")
        if (
            pathlib.PurePosixPath(filename).name != filename
            or "\\" in filename
            or filename.startswith("/")
        ):
            raise SystemExit(f"SHA256SUMS filename must be a basename: {filename!r}")
        if filename in seen:
            raise SystemExit(f"duplicate SHA256SUMS filename: {filename}")
        seen.add(filename)
        if filename != archive.name or digest != sha256(archive):
            raise SystemExit(f"SHA256SUMS does not match archive {archive.name}")
    if seen != set(expected_names):
        raise SystemExit("SHA256SUMS archive set does not match the public archive set")


def validate_player_manifest(
    manifest_path: pathlib.Path,
    archive_path: pathlib.Path,
    *,
    version: str,
    target: str,
) -> None:
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"cannot read player manifest {manifest_path}: {error}") from error
    if not isinstance(manifest, dict):
        raise SystemExit(f"player manifest is not an object: {manifest_path}")
    expected = {
        "archive": archive_path.name,
        "target": target,
        "version": version,
        "release_candidate": True,
    }
    for key, value in expected.items():
        if manifest.get(key) != value:
            raise SystemExit(
                f"player manifest {manifest_path.name} has {key}={manifest.get(key)!r}; "
                f"expected {value!r}"
            )
    if manifest.get("archive_sha256") != sha256(archive_path):
        raise SystemExit(f"player manifest SHA-256 mismatch for {archive_path.name}")
    if manifest.get("archive_size") != archive_path.stat().st_size:
        raise SystemExit(f"player manifest size mismatch for {archive_path.name}")


def validate_ecosystem_manifest(
    manifest_path: pathlib.Path,
    archive_path: pathlib.Path,
    *,
    version: str,
    kind: str,
    platform: str,
) -> None:
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"cannot read ecosystem manifest {manifest_path}: {error}") from error
    expected = {
        "package_kind": kind,
        "version": version,
        "platform": platform,
        "archive": archive_path.name,
        "unresolved_license_components": 0,
        "private_media_leak": False,
        "private_path_leak": False,
        "credential_leak": False,
    }
    for key, value in expected.items():
        if manifest.get(key) != value:
            raise SystemExit(
                f"ecosystem manifest {manifest_path.name} has {key}={manifest.get(key)!r}; expected {value!r}"
            )
    if manifest.get("archive_sha256") != sha256(archive_path):
        raise SystemExit(f"ecosystem manifest SHA-256 mismatch for {archive_path.name}")
    if manifest.get("archive_size") != archive_path.stat().st_size:
        raise SystemExit(f"ecosystem manifest size mismatch for {archive_path.name}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True, type=pathlib.Path)
    parser.add_argument(
        "--player-input",
        type=pathlib.Path,
        help="optional verified player-package artifact directory",
    )
    parser.add_argument(
        "--ecosystem-input",
        type=pathlib.Path,
        help="optional verified SDK/FFmpeg/GStreamer package artifact directory",
    )
    parser.add_argument("--output", required=True, type=pathlib.Path)
    parser.add_argument("--version", required=True)
    arguments = parser.parse_args()
    input_root = arguments.input.expanduser().resolve()
    output_root = arguments.output.expanduser().resolve()
    if not input_root.is_dir():
        parser.error(f"input directory does not exist: {input_root}")
    player_input = (
        arguments.player_input.expanduser().resolve()
        if arguments.player_input is not None
        else None
    )
    if player_input is not None and not player_input.is_dir():
        parser.error(f"player input directory does not exist: {player_input}")
    ecosystem_input = (
        arguments.ecosystem_input.expanduser().resolve()
        if arguments.ecosystem_input is not None
        else None
    )
    if ecosystem_input is not None and not ecosystem_input.is_dir():
        parser.error(f"ecosystem input directory does not exist: {ecosystem_input}")
    output_root.mkdir(parents=True, exist_ok=True)
    if any(output_root.iterdir()):
        parser.error(f"output directory must be empty: {output_root}")

    archives: list[pathlib.Path] = []
    for target, extension in TARGET_ORDER:
        archive_name = f"openjoc-{arguments.version}-{target}.{extension}"
        manifest_name = f"openjoc-{arguments.version}-{target}.manifest.json"
        archive = find_unique(input_root, archive_name, kind="archive")
        manifest = find_unique(input_root, manifest_name, kind="manifest")
        validate_manifest(
            manifest,
            archive,
            version=arguments.version,
            target=target,
        )
        destination = output_root / archive.name
        shutil.copy2(archive, destination)
        archives.append(destination)

    if player_input is not None:
        for target, extension in PLAYER_TARGET_ORDER:
            archive_name = f"openjoc-mpv-{arguments.version}-{target}.{extension}"
            manifest_name = f"openjoc-mpv-{arguments.version}-{target}.manifest.json"
            checksum_name = f"openjoc-mpv-{arguments.version}-{target}.SHA256SUMS"
            archive = find_unique(player_input, archive_name, kind="player archive")
            manifest = find_unique(player_input, manifest_name, kind="player manifest")
            checksum = find_unique(player_input, checksum_name, kind="player checksum manifest")
            validate_player_manifest(
                manifest,
                archive,
                version=arguments.version,
                target=target,
            )
            checksum_lines = checksum.read_text(encoding="utf-8").splitlines()
            if len(checksum_lines) != 2 or checksum_lines[0] != f"{sha256(archive)}  {archive.name}" or checksum_lines[1] != f"{sha256(manifest)}  {manifest.name}":
                raise SystemExit(f"player checksum manifest does not match {archive.name}")
            destination = output_root / archive.name
            shutil.copy2(archive, destination)
            archives.append(destination)

    if ecosystem_input is not None:
        for kind in ECOSYSTEM_PACKAGE_KINDS:
            for platform, extension in ECOSYSTEM_PLATFORM_ORDER:
                archive_name = f"openjoc-{kind}-{arguments.version}-{platform}.{extension}"
                manifest_name = f"openjoc-{kind}-{arguments.version}-{platform}.manifest.json"
                archive = find_unique(ecosystem_input, archive_name, kind="ecosystem archive")
                manifest = find_unique(ecosystem_input, manifest_name, kind="ecosystem manifest")
                validate_ecosystem_manifest(
                    manifest,
                    archive,
                    version=arguments.version,
                    kind=kind,
                    platform=platform,
                )
                destination = output_root / archive.name
                shutil.copy2(archive, destination)
                archives.append(destination)

        ecosystem_archives = sorted(
            path
            for path in ecosystem_input.rglob("*")
            if path.is_file() and (path.name.endswith(".tar.gz") or path.name.endswith(".zip"))
        )
        expected_ecosystem = {
            f"openjoc-{kind}-{arguments.version}-{platform}.{extension}"
            for kind in ECOSYSTEM_PACKAGE_KINDS
            for platform, extension in ECOSYSTEM_PLATFORM_ORDER
        }
        if {path.name for path in ecosystem_archives} != expected_ecosystem:
            raise SystemExit(
                "ecosystem artifact set does not exactly match the nine canonical package archives"
            )

    all_archives = sorted(
        path
        for path in input_root.rglob("*")
        if path.is_file() and (path.name.endswith(".tar.gz") or path.name.endswith(".zip"))
    )
    expected_archive_names = {path.name for path in archives}
    unexpected = [
        path.name for path in all_archives if path.name not in expected_archive_names
    ]
    if unexpected:
        raise SystemExit(f"unexpected archive artifacts were downloaded: {unexpected}")

    checksum_path = output_root / "SHA256SUMS"
    checksum_path.write_text(
        "".join(f"{sha256(archive)}  {archive.name}\n" for archive in archives),
        encoding="utf-8",
    )
    validate_public_checksums(checksum_path, archives)

    public_names = [archive.name for archive in archives] + [checksum_path.name]
    print(json.dumps({"public_assets": public_names, "count": len(public_names)}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
