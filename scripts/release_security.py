#!/usr/bin/env python3
"""Run release commands with an allowlist environment and scan artifacts safely."""

# pattern: Imperative Shell

from __future__ import annotations

import argparse
import os
import pathlib
import re
import subprocess
import sys
import zipfile
from collections.abc import Iterable, Mapping, Sequence

from release_security_core import (
    Finding,
    build_release_environment,
    format_finding,
    scan_text,
)


DEFAULT_ALLOWED_ENVIRONMENT = frozenset(
    {
        "ALLUSERSPROFILE",
        "AR",
        "CC",
        "CHERE_INVOKING",
        "CMAKE_GENERATOR",
        "CMAKE_PREFIX_PATH",
        "COMSPEC",
        "CommonProgramFiles",
        "CommonProgramFiles(x86)",
        "CXX",
        "INCLUDE",
        "LIB",
        "LIBPATH",
        "MINGW_CHOST",
        "MINGW_PACKAGE_PREFIX",
        "MINGW_PREFIX",
        "MSYSTEM",
        "NUMBER_OF_PROCESSORS",
        "OPENJOC_RELEASE_BUILD",
        "OS",
        "PATH",
        "PATHEXT",
        "PKG_CONFIG_PATH",
        "PROCESSOR_ARCHITECTURE",
        "PROCESSOR_IDENTIFIER",
        "ProgramData",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "RANLIB",
        "SHELL",
        "SystemDrive",
        "SystemRoot",
        "TEMP",
        "TMP",
        "VCINSTALLDIR",
        "VCToolsInstallDir",
        "VCToolsRedistDir",
        "VSINSTALLDIR",
        "WindowsSdkDir",
        "WindowsSDKVersion",
        "WINDIR",
    }
)
_ASCII_STRINGS = re.compile(rb"[\x20-\x7e]{6,}")
_UTF16_LE_STRINGS = re.compile(rb"(?:[\x20-\x7e]\x00){6,}")


def _scan_bytes(
    data: bytes,
    *,
    subject: str,
    private_path_markers: Iterable[str],
) -> tuple[Finding, ...]:
    text_parts: list[str] = []
    if b"\x00" not in data:
        text_parts.append(data.decode("utf-8", errors="ignore"))
    else:
        text_parts.extend(
            match.group().decode("ascii", errors="ignore")
            for match in _ASCII_STRINGS.finditer(data)
        )
        text_parts.extend(
            match.group().decode("utf-16-le", errors="ignore")
            for match in _UTF16_LE_STRINGS.finditer(data)
        )
    findings = scan_text(
        "\n".join(text_parts),
        subject=subject,
        private_path_markers=private_path_markers,
    )
    return tuple(dict.fromkeys(findings))


def _scan_file(
    path: pathlib.Path, *, private_path_markers: Iterable[str]
) -> tuple[Finding, ...]:
    if path.suffix.casefold() == ".zip":
        findings: list[Finding] = []
        try:
            with zipfile.ZipFile(path) as archive:
                for entry in archive.infolist():
                    if entry.is_dir():
                        continue
                    findings.extend(
                        _scan_bytes(
                            archive.read(entry),
                            subject=f"{path}!{entry.filename}",
                            private_path_markers=private_path_markers,
                        )
                    )
        except (OSError, zipfile.BadZipFile):
            return (
                Finding(
                    category="scan_error",
                    subject=str(path),
                    line_number=0,
                    indicator="invalid_or_unreadable_zip",
                ),
            )
        return tuple(dict.fromkeys(findings))
    return _scan_bytes(
        path.read_bytes(),
        subject=str(path),
        private_path_markers=private_path_markers,
    )


def scan_paths(
    paths: Iterable[pathlib.Path], *, private_path_markers: Iterable[str]
) -> tuple[Finding, ...]:
    """Scan files/directories while retaining only non-sensitive finding metadata."""

    markers = tuple(private_path_markers)
    findings: list[Finding] = []
    for path in paths:
        resolved = path.expanduser().resolve()
        if not resolved.exists():
            raise FileNotFoundError(f"scan input does not exist: {resolved}")
        files = (
            sorted(item for item in resolved.rglob("*") if item.is_file())
            if resolved.is_dir()
            else (resolved,)
        )
        for file_path in files:
            findings.extend(_scan_file(file_path, private_path_markers=markers))
    return tuple(dict.fromkeys(findings))


def run_sanitized_command(
    arguments: Sequence[str],
    *,
    cwd: pathlib.Path,
    allowed_names: Iterable[str] = DEFAULT_ALLOWED_ENVIRONMENT,
    overrides: Mapping[str, str],
    capture_output: bool = False,
) -> subprocess.CompletedProcess[str]:
    """Run one command with a process-local allowlist environment."""

    if not arguments:
        raise ValueError("release command cannot be empty")
    environment = build_release_environment(
        os.environ,
        allowed_names=allowed_names,
        overrides=overrides,
    )
    return subprocess.run(
        list(arguments),
        cwd=cwd,
        env=environment,
        check=True,
        text=True,
        stdout=subprocess.PIPE if capture_output else None,
        stderr=subprocess.PIPE if capture_output else None,
    )


def _parse_overrides(values: Iterable[str]) -> dict[str, str]:
    overrides: dict[str, str] = {}
    for item in values:
        name, separator, value = item.partition("=")
        if not separator:
            raise ValueError("release environment override must use NAME=VALUE")
        overrides[name] = value
    return overrides


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="operation", required=True)

    scan_parser = subparsers.add_parser("scan", help="scan files or ZIP archives")
    scan_parser.add_argument(
        "--private-path-marker", action="append", default=[], help=argparse.SUPPRESS
    )
    scan_parser.add_argument("paths", nargs="+", type=pathlib.Path)

    run_parser = subparsers.add_parser("run", help="run with an allowlist environment")
    run_parser.add_argument("--allow", action="append", default=[])
    run_parser.add_argument("--set", action="append", default=[])
    run_parser.add_argument("command", nargs=argparse.REMAINDER)

    arguments = parser.parse_args()
    if arguments.operation == "scan":
        findings = scan_paths(
            arguments.paths,
            private_path_markers=arguments.private_path_marker,
        )
        for finding in findings:
            print(format_finding(finding))
        if findings:
            print(f"release security scan failed: findings={len(findings)}")
            return 1
        print("release security scan passed: findings=0")
        return 0

    command = list(arguments.command)
    if command and command[0] == "--":
        command.pop(0)
    allowed = set(DEFAULT_ALLOWED_ENVIRONMENT)
    allowed.update(arguments.allow)
    try:
        overrides = _parse_overrides(arguments.set)
        run_sanitized_command(
            command,
            cwd=pathlib.Path.cwd(),
            allowed_names=allowed,
            overrides=overrides,
        )
    except (ValueError, subprocess.CalledProcessError) as error:
        print(f"release command failed: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
