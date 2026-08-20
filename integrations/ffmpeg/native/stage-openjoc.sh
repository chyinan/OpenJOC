#!/bin/sh
set -eu

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
    echo "usage: $0 ABSOLUTE_PREFIX [RUST_TARGET]" >&2
    exit 2
fi

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/../../.." && pwd)
stage_prefix=$1
rust_target=${2-}
cargo_home=${CARGO_HOME:-$HOME/.cargo}
existing_rustflags=${RUSTFLAGS:-}
export RUSTFLAGS="${existing_rustflags} --remap-path-prefix=$repo_root=/openjoc --remap-path-prefix=$cargo_home=/cargo"

case "$stage_prefix" in
    /*) ;;
    *)
        echo "stage prefix must be an absolute path" >&2
        exit 2
        ;;
esac

if [ -n "$rust_target" ]; then
    cargo build --manifest-path "$repo_root/Cargo.toml" \
        -p openjoc-capi --release --target "$rust_target" --locked
    target_dir="$repo_root/target/$rust_target/release"
else
    cargo build --manifest-path "$repo_root/Cargo.toml" \
        -p openjoc-capi --release --locked
    target_dir="$repo_root/target/release"
fi

mkdir -p "$stage_prefix/include" "$stage_prefix/lib/pkgconfig" "$stage_prefix/bin"
install -m 0644 "$repo_root/crates/openjoc-capi/include/openjoc.h" \
    "$stage_prefix/include/openjoc.h"
install -m 0644 "$target_dir/libopenjoc_capi.a" \
    "$stage_prefix/lib/libopenjoc_capi.a"

case "$(uname -s)" in
    Darwin)
        install -m 0755 "$target_dir/libopenjoc_capi.dylib" \
            "$stage_prefix/lib/libopenjoc_capi.dylib"
        install_name_tool -id '@rpath/libopenjoc_capi.dylib' \
            "$stage_prefix/lib/libopenjoc_capi.dylib"
        ;;
    Linux)
        install -m 0755 "$target_dir/libopenjoc_capi.so" \
            "$stage_prefix/lib/libopenjoc_capi.so"
        ;;
    MINGW*|MSYS_NT*|CYGWIN*)
        install -m 0755 "$target_dir/openjoc_capi.dll" \
            "$stage_prefix/bin/openjoc_capi.dll"
        import_library=
        for candidate in \
            "$target_dir/openjoc_capi.dll.a" \
            "$target_dir/libopenjoc_capi.dll.a"; do
            if [ -f "$candidate" ]; then
                import_library=$candidate
                break
            fi
        done
        if [ -z "$import_library" ]; then
            echo "OpenJOC GNU import library was not produced" >&2
            find "$target_dir" -maxdepth 1 -type f -name '*openjoc_capi*' -print >&2
            exit 1
        fi
        install -m 0644 "$import_library" \
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
