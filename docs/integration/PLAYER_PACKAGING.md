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

The first fully qualified local surface is macOS arm64. The package is a
portable `.tar.gz`; it is not a `.app`, DMG, installer, signed distribution,
or notarized release. The Linux x86_64 and Windows x64 routes are isolated in
the dedicated `player-packaging.yml` workflow and are not release claims until
their jobs pass on their target runners.

The maintainer entry point is:

```sh
SOURCE_DATE_EPOCH=0 scripts/build-openjoc-player.sh \
  --platform macos-arm64 \
  --output /absolute/path/out
```

The entry point fetches exact source commits into temporary worktrees, checks
the patch SHA-256 values, requires `git apply --check` to pass, builds the
OpenJOC C ABI, builds FFmpeg with the recorded flags, builds patched mpv, and
packages an extracted runtime closure. Build worktrees and prefixes stay
outside the repository.

For Windows CI, the equivalent MSYS2/MinGW-w64 entry point is
`scripts/build-openjoc-player-windows.sh`. It builds the GNU Rust target,
patched FFmpeg, and patched mpv, then places all non-system DLLs beside
`mpv.exe`. It does not require Rust, FFmpeg, or MSYS2 on an end-user machine.

## Runtime layout

```text
bin/mpv                 patched player executable
bin/openjoc-mpv         relocatable launcher with bundle config/profile include
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
searches the executable directory. The launcher is only a small config/path
helper; mpv remains the primary player and no registry or global `PATH` change
is performed.

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
external requirements unless `DEPENDENCIES.json` says otherwise.

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

The verifier is:

```sh
scripts/verify-player-package.sh \
  --root /absolute/extracted/openjoc-player-... \
  --platform macos-arm64 --run-smoke --missing-dependency-smoke
```

It checks required files, inner checksums, target architecture, loader paths,
decoder visibility (`--ad=help`), ABI metadata, license-review status, and
private/local path leaks. The missing-dependency smoke temporarily removes the
OpenJOC runtime from a copy and requires a clear loader failure.

The existing mpv harness remains the media acceptance gate:

```sh
integrations/mpv/verify-player.sh /absolute/extracted/.../bin/mpv /path/to/external-fixtures
```

Its qualified fixture run covers ordinary E-AC-3 → `eac3`, confirmed JOC →
`libopenjoc` without `--ad=libopenjoc`, binaural null output, exact 7.1.4 and
22.2 null-output channel counts, explicit overrides, and passthrough. Pause,
seek, EOS, and real CoreAudio playback remain the existing local acceptance
scope; programme media and derived PCM never enter the archive.

The embedded SADIE II D1 KU100 resource is compiled into `libopenjoc_capi` and
is exercised without `--sofa`, repository-relative paths, or network access.

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
copied. Development artifacts use `0.7.0-git<commit>` identifiers and are not
called `0.8.0`. Signing, notarization, GitHub Release creation, tagging,
installer formats, auto-update, and upstream submission are separate future
release-hardening work.

For a future release-hardening review, the accumulated post-0.7 user-visible
work is: 22.2 rendering, the built-in HRTF, GStreamer integration, the external
FFmpeg bridge, the native FFmpeg `libopenjoc` wrapper, mpv player integration,
and reproducible player packaging. This is an internal summary, not release
publication or a version bump.
