#!/bin/sh
set -eu

if [ "$#" -ne 2 ]; then
    echo "usage: $0 /absolute/path/to/mpv /absolute/path/to/fixtures" >&2
    exit 2
fi

mpv=$1
fixtures=$2
raw_single=$fixtures/joc.single.ec3
raw_multi=$fixtures/joc.multi.ec3
joc=$fixtures/joc.mp4
ordinary=$fixtures/ordinary.eac3

for input in "$mpv" "$raw_single" "$raw_multi" "$joc" "$ordinary"; do
    if [ ! -e "$input" ]; then
        echo "missing verification input: $input" >&2
        exit 2
    fi
done

help=$("$mpv" --no-config --ad=help 2>&1)
printf '%s\n' "$help" | grep -Fq 'libopenjoc (eac3)'
printf '%s\n' "$help" | grep -Fq 'eac3 - '

run() {
    input=$1
    shift
    "$mpv" "$input" --no-config --no-video --ao=null --ao-null-untimed=yes \
        --end=1 --msg-level=all=debug,ffmpeg/audio=trace "$@" 2>&1
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

# Menu AVOptions must not force OpenJOC on an ordinary E-AC-3 stream.
ordinary_with_settings_log=$(run "$ordinary" \
    --ad-lavc-o=render_mode=speaker,speaker_layout=5.1,virtual_layout=7.1.4,hrtf=d2,dialnorm=default)
printf '%s\n' "$ordinary_with_settings_log" | grep -Fq 'Selected decoder: eac3 '
if printf '%s\n' "$ordinary_with_settings_log" | grep -Fq 'OpenJOC config'; then
    echo "OpenJOC menu decoder options forced an OpenJOC decoder for ordinary E-AC-3" >&2
    exit 1
fi

joc_log=$(run "$joc" --ad-lavc-o=render_mode=binaural)
printf '%s\n' "$joc_log" | grep -Fq 'OpenJOC classifier: CONFIRMED_JOC'
printf '%s\n' "$joc_log" | grep -Fq 'Selected decoder: libopenjoc '
printf '%s\n' "$joc_log" | grep -Fq 'AO: [null] 48000Hz stereo 2ch'

raw_single_log=$(run "$raw_single")
printf '%s\n' "$raw_single_log" | grep -Fq 'OpenJOC raw pre-admission: CONFIRMED_JOC'
printf '%s\n' "$raw_single_log" | grep -Fq 'OpenJOC raw demux policy: parser=normal probe-info=no seekable=no'
printf '%s\n' "$raw_single_log" | grep -Fq 'Using pre-confirmed raw OpenJOC admission'
printf '%s\n' "$raw_single_log" | grep -Fq 'Selected decoder: libopenjoc '
printf '%s\n' "$raw_single_log" | grep -Fq 'OpenJOC consumed compressed chunk size=4096'

raw_multi_log=$(run "$raw_multi")
printf '%s\n' "$raw_multi_log" | grep -Fq 'OpenJOC raw pre-admission: CONFIRMED_JOC'
printf '%s\n' "$raw_multi_log" | grep -Fq 'Selected decoder: libopenjoc '
multi_chunks=$(printf '%s\n' "$raw_multi_log" | grep -Fc 'OpenJOC consumed compressed chunk size=4096')
[ "$multi_chunks" -ge 2 ]

# The MP4 control wraps the exact single raw AU. Identical renderer-visible
# WAVE bytes prove the raw path neither drops nor duplicates that first AU.
raw_pcm=openjoc-first-au-raw-$$.wav
mp4_pcm=openjoc-first-au-mp4-$$.wav
hrtf_d1_pcm=openjoc-hrtf-d1-$$.wav
hrtf_d2_pcm=openjoc-hrtf-d2-$$.wav
pcm_prefix="openjoc-gain-$$"
trap 'rm -f "$raw_pcm" "$mp4_pcm" "$hrtf_d1_pcm" "$hrtf_d2_pcm" "$pcm_prefix"-*.wav "$pcm_prefix"-runtime.log' EXIT HUP INT TERM
run "$raw_single" --ao=pcm --ao-pcm-waveheader=yes \
    --ao-pcm-file="$raw_pcm" --audio-format=float \
    '--audio-channels=5.1(side)' \
    --ad-lavc-o=render_mode=speaker,speaker_layout=5.1 >/dev/null
run "$joc" --ao=pcm --ao-pcm-waveheader=yes \
    --ao-pcm-file="$mp4_pcm" --audio-format=float \
    '--audio-channels=5.1(side)' \
    --ad-lavc-o=render_mode=speaker,speaker_layout=5.1 >/dev/null
python3 -c 'import pathlib, sys; sys.exit(pathlib.Path(sys.argv[1]).read_bytes() != pathlib.Path(sys.argv[2]).read_bytes())' \
    "$raw_pcm" "$mp4_pcm"

# The decoder wrapper maps both built-in presets through C ABI 1.6. The D2
# sample comparison exercises the option through mpv -> FFmpeg -> OpenJOC.
for hrtf in d1 d2; do
    "$mpv" "$joc" --no-config --no-video --ao=pcm --ao-pcm-waveheader=yes \
        --ao-pcm-file="openjoc-hrtf-$hrtf-$$.wav" --audio-format=float \
        --audio-channels=stereo --end=1 --ad=libopenjoc \
        --ad-lavc-o="render_mode=binaural,hrtf=$hrtf,virtual_layout=7.1.4" >/dev/null
done
python3 -c 'import pathlib, sys; sys.exit(pathlib.Path(sys.argv[1]).read_bytes() == pathlib.Path(sys.argv[2]).read_bytes())' \
    "$hrtf_d1_pcm" "$hrtf_d2_pcm"

# A named FFmpeg volume stage is unity-bypassed, but validate that through the
# entire packaged decoder/AO pipeline. Compare complete float WAVE files so a
# sample count, channel count, sample rate, or PCM-bit change cannot be hidden.
live_gain_fixture=$fixtures/joc.live-gain.mp4
gain_pcm_fixture=$fixtures/joc.lifecycle.mp4
gain_pcm_checker=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)/scripts/compare-player-gain-pcm.py
if [ ! -s "$live_gain_fixture" ] || [ ! -s "$gain_pcm_fixture" ] || [ ! -f "$gain_pcm_checker" ]; then
    echo "missing live-gain fixture or PCM checker" >&2
    exit 1
fi
render_gain_pcm() {
    input=$1
    output=$2
    audio_channels=$3
    decoder_options=$4
    tenths_db=$5
    stop_args=
    if [ "${6:-}" = cutoff ]; then
        stop_args=--end=4
    fi
    if [ "$tenths_db" = none ]; then
        "$mpv" "$input" --no-config --no-video --ao=pcm --ao-pcm-waveheader=yes \
            --ao-pcm-file="$output" --audio-format=float --audio-channels="$audio_channels" \
            $stop_args --ad=libopenjoc --ad-lavc-o="$decoder_options" \
            >/dev/null 2>&1
        return
    fi
    factor=$(python3 - "$tenths_db" <<'PY'
import math
import sys
tenths = int(sys.argv[1])
print("%.17g" % (1.0 if tenths == 0 else math.pow(10.0, tenths / 200.0)))
PY
)
    "$mpv" "$input" --no-config --no-video --ao=pcm --ao-pcm-waveheader=yes \
        --ao-pcm-file="$output" --audio-format=float --audio-channels="$audio_channels" \
        $stop_args --ad=libopenjoc --ad-lavc-o="$decoder_options" \
        "--af=@openjoc_gain:lavfi=[volume@openjoc_gain=volume=$factor:precision=float]:fix-pts=yes" \
        >/dev/null 2>&1
}
pcm_prefix_file=$pcm_prefix
render_gain_pcm "$gain_pcm_fixture" "$pcm_prefix_file-binaural-d1-none.wav" stereo \
    'render_mode=binaural,hrtf=d1,virtual_layout=7.1.4' none
render_gain_pcm "$gain_pcm_fixture" "$pcm_prefix_file-binaural-d1-unity.wav" stereo \
    'render_mode=binaural,hrtf=d1,virtual_layout=7.1.4' 0
python3 "$gain_pcm_checker" exact "$pcm_prefix_file-binaural-d1-none.wav" \
    "$pcm_prefix_file-binaural-d1-unity.wav" --channels 2 --rate 48000
render_gain_pcm "$gain_pcm_fixture" "$pcm_prefix_file-binaural-d2-none.wav" stereo \
    'render_mode=binaural,hrtf=d2,virtual_layout=7.1.4' none
render_gain_pcm "$gain_pcm_fixture" "$pcm_prefix_file-binaural-d2-unity.wav" stereo \
    'render_mode=binaural,hrtf=d2,virtual_layout=7.1.4' 0
python3 "$gain_pcm_checker" exact "$pcm_prefix_file-binaural-d2-none.wav" \
    "$pcm_prefix_file-binaural-d2-unity.wav" --channels 2 --rate 48000
render_gain_pcm "$gain_pcm_fixture" "$pcm_prefix_file-speaker-stereo-none.wav" stereo \
    'render_mode=speaker,speaker_layout=2.0' none
render_gain_pcm "$gain_pcm_fixture" "$pcm_prefix_file-speaker-stereo-unity.wav" stereo \
    'render_mode=speaker,speaker_layout=2.0' 0
python3 "$gain_pcm_checker" exact "$pcm_prefix_file-speaker-stereo-none.wav" \
    "$pcm_prefix_file-speaker-stereo-unity.wav" --channels 2 --rate 48000
speaker_714_channels=fl-fr-fc-lfe-bl-br-sl-sr-tfl-tfr-tbl-tbr
render_gain_pcm "$gain_pcm_fixture" "$pcm_prefix_file-speaker-714-none.wav" \
    "$speaker_714_channels" 'render_mode=speaker,speaker_layout=7.1.4' none
render_gain_pcm "$gain_pcm_fixture" "$pcm_prefix_file-speaker-714-unity.wav" \
    "$speaker_714_channels" 'render_mode=speaker,speaker_layout=7.1.4' 0
python3 "$gain_pcm_checker" exact "$pcm_prefix_file-speaker-714-none.wav" \
    "$pcm_prefix_file-speaker-714-unity.wav" --channels 12 --rate 48000

# Preserve the reported early-stop case as a separate strict comparison too.
# `fix-pts=yes` keeps lavfi audio timestamps aligned to input frame boundaries
# before mpv clips at --end; otherwise compressed 1536-sample frames can gain
# one trailing sample even though their common PCM prefix is unchanged.
render_gain_pcm "$live_gain_fixture" "$pcm_prefix_file-cutoff-d1-none.wav" stereo \
    'render_mode=binaural,hrtf=d1,virtual_layout=7.1.4' none cutoff
render_gain_pcm "$live_gain_fixture" "$pcm_prefix_file-cutoff-d1-unity.wav" stereo \
    'render_mode=binaural,hrtf=d1,virtual_layout=7.1.4' 0 cutoff
python3 "$gain_pcm_checker" exact "$pcm_prefix_file-cutoff-d1-none.wav" \
    "$pcm_prefix_file-cutoff-d1-unity.wav" --channels 2 --rate 48000
render_gain_pcm "$live_gain_fixture" "$pcm_prefix_file-cutoff-d2-none.wav" stereo \
    'render_mode=binaural,hrtf=d2,virtual_layout=7.1.4' none cutoff
render_gain_pcm "$live_gain_fixture" "$pcm_prefix_file-cutoff-d2-unity.wav" stereo \
    'render_mode=binaural,hrtf=d2,virtual_layout=7.1.4' 0 cutoff
python3 "$gain_pcm_checker" exact "$pcm_prefix_file-cutoff-d2-none.wav" \
    "$pcm_prefix_file-cutoff-d2-unity.wav" --channels 2 --rate 48000
render_gain_pcm "$live_gain_fixture" "$pcm_prefix_file-cutoff-stereo-none.wav" stereo \
    'render_mode=speaker,speaker_layout=2.0' none cutoff
render_gain_pcm "$live_gain_fixture" "$pcm_prefix_file-cutoff-stereo-unity.wav" stereo \
    'render_mode=speaker,speaker_layout=2.0' 0 cutoff
python3 "$gain_pcm_checker" exact "$pcm_prefix_file-cutoff-stereo-none.wav" \
    "$pcm_prefix_file-cutoff-stereo-unity.wav" --channels 2 --rate 48000
render_gain_pcm "$live_gain_fixture" "$pcm_prefix_file-cutoff-714-none.wav" \
    "$speaker_714_channels" 'render_mode=speaker,speaker_layout=7.1.4' none cutoff
render_gain_pcm "$live_gain_fixture" "$pcm_prefix_file-cutoff-714-unity.wav" \
    "$speaker_714_channels" 'render_mode=speaker,speaker_layout=7.1.4' 0 cutoff
python3 "$gain_pcm_checker" exact "$pcm_prefix_file-cutoff-714-none.wav" \
    "$pcm_prefix_file-cutoff-714-unity.wav" --channels 12 --rate 48000
echo "GAIN_CUTOFF_PCM_BITEXACT:PASS early_stop=4s modes=D1_D2_binaural speaker_stereo speaker_7.1.4"
echo "GAIN_PCM_BITEXACT:PASS EOF_and_early_stop D1_D2_binaural speaker_stereo speaker_7.1.4"

for gain_tenths in -200 -60 -1 1 60 200; do
    render_gain_pcm "$gain_pcm_fixture" "$pcm_prefix_file-speaker-714-gain-${gain_tenths}.wav" \
        "$speaker_714_channels" 'render_mode=speaker,speaker_layout=7.1.4' "$gain_tenths"
    python3 "$gain_pcm_checker" gain "$pcm_prefix_file-speaker-714-unity.wav" \
        "$pcm_prefix_file-speaker-714-gain-${gain_tenths}.wav" \
        --tenths-db "$gain_tenths" --channels 12 --rate 48000
done
echo "GAIN_SAMPLES:PASS gains_tenths_db=-200,-60,-1,1,60,200 exact_f32_scalar"

# Exercise the UI's live preview and Apply Current against paced playback. The
# named gain filter must take the +0.1 dB runtime command without reopening
# libopenjoc; Apply Current must then perform exactly one decoder reopen, reach
# the requested 7.1 layout, and recreate the filter at +0.1 dB.
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
package_root=$(CDPATH= cd -- "$(dirname -- "$mpv")/.." && pwd)
config_dir=$package_root/bin/portable_config
live_driver=$script_dir/test-openjoc-live-gain-driver.lua
live_log="$pcm_prefix_file-runtime.log"
if [ ! -f "$config_dir/scripts/openjoc-settings.lua" ] || [ ! -f "$live_driver" ]; then
    echo "missing packaged OpenJOC settings script or live-gain driver" >&2
    exit 1
fi
if ! "$mpv" "$live_gain_fixture" --no-video --ao=null --ao-null-untimed=no \
    --end=30 --config-dir="$config_dir" --load-scripts=yes --script="$live_driver" \
    --ad=libopenjoc --ad-lavc-o=render_mode=speaker,speaker_layout=5.1 \
    --msg-level=all=debug,ffmpeg/audio=trace --log-file="$live_log" \
    >/dev/null 2>&1; then
    echo "real-mpv live-gain lifecycle driver failed" >&2
    cat "$live_log" >&2
    exit 1
fi
for marker in 'LIVE_GAIN_PHASE:PRE_RUNTIME' 'LIVE_GAIN_PHASE:PRE_APPLYCURRENT' \
    'LIVE_GAIN_UI_PREVIEW:PASS' 'APPLYCURRENT_REINIT:PASS'; do
    if ! grep -Fq "$marker" "$live_log"; then
        echo "real-mpv driver omitted required marker: $marker" >&2
        cat "$live_log" >&2
        exit 1
    fi
done
if ! awk '
    index($0, "LIVE_GAIN_PHASE:PRE_RUNTIME") { phase = 1; next }
    index($0, "LIVE_GAIN_PHASE:PRE_APPLYCURRENT") { phase = 2; next }
    index($0, "APPLYCURRENT_REINIT:PASS") { phase = 3 }
    phase == 1 && /Opening decoder libopenjoc/ { runtime_opens++ }
    phase == 2 && /Opening decoder libopenjoc/ { apply_opens++ }
    END { if (phase != 3 || runtime_opens != 0 || apply_opens != 1) exit 1 }
' "$live_log"; then
    echo "decoder reopen evidence failed: expected zero gain-phase opens and exactly one Apply Current open" >&2
    cat "$live_log" >&2
    exit 1
fi
runtime_gain_factor=$(python3 - <<'PYGAIN'
import math
print("%.17g" % math.pow(10.0, 1 / 200.0))
PYGAIN
)
if ! awk -v factor="$runtime_gain_factor" '
    index($0, "LIVE_GAIN_PHASE:PRE_RUNTIME") { phase = 1; next }
    index($0, "LIVE_GAIN_PHASE:PRE_APPLYCURRENT") { phase = 0 }
    phase && index($0, "command=\"volume\", argument=\"" factor "\", target=\"volume\"") { command_seen = 1 }
    phase && command_seen && /volume@openjoc_gain: .*volume:1\.011579 volume_dB:0\.100000/ { volume_seen = 1 }
    END { if (!command_seen || !volume_seen) exit 1 }
' "$live_log"; then
    echo "live af-command was not observed at the requested runtime gain" >&2
    cat "$live_log" >&2
    exit 1
fi
reported_live_gain=$(awk '/APPLYCURRENT_REINIT:PASS/ {
    for (i = 1; i <= NF; i++) if ($i ~ /^gain=/) { sub(/^gain=/, "", $i); print $i; exit }
}' "$live_log")
if [ -z "$reported_live_gain" ] || \
   ! grep -Fq 'output_channels=8 samplerate=48000 gain=' "$live_log" || \
   ! python3 - "$reported_live_gain" <<'PY'
import math
import sys
actual = float(sys.argv[1])
expected = math.pow(10.0, 1 / 200.0)
sys.exit(0 if math.isfinite(actual) and abs(actual - expected) <= max(1e-15, expected * 1e-15) else 1)
PY
then
    echo "Apply Current did not report the requested 0.1 dB output gain" >&2
    cat "$live_log" >&2
    exit 1
fi
if \
   ! awk '
       index($0, "LIVE_GAIN_PHASE:PRE_APPLYCURRENT") { phase = 1 }
       phase == 1 && /AO: \[null\] 48000Hz 7\.1 8ch/ { saw_7_1 = 1 }
       index($0, "APPLYCURRENT_REINIT:PASS") { phase = 2 }
       END { if (!saw_7_1 || phase != 2) exit 1 }
   ' "$live_log"; then
    echo "Apply Current did not report the requested 7.1 output layout" >&2
    cat "$live_log" >&2
    exit 1
fi
echo "LIVE_GAIN_NO_RESTART:PASS decoder_opens=0"
echo "APPLYCURRENT_REINIT:PASS decoder_opens=1 output_channels=8 restored_graph=verified"

explicit_log=$(run "$raw_single" --ad=eac3)
printf '%s\n' "$explicit_log" | grep -Fq 'Selected decoder: eac3 '
if printf '%s\n' "$explicit_log" | grep -Fq 'Selected decoder: libopenjoc '; then
    echo "explicit E-AC-3 decoder override selected libopenjoc" >&2
    exit 1
fi

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

layout_log '2ch' --audio-channels=stereo \
    --ad-lavc-o=render_mode=stereo,speaker_layout=2.0
layout_log '6ch' --audio-channels='5.1(side)' \
    --ad-lavc-o=render_mode=speaker,speaker_layout=5.1
layout_log '8ch' --audio-channels=fl-fr-fc-lfe-bl-br-sl-sr \
    --ad-lavc-o=render_mode=speaker,speaker_layout=7.1
layout_log '8ch' --audio-channels=fl-fr-fc-lfe-sl-sr-tfl-tfr \
    --ad-lavc-o=render_mode=speaker,speaker_layout=5.1.2
layout_log '10ch' --audio-channels=fl-fr-fc-lfe-sl-sr-tfl-tfr-tbl-tbr \
    --ad-lavc-o=render_mode=speaker,speaker_layout=5.1.4
layout_log '10ch' --audio-channels=fl-fr-fc-lfe-bl-br-sl-sr-tfl-tfr \
    --ad-lavc-o=render_mode=speaker,speaker_layout=7.1.2
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
        ordinary_media_options_log=$(run "$fixtures/$codec" \
            --ad-lavc-o=render_mode=speaker,speaker_layout=5.1,virtual_layout=7.1.4,hrtf=d2,dialnorm=default)
        if printf '%s\n' "$ordinary_media_options_log" | grep -Fq 'OpenJOC config'; then
            echo "OpenJOC menu decoder options forced an OpenJOC decoder for $codec" >&2
            exit 1
        fi
    fi
done
if [ -f "$fixtures/video.mp4" ]; then
    run_video "$fixtures/video.mp4" >/dev/null
fi

passthrough_log=$(run "$raw_single" --audio-spdif=eac3)
printf '%s\n' "$passthrough_log" | grep -Fq 'Selected decoder: spdif_eac3'
if printf '%s\n' "$passthrough_log" | grep -Fq 'Selected decoder: libopenjoc '; then
    echo "passthrough path selected libopenjoc" >&2
    exit 1
fi

echo "mpv OpenJOC integration checks passed"
