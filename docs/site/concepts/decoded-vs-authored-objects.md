# Decoded Objects vs authored Objects

These are three different things:

```text
original authored Atmos Objects
        ≠ decoded JOC output Objects
        ≠ reconstructed ADM Objects
```

## Original authored Objects

The authored Objects live in the source DAW or ADM master. They may have names, UIDs, hierarchy, track identity, source-stem PCM, unquantized automation, and authoring metadata.

OpenJOC does not recover those properties from a lossy JOC delivery stream.

## Decoded JOC output Objects

JOC reconstruction produces carrier-local decoder outputs. OpenJOC represents those signals as `ReconstructionBasis` rows. Within the exact admitted profile, each row can be paired with the corresponding decoded OAMD movement by typed carrier-local ordinals.

That pair means:

```text
joc_ordinal = j
oamd_dynamic_ordinal = j
oamd_total_index = j + 1
```

The `+1` accounts for the leading Base LFE in the total OAMD list. It is not a lookup into authored IDs and it is not a PCM heuristic.

## Reconstructed ADM Objects

`export-adm` serializes generated ADM Objects from the decoded scene. Names such as `OpenJOC Reconstructed JOC Object 04` describe an OpenJOC output identity. They do not prove that the signal was authored Object 04.

The export can preserve meaningful decoded movement. That movement can differ from source automation because JOC may quantize metadata, reorganize object representation, change numbering, or discard authoring information.

## Recovery state

The exporter keeps these claims explicit:

| Claim | State |
| --- | --- |
| Original authored identity recovered | `false` |
| Original ADM master recovered | `false` |
| Lossless JOC-to-ADM round trip | `false` |

`ResolvedWithinCarrier` means only that the decoded JOC PCM ↔ decoded OAMD relation passed the scoped admission gate. It never upgrades to authored identity or renderer parity.

## Practical rule

Use the reconstructed ADM file to inspect and interoperate with the decoded scene carried by JOC. Use native JOC playback as the reference when you require native-renderer localization. Read [Reconstructed ADM renderer equivalence](../compatibility/renderer-equivalence.md) for the tested scope and remaining guarantee.
