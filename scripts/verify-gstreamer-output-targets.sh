#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd)

if [ "$#" -ne 1 ]; then
    echo "usage: $0 /path/to/raw-joc.ec3" >&2
    exit 2
fi

fixture=$1
if [ ! -f "$fixture" ]; then
    echo "fixture does not exist: $fixture" >&2
    exit 2
fi

command -v gst-inspect-1.0 >/dev/null
command -v gst-launch-1.0 >/dev/null

gstreamer_target=${OPENJOC_GSTREAMER_TARGET_DIR:-$repo_root/target-gstreamer}
CARGO_TARGET_DIR="$gstreamer_target" cargo build --manifest-path "$repo_root/Cargo.toml" \
    -p gst-plugin-openjoc --release --features gstreamer

plugin_dir=$gstreamer_target/release
if [ -n "${GST_PLUGIN_PATH:-}" ]; then
    plugin_path=$plugin_dir:$GST_PLUGIN_PATH
else
    plugin_path=$plugin_dir
fi

run_target() {
    target=$1
    channels=$2
    mask=$3
    log_file=$(mktemp "${TMPDIR:-/tmp}/openjoc-gstreamer-target.XXXXXX")
    if GST_PLUGIN_PATH=$plugin_path GST_DEBUG_NO_COLOR=1 \
        GST_DEBUG='openjocdec:5' \
        gst-launch-1.0 -e -v \
            filesrc location="$fixture" ! ac3parse ! openjocclassify ! \
            openjocdec render-mode=auto ! \
            "audio/x-raw,format=F32LE,rate=48000,layout=interleaved,channels=$channels,channel-mask=(bitmask)$mask" ! \
            fakesink sync=false >"$log_file" 2>&1; then
        result=0
    else
        result=$?
    fi

    if [ "$result" -ne 0 ] || ! grep -q "target=speaker:$target" "$log_file" || \
        ! grep -q "channels=(int)$channels" "$log_file"; then
        echo "FAIL target=$target" >&2
        echo "command/result: gst-launch-1.0 exit=$result channels=$channels mask=$mask" >&2
        echo "relevant GStreamer log tail (full log: $log_file):" >&2
        tail -n 40 "$log_file" >&2
        return 1
    fi

    echo "PASS target=$target"
    rm -f "$log_file"
}

GST_PLUGIN_PATH=$plugin_path gst-inspect-1.0 openjocdec >/dev/null
run_target 2.0 2 0x3
run_target 5.1 6 0xc0f
run_target 7.1.4 12 0x33c3f

echo "GStreamer exact output-target negotiation passed: 2.0, 5.1, and 7.1.4."
