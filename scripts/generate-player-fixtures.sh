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
command -v ffprobe >/dev/null 2>&1 || { echo "fixture generation requires ffprobe" >&2; exit 1; }
command -v cmp >/dev/null 2>&1 || { echo "fixture generation requires cmp" >&2; exit 1; }
command -v grep >/dev/null 2>&1 || { echo "fixture generation requires grep" >&2; exit 1; }
command -v awk >/dev/null 2>&1 || { echo "fixture generation requires awk" >&2; exit 1; }
if command -v sha256sum >/dev/null 2>&1; then
    sha256_files() { sha256sum "$@"; }
elif command -v shasum >/dev/null 2>&1; then
    sha256_files() { shasum -a 256 "$@"; }
else
    echo "fixture generation requires sha256sum or shasum" >&2
    exit 1
fi
mkdir -p "$output"

require_exact_row() {
    expected=$1
    rows=$2
    if ! grep -Fx "$expected" "$rows" >/dev/null; then
        echo "missing exact row '$expected' in $rows" >&2
        cat "$rows" >&2
        return 1
    fi
}

verify_exact_mp4_payload() {
    raw=$1
    mp4=$2
    label=$3
    demuxed="$output/.${label}.demux.eac3"
    rm -f "$demuxed"
    ffmpeg -v error -i "$mp4" -map 0:a:0 -c:a copy -f eac3 -y "$demuxed"
    cmp "$raw" "$demuxed"
    echo "exact E-AC-3 wrapper payload: $raw == $mp4"
    sha256_files "$raw" "$mp4" "$demuxed"
    rm -f "$demuxed"
}

verify_seekable_eac3_timing() {
    mp4=$1
    label=$2
    expected_packets=$3
    expected_duration=$4
    stream_rows="$output/${label}.stream-timing.txt"
    packet_pts="$output/${label}.packet-pts.txt"
    frame_rows="$output/${label}.frame-timing.csv"

    ffprobe -v error -select_streams a:0 -count_packets \
        -show_entries stream=time_base,duration_ts,duration,nb_frames,nb_read_packets \
        -of default=noprint_wrappers=1 "$mp4" > "$stream_rows"
    ffprobe -v error -select_streams a:0 -show_packets \
        -show_entries packet=pts,dts -of csv=p=0 "$mp4" > "$packet_pts"
    ffprobe -v error -select_streams a:0 -show_frames \
        -show_entries frame=pts,pkt_dts,duration,nb_samples:frame_side_data= \
        -of csv=p=0 "$mp4" > "$frame_rows"

    require_exact_row "time_base=1/48000" "$stream_rows"
    require_exact_row "duration_ts=196608" "$stream_rows"
    require_exact_row "duration=$expected_duration" "$stream_rows"
    require_exact_row "nb_frames=$expected_packets" "$stream_rows"
    require_exact_row "nb_read_packets=$expected_packets" "$stream_rows"
    if ! awk -F, -v expected_count="$expected_packets" '
        BEGIN { expected_pts = 0; count = 0 }
        $1 !~ /^[0-9]+$/ || $2 !~ /^[0-9]+$/ { exit 1 }
        $1 != expected_pts || $2 != expected_pts { exit 1 }
        { expected_pts += 1536; count += 1 }
        END { if (count != expected_count) exit 1 }
    ' "$packet_pts"; then
        echo "invalid packet PTS/DTS timeline in $packet_pts" >&2
        cat "$packet_pts" >&2
        return 1
    fi
    if ! awk -F, -v expected_count="$expected_packets" '
        BEGIN { expected_pts = 0; count = 0 }
        $1 !~ /^[0-9]+$/ || $2 !~ /^[0-9]+$/ || $3 != "N/A" || $4 != 1536 { exit 1 }
        $1 != expected_pts || $2 != expected_pts { exit 1 }
        { expected_pts += 1536; count += 1 }
        END { if (count != expected_count) exit 1 }
    ' "$frame_rows"; then
        echo "invalid decoded-frame timeline in $frame_rows" >&2
        cat "$frame_rows" >&2
        return 1
    fi
    echo "seekable E-AC-3 timing: $mp4 packets=$expected_packets pts_dts_step=1536 frame_samples=1536 frame_duration=N/A duration_ts=196608 duration=$expected_duration"
    sha256_files "$mp4" "$stream_rows" "$packet_pts" "$frame_rows"
}

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
verify_exact_mp4_payload "$output/joc.multi.ec3" "$output/joc.multi.mp4" joc-multi

# Lifecycle evidence uses an independent stream with monotonically advancing
# JOC sequence counts. The test exporter refuses to write it unless two full
# decode passes (including reset to a new PTS origin) conserve every programme
# sample plus the declared linked-gain tail. The MP4 bitstream filter assigns
# one 1536/48000 interval to every access unit without transcoding.
OPENJOC_LIFECYCLE_JOC_PATH="$output/joc.lifecycle.ec3" \
    cargo test -p openjoc-ffmpeg --lib \
    tests::export_synthetic_joc_lifecycle_fixture_when_requested \
    -- --exact --nocapture
ffmpeg -v error -f eac3 -i "$output/joc.lifecycle.ec3" -map 0:a:0 -c:a copy \
    -bsf:a 'setts=time_base=1/48000:pts=N*1536:dts=N*1536:duration=1536' \
    -f mp4 -y "$output/joc.lifecycle.mp4"
verify_exact_mp4_payload "$output/joc.lifecycle.ec3" \
    "$output/joc.lifecycle.mp4" joc-lifecycle
verify_seekable_eac3_timing "$output/joc.lifecycle.mp4" joc-lifecycle 128 4.096000

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
# A non-silent, channel-distinguishing stock-path oracle. Each 5.1 channel has
# a unique deterministic tone, and the MP4 file below wraps this exact encoded
# E-AC-3 payload without transcoding.
ffmpeg -v error -y -f lavfi \
    -i "aevalsrc=0.12*sin(2*PI*211*t)|0.11*sin(2*PI*307*t)|0.10*sin(2*PI*401*t)|0.09*sin(2*PI*61*t)|0.08*sin(2*PI*601*t)|0.07*sin(2*PI*701*t):s=48000:d=1:c=5.1(side)" \
    -c:a eac3 -b:a 640k -f eac3 "$output/ordinary.fingerprint.eac3"
ffmpeg -v error -f eac3 -i "$output/ordinary.fingerprint.eac3" -map 0:a:0 -c:a copy \
    -f mp4 -y "$output/ordinary.fingerprint.mp4"
verify_exact_mp4_payload "$output/ordinary.fingerprint.eac3" \
    "$output/ordinary.fingerprint.mp4" ordinary-fingerprint
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
