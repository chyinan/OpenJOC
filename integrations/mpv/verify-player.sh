#!/bin/sh
set -eu

if [ "$#" -ne 2 ]; then
    echo "usage: $0 /absolute/path/to/mpv /absolute/path/to/fixtures" >&2
    exit 2
fi

mpv=$1
fixtures=$2
joc=$fixtures/joc.mp4
ordinary=$fixtures/ordinary.eac3
[ -f "$joc" ] || joc=$fixtures/joc.ec3

for input in "$mpv" "$joc" "$ordinary"; do
    if [ ! -e "$input" ]; then
        echo "missing verification input: $input" >&2
        exit 2
    fi
done

help=$($mpv --no-config --ad=help 2>&1)
printf '%s\n' "$help" | grep -Fq 'libopenjoc (eac3)'
printf '%s\n' "$help" | grep -Fq 'eac3 - '

run() {
    input=$1
    shift
    "$mpv" "$input" --no-config --no-video --ao=null --ao-null-untimed=yes \
        --end=1 --msg-level=all=debug "$@" 2>&1
}

run_video() {
    input=$1
    shift
    "$mpv" "$input" --no-config --vo=null --ao=null --ao-null-untimed=yes \
        --end=1 --msg-level=all=debug "$@" 2>&1
}

ordinary_log=$(run "$ordinary")
printf '%s\n' "$ordinary_log" | grep -Fq 'OpenJOC classifier: CONFIRMED_NON_JOC'
printf '%s\n' "$ordinary_log" | grep -Fq 'Selected decoder: eac3 '
if printf '%s\n' "$ordinary_log" | grep -Fq 'OpenJOC config'; then
    echo "ordinary E-AC-3 created an OpenJOC decoder" >&2
    exit 1
fi

joc_log=$(run "$joc" --ad-lavc-o=render_mode=binaural)
printf '%s\n' "$joc_log" | grep -Fq 'OpenJOC classifier: CONFIRMED_JOC'
printf '%s\n' "$joc_log" | grep -Fq 'Selected decoder: libopenjoc '
printf '%s\n' "$joc_log" | grep -Fq 'AO: [null] 48000Hz stereo 2ch'

# No --sofa option is supplied: successful binaural decode exercises the
# embedded SADIE resource. The qualification wrapper disables networking and
# runs from the extracted bundle directory.

layout_log() {
    name=$1
    shift
    log=$(run "$joc" "$@")
    printf '%s\n' "$log" | grep -Fq 'Selected decoder: libopenjoc '
    printf '%s\n' "$log" | grep -Fq "$name"
    if printf '%s\n' "$log" | grep -Fq '[swresample] Remix:'; then
        echo "exact $name path remixed after OpenJOC rendering" >&2
        exit 1
    fi
}

layout_log '2ch' --audio-channels=2.0 \
    --ad-lavc-o=render_mode=speaker,speaker_layout=2.0
layout_log '6ch' --audio-channels='5.1(side)' \
    --ad-lavc-o=render_mode=speaker,speaker_layout=5.1
layout_log '12ch' \
    --audio-channels=fl-fr-fc-lfe-bl-br-sl-sr-tfl-tfr-tbl-tbr \
    --ad-lavc-o=render_mode=speaker,speaker_layout=7.1.4
layout_log '16ch' \
    --audio-channels=fl-fr-fc-lfe-bl-br-sl-sr-wl-wr-tfl-tfr-tsl-tsr-tbl-tbr \
    --ad-lavc-o=render_mode=speaker,speaker_layout=9.1.6
layout_log '24ch' --audio-channels=22.2 \
    --ad-lavc-o=render_mode=speaker,speaker_layout=22.2

# Exercise a seek/flush boundary on the extracted package.
run "$joc" --start=0.02 --length=0.2 >/dev/null

for codec in aac.m4a flac.flac mp3.mp3 ac3.ac3; do
    if [ -f "$fixtures/$codec" ]; then
        run "$fixtures/$codec" >/dev/null
    fi
done
if [ -f "$fixtures/video.mp4" ]; then
    run_video "$fixtures/video.mp4" >/dev/null
fi

passthrough_log=$(run "$joc" --audio-spdif=eac3)
printf '%s\n' "$passthrough_log" | grep -Fq 'Selected decoder: spdif_eac3'
if printf '%s\n' "$passthrough_log" | grep -Fq 'OpenJOC classifier:'; then
    echo "passthrough path ran the OpenJOC classifier" >&2
    exit 1
fi

echo "mpv OpenJOC integration checks passed"
