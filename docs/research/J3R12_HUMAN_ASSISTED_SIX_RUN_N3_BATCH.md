# J3R12 — Human-Assisted Six-Run N3 Producer Batch

Status: **bounded producer batch complete; provenance admitted**

Decision: `J3R12_HUMAN_ASSISTED_SIX_RUN_N3_BATCH_COMPLETED_AND_PROVENANCE_ADMITTED`

## Scope

J3R12 executed the already-frozen six-run replacement queue as three
same-condition pairs using Logic Pro 12.3, Dolby Digital Plus with Dolby Atmos,
Music profile, 768 kbps, and 48 kHz. No Logic fixture was created: the six
disposable project copies were prepared before this stage and were used only as
the explicitly authorized producer inputs.

The human action was limited to one mechanical click of the current Logic Save
button per run after the controller verified the project, process identity,
panel settings, discovery realm, output leaf, and consumed nonce. Human
judgment and parameter choice were not used.

## Admitted runs

| Pair | Runs | AU count | Raw EC-3 determinism |
| --- | --- | ---: | --- |
| `N3R2_S_FL_PAIR` | `N3R2_S_FL_0`, `N3R2_S_FL_1` | 129 / 129 | byte-identical |
| `N3R2_S_FR_PAIR` | `N3R2_S_FR_0`, `N3R2_S_FR_1` | 129 / 129 | byte-identical |
| `N3R2_D_SWAP_PAIR` | `N3R2_D_SWAP_0`, `N3R2_D_SWAP_1` | 126 / 126 | byte-identical |

All six outputs were observed exactly once in the frozen bounded discovery
realm, each with a consumed nonce and a `RUN_VERIFIED` controller state. The
stream-copied elementary carriers are the determinism boundary. MP4 container
hashes differ within pairs, which is not treated as raw-carrier nondeterminism.

## Structural boundary

The bounded audit confirms raw E-AC-3 extraction, sample rate, frame/AU count,
and the unchanged profile behavior for every output:

- `ETSI_STRICT` continues to fail on the observed real-world signaling
  deviations.
- `DOLBY_VENDOR_COMPAT` continues to report
  `accepted_with_deviation`.
- No warp-3 alias, vendor semantic rule, trim continuation, or parser change
  was added.

The bounded inspect path does not expose payload IDs, `object_count`,
`element_count`, or metadata block count without a trim configuration. These
fields are recorded as unavailable; none is inferred from hidden configuration
or from output length.

## Scientific boundary

This batch admits six producer carriers and their provenance only. It does not
admit a producer envelope, C1/C2, JOC reconstruction, ObjectScene, authored-
object PCM, renderer result, or semantic binding. `SemanticBindingState` remains
`Unresolved`; reserved warp raw 3 remains unresolved and unchanged.

Private evidence is frozen in the J3R12 run package, including per-run
attestations, pair hashes, structural audits, and deterministic JSON/text
outputs.
