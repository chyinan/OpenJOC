#!/bin/sh
set -eu

# Build a pinned OpenJOC + FFmpeg + mpv runtime bundle. The build trees are
# deliberately outside the repository and the final archive contains no
# source, Cargo target directory, private media, or developer configuration.

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd)
manifest="$repo_root/packaging/player/PLAYER_PACKAGE_MANIFEST.json"
platform=${OPENJOC_PLAYER_PLATFORM:-}
output=${OPENJOC_PLAYER_OUTPUT:-}
work=${OPENJOC_PLAYER_WORK:-}
keep_work=0

usage() {
    cat >&2 <<'EOF'
usage: scripts/build-openjoc-player.sh --platform macos-arm64|linux-x86_64|windows-x64 --output /absolute/output [--work /absolute/work] [--keep-work]

This is the maintainer/CI entry point. It fetches the pinned FFmpeg and mpv
commits, applies the exported patches with --check, builds OpenJOC/FFmpeg/mpv,
creates a relocatable archive, and verifies the extracted package.
EOF
    exit 2
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --platform)
            [ "$#" -ge 2 ] || usage
            platform=$2
            shift 2
            ;;
        --output)
            [ "$#" -ge 2 ] || usage
            output=$2
            shift 2
            ;;
        --work)
            [ "$#" -ge 2 ] || usage
            work=$2
            shift 2
            ;;
        --keep-work)
            keep_work=1
            shift
            ;;
        *)
            usage
            ;;
    esac
done

case "$platform" in
    macos-arm64|linux-x86_64|windows-x64) ;;
    *) echo "a supported --platform is required" >&2; usage ;;
esac
case "$output" in
    /*) ;;
    *) echo "--output must be an absolute path outside the source repository" >&2; exit 2 ;;
esac
case "$output" in
    "$repo_root"|"$repo_root"/*) echo "--output must be outside the source repository" >&2; exit 2 ;;
esac

if [ -z "$work" ]; then
    work=$(mktemp -d "${TMPDIR:-/tmp}/openjoc-player-build.XXXXXX")
    owns_work=1
else
    case "$work" in
        /*) ;;
        *) echo "--work must be absolute" >&2; exit 2 ;;
    esac
    mkdir -p "$work"
    owns_work=0
fi
if [ "$keep_work" -eq 1 ]; then
    owns_work=0
fi
cleanup() {
    if [ "$owns_work" -eq 1 ]; then
        rm -rf -- "$work"
    fi
}
trap cleanup EXIT HUP INT TERM

json_value() {
    python3 - "$manifest" "$1" <<'PY'
import json
import sys
data = json.load(open(sys.argv[1], encoding="utf-8"))
value = data
for part in sys.argv[2].split("."):
    value = value[part]
print(value)
PY
}

ffmpeg_commit=$(json_value pinned_stack.ffmpeg.commit)
ffmpeg_patch="$repo_root/$(json_value pinned_stack.ffmpeg.patch_path)"
ffmpeg_patch_sha=$(json_value pinned_stack.ffmpeg.patch_sha256)
mpv_commit=$(json_value pinned_stack.mpv.commit)
mpv_patch="$repo_root/$(json_value pinned_stack.mpv.patch_path)"
mpv_patch_sha=$(json_value pinned_stack.mpv.patch_sha256)

sha256_file() {
    shasum -a 256 "$1" | awk '{print $1}'
}
if [ "$(sha256_file "$ffmpeg_patch")" != "$ffmpeg_patch_sha" ]; then
    echo "FFmpeg patch SHA-256 does not match PLAYER_PACKAGE_MANIFEST.json" >&2
    exit 1
fi
if [ "$(sha256_file "$mpv_patch")" != "$mpv_patch_sha" ]; then
    echo "mpv patch SHA-256 does not match PLAYER_PACKAGE_MANIFEST.json" >&2
    exit 1
fi
if ! git -C "$repo_root" diff --quiet; then
    echo "tracked source changes are not allowed for a reproducible player build" >&2
    exit 1
fi

fetch_checkout() {
    url=$1
    commit=$2
    destination=$3
    mkdir -p "$destination"
    git -C "$destination" init -q
    git -C "$destination" remote add origin "$url" 2>/dev/null || true
    attempt=1
    while ! git -C "$destination" fetch --depth=1 origin "$commit"; do
        if [ "$attempt" -ge 3 ]; then
            echo "failed to fetch pinned source after $attempt attempts: $url @ $commit" >&2
            exit 1
        fi
        attempt=$((attempt + 1))
    done
    git -C "$destination" checkout --detach --quiet FETCH_HEAD
    test "$(git -C "$destination" rev-parse HEAD)" = "$commit"
    test -z "$(git -C "$destination" status --porcelain)"
}

echo "OpenJOC player build: platform=$platform source=$(git -C "$repo_root" rev-parse HEAD)"
mkdir -p "$work/src" "$work/build" "$work/prefix" "$work/stage/bin" "$work/stage/lib"

case "$platform" in
    macos-arm64)
        test "$(uname -s)" = Darwin || { echo "macos-arm64 requires Darwin" >&2; exit 1; }
        test "$(uname -m)" = arm64 || { echo "macos-arm64 requires arm64" >&2; exit 1; }
        for command in cargo rustc git make pkg-config meson ninja clang otool install_name_tool python3; do
            command -v "$command" >/dev/null 2>&1 || { echo "missing build command: $command" >&2; exit 1; }
        done
        if ! pkg-config --exists libass libplacebo; then
            echo "macOS player build requires libass and libplacebo development packages" >&2
            echo "Install them with: brew install libass libplacebo meson ninja" >&2
            exit 1
        fi
        ffmpeg_source="$work/src/ffmpeg"
        mpv_source="$work/src/mpv"
        echo '::group::Pinned FFmpeg/mpv sources and patch gates'
        fetch_checkout https://github.com/FFmpeg/FFmpeg.git "$ffmpeg_commit" "$ffmpeg_source"
        fetch_checkout https://github.com/mpv-player/mpv.git "$mpv_commit" "$mpv_source"
        git -C "$ffmpeg_source" apply --check "$ffmpeg_patch"
        git -C "$ffmpeg_source" apply "$ffmpeg_patch"
        git -C "$mpv_source" apply --check "$mpv_patch"
        git -C "$mpv_source" apply "$mpv_patch"
        echo '::endgroup::'
        echo '::group::OpenJOC C ABI'
        cargo build --manifest-path "$repo_root/Cargo.toml" -p openjoc-capi --release --locked
        openjoc_prefix="$work/prefix/openjoc"
        "$repo_root/integrations/ffmpeg/native/stage-openjoc.sh" "$openjoc_prefix"
        echo '::endgroup::'
        echo '::group::Patched FFmpeg configure and build'
        ffmpeg_prefix="$work/prefix/ffmpeg"
        (cd "$work/build" && PKG_CONFIG_PATH="$openjoc_prefix/lib/pkgconfig" \
            "$ffmpeg_source/configure" --prefix="$ffmpeg_prefix" \
            --disable-doc --disable-debug --disable-autodetect \
            --disable-static --enable-shared --enable-version3 \
            --enable-libopenjoc --disable-programs --disable-network \
            --enable-videotoolbox --enable-audiotoolbox)
        make -C "$work/build" -j"${CARGO_BUILD_JOBS:-2}"
        make -C "$work/build" install
        echo '::endgroup::'
        echo '::group::Patched mpv configure and build'
        mpv_prefix="$work/prefix/mpv"
        brew_prefix=$(brew --prefix)
        dep_pkgconfig="$brew_prefix/lib/pkgconfig"
        (cd "$work" && PKG_CONFIG_PATH="$ffmpeg_prefix/lib/pkgconfig:$openjoc_prefix/lib/pkgconfig:$dep_pkgconfig" \
            meson setup "$work/build/mpv" "$mpv_source" \
            --prefix=/usr --buildtype=release -Dtests=false \
            -Dmanpage-build=disabled -Dhtml-build=disabled -Dpdf-build=disabled)
        meson compile -C "$work/build/mpv" -j "${CARGO_BUILD_JOBS:-2}"
        DESTDIR="$mpv_prefix" meson install -C "$work/build/mpv"
        cp "$mpv_prefix/usr/bin/mpv" "$work/stage/bin/mpv"
        echo '::endgroup::'
        echo '::group::macOS package assembly and dependency audit'
        cp "$openjoc_prefix/lib/libopenjoc_capi.dylib" "$work/stage/lib/libopenjoc_capi.dylib"
        for file in "$ffmpeg_prefix"/lib/*.dylib; do [ -f "$file" ] && cp "$file" "$work/stage/lib/"; done
        python3 "$repo_root/scripts/player-package.py" bundle \
            --stage-root "$work/stage" --output "$output" --platform macos-arm64 \
            --search-dir "$ffmpeg_prefix/lib" --search-dir "$openjoc_prefix/lib" \
            --search-dir "$brew_prefix/lib" --ffmpeg-source "$ffmpeg_source" \
            --mpv-source "$mpv_source" --private-prefix "$work" \
            --toolchain "$(rustc -vV | tr '\n' '; ')"
        echo '::endgroup::'
        echo '::group::Extracted macOS package runtime smoke'
        archive=$(find "$output" -maxdepth 1 -name 'openjoc-mpv-*-macos-arm64.tar.gz' -type f -print | head -n 1)
        extract="$work/extracted"
        mkdir -p "$extract"
        tar -xzf "$archive" -C "$extract"
        root=$(find "$extract" -mindepth 1 -maxdepth 1 -type d -print | head -n 1)
        python3 "$repo_root/scripts/player-package.py" verify --root "$root" --platform macos-arm64 --run-smoke --missing-dependency-smoke
        echo '::endgroup::'
        ;;
    linux-x86_64)
        test "$(uname -s)" = Linux || { echo "linux-x86_64 must be built on a Linux runner" >&2; exit 1; }
        for command in cargo rustc git make pkg-config meson ninja gcc readelf patchelf python3; do
            command -v "$command" >/dev/null 2>&1 || { echo "missing build command: $command" >&2; exit 1; }
        done
        ffmpeg_source="$work/src/ffmpeg"
        mpv_source="$work/src/mpv"
        echo '::group::Pinned FFmpeg/mpv sources and patch gates'
        fetch_checkout https://github.com/FFmpeg/FFmpeg.git "$ffmpeg_commit" "$ffmpeg_source"
        fetch_checkout https://github.com/mpv-player/mpv.git "$mpv_commit" "$mpv_source"
        git -C "$ffmpeg_source" apply --check "$ffmpeg_patch"
        git -C "$ffmpeg_source" apply "$ffmpeg_patch"
        git -C "$mpv_source" apply --check "$mpv_patch"
        git -C "$mpv_source" apply "$mpv_patch"
        echo '::endgroup::'
        echo '::group::OpenJOC C ABI'
        cargo build --manifest-path "$repo_root/Cargo.toml" -p openjoc-capi --release --locked
        openjoc_prefix="$work/prefix/openjoc"
        "$repo_root/integrations/ffmpeg/native/stage-openjoc.sh" "$openjoc_prefix"
        echo '::endgroup::'
        echo '::group::Patched FFmpeg configure and build'
        ffmpeg_prefix="$work/prefix/ffmpeg"
        (cd "$work/build" && PKG_CONFIG_PATH="$openjoc_prefix/lib/pkgconfig" \
            "$ffmpeg_source/configure" --prefix="$ffmpeg_prefix" \
            --disable-doc --disable-debug --disable-autodetect \
            --disable-static --enable-shared --enable-version3 \
            --enable-libopenjoc --disable-programs --disable-network \
            )
        make -C "$work/build" -j"${CARGO_BUILD_JOBS:-2}"
        make -C "$work/build" install
        echo '::endgroup::'
        echo '::group::Patched mpv configure and build'
        mpv_prefix="$work/prefix/mpv"
        (cd "$work" && PKG_CONFIG_PATH="$ffmpeg_prefix/lib/pkgconfig:$openjoc_prefix/lib/pkgconfig:${PKG_CONFIG_PATH:-}" \
            meson setup "$work/build/mpv" "$mpv_source" \
            --prefix=/usr --buildtype=release -Dtests=false \
            -Dmanpage-build=disabled -Dhtml-build=disabled -Dpdf-build=disabled)
        meson compile -C "$work/build/mpv" -j "${CARGO_BUILD_JOBS:-2}"
        DESTDIR="$mpv_prefix" meson install -C "$work/build/mpv"
        cp "$mpv_prefix/usr/bin/mpv" "$work/stage/bin/mpv"
        echo '::endgroup::'
        echo '::group::Linux package assembly and dependency audit'
        cp "$openjoc_prefix/lib/libopenjoc_capi.so" "$work/stage/lib/libopenjoc_capi.so"
        for file in "$ffmpeg_prefix"/lib/*.so*; do [ -f "$file" ] && cp "$file" "$work/stage/lib/"; done
        LD_LIBRARY_PATH="$ffmpeg_prefix/lib:$openjoc_prefix/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
        python3 "$repo_root/scripts/player-package.py" bundle \
            --stage-root "$work/stage" --output "$output" --platform linux-x86_64 \
            --ffmpeg-source "$ffmpeg_source" --mpv-source "$mpv_source" \
            --private-prefix "$work" \
            --toolchain "$(rustc -vV | tr '\n' '; '); compiler=$(gcc --version | head -n 1); glibc=$(ldd --version | head -n 1); kernel=$(uname -sr)"
        echo '::endgroup::'
        echo '::group::Extracted Linux package runtime smoke'
        archive=$(find "$output" -maxdepth 1 -name 'openjoc-mpv-*-linux-x86_64.tar.gz' -type f -print | head -n 1)
        extract="$work/extracted"
        mkdir -p "$extract"
        tar -xzf "$archive" -C "$extract"
        root=$(find "$extract" -mindepth 1 -maxdepth 1 -type d -print | head -n 1)
        python3 "$repo_root/scripts/player-package.py" verify --root "$root" --platform linux-x86_64 --run-smoke --missing-dependency-smoke
        echo '::endgroup::'
        ;;
    windows-x64)
        if [ -n "$work" ]; then
            exec "$repo_root/scripts/build-openjoc-player-windows.sh" --output "$output" --work "$work"
        fi
        exec "$repo_root/scripts/build-openjoc-player-windows.sh" --output "$output"
        ;;
esac

echo "OpenJOC player packaging complete: $output"
