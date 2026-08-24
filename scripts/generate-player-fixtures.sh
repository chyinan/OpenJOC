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
# builder. Keep both the full eight-access-unit stream and the one-access-unit
# regression that exercises low-score raw probing. The MP4 control wraps the
# exact one-access-unit bytes, not private or commercial programme media.
OPENJOC_SYNTHETIC_JOC_PATH="$output/joc.multi.ec3" \
    cargo test -p openjoc-ffmpeg --lib tests::export_synthetic_joc_fixture_when_requested \
    -- --exact --nocapture
ffmpeg -v error -f eac3 -i "$output/joc.multi.ec3" -map 0:a:0 -c:a copy \
    -f mp4 -y "$output/joc.multi.mp4"

# The qualification probe uses distinct bed excitation paths (BAP-0 dither
# driven by separate exponent paths), grouped LFE mantissas, and an asymmetric
# object-position sweep. Its test-only exporter decodes the complete stream
# through every representable policy and refuses to write it unless every
# output channel has a stable, pairwise-distinct time-series fingerprint.
OPENJOC_FINGERPRINT_JOC_PATH="$output/joc.fingerprint.ec3" \
    cargo test -p openjoc-ffmpeg --lib \
    tests::export_synthetic_joc_fingerprint_fixture_when_requested \
    -- --exact --nocapture
ffmpeg -v error -f eac3 -i "$output/joc.fingerprint.ec3" -map 0:a:0 -c:a copy \
    -f mp4 -y "$output/joc.fingerprint.mp4"

# The compatibility name remains the single-AU input used by older local
# harnesses. New qualification names both raw controls explicitly.
head -c 4096 "$output/joc.multi.ec3" > "$output/joc.single.ec3"
cp "$output/joc.single.ec3" "$output/joc.ec3"
ffmpeg -v error -f eac3 -i "$output/joc.single.ec3" -map 0:a:0 -c:a copy \
    -f mp4 -y "$output/joc.mp4"

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
    -i "anullsrc=r=48000:cl=stereo" -t 0.5 -c:v mpeg4 -pix_fmt yuv420p \
    -c:a aac -shortest "$output/video.mp4"

for fixture in "$output"/*; do
    test -s "$fixture"
done
echo "OpenJOC player fixtures generated in temporary directory: $output"
