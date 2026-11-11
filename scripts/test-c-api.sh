#!/bin/sh
set -eu

repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_dir"

fixture_dir=$(mktemp -d "${TMPDIR:-/tmp}/openjoc-c-api-fixture.XXXXXX")
trap 'rm -rf "$fixture_dir"' EXIT

# Cargo owns the output layout, including configured target directories/triples.
# Select the archive from this successful build instead of a possibly stale
# target/debug library. json-render-diagnostics keeps compiler errors visible.
cargo build -p openjoc-capi --message-format=json-render-diagnostics > "$fixture_dir/build.jsonl"
static_library=$(python3 - "$fixture_dir/build.jsonl" <<'PYTHON'
import json
from pathlib import Path
import sys

archives = []
with open(sys.argv[1], encoding="utf-8") as stream:
    for line in stream:
        message = json.loads(line)
        target = message.get("target", {})
        if (message.get("reason") == "compiler-artifact"
                and target.get("name") == "openjoc_capi"
                and "staticlib" in target.get("crate_types", [])):
            archives.extend(path for path in message["filenames"] if path.endswith(".a"))
if len(archives) != 1 or not Path(archives[0]).is_file():
    raise SystemExit("expected one existing openjoc-capi static library from Cargo")
print(archives[0])
PYTHON
)
artifact_dir=$(dirname "$static_library")

cc -std=c11 -Wall -Wextra -Werror \
  -Icrates/openjoc-capi/include \
  crates/openjoc-capi/examples/c_api_example.c \
  "$static_library" -ldl -lm \
  -o "$artifact_dir/openjoc-c-api-example"
"$artifact_dir/openjoc-c-api-example"

c++ -std=c++17 -Wall -Wextra -Werror \
  -Icrates/openjoc-capi/include \
  -c crates/openjoc-capi/examples/c_api_header.cpp \
  -o "$artifact_dir/openjoc-c-api-header.o"

cc -std=c11 -Wall -Wextra -Werror \
  -Icrates/openjoc-capi/include \
  crates/openjoc-capi/tests/orientation_22_2_caller.c \
  "$static_library" -ldl -lm \
  -o "$artifact_dir/openjoc-orientation-22-2-caller"
"$artifact_dir/openjoc-orientation-22-2-caller"

scripts/generate-player-fixtures.sh "$fixture_dir"

cc -std=c11 -Wall -Wextra -Werror \
  -Icrates/openjoc-capi/tests/fixtures \
  crates/openjoc-capi/tests/abi13_caller.c \
  "$static_library" -ldl -lm \
  -o "$artifact_dir/openjoc-abi13-caller"
"$artifact_dir/openjoc-abi13-caller" "$fixture_dir/joc.ec3"
