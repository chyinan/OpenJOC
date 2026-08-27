# Reconstructed ADM semantics

The ADM exporter writes a transportable representation of decoded JOC object signals and the spatial metadata that OpenJOC can bind within its admitted profile.

It does not export a speaker render, FinalLinkedGain output, or HRTF output. Export occurs at the decoder-domain scene boundary:

```text
decoded JOC object PCM + decoded OAMD
                │
                ▼
       carrier-local binding gate
                │
                ▼
       reconstructed ADM Objects
```

## What is serialized

- decoded object PCM as signed 24-bit little-endian PCM;
- generated ADM Object, channel, stream, track, and TrackUID identities;
- decoded OAMD position events within the admitted profile;
- a minimum legal 5.1 DirectSpeakers bed when Base LFE is present;
- RIFF/RF64 accounting, `chna`, public `dbmd`, and EBUCore XML relationships;
- an adjacent JSON report with mapping, omissions, headroom, and recovery-state fields.

For the admitted dynamic path, OpenJOC maps finite normalized OAMD room coordinates to normalized ADM Cartesian positions:

```text
ADM X = 2 × OAMD X - 1
ADM Y = 1 - 2 × OAMD Y
ADM Z = OAMD Z
```

ADM position blocks use decoded sample-domain event boundaries. The exporter emits the target profile's first-block `0` and subsequent-block `250` sample jump interpolation metadata; it does not copy the source OAMD ramp value into an invented ADM field.

## What is not serialized as recovered truth

Generated names, numbers, UIDs, and track assignments belong to OpenJOC. The exporter does not recover authored DAW/Logic identity, original ADM hierarchy, source-stem PCM, unquantized automation, Dolby authoring provenance, or a lossless JOC-to-ADM inverse.

See [Decoded Objects vs authored Objects](../concepts/decoded-vs-authored-objects.md) for the identity model and [Reconstructed ADM export](reconstructed-adm-export.md) for the complete file and policy contract.
