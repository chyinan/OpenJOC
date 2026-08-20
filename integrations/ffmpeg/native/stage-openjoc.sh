#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
    echo "usage: $0 ABSOLUTE_PREFIX" >&2
    exit 2
fi

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/../../.." && pwd)
stage_prefix=$1

case "$stage_prefix" in
    /*) ;;
    *)
        echo "stage prefix must be an absolute path" >&2
        exit 2
        ;;
esac

cargo build --manifest-path "$repo_root/Cargo.toml" \
    -p openjoc-capi --release --locked

mkdir -p "$stage_prefix/include" "$stage_prefix/lib/pkgconfig" "$stage_prefix/bin"
install -m 0644 "$repo_root/crates/openjoc-capi/include/openjoc.h" \
    "$stage_prefix/include/openjoc.h"
install -m 0644 "$repo_root/target/release/libopenjoc_capi.a" \
    "$stage_prefix/lib/libopenjoc_capi.a"

case "$(uname -s)" in
    Darwin)
        install -m 0755 "$repo_root/target/release/libopenjoc_capi.dylib" \
            "$stage_prefix/lib/libopenjoc_capi.dylib"
        install_name_tool -id '@rpath/libopenjoc_capi.dylib' \
            "$stage_prefix/lib/libopenjoc_capi.dylib"
        ;;
    Linux)
        install -m 0755 "$repo_root/target/release/libopenjoc_capi.so" \
            "$stage_prefix/lib/libopenjoc_capi.so"
        ;;
    MINGW*|MSYS_NT*|CYGWIN*)
        target_dir="$repo_root/target/x86_64-pc-windows-gnu/release"
        install -m 0755 "$target_dir/openjoc_capi.dll" \
            "$stage_prefix/bin/openjoc_capi.dll"
        install -m 0644 "$target_dir/openjoc_capi.dll.a" \
            "$stage_prefix/lib/libopenjoc_capi.dll.a"
        ;;
    *)
        echo "dynamic C ABI staging is not implemented for $(uname -s)" >&2
        exit 1
        ;;
esac

sed "s|@PREFIX@|$stage_prefix|g" \
    "$repo_root/crates/openjoc-capi/openjoc.pc.in" \
    > "$stage_prefix/lib/pkgconfig/openjoc.pc"
