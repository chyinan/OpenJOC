# J3R2 — Six-Run N3 Producer Batch: Recovery Classification

J3R2 produced six private, exact-condition Logic Pro 12.3 Dolby Digital Plus
Atmos carriers: two S_FL, two S_FR, and two D_SWAP. All remain available as
private producer originals. This document records their bounded structural
audit and, importantly, the provenance limit discovered during same-stage
recovery.

## Recoverable facts

All six outputs are recognized as ISO BMFF E-AC-3/JOC at 48 kHz with 1536
samples per access unit. S_FL and S_FR each contain 129 access units; D_SWAP
contains 126. The observed base, skip-field, and EMDF paths close structurally.
Each bounded f32 capture retained 15 ReconstructionBasis rows, finite samples,
and a separate RcLfe artifact.

Within every same-condition pair, the MP4 containers differ bytewise while the
stream-copied elementary E-AC-3 payload is identical. This is a byte-relation
observation only: it does not establish a producer variability envelope,
context dependence, or any semantic property of a reconstruction row.

`ETSI_STRICT` remains a normative failure for the observed commercial
signaling, including the reserved warp value. `DOLBY_VENDOR_COMPAT` continues
to accept with recorded deviations. No profile rule or warp interpretation was
added. `SemanticBindingState` remains `Unresolved`.

## Provenance admission result

The six physical outputs cannot be admitted as a completed J3R2 provenance
batch. The original durable records retained distinct PIDs, approved parents,
typed/final leaf names, and output hashes, but did not retain terminal nonce
states, process start times, Logic instance IDs, or export-completion records.
Those missing facts cannot be reconstructed truthfully after the executions.

```text
J3R2_N3_EXPORT_AUTHORIZATION_OR_DESTINATION_INTEGRITY_FAILED
```

This does not invalidate the physical carrier files or their structural audit.
It prevents the stronger claim that the three same-condition pairs are
provenance-admitted for a future producer-envelope experiment. J3R3 and all
C1/C2/C3 analysis therefore remain unauthorized.

## Debug-retention containment

The initial reference-f64 probe was not admitted. Its per-frame Debug output
formatted complete reconstruction arrays, creating a current-stage
regenerable-debug spill that exhausted disk space. It did not corrupt producer
outputs, run records, or repository files; the spill was inventoried and
removed under the current-stage recovery policy.

The CLI now fails closed under a bounded retention contract: at most 64 frame
debug records, at most 64 KiB per textual debug artifact, at most 128 MiB of
retained diagnostic PCM, and at most 128 MiB of ReconstructionBasis JSON.
After the frame limit, it writes one explicit truncation marker rather than
retaining additional frame traces. Per-sample reconstruction Debug formatting
is replaced by a structural summary. This is an output-retention safety repair,
not a decoder-semantic change.

Private evidence contains the per-run hashes, exact failure inventory, and the
deterministic recovery manifest. No private paths, media, or producer originals
are included in the repository.
