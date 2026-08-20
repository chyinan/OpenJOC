#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
    echo "usage: $0 /absolute/temporary/fixture-directory" >&2
    exit 2
fi

output=$1
case "$output" in
    /*) ;;
    *) echo "fixture output must be an absolute path" >&2; exit 2 ;;
esac

command -v cargo >/dev/null 2>&1 || { echo "fixture generation requires cargo" >&2; exit 1; }
command -v ffmpeg >/dev/null 2>&1 || { echo "fixture generation requires ffmpeg" >&2; exit 1; }
mkdir -p "$output"

# This test-only exporter is project-owned and contains no programme media. It
# writes a bounded synthetic JOC stream using the existing OpenJOC fixture
# builder. The raw EC-3 form avoids adding a private/commercial container file.
OPENJOC_SYNTHETIC_JOC_PATH="$output/joc.ec3" \
    cargo test -p openjoc-ffmpeg --lib tests::export_synthetic_joc_fixture_when_requested \
    -- --exact --nocapture

# Generate ordinary codec controls from deterministic lavfi sources. These are
# temporary inputs only; the package archive and uploaded reports never include
# them or any decoded PCM.
ffmpeg -v error -y -f lavfi -i "anullsrc=r=48000:cl=stereo" -t 0.5 \
    -c:a eac3 -b:a 256k -f eac3 "$output/ordinary.eac3"
ffmpeg -v error -y -f lavfi -i "sine=frequency=997:sample_rate=48000:duration=0.5" \
    -c:a aac -b:a 128k "$output/aac.m4a"
ffmpeg -v error -y -f lavfi -i "sine=frequency=1001:sample_rate=48000:duration=0.5" \
    -c:a flac "$output/flac.flac"
ffmpeg -v error -y -f lavfi -i "sine=frequency=1003:sample_rate=48000:duration=0.5" \
    -c:a libmp3lame -b:a 128k "$output/mp3.mp3"
ffmpeg -v error -y -f lavfi -i "anullsrc=r=48000:cl=stereo" -t 0.5 \
    -c:a ac3 -b:a 192k -f ac3 "$output/ac3.ac3"
ffmpeg -v error -y -f lavfi -i "testsrc=size=320x180:rate=24" -f lavfi \
    -i "anullsrc=r=48000:cl=stereo" -t 0.5 -c:v libx264 -pix_fmt yuv420p \
    -c:a aac -shortest "$output/video.mp4"

for fixture in "$output"/*; do
    test -s "$fixture"
done
echo "OpenJOC player fixtures generated in temporary directory: $output"
