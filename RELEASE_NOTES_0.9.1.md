# OpenJOC 0.9.1 — Post-release Hotfix

OpenJOC 0.9.1 fixes two real user-facing issues discovered immediately after
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
status, and preserves the bundled config/profile injection. Extracted-package
qualification checks the wrapper’s version/help/playback paths and records the
GUI executable separately.

## SDK first-use qualification

The SDK’s CMake CONFIG paths now resolve from `lib/cmake/OpenJOC` to the
package-root `include` and `lib` directories. Fresh extracted SDKs are tested
through direct compiler, pkg-config, and `find_package(OpenJOC CONFIG)`
consumers, with hermetic runtime execution and Windows PE closure audits.
