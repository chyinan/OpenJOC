# Documentation semantic closure

Status: `PASS`

This audit follows the scoped decoded-JOC/OAMD implementation. It changes
current user-facing claims where they contradicted the implementation and
keeps dated research, release, and provenance statements intact.

## Canonical claim

For `E_AC_3_JOC_OBSERVED_ORDINARY_PROFILE` and the exact observed
`E_AC_3_JOC_OBSERVED_ORDINARY_COMPAT_WARP3_PROFILE`, OpenJOC can associate
decoded JOC object audio with corresponding decoded OAMD movement metadata and
export reconstructed dynamic ADM Objects. This is a carrier-local
decoded-object claim. It is not recovery of the original authored ADM master.

The admitted profiles are exactly 15 decoded JOC Objects, no bed, one leading
Base LFE, no ISF, 15 dynamic OAMD Objects, and 16 total OAMD entries. The
mapping is `joc_ordinal=j`, `oamd_dynamic_ordinal=j`, and
`oamd_total_index=j+1`. The compatibility variant is accepted only for the
known deviation family and exact opaque raw3 shape; raw3 remains reserved
under ETSI strict and receives no guessed transform.

## Semantic census

| Phrase or concept | Location | Classification | Result |
|---|---|---|---|
| Unresolved authored binding in the README | `README.md` | Stale current claim | Replaced with scoped decoded-object binding and an explicit authored-identity boundary. |
| Neutral/static ADM Objects | `docs/ADM_EXPORT.md` | Partly stale current claim | Clarified as the unsupported/unresolved-profile path; documented moving Objects for the admitted profile. |
| `dynamic position` not representable | `docs/ADM_EXPORT.md` | Stale by overbreadth | Position is admitted within scope; unsupported properties remain non-representable. |
| Metadata-only `ObjectScene` | `docs/ARCHITECTURE.md`, `docs/CAPABILITIES.md` | Still-true general model | Clarified that the exact profile adds a carrier-local decoded-object interpretation without authored identity. |
| `ReconstructionBasis` rows are not authored Objects | current docs and diagnostics | Still-true stronger authored-identity claim | Retained; admitted rows gain only decoded carrier-local meaning. |
| `T(t)` unresolved | `docs/ARCHITECTURE.md`, `docs/JOC_SPATIAL_BRIDGE.md` | Still-true renderer/operator claim | Retained and explicitly separated from the ADM decoded-object binding gate. |
| No authored-object binding in JOC rendering | `docs/CAPABILITIES.md`, renderer docs | Still-true stronger claim | Retained for authored identity and renderer fidelity. |
| Capability status for dynamic ADM | `docs/CAPABILITIES.md` | Stale/incomplete current matrix | Added separate `ADMITTED_WITH_SCOPE` rows for decoded-object binding and reconstructed dynamic ADM, plus `UNRESOLVED` profiles and non-admitted original recovery. |
| Report fields and original recovery flags | `docs/ADM_EXPORT.md`, `docs/KNOWN_LIMITATIONS.md` | Incomplete current explanation | Documented separately: binding state, dynamic metadata export, bound/unbound counts, authored identity false, ADM master false, and lossless round trip false. |
| Future binding requires evidence | `docs/ROADMAP.md` | Overbroad current wording | Narrowed it to broader or unknown-deviation profiles; the exact ordinary and observed raw3-compatible profiles are now current capability. |
| Old unresolved binding reports | `CHANGELOG.md`, `docs/PROVENANCE.md`, `docs/research/**` | Historical/research/archived claims | Intentionally retained as dated records; they describe the state at that time. |

## User-understandability answers

1. Reconstructed Objects move for the admitted profile because decoded JOC
   audio is paired with decoded OAMD position events.
2. Their motion is the encoded/decoded JOC scene, not guaranteed source DAW
   automation.
3. They are not the original authored Objects.
4. The PCM and delivery representation remain lossy; `lossless_round_trip` is
   false.
5. Generated IDs, names, and numbering are not original authored identities.
6. Dynamic binding is not universal; the exact profile gate is required.
7. Unsupported profiles remain neutral in best-effort mode or fail closed in
   strict mode, with a report reason.

## Current documentation audited

- `README.md`
- `docs/ADM_EXPORT.md`
- `docs/CAPABILITIES.md`
- `docs/KNOWN_LIMITATIONS.md`
- `docs/ARCHITECTURE.md`
- `docs/JOC_SPATIAL_BRIDGE.md`
- `docs/README.md`
- `docs/ROADMAP.md`
- `docs/RENDER_SCENE.md` and `docs/JOC_RENDER.md` for renderer-domain claims
- the remaining current Markdown set by semantic search

No contradictory current documentation remains in the audited scope.
