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

The loader accepts the documented `SimpleFreeFieldHRIR` NetCDF classic CDF-1 subset. The file must provide two receivers, the matching sample rate, and exact or safely interpolatable coverage for every requested non-LFE virtual direction. HDF5/NetCDF-4, resampling, downloads, writing, moving sources, and universal coverage are not supported.

Inspect a file before using it:

```sh
openjoc sofa inspect listener.sofa --json
```

`direct` is the numerical reference backend. `partitioned` uses one fixed power-of-two partition size and preserves the complete input and FIR tail. Both backends fail closed on unsupported coverage or sample-rate mismatch.

## LFE policy

The CLI defaults to `exclude`. Use `equal-power-dual-mono` when you explicitly want the logical LFE contribution sent to both ears:

```sh
openjoc render-joc input.m4a \\
  --binaural \\
  --lfe-policy equal-power-dual-mono \\
  --output headphones-with-lfe.wav
```

The choice is a renderer policy. It does not infer a physical subwoofer or alter the source scene.
