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

GST_PLUGIN_PATH=$plugin_path gst-inspect-1.0 openjocclassify
GST_PLUGIN_PATH=$plugin_path gst-inspect-1.0 openjocdec
GST_PLUGIN_PATH=$plugin_path gst-launch-1.0 -e \
    filesrc location="$fixture" ! \
    ac3parse ! openjocclassify ! openjocdec render-mode=binaural ! \
    audioconvert ! audioresample ! fakesink sync=false

GST_PLUGIN_PATH=$plugin_path gst-launch-1.0 -e \
    filesrc location="$fixture" ! decodebin ! \
    audioconvert ! audioresample ! fakesink sync=false
