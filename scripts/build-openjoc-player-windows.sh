#!/usr/bin/env bash
set -euo pipefail

# MSYS2/MinGW-w64 Windows player build. Run from a MINGW64 shell on a clean
# Windows runner; the resulting ZIP is extract-and-run and keeps every non-
# system DLL adjacent to mpv.exe.

script_dir=$(cd "$(dirname "$0")" && pwd)
repo_root=$(cd "$script_dir/.." && pwd)
manifest="$repo_root/packaging/player/PLAYER_PACKAGE_MANIFEST.json"
output=
work=
phase=MSYS2_PROVISIONING

on_error() {
    status=$?
    echo "WINDOWS_PLAYER_FAILURE_CLASS=$phase" >&2
    exit "$status"
}
trap on_error ERR

while [[ $# -gt 0 ]]; do
    case "$1" in
        --output) output=$2; shift 2 ;;
        --work) work=$2; shift 2 ;;
        *) echo "usage: $0 --output /absolute/output [--work /absolute/work]" >&2; exit 2 ;;
    esac
done
[[ -n "$output" && "$output" = /* ]] || { echo "--output must be an absolute path" >&2; exit 2; }
[[ "$output" != "$repo_root" && "$output" != "$repo_root"/* ]] || { echo "output must be outside the source repository" >&2; exit 2; }
if [[ -z "$work" ]]; then
    work=$(mktemp -d "${TMPDIR:-/tmp}/openjoc-player-windows.XXXXXX")
    owns_work=1
else
    mkdir -p "$work"
    owns_work=0
fi
cleanup() { if [[ "$owns_work" == 1 ]]; then rm -rf -- "$work"; fi; }
trap cleanup EXIT HUP INT TERM

json_value() {
    python3 - "$manifest" "$1" <<'PY'
import json, sys
value = json.load(open(sys.argv[1], encoding="utf-8"))
for part in sys.argv[2].split('.'):
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
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

[[ "$(sha256_file "$ffmpeg_patch")" == "$ffmpeg_patch_sha" ]] || {
    echo "FFmpeg patch SHA-256 does not match PLAYER_PACKAGE_MANIFEST.json" >&2
    exit 1
}
[[ "$(sha256_file "$mpv_patch")" == "$mpv_patch_sha" ]] || {
    echo "mpv patch SHA-256 does not match PLAYER_PACKAGE_MANIFEST.json" >&2
    exit 1
}

for command in cargo rustc git make pkg-config meson ninja x86_64-w64-mingw32-gcc objdump python3; do
    command -v "$command" >/dev/null || { echo "missing MSYS2 command: $command" >&2; exit 1; }
done
if ! git -C "$repo_root" diff --quiet; then
    echo "tracked source changes are not allowed for a reproducible player build" >&2
    exit 1
fi
mkdir -p "$work/src" "$work/build" "$work/prefix" "$work/stage/bin" "$work/stage/lib"

echo '::group::OpenJOC player Windows prerequisites and source pinning'
phase=PINNED_SOURCE_FETCH

fetch_checkout() {
    local url=$1 commit=$2 destination=$3
    mkdir -p "$destination"
    git -C "$destination" init -q
    git -C "$destination" remote add origin "$url" 2>/dev/null || true
    local attempt=1
    while ! git -C "$destination" fetch --depth=1 origin "$commit"; do
        if [[ "$attempt" -ge 3 ]]; then
            echo "failed to fetch pinned source after $attempt attempts: $url @ $commit" >&2
            exit 1
        fi
        attempt=$((attempt + 1))
    done
    git -C "$destination" checkout --detach --quiet FETCH_HEAD
    [[ "$(git -C "$destination" rev-parse HEAD)" == "$commit" ]]
    [[ -z "$(git -C "$destination" status --porcelain)" ]]
}

ffmpeg_source="$work/src/ffmpeg"
mpv_source="$work/src/mpv"
fetch_checkout https://github.com/FFmpeg/FFmpeg.git "$ffmpeg_commit" "$ffmpeg_source"
fetch_checkout https://github.com/mpv-player/mpv.git "$mpv_commit" "$mpv_source"
git -C "$ffmpeg_source" apply --check "$ffmpeg_patch"
git -C "$ffmpeg_source" apply "$ffmpeg_patch"
git -C "$mpv_source" apply --check "$mpv_patch"
git -C "$mpv_source" apply "$mpv_patch"
echo '::endgroup::'

echo '::group::OpenJOC C ABI'
phase=OPENJOC_C_ABI
rust_target=x86_64-pc-windows-gnu
rustup target add "$rust_target"
cargo build --manifest-path "$repo_root/Cargo.toml" -p openjoc-capi --release --target "$rust_target" --locked
openjoc_prefix="$work/prefix/openjoc"
"$repo_root/integrations/ffmpeg/native/stage-openjoc.sh" "$openjoc_prefix" "$rust_target"
echo '::endgroup::'

echo '::group::Patched FFmpeg configure and build'
phase=FFMPEG_CONFIGURE_BUILD
ffmpeg_prefix="$work/prefix/ffmpeg"
(
    cd "$work/build"
    PKG_CONFIG_PATH="$openjoc_prefix/lib/pkgconfig" \
    "$ffmpeg_source/configure" --prefix="$ffmpeg_prefix" \
        --target-os=mingw32 --arch=x86_64 --enable-cross-compile \
        --cc=x86_64-w64-mingw32-gcc --pkg-config=pkg-config \
        --disable-doc --disable-debug --disable-autodetect \
        --disable-static --enable-shared --enable-version3 \
        --enable-libopenjoc --disable-programs --disable-network
)
make -C "$work/build" -j"${CARGO_BUILD_JOBS:-2}"
make -C "$work/build" install
echo '::endgroup::'

echo '::group::Patched mpv configure and build'
phase=MPV_CONFIGURE_BUILD
mpv_prefix="$work/prefix/mpv"
(
    cd "$work"
    PKG_CONFIG_PATH="$ffmpeg_prefix/lib/pkgconfig:$openjoc_prefix/lib/pkgconfig:/mingw64/lib/pkgconfig" \
    MSYS_NO_PATHCONV=1 meson setup "$work/build/mpv" "$mpv_source" --prefix=/usr --buildtype=release \
        -Dtests=false -Dmanpage-build=disabled -Dhtml-build=disabled -Dpdf-build=disabled
)
PATH="$openjoc_prefix/bin:$ffmpeg_prefix/bin:/mingw64/bin:$PATH" \
    meson compile -C "$work/build/mpv" -j "${CARGO_BUILD_JOBS:-2}"
PATH="$openjoc_prefix/bin:$ffmpeg_prefix/bin:/mingw64/bin:$PATH" \
    DESTDIR="$mpv_prefix" meson install -C "$work/build/mpv"
cp "$mpv_prefix/usr/bin/mpv.exe" "$work/stage/bin/mpv.exe"
echo '::endgroup::'

echo '::group::Windows package assembly and dependency audit'
phase=DLL_CLOSURE_AND_PACKAGE
cp "$openjoc_prefix/bin/openjoc_capi.dll" "$work/stage/bin/openjoc_capi.dll"
for file in "$ffmpeg_prefix"/bin/*.dll; do [[ -f "$file" ]] && cp "$file" "$work/stage/bin/"; done

python3 "$repo_root/scripts/player-package.py" bundle \
    --stage-root "$work/stage" --output "$output" --platform windows-x64 \
    --search-dir "$ffmpeg_prefix/bin" --search-dir "$openjoc_prefix/bin" \
    --search-dir /mingw64/bin --ffmpeg-source "$ffmpeg_source" \
    --mpv-source "$mpv_source" --private-prefix "$work" \
    --toolchain "$(rustc -vV | tr '\n' '; '); compiler=$(x86_64-w64-mingw32-gcc --version | head -n 1); msys2=$(uname -srv)"
echo '::endgroup::'

echo '::group::Extracted Windows package runtime smoke'
phase=PACKAGE_EXTRACTION_RUNTIME
archive=$(find "$output" -maxdepth 1 -name 'openjoc-mpv-*-windows-x64.zip' -type f -print | head -n 1)
extract="$work/extracted"
mkdir -p "$extract"
unzip -q "$archive" -d "$extract"
root=$(find "$extract" -mindepth 1 -maxdepth 1 -type d -print | head -n 1)
python3 "$repo_root/scripts/player-package.py" verify --root "$root" --platform windows-x64 --run-smoke --missing-dependency-smoke
echo '::endgroup::'
phase=COMPLETE
echo "OpenJOC Windows player packaging complete: $output"
