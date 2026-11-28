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
    set -- --target "$rust_target"
else
    set --
fi

# The artifact reported by this build already accounts for the configured
# target directory and any explicit or implicit target triple.
cargo_output=$(cargo build --manifest-path "$repo_root/Cargo.toml" \
    -p openjoc-capi --release --locked --message-format=json-render-diagnostics "$@")
archive=$(printf '%s\n' "$cargo_output" | python3 -c '
import json, sys
archives = set()
for line in sys.stdin.buffer:
    if not line.startswith(b"{"):
        continue
    message = json.loads(line)
    target = message.get("target", {})
    if (message.get("reason") == "compiler-artifact"
            and target.get("name") == "openjoc_capi"
            and "staticlib" in target.get("crate_types", [])):
        for filename in message.get("filenames", []):
            if filename.replace(chr(92), "/").rsplit("/", 1)[-1] == "libopenjoc_capi.a":
                archives.add(filename)
if len(archives) != 1:
    sys.exit("Cargo did not report exactly one OpenJOC C ABI static archive")
sys.stdout.buffer.write(archives.pop().encode("utf-8"))
')
host_os=$(uname -s)
case "$host_os" in
    MINGW*|MSYS_NT*|CYGWIN*)
        case "$archive" in
            /*) ;;
            *) archive=$(cygpath -u "$archive") ;;
        esac
        ;;
esac
case "$archive" in
    /*) target_dir=${archive%/*} ;;
    *)
        echo "Cargo did not report an absolute OpenJOC C ABI artifact path" >&2
        exit 1
        ;;
esac

mkdir -p "$stage_prefix/include" "$stage_prefix/lib/pkgconfig" "$stage_prefix/bin"
install -m 0644 "$repo_root/crates/openjoc-capi/include/openjoc.h" \
    "$stage_prefix/include/openjoc.h"
install -m 0644 "$archive" \
    "$stage_prefix/lib/libopenjoc_capi.a"

case "$host_os" in
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
        echo "dynamic C ABI staging is not implemented for $host_os" >&2
        exit 1
        ;;
esac

# Use Cargo's resolved package version, including workspace inheritance.
# Python replacement also preserves literal &, |, and backslashes in prefixes.
cargo metadata --manifest-path "$repo_root/Cargo.toml" --format-version 1 --no-deps \
    | python3 -c '
import json, pathlib, sys
metadata = json.load(sys.stdin)
version = next(p["version"] for p in metadata["packages"] if p["name"] == "openjoc-capi")
template = pathlib.Path(sys.argv[1]).read_text()
pathlib.Path(sys.argv[3]).write_text(template.replace("@PREFIX@", sys.argv[2]).replace("@VERSION@", version))
' "$repo_root/crates/openjoc-capi/openjoc.pc.in" "$stage_prefix" \
    "$stage_prefix/lib/pkgconfig/openjoc.pc"
