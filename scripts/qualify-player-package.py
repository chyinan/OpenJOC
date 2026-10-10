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
    "ORDINARY_EAC3", "BINAURAL", "BINAURAL_D2", "2_0", "5_1", "7_1",
    "5_1_2", "5_1_4", "7_1_2", "7_1_4", "9_1_6",
    "22_2", "EOS", "GAIN_PCM_BITEXACT", "GAIN_SAMPLES",
    "LIVE_GAIN_NO_RESTART", "APPLYCURRENT_REINIT",
)
GAIN_HARNESS_MARKERS = {
    "GAIN_PCM_BITEXACT": "GAIN_PCM_BITEXACT:PASS",
    "GAIN_SAMPLES": "GAIN_SAMPLES:PASS",
    "LIVE_GAIN_NO_RESTART": "LIVE_GAIN_NO_RESTART:PASS",
    "APPLYCURRENT_REINIT": "APPLYCURRENT_REINIT:PASS",
}
FIELDS = [
    "BUILD", "PACKAGE", "DEPENDENCIES", "LICENSE", "RUNTIME",
    "DECODER_SELECTION", "GUI_EXECUTABLE", "DIRECT_GUI_CONFIG_AUTOLOAD",
    "SETTINGS_IO_ROUNDTRIP",
    "CONSOLE_ENTRYPOINT", "CONSOLE_INTERRUPT",
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


def apply_gain_harness_statuses(statuses: dict[str, str], output: str) -> list[str]:
    """Mark gain evidence from literal harness PASS markers; missing is FAIL."""
    missing: list[str] = []
    for field, marker in GAIN_HARNESS_MARKERS.items():
        if marker in output:
            statuses[field] = "PASS"
        else:
            statuses[field] = "FAIL"
            missing.append(marker)
    return missing


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


def run_windows_settings_io_roundtrip(
    root: pathlib.Path, temporary: pathlib.Path, env: dict[str, str],
) -> tuple[bool, str]:
    """Drive the packaged Lua menu in mpv.exe under its Unicode config path."""
    config_dir = root / "bin" / "portable_config"
    settings = config_dir / "openjoc-settings.json"
    temp_path = settings.with_name(settings.name + ".tmp")
    backup_path = settings.with_name(settings.name + ".bak")
    driver = REPOSITORY / "integrations/mpv/test-openjoc-settings-mpv-driver.lua"
    executable = root / "bin" / "mpv.exe"
    if not driver.is_file() or not executable.is_file():
        return False, "mpv settings roundtrip driver or packaged mpv.exe is missing"
    if "日本語" not in str(settings) or " " not in str(settings):
        return False, "settings roundtrip config path must contain spaces and Japanese characters"

    # Seed the existing destination with Python's Unicode-safe file API. The
    # actual mpv/LuaJIT settings script then has to read, replace, and reload it.
    for path in (temp_path, backup_path):
        path.unlink(missing_ok=True)
    try:
        settings.write_text(
            json.dumps({"schema": 1, "options": {
                "render_mode": "speaker", "speaker_layout": "5.1",
            }}) + "\n",
            encoding="utf-8",
        )
    except OSError as error:
        return False, f"could not seed the Unicode-path settings file: {error}"
    outputs: list[str] = []
    for pass_number, expected_layout in ((1, "7.1"), (2, "5.1.2")):
        log_path = temporary / f"mpv-settings-roundtrip-{pass_number}.log"
        command = [
            str(executable), "--idle=yes", "--load-scripts=yes",
            "--force-window=no", "--vo=null", "--ao=null", "--no-video",
            "--msg-level=all=info", f"--log-file={log_path}",
            f"--script={driver}",
        ]
        try:
            result = subprocess.run(
                command, cwd=root, env=env, check=False, timeout=25,
                stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                text=True, encoding="utf-8", errors="replace",
            )
        except subprocess.TimeoutExpired as error:
            log = log_path.read_text(encoding="utf-8", errors="replace") if log_path.is_file() else ""
            outputs.append(f"pass {pass_number} timed out\n{log}\n{error.stdout or ''}")
            return False, "\n".join(outputs)

        log = log_path.read_text(encoding="utf-8", errors="replace") if log_path.is_file() else result.stdout
        outputs.append(f"pass {pass_number}: exit={result.returncode}\n{log}")
        if result.returncode != 0:
            return False, "\n".join(outputs)
        if "OpenJOC settings menu loaded" not in log or "OPENJOC_SETTINGS_MPV_DRIVER_DONE" not in log:
            return False, "settings script or headless driver did not complete\n" + "\n".join(outputs)
        try:
            document = json.loads(settings.read_text(encoding="utf-8"))
        except (OSError, UnicodeError, json.JSONDecodeError) as error:
            return False, f"could not read saved Unicode-path state after pass {pass_number}: {error}\n" + "\n".join(outputs)
        options = document.get("options") if isinstance(document, dict) else None
        actual_layout = options.get("speaker_layout") if isinstance(options, dict) else None
        if not isinstance(document, dict) or document.get("schema") != 1 or actual_layout != expected_layout:
            return False, (
                f"pass {pass_number} expected saved layout {expected_layout!r}, got {actual_layout!r}\n"
                + "\n".join(outputs)
            )
        if temp_path.exists() or backup_path.exists():
            return False, f"pass {pass_number} left a temporary or backup settings file\n" + "\n".join(outputs)

    for path in (settings, temp_path, backup_path):
        path.unlink(missing_ok=True)
    return True, "actual mpv Lua settings save/load roundtrip passed under Unicode portable_config\n" + "\n".join(outputs)


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
    direct_gui_config_ok = args.platform != "windows-x64"
    settings_io_roundtrip_ok = args.platform != "windows-x64"

    with tempfile.TemporaryDirectory(prefix="openjoc-player-qualification-") as temporary_name:
        temporary = pathlib.Path(temporary_name)
        # The native Windows executable and launchers must handle ordinary
        # relocated installs containing spaces and non-ASCII characters.
        extract_dir = temporary / "extracted bundle with spaces — 日本語"
        extract_dir.mkdir()
        root = safe_extract(archive, extract_dir)
        env = dict(os.environ)
        env["HOME"] = str(temporary / "home")
        env["LC_ALL"] = "C"
        env.pop("MPV_HOME", None)
        env["NO_PROXY"] = "*"
        for key in ("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy"):
            env.pop(key, None)
        (temporary / "home").mkdir()
        if args.platform == "windows-x64":
            system_root = env.get("SystemRoot") or env.get("WINDIR") or r"C:\Windows"
            isolated_profile = temporary / "isolated-user-profile"
            native_profile = str(isolated_profile)
            cygpath = shutil.which("cygpath")
            if cygpath:
                converted = subprocess.run(
                    [cygpath, "-w", native_profile], check=False,
                    stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                    text=True, errors="replace",
                )
                if converted.returncode == 0 and converted.stdout.strip():
                    native_profile = converted.stdout.strip()
            env["USERPROFILE"] = native_profile
            env["APPDATA"] = native_profile + r"\AppData\Roaming"
            env["LOCALAPPDATA"] = native_profile + r"\AppData\Local"
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
        verifier.extend(["--fixture", str(fixtures / "joc.single.ec3")])
        code, output = run(verifier, cwd=root, env=env)
        evidence["package_verifier"] = clean_output(output, temporary, fixtures)
        if code == 0:
            package_ok = True
            for field in ("BUILD", "PACKAGE", "DEPENDENCIES", "LICENSE", "RUNTIME", "DECODER_SELECTION", "PRIVATE_PATH_SCAN"):
                statuses[field] = "PASS"
            if args.platform == "windows-x64":
                statuses["GUI_EXECUTABLE"] = "PASS"
                direct_gui_config_ok = "mpv.exe direct portable_config Lua menu autoload (no --config-dir): PASS" in output
                statuses["DIRECT_GUI_CONFIG_AUTOLOAD"] = "PASS" if direct_gui_config_ok else "FAIL"
                statuses["CONSOLE_ENTRYPOINT"] = "PASS"
                statuses["CONSOLE_INTERRUPT"] = "PASS" if "mpv.com console interrupt smoke: PASS" in output else "NOT_APPLICABLE"
        else:
            for field in ("BUILD", "PACKAGE", "DEPENDENCIES", "LICENSE", "RUNTIME", "DECODER_SELECTION", "PRIVATE_PATH_SCAN"):
                statuses[field] = "FAIL"
            if args.platform == "windows-x64":
                statuses["GUI_EXECUTABLE"] = "FAIL"
                statuses["DIRECT_GUI_CONFIG_AUTOLOAD"] = "FAIL"
                statuses["CONSOLE_ENTRYPOINT"] = "FAIL"
                statuses["CONSOLE_INTERRUPT"] = "FAIL"

        if package_ok and args.platform == "windows-x64":
            settings_io_roundtrip_ok, settings_output = run_windows_settings_io_roundtrip(
                root, temporary, env,
            )
            evidence["settings_io_roundtrip"] = clean_output(settings_output, temporary, fixtures)
            statuses["SETTINGS_IO_ROUNDTRIP"] = "PASS" if settings_io_roundtrip_ok else "FAIL"

        if package_ok:
            harness_executable = "mpv.com" if args.platform == "windows-x64" else "mpv"
            harness = [shutil.which("sh") or "sh", str(PLAYER_HARNESS), str(root / "bin" / harness_executable), str(fixtures)]
            code, output = run(harness, cwd=root, env=env)
            evidence["player_harness"] = clean_output(output, temporary, fixtures)
            if code == 0:
                for field in HARNESS_FIELDS:
                    statuses[field] = "PASS"
                missing_markers = apply_gain_harness_statuses(statuses, output)
                harness_ok = not missing_markers
                if missing_markers:
                    evidence["gain_marker_contract"] = (
                        "missing required harness markers: " + ", ".join(missing_markers)
                    )
            else:
                harness_ok = False
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
            "qualification": "QUALIFIED" if package_ok and harness_ok and direct_gui_config_ok and settings_io_roundtrip_ok else "BLOCKED",
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
