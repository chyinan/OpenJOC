# Output formats

OpenJOC keeps renderer semantics and container semantics separate. The selected output extension controls the container, but it does not change the rendered PCM.

For speaker-layout rendering, both `.wav` and `.caf` support `22.2`; the
containers record its channel identities differently:

| Layout | WAV (`.wav`) | CAF (`.caf`) |
| --- | --- | --- |
| Standard presets representable by a WAVEFORMATEXTENSIBLE mask | Truthful `WAVEFORMATEXTENSIBLE` identities and mask. | Semantic channel labels. |
| `7.1.6` or `9.1.x` | Fails closed. | Semantic channel labels. |
| `22.2` | Explicit unmasked 24-channel PCM in canonical order. | Semantic output using standard CAF channel labels where available and coordinate descriptions for remaining positions, preserving the complete 22.2 channel identity. |
| Custom layout | Explicit unmasked PCM in the declared order. | Semantic channel descriptions; custom geometry uses coordinate descriptions. |

| Other output | Contract |
| --- | --- |
| Binaural | Two-channel L/R-ear output. |
| `export-adm` `.wav` / `.bw64` | Reconstructed ADM BWF with signed 24-bit PCM and an adjacent JSON report. |

LFE channels are logical destinations. They are not projection vertices, and OpenJOC does not perform crossover or Bass Management.

## Levels and latency

The recommended render order is:

```text
encoded DRC → programme dialnorm → JOC rendering
  → speaker FinalLinkedGain → optional static peak scalar → file
```

Speaker output reports 609 samples of availability delay. Binaural output reports 577 samples for built-in or 48 kHz custom HRIRs. Non-48 kHz custom SOFA adds the resampler's common causal filter delay. Source `Data.Delay` and the finite FIR tail are not added to the reported latency. Logical PTS is not shifted to hide this delay.

`export-adm` keeps floating-point reconstruction until the integer boundary. Its signed-24-bit writer rejects non-finite or out-of-range samples instead of clipping, normalizing, or silently attenuating them. See [PCM24 headroom](../compatibility/pcm24-headroom.md).
