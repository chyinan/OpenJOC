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

for input in "$mpv" "$joc" "$ordinary"; do
    if [ ! -e "$input" ]; then
        echo "missing verification input: $input" >&2
        exit 2
    fi
done

help=$($mpv --ad=help 2>&1)
printf '%s\n' "$help" | grep -Fq 'libopenjoc (eac3)'
printf '%s\n' "$help" | grep -Fq 'eac3 - '

run() {
    input=$1
    shift
    "$mpv" "$input" --no-video --ao=null --ao-null-untimed=yes --end=1 \
        --msg-level=all=debug "$@" 2>&1
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

explicit_log=$(run "$joc" --ad=libopenjoc)
printf '%s\n' "$explicit_log" | grep -Fq 'Selected decoder: libopenjoc '

speaker_log=$(run "$joc" \
    --audio-channels=fl-fr-fc-lfe-bl-br-sl-sr-tfl-tfr-tbl-tbr \
    --ad-lavc-o=render_mode=speaker,speaker_layout=7.1.4)
printf '%s\n' "$speaker_log" | grep -Fq 'Selected decoder: libopenjoc '
printf '%s\n' "$speaker_log" | grep -Fq '12ch'
if printf '%s\n' "$speaker_log" | grep -Fq '[swresample] Remix:'; then
    echo "exact 7.1.4 path remixed after OpenJOC rendering" >&2
    exit 1
fi

wide_log=$(run "$joc" --audio-channels=22.2 \
    --ad-lavc-o=render_mode=speaker,speaker_layout=22.2)
printf '%s\n' "$wide_log" | grep -Fq 'Selected decoder: libopenjoc '
printf '%s\n' "$wide_log" | grep -Fq '24ch'
if printf '%s\n' "$wide_log" | grep -Fq '[swresample] Remix:'; then
    echo "exact 22.2 path remixed after OpenJOC rendering" >&2
    exit 1
fi

passthrough_log=$(run "$joc" --audio-spdif=eac3)
printf '%s\n' "$passthrough_log" | grep -Fq 'Selected decoder: spdif_eac3'
if printf '%s\n' "$passthrough_log" | grep -Fq 'OpenJOC classifier:'; then
    echo "passthrough path ran the OpenJOC classifier" >&2
    exit 1
fi

echo "mpv OpenJOC integration checks passed"
