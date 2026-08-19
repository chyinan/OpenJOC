#!/bin/sh
set -eu

pkg-config --atleast-version=61 libavutil
pkg-config --atleast-version=63 libavcodec
pkg-config --atleast-version=63 libavformat

cargo build -p openjoc-ffmpeg --release --features ffmpeg --locked
cargo test -p openjoc-ffmpeg --features ffmpeg --locked

if [ "$#" -gt 0 ]; then
    target/release/openjoc-avdecode "$1" --binaural --null --checksum
fi
