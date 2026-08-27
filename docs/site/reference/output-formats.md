# Output formats

OpenJOC keeps renderer semantics and container semantics separate. The selected output extension controls the container, but it does not change the rendered PCM.

| Output | Contract |
| --- | --- |
| Standard preset to `.wav` | Truthful `WAVEFORMATEXTENSIBLE` identities and mask. |
| `7.1.6` or `9.1.x` to `.wav` | Fails closed; use semantic CAF. |
| `22.2` to `.wav` | Explicit unmasked 24-channel PCM in canonical order. |
| Custom layout to `.wav` | Explicit unmasked PCM in the declared order. |
| Preset or custom layout to `.caf` | Semantic channel descriptions; custom geometry can preserve coordinates. |
| Binaural | Two-channel L/R-ear output. |
| `export-adm` `.wav` / `.bw64` | Reconstructed ADM BWF with signed 24-bit PCM and an adjacent JSON report. |

LFE channels are logical destinations. They are not projection vertices, and OpenJOC does not perform crossover or Bass Management.

## Levels and latency

The recommended render order is:

```text
encoded DRC → programme dialnorm → JOC rendering
  → speaker FinalLinkedGain → optional static peak scalar → file
```

Speaker output reports 609 samples of availability delay. Binaural output reports 577 samples before its finite FIR tail is drained. Logical PTS is not shifted to hide this delay.

`export-adm` keeps floating-point reconstruction until the integer boundary. Its signed-24-bit writer rejects non-finite or out-of-range samples instead of clipping, normalizing, or silently attenuating them. See [PCM24 headroom](../compatibility/pcm24-headroom.md).
