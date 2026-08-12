# J3R2 — Six-Run N3 Producer Export Batch

J3R2 freezes a bounded, exact-condition producer batch for the N3
ReconstructionBasis question. The batch uses Logic Pro 12.3 with Dolby Digital
Plus Atmos, Music, 768 kbps, Project scope, and no ADM companion export.
`SemanticBindingState` remains `Unresolved`.

## Batch

Six authorized producer executions were completed, without creating a new
Logic fixture or changing the controlled source projects:

```text
S_FL_PRODUCER_0, S_FL_PRODUCER_1
S_FR_PRODUCER_0, S_FR_PRODUCER_1
D_SWAP_PRODUCER_0, D_SWAP_PRODUCER_1
```

Each execution used its own approved output parent and produced exactly one
stream-copied E-AC-3 carrier. The three within-condition pairs were all
bytewise different. This is recorded as producer variability; no retry was
used and no determinism pass is claimed.

All six carriers were recognized as ISO BMFF E-AC-3 containers at 48 kHz with
1536 samples per access unit and 3072-byte packet frames. The static pairs have
129 access units each; the dual-object swap pair has 126 access units each.
The inspect path closed the observed audio-block, skip-field, and EMDF
boundaries for every carrier.

## Decoder boundary

`ETSI_STRICT` behavior is unchanged: the observed commercial signaling remains
a normative validation failure, including `codecdatae=0`,
`payload_frame_aligned=0`, and the reserved warp value. The explicit
`DOLBY_VENDOR_COMPAT` profile accepts the observed carrier with deviations; no
new vendor rule or warp interpretation was added.

With the explicit diagnostic `trim-config-count=1`, the internal f32 capture
path produced 15 ReconstructionBasis rows for every carrier. Repeated captures
from the same frozen input were byte-identical at the retained JSON and WAV
boundaries, all retained samples were finite, and row lengths matched the
carrier duration. RcLfe remains a separate base-LFE artifact and is not part of
the row semantics. Diagnostic names continue to describe reconstruction-basis
rows, never authored-object PCM.

The reference-f64 probe was not admitted: the first bounded attempt exhausted
available space while the CLI was emitting unbounded per-frame debug output.
Full six-way capture/stream equivalence was likewise not claimed; one
representative streaming smoke run per condition was retained. These are
declared limitations, not relaxed thresholds.

## Result and boundary

```text
RECONSTRUCTION_BASIS_NUMERICAL_HANDLING_STRENGTHENED
```

The result establishes a finite, structurally stable, repeated f32 numerical
signature for the retained ReconstructionBasis output within this batch. It
does not establish authored-object identity, object PCM, ObjectScene audio
binding, renderer fidelity, or any meaning for the reserved warp value.

Private evidence contains the exact carrier and capture hashes, output-parent
provenance, canonical compressed decode cache, and deterministic report hashes.
Producer media and private evidence are intentionally excluded from this
repository document.
