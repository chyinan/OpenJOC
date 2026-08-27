# CLI reference

This page was audited against the v0.13.0 executable output from:

```sh
cargo run -p openjoc-cli --locked -- --help
cargo run -p openjoc-cli --locked -- render-joc --help
cargo run -p openjoc-cli --locked -- export-adm --help
```

The CLI source in `crates/openjoc-cli/src/main.rs` remains the source of truth. Re-run those commands when command syntax changes.

## Commands

```text
openjoc inspect <FILE> [--trim-config-count N]
openjoc decode <FILE> -o <DIR> [--downmix <FILE> | --internal-base] [--streaming]
openjoc export-adm <INPUT|SCENE_DIR> -o <OUTPUT.wav|OUTPUT.bw64> [--adm-policy best-effort|strict] [--overwrite]
openjoc validate-adm <FILE> [--json]
openjoc self-test [--fixture <JOC.ec3>]
openjoc diagnose-tools <FILE> --vector-id <ID> --json <OUTPUT>
openjoc census [MANIFEST] -o <DIR>
openjoc diagnose-oamd <FILE> [-o <DIR>] [--access-unit N | --au START..END | --all-access-units]
openjoc render-scene <SCENE> --binaural-sofa <FILE> --output <DIR> --backend direct|partitioned
openjoc render-joc <FILE> (--layout <PRESET> | --layout-file <CUSTOM.json>) --output <OUTPUT.wav|OUTPUT.caf>
openjoc decode-payload --downmix <FILE> --joc <FILE> --oamd <FILE> -o <DIR>
openjoc sofa inspect <FILE> [--json]
openjoc --version
```

## `render-joc`

```text
usage: openjoc render-joc <FILE> [--topology <TOPOLOGY.json>] (--layout <PRESET> | --layout-file <CUSTOM.json>) --output <OUTPUT.wav|OUTPUT.caf>
       [--downmix auto|loro|ltrt] (2.0 speaker output only; not binaural)
       [--dialnorm default|digital|analog] [--normalize-peak <TARGET_DBFS>]
       [--binaural [--sofa <HRTF.sofa>] [--virtual-layout <LAYOUT>] | --binaural-sofa <HRTF.sofa>]
       [--backend direct|partitioned --partition-size N]
       [--lfe-policy exclude|equal-power-dual-mono]
       [--validation-profile auto|etsi-strict|observed-vendor-compat]
       [--trim-config-count N] [--internal-base-policy current-default|codec-core]
       [--drc disabled|line|rf|custom] [--drc-boost 0..=100 --drc-cut 0..=100]
       [--reference-f64] [--diagnostic-contribution full|base-only|reconstruction-only]
       [--no-progress] [--performance-report <FILE.json>] [--overwrite]
```

Supported presets are `2.0`, `5.1`, `5.1.2`, `5.1.4`, `7.1`, `7.1.2`, `7.1.4`, `7.1.6`, `9.1`, `9.1.2`, `9.1.4`, `9.1.6`, and `22.2`. `--layout-file` accepts versioned custom spherical geometry; presets remain the ordinary path.

`--drc` controls encoded E-AC-3 dynamic-range metadata. `--dialnorm` controls programme calibration. `--normalize-peak` applies one optional static file-output scalar after rendering. None of these options is a limiter or LUFS/true-peak normalizer.

## `export-adm`

```text
usage: openjoc export-adm <INPUT|SCENE_DIR> -o <OUTPUT.wav|OUTPUT.bw64> [--adm-policy best-effort|strict] [--no-progress] [--overwrite]
```

The command exports a reconstructed RIFF/RF64 ADM BWF representation. It cannot recover the original ADM master. Best-effort is the default; strict rejects unsupported or unresolved dynamic binding.

## Output and profile boundaries

- `ETSI_STRICT` is never auto-downgraded.
- `OBSERVED_VENDOR_COMPAT` is explicit and partial.
- Non-seekable or fragmented MP4 streaming is not admitted.
- `render-scene` accepts explicit static sources and the strict local SOFA subset only.
- ReconstructionBasis rows are not authored-object PCM.
- `--overwrite` is required for existing outputs in non-interactive execution; replacements remain transactional.
