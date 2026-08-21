# OpenJOC 0.9.1 — Post-release Hotfix

OpenJOC 0.9.1 fixes user-facing issues discovered immediately after
the 0.9.0 Interchange & Ecosystem release.

## ADM compressed-input export

`openjoc export-adm INPUT.ec3 -o OUTPUT.wav` now allocates an existing,
owned temporary root and passes an uncreated `root/scene` directory to the
decoder. The decoder retains its refusal to overwrite any existing output
directory. Both successful export and decode failure clean the owned root.

The regression exercises a synthetic compressed JOC input through the actual
CLI path, checks the adjacent `*.adm-report.json`, runs `validate-adm`, and
asserts cleanup on success and failure.

## Windows ecosystem runtime closure

The SDK and custom FFmpeg/FFprobe Windows ZIPs now include the complete
recursive non-system PE DLL closure from their executable/C-ABI roots. Package
qualification audits that closure with the same qualified `objdump` resolver
used by the player packaging path and records `missing: 0`.

Windows runtime smoke uses only the extracted package plus Windows system
locations. It no longer inherits MSYS2, FFmpeg, OpenJOC, workspace, or runner
library paths. Qualification removes a required DLL, requires the smoke to
fail, restores it, and requires the smoke to pass again. FFmpeg and FFprobe
version banners plus `eac3` and `libopenjoc` decoder inventory are checked.

The 0.9.0 tag and release remain immutable.

## ADM BWF filename convention

The recommended command is now:

```sh
openjoc export-adm INPUT.ec3 -o reconstructed.wav
```

The `.wav` name matches common ADM/BWF workflow expectations, while the file
itself remains a `BW64` WAVE-family container with `ds64`, `fmt `, `data`,
`axml`, and `chna`. `.bw64` remains accepted and produces byte-equivalent
content for deterministic inputs.

## Windows player console entrypoint

The Windows player ZIP now ships the upstream `mpv.com` console wrapper beside
the GUI `mpv.exe`. `openjoc-mpv.cmd` invokes `mpv.com`, propagates its exit
status, and preserves the bundled config/profile injection. It contains no
filename-based demux policy.

The patched player now classifies at most 128 KiB from lavf's non-destructive
probe buffer. A positive JOC result can admit a low-score raw E-AC-3 stream,
including the one-access-unit synthetic regression, and is carried explicitly
to the decoder wrapper. The raw path keeps FFmpeg's normal E-AC-3 parser but
skips the stream-info scan and unsafe timestamp seek that could consume the
only AU. MP4 JOC retains packet classification/replay, ordinary E-AC-3 retains
the stock decoder, and explicit decoder and passthrough requests still win.
Extracted-package qualification covers single- and multi-AU raw JOC, MP4 JOC,
ordinary E-AC-3, and exact raw-versus-MP4 PCM identity for the first AU.

## SDK first-use qualification

The SDK’s CMake CONFIG paths now resolve from `lib/cmake/OpenJOC` to the
package-root `include` and `lib` directories. Fresh extracted SDKs are tested
through direct compiler, pkg-config, and `find_package(OpenJOC CONFIG)`
consumers, with hermetic runtime execution and Windows PE closure audits.
