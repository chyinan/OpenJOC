#!/bin/sh
set -eu

if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
    echo "usage: $0 FFMPEG_SOURCE BUILD_DIR OPENJOC_PREFIX [POSITIVE_JOC_FIXTURE]" >&2
    exit 2
fi

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/../../.." && pwd)
ffmpeg_source=$1
build_dir=$2
openjoc_prefix=$3
positive_fixture=${4-}
patch_file="$script_dir/patches/0001-avcodec-add-experimental-libopenjoc-decoder-wrapper.patch"

for path in "$ffmpeg_source" "$build_dir" "$openjoc_prefix"; do
    case "$path" in
        /*) ;;
        *)
            echo "all source/build/prefix paths must be absolute" >&2
            exit 2
            ;;
    esac
done

if [ -e "$build_dir/config.mak" ]; then
    echo "build directory is already configured: $build_dir" >&2
    exit 2
fi
if [ -n "$(git -C "$ffmpeg_source" status --porcelain)" ]; then
    echo "FFmpeg source worktree must be clean" >&2
    exit 2
fi

base=$(git -C "$ffmpeg_source" rev-parse HEAD)
case "$base" in
    bf1b838f2ab88b4f8fd83443325c782ea0e0f7fa|3bdd895832244780c250713e49135615ac4de003) ;;
    *)
        echo "unsupported FFmpeg base: $base" >&2
        exit 2
        ;;
esac

git -C "$ffmpeg_source" apply --check "$patch_file"
git -C "$ffmpeg_source" apply "$patch_file"
"$script_dir/stage-openjoc.sh" "$openjoc_prefix"

mkdir -p "$build_dir"
(cd "$build_dir" && \
    PKG_CONFIG_PATH="$openjoc_prefix/lib/pkgconfig" \
    "$ffmpeg_source/configure" \
        --disable-doc \
        --disable-debug \
        --disable-autodetect \
        --disable-static \
        --enable-shared \
        --enable-version3 \
        --enable-libopenjoc \
        --disable-ffplay \
        --extra-ldflags="-Wl,-rpath,$openjoc_prefix/lib")
make -C "$build_dir" -j2

library_path="$build_dir/libavcodec:$build_dir/libavformat:$build_dir/libavutil:$build_dir/libavfilter:$build_dir/libavdevice:$build_dir/libswresample:$build_dir/libswscale:$openjoc_prefix/lib"
case "$(uname -s)" in
    Darwin) loader_variable=DYLD_LIBRARY_PATH ;;
    Linux) loader_variable=LD_LIBRARY_PATH ;;
    *)
        echo "runtime verification is not implemented for $(uname -s)" >&2
        exit 1
        ;;
esac

run_with_libraries() {
    env "$loader_variable=$library_path" "$@"
}

run_with_libraries "$build_dir/ffmpeg" -hide_banner -decoders \
    | grep -E '(^| )eac3|libopenjoc'
run_with_libraries "$build_dir/ffmpeg" -hide_banner -h decoder=libopenjoc \
    | grep -E 'OpenJOC|Supported sample formats: flt|speaker_layout|virtual_layout'

cc "$script_dir/verify-decoder-selection.c" \
    -I"$ffmpeg_source" -I"$build_dir" \
    -L"$build_dir/libavcodec" -L"$build_dir/libavutil" \
    -lavcodec -lavutil -o "$build_dir/verify-decoder-selection"
run_with_libraries "$build_dir/verify-decoder-selection"

if [ -n "$positive_fixture" ]; then
    run_with_libraries "$build_dir/ffmpeg" -hide_banner -loglevel error \
        -flags2 +skip_manual -c:a libopenjoc -strict experimental \
        -speaker_layout 2.0 -i "$positive_fixture" \
        -map 0:a:0 -c:a pcm_f32le -f f32le -y "$build_dir/openjoc.f32"
    test -s "$build_dir/openjoc.f32"
fi

run_with_libraries "$build_dir/ffmpeg" -hide_banner -loglevel error \
    -f lavfi -i sine=frequency=997:sample_rate=48000:duration=0.25 \
    -c:a eac3 -b:a 256k -y "$build_dir/ordinary.eac3"
run_with_libraries "$build_dir/ffmpeg" -hide_banner -loglevel verbose \
    -i "$build_dir/ordinary.eac3" -f null - 2> "$build_dir/ordinary.log"
grep 'eac3 (native)' "$build_dir/ordinary.log"
if run_with_libraries "$build_dir/ffmpeg" -hide_banner -loglevel error -xerror \
    -c:a libopenjoc -strict experimental -i "$build_dir/ordinary.eac3" \
    -f null - 2> "$build_dir/ordinary-forced.log"; then
    echo "ordinary E-AC-3 unexpectedly decoded with libopenjoc" >&2
    exit 1
fi
grep 'OpenJOC rejected ordinary E-AC-3 input' "$build_dir/ordinary-forced.log"

echo "FFMPEG_NATIVE_VERIFY_PASS base=$base"
