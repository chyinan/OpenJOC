# Binaural and SOFA

`--binaural` renders an OpenJOC speaker field to two-channel headphone output. It is virtual-speaker rendering; it is not a direct-object or proprietary renderer-fidelity claim.

OpenJOC provides two generic built-in profiles: `SADIE II — KU100` (the
default/reference profile) and `SADIE II — KEMAR`. Custom SOFA files remain supported. Different listeners may prefer
different non-individual HRTFs because HRTF perception depends strongly on
individual anatomy; no profile is claimed to be best for everyone.

See the [built-in HRTF record](https://github.com/chyinan/OpenJOC/blob/master/docs/hrtf.md)
for provenance, license, source hashes, and the preparation record.

```sh
openjoc render-joc input.m4a \\
  --binaural \\
  --output headphones.wav
```

The default virtual layout is 7.1.4. The CLI uses the bundled offline SADIE II D1 / KU100 profile unless you select another HRTF. Pass a built-in profile ID with `--binaural-hrtf`:

## Select a built-in HRTF

Pass a stable preset ID to `--binaural-hrtf`; the option accepts IDs, not display names. Omitting it keeps the default SADIE II D1 / KU100 profile:

```sh
openjoc render-joc input.m4a --binaural --binaural-hrtf sadie-ii-d1-ku100 --output headphones-ku100.wav
openjoc render-joc input.m4a --binaural --binaural-hrtf sadie-ii-d2-kemar --output headphones-kemar.wav
```

To use your own SOFA file, pass `--binaural-sofa listener.sofa` (or `--sofa listener.sofa`) instead. When a SOFA path is supplied, that file is used instead of a built-in preset:

```sh
openjoc render-joc input.m4a \\
  --binaural \\
  --binaural-sofa listener.sofa \\
  --backend direct \\
  --output custom-headphones.wav
```

`--virtual-layout 9.1.6` selects the canonical experimental 16-channel virtual
layout (`FL, FR, FC, LFE, Lb, Rb, Ls, Rs, Lw, Rw, Ltf, Rtf, Ltm, Rtm, Ltr,
Rtr`). The virtual feeds still pass through the same SOFA/HRTF backend and the
final output remains two-channel binaural PCM. The default remains 7.1.4;
9.1.6 is not a claim of perceptual superiority.

## SOFA scope

The loader accepts `SimpleFreeFieldHRIR` in NetCDF classic CDF-1 or NetCDF-4/HDF5 files. It keeps the existing field, two-receiver, coordinate, and direction-coverage checks. HRIRs are resampled to 48 kHz with a bounded, deterministic windowed-sinc filter. The 48 kHz path keeps its original coefficients bit-for-bit. Fractional source delays remain unsupported; converted integer delays are rounded to the nearest 48 kHz sample.

Rate conversion preserves FIR convolution gain with a fixed rate-ratio scale.
It retains fractional timing in the coefficients and adds a common filter
delay to both ears to preserve the complete sinc precursor. This delay is
`ceil(16 * max(output_rate / source_rate, 1)) + 1` output samples: 19 samples
for 44.1→48 kHz and 17 for 96→48 kHz. It remains in the HRIR and its complete
tail. The reported renderer latency includes this added filter delay and
continues to exclude the SOFA's original measured delays.
Matching-rate input adds no delay. Conversion ratios above 16:1 are rejected.
With `equal-power-dual-mono`, the LFE receives the same added delay, including
its drained tail, to preserve timing relative to the spatial channels.

Standard fixed `ReceiverPosition [R,C,1]` and `Data.Delay [1,R]` layouts are
accepted. Receiver positions are listener-local; rotating or translating the
listener does not change the ear assignment. `ListenerUp` inherits coordinate
type and units from `ListenerView` when its own attributes are omitted.
`Data.Delay` is in samples even without a `Units` attribute. Explicit incompatible
coordinate or delay units are rejected. Listener positions and orientation
vectors must use Cartesian coordinates in metres; spherical listener metadata
is not currently supported.

Each HDF5 chunk must decompress to at most 16 MiB, further limited
by the configured file-byte and coefficient budgets. Oversized chunks are
rejected before decompression.

Inspect a file before using it:

```sh
openjoc sofa inspect listener.sofa --json
```

`sofa inspect` reports the source sample rate; rendering converts HRIRs to 48 kHz before either binaural backend starts. `direct` is the numerical reference backend. `partitioned` uses one fixed power-of-two partition size and preserves the complete input and FIR tail. Both backends fail closed on unsupported coverage.

## LFE policy

The CLI defaults to `exclude`. Use `equal-power-dual-mono` when you explicitly want the logical LFE contribution sent to both ears:

```sh
openjoc render-joc input.m4a \\
  --binaural \\
  --lfe-policy equal-power-dual-mono \\
  --output headphones-with-lfe.wav
```

The choice is a renderer policy. It does not infer a physical subwoofer or alter the source scene.
