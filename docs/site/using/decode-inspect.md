# Decode and inspect

The CLI separates inspection, diagnostic decode, and final rendering.

## Inspect a carrier

```sh
openjoc inspect input.ec3
```

`inspect` reports access-unit structure, JOC metadata, profile selection, and bounded parser results. It does not write a final audio render.

For machine-readable output or additional detail:

```sh
openjoc inspect input.ec3 --json
openjoc inspect input.ec3 --aus --objects --emdf
openjoc inspect input.ec3 --au-range 10:20 --json
openjoc inspect input.ec3 --verbose
```

`--json` writes the versioned report to stdout. `--aus` includes access-unit details. `--au N` selects one zero-based access unit, while `--au-range START:END` selects an inclusive range; either selection leaves the full-stream census intact. `--objects` adds per-OAMD-slot statistics, `--emdf` expands human-readable EMDF configuration details, and `--verbose` adds bounded diagnostic explanations. Use the [CLI reference](../reference/cli-reference.md#inspect) for the complete syntax.

## Capture diagnostic decoder output

```sh
openjoc decode input.ec3 --output decoded
```

The capture path writes metadata manifests, a truthful decoded-component manifest, and diagnostic ReconstructionBasis row WAVs. These rows are decoder outputs. They are not authored-object stems.

For a bounded streaming decode with internal base diagnostics:

```sh
openjoc decode input.ec3 \\
  --output decoded \\
  --internal-base \\
  --streaming
```

The Rust packet API accepts one complete General E-AC-3 JOC access unit per push: I0 plus ordered D0..D7 within the public maximum. CMAF and the existing legacy AC-3 Annex-J combination remain D0-only. Demuxing, arbitrary byte fragmentation, and multiple access units belong to the bounded stream decoder or framework adapters.

## Select a validation profile

`auto` is the default policy for decode. `etsi-strict` never falls back. `observed-vendor-compat` is an explicit partial policy that retains opaque continuation without assigning it vendor semantics.

```sh
openjoc decode input.ec3 \\
  --output decoded \\
  --validation-profile etsi-strict
```

When a stream uses a reserved OAMD value such as raw `warp=3`, strict rejection is an expected profile result, not a silent downgrade.
