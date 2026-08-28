# PCM24 headroom

JOC reconstruction is performed in floating-point form. A valid reconstructed sample can lie outside the normalized signed-24-bit range before file quantization.

If you want to investigate a standards-compatible storage path, read the
corresponding [open problem and contribution guidance](../project/open-problems.md)
first. The current fail-closed policy is intentional.

```text
valid floating-point reconstruction
        ≠
guaranteed PCM24 representability
```

## Export policy

The ADM exporter fails closed when a sample is non-finite or outside the signed 24-bit range. It does not:

- clip or saturate;
- normalize individual objects or tracks;
- apply a hidden limiter;
- silently attenuate the programme.

The adjacent ADM report includes a bounded headroom census with whole-programme and per-signal statistics when export succeeds.

This policy protects the meaning of the output. A real-media headroom case is not, by itself, a decoder failure; it means the valid floating reconstruction cannot be represented by the selected integer container without an explicit policy that the exporter intentionally does not invent.

If you need a floating-point speaker or binaural file, use the WAV/CAF render path. If you need reconstructed ADM, treat a range error as an actionable export decision rather than as permission to clip.
