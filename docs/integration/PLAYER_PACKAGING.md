# OpenJOC Player Bundle packaging

This document describes the maintainer and CI path for the portable
OpenJOC-enabled mpv builds. It is packaging documentation, not an upstream mpv
or FFmpeg release announcement. The product wording is deliberately
“OpenJOC-enabled mpv build” / “OpenJOC Player Bundle”.

## Scope and architecture

The closed playback stack is:

```text
OpenJOC renderer and C ABI 1.3
        ↓
patched FFmpeg 9.0.1 native libavcodec wrapper (libopenjoc)
        ↓
patched mpv 0.41.0 bounded positive JOC classifier
        ↓
normal mpv PCM transport and audio output
```

Packaging does not change the decoder, renderer, layout mathematics, HRTF,
normalization, DRC, dialnorm, or video pipeline. All platforms use the same
OpenJOC renderer. Only executable format, loader model, path layout, and
normal mpv audio/video backends vary.

The canonical contract is
[`packaging/player/PLAYER_PACKAGE_MANIFEST.json`](../../packaging/player/PLAYER_PACKAGE_MANIFEST.json).
It pins FFmpeg `n9.0.1` at
`bf1b838f2ab88b4f8fd83443325c782ea0e0f7fa`, mpv `v0.41.0` at
`41f6a645068483470267271e1d09966ca3b9f413`, both OpenJOC patch hashes, ABI
1.3, archive names, profiles, loader policy, and external-runtime policy.

## Qualified artifact surface

The qualified player surface is a portable OpenJOC 0.9.1 package for macOS
arm64, Linux x86_64, and Windows x64. The package is not a `.app`, DMG,
installer, or official mpv/FFmpeg distribution. Linux x86_64 and Windows x64
are qualified by the dedicated `player-packaging.yml` jobs on native runners;
a local macOS cross-toolchain is not Windows evidence.

The maintainer entry point is:

```sh
SOURCE_DATE_EPOCH=0 scripts/build-openjoc-player.sh \
  --platform macos-arm64 \
  --release \
  --output /absolute/path/out
```

The entry point fetches exact source commits into temporary worktrees, checks
the patch SHA-256 values, requires `git apply --check` to pass, builds the
OpenJOC C ABI, builds FFmpeg with the recorded flags, builds patched mpv, and
packages an extracted runtime closure. Build worktrees and prefixes stay
outside the repository.

`--release` is required for final archive names such as
`openjoc-mpv-0.9.1-macos-arm64.tar.gz`. Without it, the same build machinery
uses a development `0.9.1-git<commit>` name. Neither mode publishes anything.

For Windows CI, the equivalent MSYS2/MinGW-w64 entry point is
`scripts/build-openjoc-player-windows.sh`. It builds the GNU Rust target,
patched FFmpeg, and patched mpv, then places all non-system DLLs beside
`mpv.exe`. It also preserves the upstream `mpv.com` console wrapper beside the
GUI executable; `openjoc-mpv.cmd` invokes `mpv.com` so console attachment and
exit-status propagation follow upstream Windows behavior. The canonical job uses a GitHub `windows-2025` runner, the MSYS2
`MINGW64` shell, `x86_64-pc-windows-gnu`, `x86_64-w64-mingw32-gcc`, and
`pkg-config` with the staged OpenJOC/FFmpeg prefixes first. `pacman -Q` is
retained as a workflow artifact so the actual MSYS2 package set is recorded.
It does not require Rust, FFmpeg, or MSYS2 on an end-user machine.

## Runtime layout

```text
bin/mpv.exe             patched GUI player executable (Windows)
bin/mpv.com             upstream console wrapper (Windows)
bin/openjoc-mpv.cmd     Windows console launcher with bundle config/profile include
bin/mpv                 patched player executable (macOS/Linux)
bin/openjoc-mpv         relocatable launcher with bundle config/profile include (macOS/Linux)
lib/                    macOS/Linux private shared-library closure
config/mpv.conf         neutral portable config
config/profiles.conf    opt-in OpenJOC output profiles
licenses/               OpenJOC, SADIE, mpv, FFmpeg, and closure evidence
BUILD_INFO.json/.txt    resolved source/toolchain/feature/signing metadata
DEPENDENCIES.json       bundled and external dependency inventory
THIRD_PARTY_NOTICES.txt component notices for this exact bundle
SHA256SUMS              inner bundle checksum manifest
```

Windows keeps runtime DLLs in `bin/` because the Windows loader naturally
searches the executable directory. `mpv.exe` remains the GUI/Explorer entry;
`mpv.com` is the upstream console entry, and `openjoc-mpv.cmd` injects the
portable config/profile paths before forwarding the child exit status. The
launcher has no filename-extension or demux policy. Positive raw-JOC admission
is implemented inside the patched lavf demux path from the non-destructive
probe buffer. No registry or global `PATH` change is performed.

The extracted Windows acceptance matrix checks direct `mpv.com --version`,
`openjoc-mpv.cmd --version`, `openjoc-mpv.cmd --ad=help` with `eac3` and
`libopenjoc`, synthetic JOC null-output playback, package checksums, and the
presence/PE audit of both GUI and console executables. Native Windows runners
also attempt a console interrupt smoke where the platform exposes
`CTRL_BREAK_EVENT`.

On macOS, `install_name_tool` rewrites private dependencies to `@rpath` and
adds `@loader_path/../lib` to the executable and `@loader_path` to bundled
dylibs. Mach-O is inspected with `otool -L`; Apple frameworks and system
libraries remain external. Post-rewrite ad-hoc signatures are used because
Mach-O load-command edits invalidate the existing Swift/linker signature. No
Developer ID identity or notarization is assumed or implied.

On Linux, `patchelf` sets `$ORIGIN/../lib` on the player and `$ORIGIN` on
bundled libraries. `ldd` and `readelf -d` audit the result. The initial
compatibility baseline is the CI runner’s Ubuntu 24.04/glibc floor; glibc,
libstdc++, compiler runtimes, device backends, and GPU drivers remain explicit
external requirements unless `DEPENDENCIES.json` says otherwise. The exact
glibc symbol floor and compiler/runtime strings are recorded in `BUILD_INFO.json`
for each Linux artifact; this is not a universal-Linux compatibility claim.

## End-user behavior and profiles

Running `bin/openjoc-mpv file` keeps normal mpv behavior. The patched player
uses the bounded positive classifier only for an E-AC-3 stream when no decoder
override or E-AC-3 passthrough request is active:

| Input/request | Decoder/output policy |
| --- | --- |
| ordinary E-AC-3 | stock `eac3` |
| confirmed JOC | automatic `libopenjoc` |
| `--ad=eac3` | explicit stock decoder control |
| `--ad=libopenjoc` | explicit engineering/debug control |
| `--audio-spdif=eac3` | compressed passthrough; OpenJOC bypassed |
| AAC/FLAC/MP3/video/subtitles | normal patched mpv/FFmpeg behavior |

No device-name heuristic treats two output channels as headphones. The
launcher makes these profiles available without changing a user’s global mpv
configuration:

| Profile | OpenJOC renderer meaning | Channels | HRTF |
| --- | --- | ---: | --- |
| `openjoc-headphones` | binaural from virtual 7.1.4 | 2 | yes |
| `openjoc-stereo` | physical speaker 2.0 | 2 | no |
| `openjoc-51` | physical speaker 5.1 | 6 | no |
| `openjoc-714` | physical speaker 7.1.4 | 12 | no |
| `openjoc-916` | physical speaker 9.1.6 | 16 | no |
| `openjoc-222` | physical speaker 22.2 | 24 | no |

Use `--ao=null` for deterministic multichannel verification when the hardware
cannot carry the requested layout. A physical profile is not silently changed
to binaural if the device rejects its channel map.

## Verification

The package verifier is:

```sh
scripts/verify-player-package.sh \
  --root /absolute/extracted/openjoc-mpv-... \
  --platform macos-arm64 --run-smoke --missing-dependency-smoke
```

It checks required files, inner checksums, target architecture, loader paths,
the extracted ELF/PE dependency closure, decoder visibility (`--ad=help`), ABI
metadata, license-review status, and private/local path leaks. The
missing-dependency smoke temporarily removes the OpenJOC runtime from a copy
and requires a clear loader failure.

The native-runner qualification wrapper is:

```sh
scripts/generate-player-fixtures.sh /absolute/temporary/fixtures
python3 scripts/qualify-player-package.py \
  --archive /absolute/output/openjoc-mpv-...-linux-x86_64.tar.gz \
  --platform linux-x86_64 \
  --fixtures /absolute/temporary/fixtures \
  --report /absolute/output/qualification/linux-x86_64.json
```

`generate-player-fixtures.sh` uses the project-owned synthetic JOC exporter
and deterministic lavfi codec/video controls. It retains an exact one-AU raw
regression, an eight-AU raw control, and an MP4 wrapper around the exact one-AU
bytes. The fixture directory is temporary and is never packaged or uploaded.
The qualification wrapper
extracts into a fresh directory away from source/build trees, disables network
access for runtime checks, runs the package verifier, invokes the full mpv
selection/layout/codec harness, and writes machine-readable JSON plus a
human-readable text report. Each report field is `PASS`, `FAIL`, or
`NOT_APPLICABLE`.

The existing mpv harness remains the media acceptance gate:

```sh
integrations/mpv/verify-player.sh /absolute/extracted/.../bin/mpv /path/to/external-fixtures
```

Its qualified fixture run covers ordinary E-AC-3 → `eac3`, raw single- and
multi-AU JOC → pre-confirmed `libopenjoc`, MP4 JOC → packet-classified
`libopenjoc`, exact raw-versus-MP4 first-AU PCM identity, explicit stock
decoder override, binaural null output, exact 2.0, 5.1, 7.1.4, 9.1.6, and 22.2
null-output channel counts, ordinary AAC/FLAC/MP3/AC-3 and video smoke,
seek/flush, EOS, and passthrough. Multichannel PCM generation
and transport are qualified in CI; physical speaker-system playback has not
been separately validated on Linux/Windows hardware. Programme media and
derived PCM never enter the archive.

The embedded SADIE II D1 KU100 resource is compiled into `libopenjoc_capi` and
is exercised without `--sofa`, repository-relative paths, or network access.
`BUILD_INFO.json` records the source resource SHA-256 so macOS, Linux, and
Windows bundles can prove that the same HRTF bytes were used.

## Licensing and notices

OpenJOC remains Apache-2.0. The SADIE II D1 attribution and data terms remain
in the shipped OpenJOC notice evidence. The exact FFmpeg recipe uses shared
libraries, `--enable-version3`, and no `--enable-gpl`, and the generated
package records FFmpeg’s reported LGPL version-3-or-later mode. mpv’s actual
build is recorded as GPL-2.0-or-later. Other runtime licenses are resolved from
the actual closure; unknown mappings set `license_review_required` and block
the verifier.

This is engineering provenance, not an absolute legal conclusion. Before any
future public distribution, satisfy the applicable source-availability,
corresponding-source, notices, attribution, and offer obligations for every
redistributed component. No package job uploads or publishes an artifact.

## Release constraints

No source tree, Cargo target directory, compiler cache, private media, test
PCM, commercial HRTF data, credentials, cookies, or user configuration is
copied. Development artifacts use `0.9.1-git<commit>` identifiers; final
release candidates use the exact `0.9.1` project version and record the full
OpenJOC commit in `BUILD_INFO`. macOS packages are ad-hoc signed where
required, not Developer-ID signed, and not notarized. Tagging, GitHub Release
creation, installer formats, auto-update, and upstream submission remain
explicitly outside this hardening phase.

The 0.9.1 release theme is the accumulated cross-platform player surface:
22.2 rendering, the built-in HRTF, GStreamer integration, the external FFmpeg
bridge, the native FFmpeg `libopenjoc` wrapper, mpv player integration, and
reproducible player packaging. The custom FFmpeg/mpv integrations are
project-provided builds and patches; upstream FFmpeg and mpv do not ship
OpenJOC.
