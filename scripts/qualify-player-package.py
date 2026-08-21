#!/usr/bin/env python3
"""Run the extracted-player qualification contract and write a report."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tarfile
import tempfile
import zipfile


REPOSITORY = pathlib.Path(__file__).resolve().parent.parent
PACKAGE_VERIFIER = REPOSITORY / "scripts/player-package.py"
PLAYER_HARNESS = REPOSITORY / "integrations/mpv/verify-player.sh"
HARNESS_FIELDS = (
    "JOC", "RAW_SINGLE_AU_JOC", "RAW_MULTI_AU_JOC", "MP4_JOC",
    "FIRST_AU_INTEGRITY", "EXPLICIT_OVERRIDE", "PASSTHROUGH",
    "ORDINARY_EAC3", "BINAURAL", "2_0", "5_1", "7_1_4", "9_1_6",
    "22_2", "EOS",
)
FIELDS = [
    "BUILD", "PACKAGE", "DEPENDENCIES", "LICENSE", "RUNTIME",
    "DECODER_SELECTION", "GUI_EXECUTABLE", "CONSOLE_ENTRYPOINT", "CONSOLE_INTERRUPT",
    *HARNESS_FIELDS, "PRIVATE_PATH_SCAN",
]


def digest(path: pathlib.Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def safe_extract(archive: pathlib.Path, destination: pathlib.Path) -> pathlib.Path:
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive) as package:
            members = package.infolist()
            names = [pathlib.PurePosixPath(member.filename) for member in members]
            if not names or any(name.is_absolute() or ".." in name.parts for name in names):
                raise SystemExit("qualification: archive contains an unsafe path")
            package.extractall(destination)
    else:
        with tarfile.open(archive, "r:gz") as package:
            members = package.getmembers()
            names = [pathlib.PurePosixPath(member.name) for member in members]
            if not names or any(name.is_absolute() or ".." in name.parts for name in names):
                raise SystemExit("qualification: archive contains an unsafe path")
            package.extractall(destination)
    roots = {name.parts[0] for name in names if name.parts}
    if len(roots) != 1:
        raise SystemExit("qualification: archive must contain exactly one package root")
    root = destination / next(iter(roots))
    if not root.is_dir():
        raise SystemExit("qualification: archive root is missing after extraction")
    return root


def clean_output(value: str, temporary: pathlib.Path, fixtures: pathlib.Path) -> str:
    return (
        value.replace(str(temporary), "<qualification-temp>")
        .replace(str(fixtures), "<fixtures>")
        .replace(str(REPOSITORY), "<repository>")
        .replace(str(fixtures).replace("/", "\\"), "<fixtures>")
        .replace(str(REPOSITORY).replace("/", "\\"), "<repository>")
    )


def run(command: list[str], *, cwd: pathlib.Path, env: dict[str, str]) -> tuple[int, str]:
    result = subprocess.run(
        command,
        cwd=cwd,
        env=env,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        errors="replace",
    )
    return result.returncode, result.stdout


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--archive", type=pathlib.Path, required=True)
    parser.add_argument("--platform", choices=["macos-arm64", "linux-x86_64", "windows-x64"], required=True)
    parser.add_argument("--fixtures", type=pathlib.Path, required=True)
    parser.add_argument("--report", type=pathlib.Path, required=True)
    args = parser.parse_args()

    archive = args.archive.resolve()
    fixtures = args.fixtures.resolve()
    report_path = args.report.resolve()
    if not archive.is_file() or not fixtures.is_dir():
        raise SystemExit("qualification: archive and fixture directory must exist")

    statuses = {field: "NOT_APPLICABLE" for field in FIELDS}
    evidence: dict[str, str] = {}
    package_ok = False
    harness_ok = False

    with tempfile.TemporaryDirectory(prefix="openjoc-player-qualification-") as temporary_name:
        temporary = pathlib.Path(temporary_name)
        extract_dir = temporary / "extracted"
        extract_dir.mkdir()
        root = safe_extract(archive, extract_dir)
        env = dict(os.environ)
        env["HOME"] = str(temporary / "home")
        env["LC_ALL"] = "C"
        env["NO_PROXY"] = "*"
        for key in ("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy"):
            env.pop(key, None)
        (temporary / "home").mkdir()
        if args.platform == "windows-x64":
            system_root = env.get("SystemRoot") or env.get("WINDIR") or r"C:\Windows"
            sh_path = shutil.which("sh")
            tool_entries = [str(root / "bin")]
            if sh_path:
                tool_entries.append(str(pathlib.Path(sh_path).resolve().parent))
            tool_entries.extend([f"{system_root}\\System32", f"{system_root}\\System32\\Wbem"])
            env["PATH"] = ";".join(dict.fromkeys(tool_entries))
            env["SystemRoot"] = system_root
            env["WINDIR"] = system_root
        else:
            env["PATH"] = f"{root / 'bin'}:/usr/bin:/bin"
            env["LD_LIBRARY_PATH"] = str(root / "lib")
            env["DYLD_LIBRARY_PATH"] = str(root / "lib")

        verifier = [
            sys.executable, str(PACKAGE_VERIFIER), "verify", "--root", str(root),
            "--platform", args.platform, "--run-smoke", "--missing-dependency-smoke",
        ]
        if args.platform == "windows-x64":
            verifier.extend(["--fixture", str(fixtures / "joc.single.ec3")])
        code, output = run(verifier, cwd=root, env=env)
        evidence["package_verifier"] = clean_output(output, temporary, fixtures)
        if code == 0:
            package_ok = True
            for field in ("BUILD", "PACKAGE", "DEPENDENCIES", "LICENSE", "RUNTIME", "DECODER_SELECTION", "PRIVATE_PATH_SCAN"):
                statuses[field] = "PASS"
            if args.platform == "windows-x64":
                statuses["GUI_EXECUTABLE"] = "PASS"
                statuses["CONSOLE_ENTRYPOINT"] = "PASS"
                statuses["CONSOLE_INTERRUPT"] = "PASS" if "mpv.com console interrupt smoke: PASS" in output else "NOT_APPLICABLE"
        else:
            for field in ("BUILD", "PACKAGE", "DEPENDENCIES", "LICENSE", "RUNTIME", "DECODER_SELECTION", "PRIVATE_PATH_SCAN"):
                statuses[field] = "FAIL"
            if args.platform == "windows-x64":
                statuses["GUI_EXECUTABLE"] = "FAIL"
                statuses["CONSOLE_ENTRYPOINT"] = "FAIL"
                statuses["CONSOLE_INTERRUPT"] = "FAIL"

        if package_ok:
            harness_executable = "mpv.com" if args.platform == "windows-x64" else "mpv"
            harness = [shutil.which("sh") or "sh", str(PLAYER_HARNESS), str(root / "bin" / harness_executable), str(fixtures)]
            code, output = run(harness, cwd=root, env=env)
            evidence["player_harness"] = clean_output(output, temporary, fixtures)
            if code == 0:
                harness_ok = True
                for field in HARNESS_FIELDS:
                    statuses[field] = "PASS"
            else:
                for field in HARNESS_FIELDS:
                    statuses[field] = "FAIL"

        build_info = {}
        build_info_path = root / "BUILD_INFO.json"
        if build_info_path.is_file():
            build_info = json.loads(build_info_path.read_text(encoding="utf-8"))
        report = {
            "schema": "openjoc.player-qualification.v1",
            "platform": args.platform,
            "archive": archive.name,
            "archive_sha256": digest(archive),
            "archive_size": archive.stat().st_size,
            "qualification": "QUALIFIED" if package_ok and harness_ok else "BLOCKED",
            "statuses": statuses,
            "build_info": {
                "target": build_info.get("target"),
                "architecture": build_info.get("architecture"),
                "toolchain": build_info.get("toolchain"),
                "source": build_info.get("source"),
                "pinned_stack": build_info.get("pinned_stack"),
            },
            "environment": {
                "runner_os": os.environ.get("RUNNER_OS", sys.platform),
                "runner_arch": os.environ.get("RUNNER_ARCH", "unknown"),
                "cwd_for_runtime": "freshly extracted package directory",
                "network": "disabled for runtime qualification",
            },
            "evidence": evidence,
        }

    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    text_path = report_path.with_suffix(".txt")
    lines = [
        f"OpenJOC player qualification: {args.platform}",
        f"Archive: {archive.name}",
        f"Archive SHA-256: {report['archive_sha256']}",
        f"Qualification: {report['qualification']}",
        "",
    ]
    lines.extend(f"{field}: {statuses[field]}" for field in FIELDS)
    text_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(json.dumps({"report": str(report_path), "qualification": report["qualification"]}, sort_keys=True))
    return 0 if report["qualification"] == "QUALIFIED" else 1


if __name__ == "__main__":
    sys.exit(main())
