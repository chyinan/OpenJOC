# OpenJOC ecosystem packages

OpenJOC 0.9 package categories are built from the exact release commit with
`scripts/package-ecosystem.py`:

```sh
python3 scripts/package-ecosystem.py sdk \
  --platform macos-arm64 \
  --target-dir target/release \
  --output /tmp/openjoc-sdk-release

python3 scripts/package-ecosystem.py gstreamer \
  --platform linux-x86_64 \
  --plugin target-gstreamer/release/libgstopenjoc.so \
  --output /tmp/openjoc-gstreamer-release

python3 scripts/package-ecosystem.py ffmpeg \
  --platform linux-x86_64 \
  --ffmpeg /path/to/custom/ffmpeg \
  --ffprobe /path/to/custom/ffprobe \
  --openjoc-prefix /path/to/openjoc-prefix \
  --ffmpeg-source /path/to/pinned/ffmpeg-source \
  --ffmpeg-revision <pinned-commit> \
  --openjoc-patch-sha256 <patch-sha256> \
  --output /tmp/openjoc-ffmpeg-release
```

The output directory must be empty and outside the source checkout. Each
archive is deterministic for a fixed stage directory and includes:

- `BUILD_INFO` with the OpenJOC commit, platform, runtime baseline, and
  qualification state;
- `DEPENDENCIES` with the actual package inventory and zero unresolved license
  components;
- `LICENSE`, `THIRD_PARTY_NOTICES.md`, and a package-local `SHA256SUMS`;
- a quick-start guide and the package-specific runtime model.

The script also writes an external manifest and archive checksum. It rejects
known developer/runner paths. Packages must still be extracted and exercised
on their target CI runner before publication.

## FFmpeg

An OpenJOC-enabled FFmpeg bundle contains custom `openjoc-ffmpeg` and
`openjoc-ffprobe` launchers plus the runtime libraries required by the selected
build. It is not an official upstream FFmpeg distribution. The pinned FFmpeg
revision and OpenJOC integration patch hash are mandatory manifest fields.

## GStreamer

The plugin pack contains the authoritative feature-enabled `gst-plugin-openjoc`
library. It does not bundle an arbitrary GStreamer runtime. The tested runtime
baseline is recorded in `BUILD_INFO`; users install that matching runtime and
activate the extracted plugin directory with `activate.sh` or `activate.ps1`.

## SDK

The SDK contains `include/openjoc.h`, the C ABI libraries, Unix pkg-config
metadata, a small CMake configuration, and a C example. The ABI remains
experimental 1.3 during the 0.x release line; the package version does not
change the ABI.
