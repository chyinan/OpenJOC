# Public synthetic JOC smoke fixture

The project-owned smoke fixture is generated on demand; no private or
commercial programme media is committed:

```sh
fixture_dir="$(mktemp -d /tmp/openjoc-public-fixture.XXXXXX)"
scripts/generate-player-fixtures.sh "$fixture_dir"
OPENJOC_PUBLIC_JOC_FIXTURE="$fixture_dir/joc.ec3" \
  cargo run -p openjoc-cli --release -- self-test --fixture "$fixture_dir/joc.ec3"
```

The generator uses the existing public-syntax synthetic JOC builder in the
`openjoc-ffmpeg` tests, retains one bounded raw E-AC-3/JOC access unit, and
creates a temporary MP4 wrapper plus ordinary codec controls for integration
qualification. Temporary PCM and media controls are not release inputs.

For the current 0.9 development commit, the deterministic raw fixture hash is:

```text
SHA-256 54b48754b915cef97c13752de5eace4a219da6599cdfcf26f92b5b6fffc6e3e4  joc.ec3
```

`openjoc self-test` reports `CLASSIFIER`, `DECODE`, `7.1.4`, `BINAURAL`, and
`HRTF` as `PASS` when this fixture is supplied. Without it, those optional
checks report `NOT_APPLICABLE` rather than silently passing.
