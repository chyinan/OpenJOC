#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd)

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
    echo "usage: $0 /path/to/private-joc.ec3 [/path/to/ordinary-eac3.ec3]" >&2
    exit 2
fi

joc_fixture=$1
ordinary_fixture=${2:-}
if [ ! -f "$joc_fixture" ]; then
    echo "JOC fixture does not exist: $joc_fixture" >&2
    exit 2
fi
if [ -n "$ordinary_fixture" ] && [ ! -f "$ordinary_fixture" ]; then
    echo "ordinary E-AC-3 fixture does not exist: $ordinary_fixture" >&2
    exit 2
fi

gstreamer_target=${OPENJOC_GSTREAMER_TARGET_DIR:-$repo_root/target-gstreamer}
CARGO_TARGET_DIR="$gstreamer_target" cargo build \
    --manifest-path "$repo_root/Cargo.toml" \
    -p gst-plugin-openjoc --release --features gstreamer

plugin_dir=$gstreamer_target/release
if [ -n "${GST_PLUGIN_PATH:-}" ]; then
    plugin_path=$plugin_dir:$GST_PLUGIN_PATH
else
    plugin_path=$plugin_dir
fi

GST_PLUGIN_PATH=$plugin_path gst-inspect-1.0 openjocclassify >/dev/null
GST_PLUGIN_PATH=$plugin_path gst-inspect-1.0 openjocdec >/dev/null

log_file=$(mktemp "${TMPDIR:-/tmp}/openjoc-gstreamer-autoplug.XXXXXX")
trap 'rm -f "$log_file"' EXIT HUP INT TERM

GST_PLUGIN_PATH=$plugin_path GST_DEBUG_NO_COLOR=1 \
GST_DEBUG='openjocdec:5,decodebin:5,parsebin:5' \
gst-launch-1.0 -e \
    filesrc location="$joc_fixture" ! decodebin ! \
    audioconvert ! audioresample ! fakesink sync=false >"$log_file" 2>&1

grep -q 'openjocclassify' "$log_file"
grep -q 'started OpenJOC session' "$log_file"
grep -q 'Got EOS' "$log_file"

if [ -n "$ordinary_fixture" ]; then
    GST_PLUGIN_PATH=$plugin_path GST_DEBUG_NO_COLOR=1 \
    GST_DEBUG='openjocdec:5,decodebin:5,parsebin:5' \
    gst-launch-1.0 -e \
        filesrc location="$ordinary_fixture" ! decodebin ! \
        audioconvert ! audioresample ! fakesink sync=false >"$log_file" 2>&1

    grep -q 'openjocclassify' "$log_file"
    grep -q 'Got EOS' "$log_file"
    if grep -q 'started OpenJOC session' "$log_file"; then
        echo "ordinary E-AC-3 instantiated OpenJOC" >&2
        exit 1
    fi
fi

echo "GStreamer JOC-aware autoplug passed: classifier and OpenJOC were observed for JOC input."
