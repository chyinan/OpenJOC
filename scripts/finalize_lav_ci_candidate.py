#!/usr/bin/env python3
"""Label and audit a one-off LAV audio CI package; never publish a release."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import tempfile
import zipfile

from package_lav_release import FFMPEG_DLLS
from release_packaging_core import deterministic_zip, render_sha256_manifest, sha256_file

CORE = "0652b0e4c37b2cdc6e7e6e28445a6a4b024df7ab"
LAV = "71e142b0cd3c05e921206e983125f8f6929105f5"
LAV_TREE = "d9fa2e34322ba70cc7a42b8d550e641828fabdf7"
FFMPEG = "b033272d7ef2070db2dc8ef228e67b20213e90c8"
ZLIB = "51b7f2abdade71cd9bb0e7a373ef2610ec6f9daf"
LABEL = "openjoc-lav-ci-20261010-core0652b0e-lav71e142b-windows-x64"
SMOKES = [
    "AudioStatusCapacityTests", "OpenJocAdmissionTests", "OpenJocDiagnosticTests",
    "OpenJocDialnormPolicyTests", "OpenJocOutputTests", "OpenJocStrictOutputTests",
    "OpenJocShippedLayoutsTests", "OpenJocSettingsSmoke",
    "OpenJocSettingsSmoke --output-gain-only", "OpenJocPropertyPageSmoke",
    "LAVAudioIdentitySmoke",
]


def run(*args: str | Path) -> str:
    return subprocess.check_output([str(arg) for arg in args], text=True, encoding="utf-8").strip()


def git(path: Path, *args: str) -> str:
    return run("git", "-C", path, *args)


def require_x64(path: Path) -> None:
    data = path.read_bytes()
    if data[:2] != b"MZ" or len(data) < 64:
        raise ValueError(f"not a PE binary: {path.name}")
    offset = struct.unpack_from("<I", data, 60)[0]
    if data[offset:offset + 4] != b"PE\x00\x00" or len(data) < offset + 6:
        raise ValueError(f"invalid PE signature: {path.name}")
    if struct.unpack_from("<H", data, offset + 4)[0] != 0x8664:
        raise ValueError(f"not an x64 PE binary: {path.name}")


def verify_manifest(root: Path) -> None:
    text = (root / "PACKAGE_SHA256SUMS.txt").read_text(encoding="utf-8")
    actual = render_sha256_manifest(root, excluded={"PACKAGE_SHA256SUMS.txt"})
    if text != actual:
        raise ValueError("package file inventory or SHA-256 mismatch")


def finalize(package: Path, workspace: Path, output: Path) -> Path:
    if output.exists():
        raise FileExistsError(f"candidate output already exists: {output}")
    sources = {}
    for relative, expected in (("openjoc-release", CORE), ("lav", LAV), ("lav/ffmpeg", FFMPEG), ("zlib", ZLIB)):
        actual = git(workspace / relative, "rev-parse", "HEAD")
        if actual != expected:
            raise ValueError(f"source identity mismatch: {relative}")
        sources[relative] = actual
    if git(workspace / "lav", "rev-parse", "HEAD^{tree}") != LAV_TREE:
        raise ValueError("reviewed LAV source tree mismatch")
    submodules = git(workspace / "lav", "submodule", "status", "--recursive").splitlines()
    if any(line.startswith(("+", "U", "-")) for line in submodules):
        raise ValueError("LAV submodule mismatch")
    expected_majors = {"libavcodec": 63, "libavfilter": 12, "libavformat": 63, "libavutil": 61, "libswresample": 7, "libswscale": 10}
    for library, major in expected_majors.items():
        source = workspace / "lav" / "ffmpeg" / library
        text = "\n".join(p.read_text(encoding="utf-8") for p in source.glob("version*.h"))
        match = re.search(r"#define\s+" + library.upper() + r"_VERSION_MAJOR\s+(\d+)", text)
        if not match or int(match.group(1)) != major:
            raise ValueError(f"unexpected FFmpeg ABI major for {library}")

    vswhere = Path(os.environ["ProgramFiles(x86)"]) / "Microsoft Visual Studio/Installer/vswhere.exe"
    vs = json.loads(run(vswhere, "-latest", "-products", "*", "-requires", "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-format", "json"))[0]
    toolset = (Path(vs["installationPath"]) / "VC/Auxiliary/Build/Microsoft.VCToolsVersion.default.txt").read_text().strip()
    provenance = {
        "kind": "CI test candidate, not a stable release",
        "scope": "OpenJOC-enabled LAVAudio only; matching FFmpeg DLLs rebuilt from source",
        "sources": sources,
        "lav_tree": LAV_TREE,
        "lav_submodules": submodules,
        "workflow_commit": os.environ["GITHUB_SHA"],
        "workflow_run": f"https://github.com/{os.environ['GITHUB_REPOSITORY']}/actions/runs/{os.environ['GITHUB_RUN_ID']}",
        "toolchain": {"rustc": run("rustc", "-vV"), "cargo": run("cargo", "-Vv"), "visual_studio": vs["installationVersion"], "msvc_toolset": toolset, "runner_os": os.environ.get("RUNNER_OS"), "runner_arch": os.environ.get("RUNNER_ARCH"), "image_version": os.environ.get("ImageVersion")},
        "configuration": {"architecture": "x64", "configuration": "Release", "EnableOpenJOC": True, "EnableOpenJOCSideBySide": True, "cargo": "build -p openjoc-capi --release --locked --jobs 1", "ffmpeg": "build_ffmpeg_msvc.sh x64 release; clean build; see BUILD_EVIDENCE"},
        "native_validation_passed": ["package x64 dependency-load preflight", *SMOKES],
        "not_validated": ["real PotPlayer playback", "physical audio endpoint behavior", "LAVVideo and LAVSplitter runtime changes"],
    }
    with tempfile.TemporaryDirectory(prefix="openjoc-lav-ci-") as temporary:
        root = Path(temporary) / "package"
        with zipfile.ZipFile(package) as archive:
            for name in archive.namelist():
                p = Path(name)
                if p.is_absolute() or ".." in p.parts:
                    raise ValueError("unsafe archive entry")
            archive.extractall(root)
        verify_manifest(root)
        runtime = root / "runtime"
        for binary in runtime.iterdir():
            if binary.suffix.lower() in {".dll", ".ax"}:
                require_x64(binary)
        fresh = {name: workspace / "lav/bin_x64" / name for name in (*FFMPEG_DLLS, "LAVAudio.ax", "libbluray.dll")}
        fresh["openjoc_capi.dll"] = workspace / "openjoc-release/target/release/openjoc_capi.dll"
        for name, source in fresh.items():
            if sha256_file(source) != sha256_file(runtime / name):
                raise ValueError(f"package differs from freshly built output: {name}")
        evidence = root / "BUILD_EVIDENCE"
        evidence.mkdir()
        for relative, name in (("lav/build_ffmpeg_msvc.sh", "build_ffmpeg_msvc.sh"), ("lav/ffmpeg/config.h", "ffmpeg-config.h"), ("lav/ffmpeg/ffbuild/config.mak", "ffmpeg-config.mak")):
            (evidence / name).write_bytes((workspace / relative).read_bytes())
        for relative, name in (("lav", "lav-build-adjustments.patch"), ("lav/ffmpeg", "ffmpeg-build-adjustments.patch"), ("zlib", "zlib-build-adjustments.patch")):
            (evidence / name).write_text(git(workspace / relative, "diff", "--", ".") + "\n", encoding="utf-8")
        provenance["cargo_lock_sha256"] = sha256_file(workspace / "openjoc-release/Cargo.lock")
        provenance["runtime_sha256"] = {p.name: sha256_file(p) for p in sorted(runtime.iterdir()) if p.is_file()}
        (root / "BUILD_PROVENANCE.json").write_text(json.dumps(provenance, indent=2) + "\n", encoding="utf-8")
        readme = root / "README.md"
        readme.write_text("# CI test candidate: 2026-10-10\n\nThis is an experimental audio integration package, not stable 0.19.0.\nThe installer profile retains 0.19.0 for compatibility; source identities are in BUILD_PROVENANCE.json.\nLAVAudio and all bundled FFmpeg libraries were rebuilt together. Do not mix DLLs from another package.\n\nBefore testing, close PotPlayer and retain your previous complete package and settings.\nInstalling this candidate can replace an existing OpenJOC LAV installation using the same registration.\nTo roll back, close PotPlayer, run this package's uninstall.bat, then reinstall your previous complete package and reselect it in PotPlayer.\nDo not delete your old package until playback is verified.\n\nThis artifact passed the listed native package/smoke checks. Real PotPlayer playback, physical audio endpoints, and upstream video/splitter changes remain unverified.\n\n" + readme.read_text(encoding="utf-8"), encoding="utf-8")
        (root / "PACKAGE_SHA256SUMS.txt").write_text(render_sha256_manifest(root, excluded={"PACKAGE_SHA256SUMS.txt"}), encoding="utf-8")
        verify_manifest(root)
        output.mkdir(parents=True)
        result = output / f"{LABEL}.zip"
        deterministic_zip(root, result)
        with zipfile.ZipFile(result) as archive:
            names = set(archive.namelist())
            if not {"BUILD_PROVENANCE.json", "PACKAGE_SHA256SUMS.txt", "runtime/LAVAudio.ax"} <= names:
                raise ValueError("final candidate inventory is incomplete")
        (output / "BUILD_PROVENANCE.json").write_bytes((root / "BUILD_PROVENANCE.json").read_bytes())
        (output / "SHA256SUMS.txt").write_text(f"{sha256_file(result)}  {result.name}\n{sha256_file(output / 'BUILD_PROVENANCE.json')}  BUILD_PROVENANCE.json\n", encoding="utf-8")
        print(f"candidate={result}")
        print(f"candidate_sha256={sha256_file(result)}")
        return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", required=True, type=Path)
    parser.add_argument("--workspace", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    finalize(args.package, args.workspace, args.output)
