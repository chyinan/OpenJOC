#!/usr/bin/env python3
"""Check complete session captures against independently frozen file digests.

This runner has no expected-output generation or update mode. It supplements,
and does not replace, verify_pcm_bitexact.py and its pinned historical oracle.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = Path("scripts/fixtures/session-contracts/frozen.json")
SUITES = {
    "constructors": ("OPENJOC_CONSTRUCTOR_CAPTURE", ["--test", "session_constructors"]),
    "terminal": ("OPENJOC_TERMINAL_CAPTURE", ["--lib", "terminal_error_contract::"]),
    "lifecycle": ("OPENJOC_LIFECYCLE_CAPTURE", ["--lib", "session::lifecycle_contract::"]),
}
BUILD_ENV = {
    "CARGO_BUILD_JOBS": "1",
    "CARGO_INCREMENTAL": "0",
    "CARGO_PROFILE_DEV_DEBUG": "0",
    "CARGO_PROFILE_TEST_DEBUG": "0",
    "CARGO_TERM_COLOR": "never",
}


class ContractError(RuntimeError):
    pass


def file_record(path):
    if path.is_symlink() or not path.is_file():
        raise ContractError(f"not a regular non-symlink file: {path}")
    digest = hashlib.sha256()
    size = 0
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            size += len(block)
            digest.update(block)
    return {"bytes": size, "sha256": digest.hexdigest()}


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ContractError(f"duplicate manifest key: {key}")
        result[key] = value
    return result


def load_manifest(path):
    manifest = json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique_object)
    if manifest.get("schema_version") != 1 or set(manifest.get("suites", {})) != set(SUITES):
        raise ContractError("unsupported or incomplete session contract manifest")
    for name, suite in manifest["suites"].items():
        files = suite["files"]
        if not files or len(files) != suite["cases"] * 2:
            raise ContractError(f"incomplete frozen file set: {name}")
        stems = set()
        for filename, expected in files.items():
            if not re.fullmatch(r"[A-Za-z0-9_-]+\.(?:f32le|txt)", filename):
                raise ContractError(f"invalid capture filename: {filename}")
            stems.add(filename.rsplit(".", 1)[0])
            if (
                set(expected) != {"bytes", "sha256"}
                or type(expected["bytes"]) is not int
                or expected["bytes"] < 0
                or not re.fullmatch(r"[a-f0-9]{64}", expected["sha256"])
            ):
                raise ContractError(f"invalid frozen digest: {filename}")
        if set(files) != {f"{stem}.{ext}" for stem in stems for ext in ("f32le", "txt")}:
            raise ContractError(f"unpaired frozen PCM/transcript files: {name}")
    return manifest


def verify_inputs(root, manifest):
    records = {}
    for name, expected in manifest["inputs"].items():
        relative = Path(name)
        if relative.is_absolute() or ".." in relative.parts:
            raise ContractError(f"invalid input path: {name}")
        record = file_record(root / relative)
        if record["sha256"] != expected:
            raise ContractError(f"frozen fixture, lockfile or capture harness changed: {name}")
        records[name] = record
    return records


def source_record(root):
    paths = [root / "Cargo.toml", root / "Cargo.lock", root / MANIFEST, root / "scripts/verify_session_contracts.py"]
    paths += sorted(path for path in (root / "crates").rglob("*") if path.is_file() or path.is_symlink())
    files = {path.relative_to(root).as_posix(): file_record(path) for path in paths}
    encoded = json.dumps(files, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return {"sha256": hashlib.sha256(encoded).hexdigest(), "files": files}


def compare_capture(directory, expected):
    if directory.is_symlink() or not directory.is_dir():
        raise ContractError(f"capture directory missing or symlinked: {directory}")
    entries = {path.name: path for path in directory.iterdir()}
    missing = sorted(set(expected) - set(entries))
    extra = sorted(set(entries) - set(expected))
    if missing or extra:
        raise ContractError(f"capture file set differs: missing={missing}; extra={extra}")
    actual = {}
    for name in sorted(expected):
        actual[name] = file_record(entries[name])
        if actual[name] != expected[name]:
            raise ContractError(
                f"capture differs: {name}; expected={expected[name]}; actual={actual[name]}"
            )
    return actual


def check_mutation_sensitivity(directory, expected, scratch):
    """Change one bit in a disposable copy; neither captures nor expectations change."""
    scratch.mkdir()
    checked = []
    for extension in ("f32le", "txt"):
        name = next(name for name, item in expected.items() if name.endswith("." + extension) and item["bytes"])
        destination = scratch / extension
        destination.mkdir()
        data = bytearray((directory / name).read_bytes())
        data[len(data) // 2] ^= 1
        (destination / name).write_bytes(data)
        try:
            compare_capture(destination, {name: expected[name]})
        except ContractError as error:
            if "capture differs:" not in str(error):
                raise
        else:
            raise ContractError(f"single-bit mutation escaped comparison: {name}")
        checked.append(name)
    return checked


def run_checked(command, root, env, log):
    with log.open("xb") as stream:
        completed = subprocess.run(command, cwd=root, env=env, stdout=stream, stderr=subprocess.STDOUT)
    if completed.returncode:
        raise ContractError(f"command failed ({completed.returncode}): {' '.join(command)}; see {log}")
    return log.read_text(encoding="utf-8", errors="replace")


def argument_parser():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--toolchain", default="1.99.0")
    parser.add_argument("--output-dir", type=Path, required=True, help="new directory; never reused or overwritten")
    parser.add_argument("--target-dir", type=Path, help="optional reusable Cargo target directory")
    parser.add_argument("--online", action="store_true", help="allow Cargo to fetch locked dependencies")
    return parser


def main(argv=None):
    args = argument_parser().parse_args(argv)
    root = args.root.resolve()
    output = args.output_dir.resolve()
    # Claim the output once; never clean or replace a directory supplied by a caller.
    try:
        output.mkdir(parents=True, exist_ok=False)
    except OSError as error:
        print(f"FAIL: cannot create fresh output directory: {error}", file=sys.stderr)
        return 1
    report = {"result": "FAIL", "comparison": "full-file length and SHA-256", "suites": {}}
    try:
        manifest_path = root / MANIFEST
        manifest = load_manifest(manifest_path)
        report["manifest"] = file_record(manifest_path)
        report["inputs"] = verify_inputs(root, manifest)
        report["source"] = source_record(root)
        report["ci_revision"] = os.environ.get("GITHUB_SHA")
        env = os.environ.copy()
        env.update(BUILD_ENV)
        env["CARGO_TARGET_DIR"] = str((args.target_dir or output / "target").resolve())
        # Capture locations are owned by this invocation, never inherited.
        for variable, _ in SUITES.values():
            env.pop(variable, None)
        report["build_environment"] = {key: env.get(key) for key in [*BUILD_ENV, "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_TARGET_DIR"]}
        report["toolchain_selector"] = args.toolchain
        for program in ("rustc", "cargo"):
            command = [program, "+" + args.toolchain, "-vV" if program == "rustc" else "-Vv"]
            report[program] = run_checked(command, root, env, output / (program + ".txt"))
        for name, (variable, selection) in SUITES.items():
            directory = output / name
            directory.mkdir()
            command = ["cargo", "+" + args.toolchain, "test", "--locked"]
            if not args.online:
                command.append("--offline")
            command += ["-p", "openjoc-api", *selection, "--", "--nocapture", "--test-threads=1"]
            record = {"command": command, "result": "FAIL"}
            report["suites"][name] = record
            run_checked(command, root, env | {variable: str(directory)}, output / (name + ".log"))
            expected = manifest["suites"][name]["files"]
            record["files"] = compare_capture(directory, expected)
            record["single_bit_negative_controls"] = check_mutation_sensitivity(directory, expected, output / (name + "-negative-controls"))
            record["result"] = "PASS"
            print(f"PASS {name}: {len(expected)} complete PCM/transcript files; single-bit controls rejected", flush=True)
        if report["manifest"] != file_record(manifest_path):
            raise ContractError("frozen manifest changed during the run")
        verify_inputs(root, manifest)
        if report["source"] != source_record(root):
            raise ContractError("source changed during the run")
        report["result"] = "PASS"
    except (ContractError, OSError, ValueError, KeyError, TypeError) as error:
        report["error"] = str(error)
        print(f"FAIL: {error}", file=sys.stderr)
    finally:
        with (output / "report.json").open("x", encoding="utf-8", newline="\n") as stream:
            json.dump(report, stream, indent=2)
            stream.write("\n")
    return 0 if report["result"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
