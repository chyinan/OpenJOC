#!/usr/bin/env python3
"""Build and inspect portable OpenJOC-enabled mpv player archives.

The build scripts deliberately keep source trees and build prefixes outside the
archive.  This module performs the final runtime-closure copy, loader rewrite,
metadata generation, deterministic archive creation, and package inspection.
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import os
import pathlib
import platform
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import time
import zipfile


REPOSITORY = pathlib.Path(__file__).resolve().parent.parent
MANIFEST_PATH = REPOSITORY / "packaging/player/PLAYER_PACKAGE_MANIFEST.json"
QUICKSTART_PATH = REPOSITORY / "packaging/player/QUICKSTART.md"
PROFILES_PATH = REPOSITORY / "packaging/player/profiles.conf"
OPENJOC_LICENSE = REPOSITORY / "LICENSE"
OPENJOC_NOTICES = REPOSITORY / "THIRD_PARTY_NOTICES.md"
BUILTIN_HRTF = REPOSITORY / "crates/openjoc-sofa/assets/sadie-ii-d1-48k-256tap.sofa"
PRIVATE_MARKERS = (
    "/Users/",
    "/Users/runner/",
    "C:/Users/runneradmin/",
    "C:\\Users\\runneradmin\\",
    "\\Users\\runneradmin\\",
    "D:/a/",
    "D:\\a\\",
    "/opt/homebrew/",
    "/usr/local/Cellar/",
    "/home/runner/",
    "\\home\\runner\\",
    "/tmp/openjoc-player-",
    "/private/tmp/openjoc-player-",
)
WINDOWS_SYSTEM_DLLS = {
    "ADVAPI32.DLL", "API-MS-WIN-CORE-", "API-MS-WIN-CRT-", "AVRT.DLL",
    "BCRYPT.DLL", "CFGMGR32.DLL", "COMBASE.DLL", "COMDLG32.DLL",
    "CRYPT32.DLL", "D3D11.DLL", "D3D12.DLL", "DINPUT8.DLL", "DNSAPI.DLL",
    "DXGI.DLL", "GDI32.DLL", "IMM32.DLL", "IPHLPAPI.DLL", "KERNEL32.DLL",
    "KERNELBASE.DLL", "MF.DLL", "MFPLAT.DLL", "MFREADWRITE.DLL", "MSVCRT.DLL",
    "NTDLL.DLL", "OLE32.DLL", "OLEAUT32.DLL", "POWRPROF.DLL", "RPCRT4.DLL",
    "SCHANNEL.DLL", "SECUR32.DLL", "SETUPAPI.DLL", "SHELL32.DLL", "SHLWAPI.DLL",
    "USER32.DLL", "USERENV.DLL", "UXTHEME.DLL", "UCRTBASE.DLL", "VERSION.DLL",
    "WINHTTP.DLL", "WINMM.DLL", "WINSPOOL.DRV", "WS2_32.DLL", "WSOCK32.DLL",
}


def run(command: list[str], *, check: bool = True, cwd: pathlib.Path | None = None) -> str:
    result = subprocess.run(
        command,
        cwd=cwd,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        errors="replace",
    )
    if check and result.returncode != 0:
        raise RuntimeError(f"command failed ({result.returncode}): {' '.join(command)}\n{result.stdout}")
    return result.stdout


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def write_json(path: pathlib.Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def copy_file(source: pathlib.Path, destination: pathlib.Path) -> pathlib.Path:
    source = source.resolve()
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, destination)
    return destination


def executable_mode(path: pathlib.Path) -> int:
    return 0o755 if path.stat().st_mode & stat.S_IXUSR else 0o644


def source_date_epoch() -> int:
    value = os.environ.get("SOURCE_DATE_EPOCH")
    if value is None:
        return 0
    try:
        return int(value)
    except ValueError as error:
        raise SystemExit("SOURCE_DATE_EPOCH must be an integer") from error


def git_value(*arguments: str) -> str:
    return run(["git", *arguments], cwd=REPOSITORY).strip()


def development_id() -> str:
    manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
    version = manifest["openjoc"]["version"]
    commit = git_value("rev-parse", "HEAD")
    return f"{version}-git{commit[:12]}"


def builtin_hrtf_evidence() -> dict[str, str]:
    if not BUILTIN_HRTF.is_file():
        raise SystemExit(f"built-in HRTF resource is missing: {BUILTIN_HRTF}")
    return {
        "dataset": "SADIE II D1 (KU100), v2-2",
        "source": "crates/openjoc-sofa/assets/sadie-ii-d1-48k-256tap.sofa",
        "sha256": sha256(BUILTIN_HRTF),
    }


def parse_macho_dependencies(path: pathlib.Path) -> list[str]:
    output = run(["otool", "-L", str(path)])
    return [line.strip().split(" ", 1)[0] for line in output.splitlines()[1:] if line.strip()]


def parse_macho_rpaths(path: pathlib.Path) -> list[str]:
    output = run(["otool", "-l", str(path)])
    values: list[str] = []
    lines = output.splitlines()
    for index, line in enumerate(lines):
        if line.strip() == "cmd LC_RPATH":
            for candidate in lines[index + 1 : index + 8]:
                match = re.search(r"path (.+?) \(offset", candidate.strip())
                if match:
                    values.append(match.group(1))
                    break
    return values


def macho_system_dependency(value: str) -> bool:
    return value.startswith((
        "/System/Library/",
        "/System/Volumes/Preboot/Cryptexes/OS/usr/lib/",
        "/usr/lib/",
        "/Library/Apple/",
    ))


def resolve_macho_dependency(
    value: str,
    owner: pathlib.Path,
    search_dirs: list[pathlib.Path],
) -> pathlib.Path | None:
    if value.startswith("/"):
        candidate = pathlib.Path(value)
        return candidate if candidate.exists() else None
    owner_dir = owner.parent
    rpaths = parse_macho_rpaths(owner)
    candidates: list[pathlib.Path] = []
    if value.startswith("@loader_path/"):
        candidates.append(owner_dir / value.removeprefix("@loader_path/"))
    elif value.startswith("@executable_path/"):
        candidates.extend(path / value.removeprefix("@executable_path/") for path in search_dirs)
    elif value.startswith("@rpath/"):
        relative = value.removeprefix("@rpath/")
        for rpath in rpaths:
            if rpath.startswith("@loader_path/"):
                candidates.append(owner_dir / rpath.removeprefix("@loader_path/") / relative)
            elif rpath.startswith("@executable_path/"):
                candidates.extend(
                    path / rpath.removeprefix("@executable_path/") / relative
                    for path in search_dirs
                )
            else:
                candidates.append(pathlib.Path(rpath) / relative)
        candidates.extend(path / relative for path in search_dirs)
    else:
        candidates.extend(pathlib.Path(value) for path in search_dirs)
    for candidate in candidates:
        if candidate.exists():
            return candidate.resolve()
    return None


def collect_macos(
    executable: pathlib.Path,
    destination: pathlib.Path,
    search_dirs: list[pathlib.Path],
) -> tuple[list[dict[str, object]], list[str]]:
    destination.mkdir(parents=True, exist_ok=True)
    copied: dict[str, pathlib.Path] = {}
    records: list[dict[str, object]] = []
    external: set[str] = set()
    queue = [(executable.resolve(), destination.parent / "bin" / "mpv")]
    while queue:
        source, target = queue.pop(0)
        for raw in parse_macho_dependencies(source):
            if macho_system_dependency(raw):
                external.add(raw)
                continue
            dependency = resolve_macho_dependency(raw, source, search_dirs)
            if dependency is None:
                raise RuntimeError(f"unresolved Mach-O dependency {raw!r} from {source}")
            name = dependency.name
            target_path = destination / name
            if name not in copied:
                copied[name] = dependency
                copy_file(dependency, target_path)
                target_path.chmod(executable_mode(dependency))
                queue.append((dependency, target_path))
    for name, source in sorted(copied.items()):
        target = destination / name
        records.append({
            "path": f"lib/{name}",
            "source": str(source),
            "sha256": sha256(target),
            "size": target.stat().st_size,
            "kind": "bundled",
        })
    binary = destination.parent / "bin" / "mpv"
    for owner in [binary, *sorted(destination.glob("*.dylib"))]:
        dependencies = parse_macho_dependencies(owner)
        changes = []
        for raw in dependencies:
            if not macho_system_dependency(raw):
                changes.extend(["-change", raw, f"@rpath/{pathlib.Path(raw).name}"])
        if owner == binary:
            changes.extend(["-add_rpath", "@loader_path/../lib"])
        else:
            changes.extend(["-id", f"@rpath/{owner.name}", "-add_rpath", "@loader_path"])
        if changes:
            run(["install_name_tool", *changes, str(owner)])
    return records, sorted(external)


def parse_ldd(path: pathlib.Path) -> list[tuple[str, pathlib.Path | None]]:
    output = run(["ldd", str(path)])
    values: list[tuple[str, pathlib.Path | None]] = []
    for line in output.splitlines():
        line = line.strip()
        if not line:
            continue
        if "not found" in line:
            name = line.split()[0]
            raise RuntimeError(f"unresolved ELF dependency {name} from {path}")
        if "=>" in line:
            name, rest = line.split("=>", 1)
            candidate = rest.strip().split(" ", 1)[0]
            values.append((name.strip(), pathlib.Path(candidate) if candidate.startswith("/") else None))
        else:
            candidate = line.split(" ", 1)[0]
            if candidate.startswith("/"):
                values.append((pathlib.Path(candidate).name, pathlib.Path(candidate)))
    return values


def linux_system_dependency(name: str) -> bool:
    return name.startswith((
        "libc.so", "libm.so", "libpthread.so", "libdl.so", "librt.so", "libutil.so",
        "libgcc_s.so", "libstdc++.so", "ld-linux", "linux-vdso.so",
    ))


def collect_linux(
    executable: pathlib.Path,
    destination: pathlib.Path,
) -> tuple[list[dict[str, object]], list[str]]:
    destination.mkdir(parents=True, exist_ok=True)
    copied: dict[str, pathlib.Path] = {}
    external: set[str] = set()
    queue = [executable.resolve()]
    while queue:
        source = queue.pop(0)
        for name, dependency in parse_ldd(source):
            if linux_system_dependency(name):
                external.add(name)
                continue
            if dependency is None:
                external.add(name)
                continue
            if name not in copied:
                copied[name] = dependency.resolve()
                target = destination / name
                copy_file(dependency, target)
                target.chmod(executable_mode(dependency))
                queue.append(dependency.resolve())
    patchelf = shutil.which("patchelf")
    if patchelf is None:
        raise RuntimeError("Linux packaging requires patchelf to create an $ORIGIN bundle")
    binary = destination.parent / "bin" / "mpv"
    owners = [binary, *sorted(destination / name for name in copied)]
    for owner in owners:
        needed = run([patchelf, "--print-needed", str(owner)], check=False)
        for value in needed.splitlines():
            value = value.strip()
            if "/" not in value:
                continue
            name = pathlib.Path(value).name
            if name in copied or (destination / name).is_file():
                run([patchelf, "--replace-needed", value, name, str(owner)])
    run([patchelf, "--set-rpath", "$ORIGIN/../lib", str(binary)])
    for name in sorted(copied):
        target = destination / name
        run([patchelf, "--set-rpath", "$ORIGIN", str(target)], check=False)
    records = [
        {
            "path": f"lib/{name}",
            "source": str(source),
            "sha256": sha256(destination / name),
            "size": (destination / name).stat().st_size,
            "kind": "bundled",
        }
        for name, source in sorted(copied.items())
    ]
    return records, sorted(external)


def pe_imports(path: pathlib.Path) -> list[str]:
    objdump = shutil.which("objdump")
    if objdump is None:
        raise RuntimeError("Windows packaging requires MinGW objdump")
    output = run([objdump, "-p", str(path)])
    return [match.group(1) for match in re.finditer(r"DLL Name: (.+)", output, re.IGNORECASE)]


def collect_windows(
    executable: pathlib.Path,
    destination: pathlib.Path,
    search_dirs: list[pathlib.Path],
) -> tuple[list[dict[str, object]], list[str]]:
    destination.mkdir(parents=True, exist_ok=True)
    copied: dict[str, pathlib.Path] = {}
    external: set[str] = set()
    queue = [executable.resolve()]
    while queue:
        source = queue.pop(0)
        for name in pe_imports(source):
            if windows_system_dependency(name):
                external.add(name)
                continue
            dependency = find_case_insensitive(search_dirs, name)
            if dependency is None:
                raise RuntimeError(f"unresolved PE dependency {name!r} from {source}")
            if name.lower() not in {key.lower() for key in copied}:
                copied[name] = dependency.resolve()
                target = destination / name
                copy_file(dependency, target)
                queue.append(dependency.resolve())
    records = [
        {
            "path": f"{destination.name}/{name}",
            "source": str(source),
            "sha256": sha256(destination / name),
            "size": (destination / name).stat().st_size,
            "kind": "bundled",
        }
        for name, source in sorted(copied.items())
    ]
    return records, sorted(external)


def find_case_insensitive(search_dirs: list[pathlib.Path], name: str) -> pathlib.Path | None:
    for directory in search_dirs:
        direct = directory / name
        if direct.is_file():
            return direct
        lowered = name.lower()
        for candidate in directory.iterdir() if directory.is_dir() else ():
            if candidate.is_file() and candidate.name.lower() == lowered:
                return candidate
    return None


def windows_system_dependency(name: str) -> bool:
    upper = name.upper()
    if upper.startswith(("API-MS-WIN-", "EXT-MS-WIN-", "VCRUNTIME")):
        return True
    if upper in WINDOWS_SYSTEM_DLLS:
        return True
    windir = os.environ.get("WINDIR")
    if windir and (pathlib.Path(windir) / "System32" / name).is_file():
        return True
    system32 = pathlib.Path("/c/Windows/System32") / name
    return system32.is_file()


def component_for_library(name: str) -> tuple[str, str]:
    lower = name.lower()
    if lower == "mpv":
        return "mpv", "GPL-2.0-or-later"
    if lower.startswith(("libav", "libsw", "avcodec-", "avdevice-", "avfilter-", "avformat-", "avutil-", "swresample-", "swscale-")):
        return "FFmpeg", "LGPL-3.0-or-later (configured with --enable-version3 and without --enable-gpl)"
    if lower.startswith(("libopenjoc", "openjoc_capi")):
        return "OpenJOC", "Apache-2.0"
    known = {
        "libx11": ("libX11", "MIT"),
        "libxau": ("libXau", "MIT"),
        "libxdmcp": ("libXdmcp", "MIT"),
        "libxfixes": ("libXfixes", "MIT"),
        "libxcb": ("libxcb", "MIT"),
        "libplacebo": ("libplacebo", "LGPL-2.1-or-later"),
        "libass": ("libass", "ISC"),
        "libfreetype": ("FreeType", "FTL"),
        "libharfbuzz": ("HarfBuzz", "MIT"),
        "libfribidi": ("FriBidi", "GPL-2.0-or-later AND LGPL-2.1-or-later"),
        "liblua": ("Lua", "MIT"),
        "libluajit": ("LuaJIT", "MIT"),
        "libunibreak": ("libunibreak", "Zlib"),
        "libpng": ("libpng", "libpng-2.0"),
        "libz": ("zlib", "Zlib"),
        "libbz2": ("bzip2", "bzip2 license"),
        "libexpat": ("Expat", "MIT"),
        "libfontconfig": ("fontconfig", "MIT"),
        "libuuid": ("util-linux", "BSD-3-Clause"),
        "libblkid": ("util-linux", "LGPL-2.1-or-later"),
        "libcap": ("libcap", "BSD-3-Clause"),
        "liblzma": ("XZ Utils", "Public Domain AND LGPL-2.1-or-later"),
        "liblz4": ("LZ4", "BSD-2-Clause"),
        "libgcrypt": ("Libgcrypt", "LGPL-2.1-or-later"),
        "libgpg-error": ("libgpg-error", "LGPL-2.1-or-later"),
        "libunistring": ("libunistring", "LGPL-3.0-or-later"),
        "libidn2": ("libidn2", "GPL-3.0-or-later"),
        "libtasn1": ("libtasn1", "LGPL-2.1-or-later"),
        "libiconv": ("GNU libiconv", "LGPL-2.1-or-later"),
        "libshaderc": ("shaderc", "Apache-2.0"),
        "libspirv": ("SPIRV-Headers/tools", "Apache-2.0"),
        "libvulkan": ("Vulkan-Loader", "Apache-2.0"),
        "libglib": ("GLib", "LGPL-2.1-or-later"),
        "libgraphite2": ("Graphite2", "MIT OR MPL-2.0 OR LGPL-2.1-or-later OR GPL-2.0-or-later"),
        "libintl": ("GNU gettext runtime", "GPL-3.0-or-later AND LGPL-2.1-or-later"),
        "libjpeg": ("jpeg-turbo", "IJG AND Zlib AND BSD-3-Clause"),
        "liblcms2": ("Little CMS", "MIT"),
        "libpcre2": ("PCRE2", "BSD-3-Clause"),
        "libgcc": ("GCC runtime", "GPL-3.0-or-later WITH GCC-exception-3.1"),
        "libstdc++": ("GNU libstdc++ runtime", "GPL-3.0-or-later WITH GCC-exception-3.1"),
        "libwinpthread": ("winpthreads", "MIT"),
        "libsdl2": ("SDL2", "Zlib"),
        "sdl2": ("SDL2", "Zlib"),
        "libzstd": ("Zstandard", "BSD-3-Clause"),
        "libbrotli": ("Brotli", "MIT"),
        "libtiff": ("libtiff", "libtiff license"),
        "libxml2": ("libxml2", "MIT"),
        "libgobject": ("GLib", "LGPL-2.1-or-later"),
        "libgio": ("GLib", "LGPL-2.1-or-later"),
        "libffi": ("libffi", "MIT"),
        "libmount": ("util-linux", "LGPL-2.1-or-later"),
        "libselinux": ("SELinux", "Public Domain AND MIT"),
        "libp11-kit": ("p11-kit", "MIT"),
        "libgnutls": ("GnuTLS", "LGPL-2.1-or-later"),
        "libnettle": ("Nettle", "LGPL-2.1-or-later"),
        "libhogweed": ("Nettle", "LGPL-2.1-or-later"),
        "libgmp": ("GMP", "LGPL-3.0-or-later"),
        "libatomic": ("GCC runtime", "GPL-3.0-or-later WITH GCC-exception-3.1"),
        "libgomp": ("GCC runtime", "GPL-3.0-or-later WITH GCC-exception-3.1"),
        "libssp": ("GCC runtime", "GPL-3.0-or-later WITH GCC-exception-3.1"),
        "libquadmath": ("GCC runtime", "GPL-3.0-or-later WITH GCC-exception-3.1"),
        "libwayland": ("Wayland", "MIT"),
        "libdecor": ("libdecor", "MIT"),
        "libdrm": ("libdrm", "MIT"),
        "libgbm": ("Mesa", "MIT"),
        "libpipewire": ("PipeWire", "MIT"),
        "libpulse": ("PulseAudio", "LGPL-2.1-or-later"),
        "libxkbcommon": ("xkbcommon", "MIT"),
        "libflac": ("FLAC", "BSD-3-Clause"),
        "libxpresent": ("libXpresent", "MIT"),
        "libapparmor": ("AppArmor", "LGPL-2.1-or-later"),
        "libasyncns": ("libasyncns", "LGPL-2.1-or-later"),
        "libbsd": ("libbsd", "BSD-3-Clause"),
        "libdbus-1": ("D-Bus", "AFL-2.1 OR GPL-2.0-or-later"),
        "libmd": ("libmd", "BSD-2-Clause"),
        "libmp3lame": ("LAME", "LGPL-2.1-or-later"),
        "libmpg123": ("mpg123", "LGPL-2.1-or-later"),
        "libogg": ("Ogg", "BSD-3-Clause"),
        "libopus": ("Opus", "BSD-3-Clause"),
        "libsndfile": ("libsndfile", "LGPL-2.1-or-later"),
        "libsystemd": ("systemd", "LGPL-2.1-or-later"),
        "libvorbis": ("Vorbis", "BSD-3-Clause"),
        "libvorbisenc": ("Vorbis", "BSD-3-Clause"),
        "libxinerama": ("libXinerama", "MIT"),
        "libxrandr": ("libXrandr", "MIT"),
        "libxrender": ("libXrender", "MIT"),
        "libxext": ("libXext", "MIT"),
        "libxss": ("libXss", "MIT"),
        "libxv": ("libXv", "MIT"),
        "libxxf86vm": ("libXxf86vm", "MIT"),
        "libvulkan": ("Vulkan-Loader", "Apache-2.0"),
        "vulkan-1": ("Vulkan-Loader", "Apache-2.0"),
        "libbluray": ("libbluray", "LGPL-2.1-or-later"),
        "libcaca": ("libcaca", "WTFPL-2.0"),
        "libdovi": ("libdovi", "BSD-3-Clause"),
        "libva": ("libva", "MIT"),
        "lua51": ("Lua", "MIT"),
        "zlib1": ("zlib", "Zlib"),
    }
    for prefix, value in known.items():
        if lower.startswith(prefix):
            return value
    return name, "REQUIRES_RELEASE_LICENSE_REVIEW"


def verify_linux_dependency_closure(root: pathlib.Path, executable: pathlib.Path) -> None:
    owners = [executable, *sorted((root / "lib").iterdir())]
    for owner in owners:
        if not owner.is_file() or owner.is_symlink():
            continue
        if owner.suffix not in {"", ".so", ".dylib"} and ".so." not in owner.name:
            continue
        dynamic = run(["readelf", "-d", str(owner)])
        if "$ORIGIN" not in dynamic:
            raise SystemExit(f"package verification: bundled ELF lacks $ORIGIN RUNPATH: {owner.name}")
        for name, resolved in parse_ldd(owner):
            if linux_system_dependency(name):
                continue
            if resolved is None or not resolved.is_file():
                raise SystemExit(f"package verification: unresolved ELF dependency {name} from {owner.name}")
            resolved = resolved.resolve()
            if root not in resolved.parents:
                raise SystemExit(
                    f"package verification: non-system ELF dependency escapes bundle: {name} -> {resolved}"
                )


def verify_windows_dependency_closure(root: pathlib.Path, executable: pathlib.Path) -> None:
    owners = [executable, *sorted((root / "bin").glob("*.dll"))]
    for owner in owners:
        for name in pe_imports(owner):
            if windows_system_dependency(name):
                continue
            if find_case_insensitive([root / "bin"], name) is None:
                raise SystemExit(f"package verification: missing bundled PE dependency {name} from {owner.name}")


def homebrew_license_evidence(licenses: pathlib.Path) -> pathlib.Path | None:
    brew = shutil.which("brew")
    if brew is None:
        return None
    formulas = [
        "libass", "libplacebo", "libx11", "libxau", "libxdmcp", "libxfixes",
        "libxcb", "freetype", "fribidi", "harfbuzz", "glib", "graphite2",
        "gettext", "jpeg-turbo", "little-cms2", "pcre2", "vulkan-loader",
        "libunibreak", "libpng", "shaderc",
    ]
    result = subprocess.run(
        [brew, "info", "--json=v2", *formulas],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if result.returncode != 0:
        return None
    try:
        data = json.loads(result.stdout)
    except json.JSONDecodeError:
        return None
    destination = licenses / "third-party/HOMEBREW_FORMULA_LICENSES.txt"
    destination.parent.mkdir(parents=True, exist_ok=True)
    lines = [
        "Homebrew formula license evidence for the exact macOS build closure",
        "Generated from `brew info --json=v2`; this is provenance, not a legal opinion.",
        "",
    ]
    for formula in sorted(data.get("formulae", []), key=lambda item: item.get("name", "")):
        lines.append(
            f"{formula.get('name')}: license={formula.get('license')}; "
            f"homepage={formula.get('homepage')}; version={formula.get('versions', {}).get('stable')}"
        )
    destination.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return destination


def copy_license_evidence(
    licenses: pathlib.Path,
    ffmpeg_source: pathlib.Path | None,
    mpv_source: pathlib.Path | None,
) -> list[dict[str, str]]:
    evidence: list[dict[str, str]] = []
    targets = [
        ("openjoc/LICENSE.txt", OPENJOC_LICENSE),
        ("openjoc/THIRD_PARTY_NOTICES.md", OPENJOC_NOTICES),
    ]
    if ffmpeg_source:
        for name in ("COPYING.LGPLv3", "COPYING.LGPLv2.1", "LICENSE.md"):
            candidate = ffmpeg_source / name
            if candidate.is_file():
                targets.append((f"ffmpeg/{name}", candidate))
    if mpv_source:
        for name in ("LICENSE.GPL", "LICENSE", "Copyright"):
            candidate = mpv_source / name
            if candidate.is_file():
                targets.append((f"mpv/{name}", candidate))
    for relative, source in targets:
        destination = licenses / relative
        copy_file(source, destination)
        evidence.append({"path": f"licenses/{relative}", "source": str(source), "sha256": sha256(destination)})
    formula_evidence = homebrew_license_evidence(licenses)
    if formula_evidence:
        evidence.append({"path": "licenses/third-party/HOMEBREW_FORMULA_LICENSES.txt", "source": "brew info --json=v2", "sha256": sha256(formula_evidence)})
    return evidence


def sanitize_private_strings(root: pathlib.Path, extra_prefixes: list[pathlib.Path]) -> None:
    """Remap compiler/package-manager path strings without changing binary size.

    Release binaries are built from external temporary prefixes and may carry
    diagnostic/default-data strings from the toolchain. Replace only known
    local prefixes with the stable `/build` marker, preserving Mach-O/ELF/PE
    offsets. Loader paths are handled separately by the dependency rewriter.
    """
    prefixes: list[bytes] = []
    for value in [REPOSITORY, *extra_prefixes]:
        text = str(value)
        for spelling in {text, text.replace("/", "\\"), text.replace("\\", "/")}:
            prefixes.append(spelling.encode())
    prefixes.extend([
        b"/private/tmp/openjoc-player-",
        b"/tmp/openjoc-player-",
        b"C:/Users/runneradmin",
        b"C:\\Users\\runneradmin",
        b"D:/a",
        b"D:\\a",
        b"/opt/homebrew",
        b"/usr/local/Cellar",
    ])
    for path in [
        p for p in root.rglob("*")
        if p.is_file()
        and (p.name in {"mpv", "mpv.exe"} or p.suffix in {".dylib", ".so", ".dll"} or ".so." in p.name)
    ]:
        data = path.read_bytes()
        for prefix in prefixes:
            if prefix not in data:
                continue
            replacement = b"/build"
            if len(replacement) > len(prefix):
                continue
            replacement += b"\0" * (len(prefix) - len(replacement))
            data = data.replace(prefix, replacement)
        path.write_bytes(data)


def refresh_dependency_records(root: pathlib.Path, records: list[dict[str, object]]) -> None:
    for item in records:
        path = root / str(item["path"])
        if path.is_file():
            item["sha256"] = sha256(path)
            item["size"] = path.stat().st_size


def sign_macos_runtime(root: pathlib.Path) -> bool:
    codesign = shutil.which("codesign")
    if codesign is None:
        raise RuntimeError("macOS packaging requires codesign for the post-rewrite ad-hoc runtime signature")
    paths = [root / "bin/mpv", *sorted((root / "lib").glob("*.dylib"))]
    for path in paths:
        run([codesign, "--force", "--sign", "-", str(path)])
    return True


def write_notices(
    path: pathlib.Path,
    dependencies: list[dict[str, object]],
    evidence: list[dict[str, str]],
) -> None:
    lines = [
        "OpenJOC Player Bundle — component redistribution notices",
        "",
        "This file records engineering provenance for the exact binaries shipped",
        "in this bundle. It is not a legal opinion. Verify source offers, notices,",
        "and license obligations before any public release.",
        "",
        "OpenJOC is Apache-2.0. The bundled SADIE II D1 resource attribution is",
        "included under licenses/openjoc/THIRD_PARTY_NOTICES.md.",
        "",
        "Shipped runtime components:",
    ]
    for item in dependencies:
        lines.append(
            f"- {item['path']}: {item['component']} — {item['license']} "
            f"[{item['kind']}]"
        )
    lines.extend(["", "License/source evidence shipped:"])
    for item in evidence:
        lines.append(f"- {item['path']} (sha256 {item['sha256']})")
    lines.extend([
        "",
        "FFmpeg was configured with --enable-version3 and without --enable-gpl",
        "by the package recipe. The actual configure command and dependency set",
        "are recorded in BUILD_INFO.json and DEPENDENCIES.json.",
        "",
        "Any dependency marked REQUIRES_RELEASE_LICENSE_REVIEW prevents release",
        "qualification until its source license and redistribution terms are",
        "verified and added to the package evidence set.",
    ])
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def deterministic_tar(source_root: pathlib.Path, destination: pathlib.Path, epoch: int) -> None:
    with destination.open("wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=epoch, compresslevel=9) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT) as archive:
                paths = [source_root, *sorted(source_root.rglob("*"), key=lambda p: p.relative_to(source_root).as_posix())]
                for path in paths:
                    name = source_root.name if path == source_root else f"{source_root.name}/{path.relative_to(source_root).as_posix()}"
                    info = archive.gettarinfo(str(path), arcname=name)
                    info.uid = 0
                    info.gid = 0
                    info.uname = "root"
                    info.gname = "wheel"
                    info.mtime = epoch
                    if path.is_file():
                        info.mode = executable_mode(path)
                        with path.open("rb") as stream:
                            archive.addfile(info, stream)
                    else:
                        info.mode = 0o755
                        archive.addfile(info)


def deterministic_zip(source_root: pathlib.Path, destination: pathlib.Path) -> None:
    with zipfile.ZipFile(destination, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for path in sorted((p for p in source_root.rglob("*") if p.is_file()), key=lambda p: p.relative_to(source_root).as_posix()):
            name = f"{source_root.name}/{path.relative_to(source_root).as_posix()}"
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = executable_mode(path) << 16
            archive.writestr(info, path.read_bytes())


def bundle(arguments: argparse.Namespace) -> int:
    manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
    platform_name = arguments.platform
    output = arguments.output.resolve()
    if output == REPOSITORY or REPOSITORY in output.parents:
        raise SystemExit("player artifact output must be outside the source repository")
    output.mkdir(parents=True, exist_ok=True)
    stage = arguments.stage_root.resolve()
    executable_name = "mpv.exe" if platform_name == "windows-x64" else "mpv"
    staged_executable = stage / "bin" / executable_name
    if not staged_executable.is_file():
        raise SystemExit(f"staged player executable missing: {staged_executable}")
    if any(output.iterdir()):
        raise SystemExit(f"output directory must be empty: {output}")
    dev_id = development_id()
    suffix = {"macos-arm64": "macos-arm64", "linux-x86_64": "linux-x86_64", "windows-x64": "windows-x64"}[platform_name]
    archive_name = f"openjoc-mpv-{dev_id}-{suffix}." + ("zip" if platform_name == "windows-x64" else "tar.gz")
    root_name = archive_name.removesuffix(".tar.gz").removesuffix(".zip")
    with tempfile.TemporaryDirectory(prefix="openjoc-player-package-") as temporary:
        work = pathlib.Path(temporary)
        root = work / root_name
        (root / "bin").mkdir(parents=True)
        (root / "lib").mkdir(parents=True)
        (root / "config").mkdir(parents=True)
        (root / "licenses").mkdir(parents=True)
        copy_file(staged_executable, root / "bin" / executable_name)
        (root / "bin" / executable_name).chmod(0o755)
        copy_file(QUICKSTART_PATH, root / "QUICKSTART.md")
        copy_file(PROFILES_PATH, root / "config/profiles.conf")
        (root / "config/mpv.conf").write_text(
            "# Portable OpenJOC Player Bundle config. Profiles are opt-in.\n",
            encoding="utf-8",
        )
        if platform_name == "windows-x64":
            (root / "bin/openjoc-mpv.cmd").write_text(
                "@echo off\r\nset \"OPENJOC_PLAYER_ROOT=%~dp0..\"\r\n\"%OPENJOC_PLAYER_ROOT%\\bin\\mpv.exe\" \"--config-dir=%OPENJOC_PLAYER_ROOT%\\config\" \"--include=%OPENJOC_PLAYER_ROOT%\\config\\profiles.conf\" %*\r\n",
                encoding="utf-8",
            )
        else:
            launcher = root / "bin/openjoc-mpv"
            launcher.write_text(
                "#!/bin/sh\nset -eu\nhere=$(CDPATH= cd -- \"$(dirname -- \"$0\")/..\" && pwd)\nexec \"$here/bin/mpv\" \"--config-dir=$here/config\" \"--include=$here/config/profiles.conf\" \"$@\"\n",
                encoding="utf-8",
            )
            launcher.chmod(0o755)
        source_dirs = [stage / "lib", *(pathlib.Path(value) for value in arguments.search_dir)]
        source_dirs = [path for path in source_dirs if path.is_dir()]
        if platform_name == "macos-arm64":
            records, external = collect_macos(root / "bin/mpv", root / "lib", source_dirs)
        elif platform_name == "linux-x86_64":
            records, external = collect_linux(root / "bin/mpv", root / "lib")
        else:
            records, external = collect_windows(root / "bin/mpv.exe", root / "bin", source_dirs)
        sanitize_private_strings(root, [pathlib.Path(value) for value in arguments.private_prefix])
        ad_hoc_signed = sign_macos_runtime(root) if platform_name == "macos-arm64" else False
        refresh_dependency_records(root, records)
        for item in records:
            # Absolute build/prefix paths are maintainer-local evidence and must
            # never become public-facing bundle metadata. The actual file hash,
            # size, component, and license remain recorded below.
            item.pop("source", None)
        for path in sorted((root / "lib").glob("*")):
            if path.is_file() and not any(item["path"] == f"lib/{path.name}" for item in records):
                records.append({"path": f"lib/{path.name}", "sha256": sha256(path), "size": path.stat().st_size, "kind": "bundled", "component": "runtime asset"})
        dependencies: list[dict[str, object]] = []
        for item in sorted(records, key=lambda value: str(value["path"])):
            component, license_name = component_for_library(pathlib.Path(str(item["path"])).name)
            item["component"] = component
            item["license"] = license_name
            dependencies.append(item)
        dependencies.insert(0, {
            "path": f"bin/{executable_name}",
            "sha256": sha256(root / "bin" / executable_name),
            "size": (root / "bin" / executable_name).stat().st_size,
            "kind": "bundled",
            "component": "mpv",
            "license": "GPL-2.0-or-later",
        })
        evidence = copy_license_evidence(root / "licenses", pathlib.Path(arguments.ffmpeg_source).resolve() if arguments.ffmpeg_source else None, pathlib.Path(arguments.mpv_source).resolve() if arguments.mpv_source else None)
        write_notices(root / "THIRD_PARTY_NOTICES.txt", dependencies, evidence)
        dependency_manifest = {
            "schema": "openjoc.player-dependencies.v1",
            "bundled": dependencies,
            "external": [{"name": name, "kind": "external-runtime"} for name in external],
            "license_review_required": any(item["license"] == "REQUIRES_RELEASE_LICENSE_REVIEW" for item in dependencies),
        }
        write_json(root / "DEPENDENCIES.json", dependency_manifest)
        build_info = {
            "schema": "openjoc.player-build-info.v1",
            "product": manifest["product"],
            "source": {
                "openjoc_git_commit": git_value("rev-parse", "HEAD"),
                "openjoc_version": manifest["openjoc"]["version"],
                "openjoc_c_abi": manifest["openjoc"]["c_abi"],
                "dirty_tracked_worktree": bool(git_value("status", "--porcelain", "--untracked-files=no")),
            },
            "pinned_stack": manifest["pinned_stack"],
            "target": platform_name,
            "architecture": platform.machine(),
            "toolchain": arguments.toolchain,
            "build_timestamp_utc": arguments.build_timestamp,
            "reproducible_archive": bool(os.environ.get("SOURCE_DATE_EPOCH")),
            "archive_timestamp_policy": "SOURCE_DATE_EPOCH or zero" if os.environ.get("SOURCE_DATE_EPOCH") else "wall-clock build timestamp recorded; archive is deterministic within this invocation only",
            "binary_path_remap": "known local source/package prefixes remapped to /build before hashing",
            "enabled_openjoc_features": manifest["openjoc"]["features"],
            "profiles": manifest["profiles"],
            "runtime_dependency_inventory": dependency_manifest,
            "configure": {"ffmpeg": manifest["pinned_stack"]["ffmpeg"]["configure_flags"], "mpv": ["-Dtests=false", "-Dmanpage-build=disabled", "-Dhtml-build=disabled", "-Dpdf-build=disabled"]},
            "signing": {"developer_id_signed": False, "notarized": False, "ad_hoc_only_if_required": True, "ad_hoc_signed": ad_hoc_signed},
            "verification": {
                "network_required_at_runtime": False,
                "source_repository_required_at_runtime": False,
                "built_in_hrtf": "embedded in libopenjoc_capi",
                "built_in_hrtf_resource": builtin_hrtf_evidence(),
            },
        }
        write_json(root / "BUILD_INFO.json", build_info)
        (root / "BUILD_INFO.txt").write_text(
            "OpenJOC Player Bundle\n\n"
            f"Target: {platform_name}\n"
            f"OpenJOC commit: {build_info['source']['openjoc_git_commit']}\n"
            f"OpenJOC version / C ABI: {build_info['source']['openjoc_version']} / {build_info['source']['openjoc_c_abi']['major']}.{build_info['source']['openjoc_c_abi']['minor']}\n"
            f"FFmpeg: {manifest['pinned_stack']['ffmpeg']['tag']} at {manifest['pinned_stack']['ffmpeg']['commit']}\n"
            f"mpv: {manifest['pinned_stack']['mpv']['tag']} at {manifest['pinned_stack']['mpv']['commit']}\n"
            f"Archive reproducible mode: {build_info['reproducible_archive']}\n"
            f"Signing: {'ad-hoc signed where required' if ad_hoc_signed else 'unsigned'}; no Developer ID identity; not notarized\n"
            "Runtime: no network or source tree required; ordinary E-AC-3 remains stock eac3; confirmed JOC selects libopenjoc.\n"
            "See BUILD_INFO.json, DEPENDENCIES.json, and THIRD_PARTY_NOTICES.txt for the complete machine-readable audit.\n",
            encoding="utf-8",
        )
        checksums = []
        for path in sorted((p for p in root.rglob("*") if p.is_file()), key=lambda p: p.relative_to(root).as_posix()):
            if path.name == "SHA256SUMS":
                continue
            checksums.append(f"{sha256(path)}  {path.relative_to(root).as_posix()}")
        (root / "SHA256SUMS").write_text("\n".join(checksums) + "\n", encoding="utf-8")
        archive_path = output / archive_name
        if platform_name == "windows-x64":
            deterministic_zip(root, archive_path)
        else:
            deterministic_tar(root, archive_path, source_date_epoch())
        outer_manifest = {
            "schema": "openjoc.player-artifact-manifest.v1",
            "archive": archive_name,
            "archive_sha256": sha256(archive_path),
            "archive_size": archive_path.stat().st_size,
            "target": platform_name,
            "development_id": dev_id,
            "build_info_sha256": sha256(root / "BUILD_INFO.json"),
            "package_files": sorted(path.relative_to(root).as_posix() for path in root.rglob("*") if path.is_file()),
            "license_review_required": dependency_manifest["license_review_required"],
            "publication_status": "LOCAL_OR_CI_ONLY_UNPUBLISHED",
        }
        manifest_path = output / f"{archive_name.rsplit('.', 2)[0]}.manifest.json"
        write_json(manifest_path, outer_manifest)
        checksum_path = output / f"{archive_name.rsplit('.', 2)[0]}.SHA256SUMS"
        checksum_path.write_text(
            f"{sha256(archive_path)}  {archive_name}\n{sha256(manifest_path)}  {manifest_path.name}\n",
            encoding="utf-8",
        )
    print(json.dumps({"archive": str(archive_path), "manifest": str(manifest_path), "checksums": str(checksum_path), "license_review_required": dependency_manifest["license_review_required"]}, indent=2, sort_keys=True))
    return 0


def verify(arguments: argparse.Namespace) -> int:
    root = arguments.root.resolve()
    launcher = "bin/openjoc-mpv.cmd" if arguments.platform == "windows-x64" else "bin/openjoc-mpv"
    required = [
        "BUILD_INFO.json", "BUILD_INFO.txt", "DEPENDENCIES.json",
        "THIRD_PARTY_NOTICES.txt", "SHA256SUMS", "QUICKSTART.md",
        "config/mpv.conf", "config/profiles.conf", launcher,
        "licenses/openjoc/LICENSE.txt", "licenses/openjoc/THIRD_PARTY_NOTICES.md",
    ]
    executable = root / ("bin/mpv.exe" if arguments.platform == "windows-x64" else "bin/mpv")
    required.append(executable.relative_to(root).as_posix())
    for relative in required:
        path = root / relative
        if not path.is_file() or path.is_symlink():
            raise SystemExit(f"package verification: missing or symlinked file {relative}")
    for line in (root / "SHA256SUMS").read_text(encoding="utf-8").splitlines():
        digest, relative = line.split("  ", 1)
        path = root / relative
        if not path.is_file() or sha256(path) != digest:
            raise SystemExit(f"package verification: checksum mismatch {relative}")
    build_info = json.loads((root / "BUILD_INFO.json").read_text(encoding="utf-8"))
    if build_info["source"]["openjoc_c_abi"]["packed_hex"] != "0x00010003":
        raise SystemExit("package verification: OpenJOC C ABI is not 1.3")
    if build_info.get("target") != arguments.platform:
        raise SystemExit("package verification: BUILD_INFO target does not match verifier platform")
    if not build_info.get("pinned_stack", {}).get("ffmpeg", {}).get("commit"):
        raise SystemExit("package verification: pinned FFmpeg commit is missing from BUILD_INFO")
    if not build_info.get("pinned_stack", {}).get("mpv", {}).get("commit"):
        raise SystemExit("package verification: pinned mpv commit is missing from BUILD_INFO")
    if build_info.get("verification", {}).get("built_in_hrtf_resource") != builtin_hrtf_evidence():
        raise SystemExit("package verification: built-in SADIE HRTF resource identity mismatch")
    dependencies = json.loads((root / "DEPENDENCIES.json").read_text(encoding="utf-8"))
    if dependencies["license_review_required"]:
        raise SystemExit("package verification: unresolved dependency license review")
    if arguments.platform == "macos-arm64":
        if shutil.which("file"):
            output = run(["file", str(executable)])
            if "arm64" not in output:
                raise SystemExit(f"package verification: unexpected Mach-O architecture: {output.strip()}")
        for path in [executable, *sorted((root / "lib").glob("*.dylib"))]:
            for dependency in parse_macho_dependencies(path):
                if dependency.startswith("/") and not macho_system_dependency(dependency):
                    raise SystemExit(f"package verification: absolute non-system Mach-O dependency {dependency} in {path.name}")
    elif arguments.platform == "linux-x86_64":
        output = run(["file", str(executable)])
        if "x86-64" not in output:
            raise SystemExit(f"package verification: unexpected ELF architecture: {output.strip()}")
        dynamic = run(["readelf", "-d", str(executable)])
        if "$ORIGIN" not in dynamic:
            raise SystemExit("package verification: executable lacks $ORIGIN RUNPATH")
        verify_linux_dependency_closure(root, executable)
    else:
        if not shutil.which("objdump"):
            raise SystemExit("package verification: Windows PE audit requires MinGW objdump")
        output = run(["objdump", "-f", str(executable)])
        if "pei-x86-64" not in output.lower():
            raise SystemExit(f"package verification: unexpected PE architecture: {output.strip()}")
        verify_windows_dependency_closure(root, executable)
    forbidden = [str(root), str(REPOSITORY), *PRIVATE_MARKERS, "target/debug", "target\\debug"]
    forbidden.extend([str(root).replace("/", "\\"), str(REPOSITORY).replace("/", "\\")])
    for path in [p for p in root.rglob("*") if p.is_file()]:
        if path.stat().st_size > 32 * 1024 * 1024:
            continue
        data = path.read_bytes()
        for marker in forbidden:
            if marker.encode() in data:
                raise SystemExit(f"package verification: private/build path leak {marker} in {path.relative_to(root)}")
    env = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "HOME": tempfile.gettempdir(), "LC_ALL": "C"}
    if arguments.run_smoke:
        version = subprocess.run([str(executable), "--version"], cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
        if version.returncode != 0:
            raise SystemExit(f"package verification: packaged mpv --version failed\n{version.stdout}")
        help_result = subprocess.run([str(executable), f"--config-dir={root / 'config'}", "--ad=help"], cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
        if help_result.returncode != 0 or "libopenjoc" not in help_result.stdout or "eac3" not in help_result.stdout:
            raise SystemExit(f"package verification: decoder visibility failed\n{help_result.stdout}")
    if arguments.missing_dependency_smoke:
        with tempfile.TemporaryDirectory(prefix="openjoc-player-missing-dependency-") as temporary:
            isolated = pathlib.Path(temporary) / root.name
            shutil.copytree(root, isolated)
            candidate_root = isolated / ("bin" if arguments.platform == "windows-x64" else "lib")
            candidates = sorted(candidate_root.glob("libopenjoc_capi.*"))
            if arguments.platform == "windows-x64":
                candidates = sorted(candidate_root.glob("openjoc_capi.dll"))
            if not candidates:
                raise SystemExit("package verification: no OpenJOC library available for missing-dependency smoke")
            missing = candidates[0]
            missing.rename(missing.with_suffix(missing.suffix + ".missing"))
            isolated_executable = isolated / executable.relative_to(root)
            failure = subprocess.run([str(isolated_executable), "--version"], cwd=isolated, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
            if failure.returncode == 0 or not re.search(r"(not loaded|cannot open|missing|no such file|loadlibrary|dll)", failure.stdout, re.IGNORECASE):
                raise SystemExit(f"package verification: missing-dependency smoke was not understandable\n{failure.stdout}")
    print(f"player package verification: PASS platform={arguments.platform} root={root}")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    bundle_parser = subparsers.add_parser("bundle")
    bundle_parser.add_argument("--stage-root", type=pathlib.Path, required=True)
    bundle_parser.add_argument("--output", type=pathlib.Path, required=True)
    bundle_parser.add_argument("--platform", choices=["macos-arm64", "linux-x86_64", "windows-x64"], required=True)
    bundle_parser.add_argument("--search-dir", action="append", default=[])
    bundle_parser.add_argument("--ffmpeg-source")
    bundle_parser.add_argument("--mpv-source")
    bundle_parser.add_argument("--toolchain", default="not supplied")
    bundle_parser.add_argument("--private-prefix", action="append", default=[])
    bundle_parser.add_argument("--build-timestamp", default=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()))
    bundle_parser.set_defaults(function=bundle)
    verify_parser = subparsers.add_parser("verify")
    verify_parser.add_argument("--root", type=pathlib.Path, required=True)
    verify_parser.add_argument("--platform", choices=["macos-arm64", "linux-x86_64", "windows-x64"], required=True)
    verify_parser.add_argument("--run-smoke", action="store_true")
    verify_parser.add_argument("--missing-dependency-smoke", action="store_true")
    verify_parser.set_defaults(function=verify)
    arguments = parser.parse_args()
    try:
        return arguments.function(arguments)
    except (RuntimeError, OSError) as error:
        print(f"player packaging error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
