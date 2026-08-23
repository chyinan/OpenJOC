#!/bin/sh
set -eu

repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_dir"

cargo build -p openjoc-capi

cc -std=c11 -Wall -Wextra -Werror \
  -Icrates/openjoc-capi/include \
  crates/openjoc-capi/examples/c_api_example.c \
  target/debug/libopenjoc_capi.a -ldl -lm \
  -o target/openjoc-c-api-example
target/openjoc-c-api-example

c++ -std=c++17 -Wall -Wextra -Werror \
  -Icrates/openjoc-capi/include \
  -c crates/openjoc-capi/examples/c_api_header.cpp \
  -o target/openjoc-c-api-header.o

fixture_dir=$(mktemp -d /tmp/openjoc-c-api-fixture.XXXXXX)
trap 'rm -rf "$fixture_dir"' EXIT
scripts/generate-player-fixtures.sh "$fixture_dir"

cc -std=c11 -Wall -Wextra -Werror \
  -Icrates/openjoc-capi/tests/fixtures \
  crates/openjoc-capi/tests/abi13_caller.c \
  target/debug/libopenjoc_capi.a -ldl -lm \
  -o target/openjoc-abi13-caller
target/openjoc-abi13-caller "$fixture_dir/joc.ec3"
