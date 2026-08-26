# Scoped decoded JOC ↔ OAMD binding implementation

Status: `RECONSTRUCTED_DYNAMIC_ADM_IMPLEMENTED_WITH_SCOPE`

This record describes the current clean-room implementation result. It does
not amend historical research records or claim recovery of an authored ADM
master.

## Clean inputs

The implementation used only these user-supplied sanitized artifacts and the
public OpenJOC source in this workspace:

- `C:\Users\chyin\Downloads\public_decoded_joc_scene_vectors.json` — SHA-256
  `5C5763A2587F07304EA762B49F267901B94EACE6638CBF905450282CC99A14F3`.
- `C:\Users\chyin\Downloads\decoded_joc_object_scene_contract.json` — SHA-256
  `CB6F2BB349CD68FEC93D31045EB0649CF972B88D6CB1966392CD67836B8D3E86`.
- `C:\Users\chyin\Downloads\DECODED_JOC_OBJECT_SCENE_CONTRACT.md` — SHA-256
  `77BF2F8DCEA9F3CFDA6C8B59E64289B2D7E34F951639FB6CBFF5EA72745CDE93`.
- `analysis-output/rb_to_decoded_joc_object_contract.json` and
  `analysis-output/public_rb_to_object_vectors.json`, both tracked in this
  workspace and recorded in the provenance census.

The literal nested paths supplied in the task were absent; the sanitized
root-level filenames above were the available equivalents. The provenance
census records this resolution and the fact that the historical original
JOC↔OAMD files were not used.

No contaminated workspace, analyst report, proprietary binary, vendor
runtime, audio fingerprint, frequency label, or tone signature was used as
implementation logic.

## Admitted profile and rule

The gate admits only `E_AC_3_JOC_OBSERVED_ORDINARY_PROFILE` or the exact
observed `E_AC_3_JOC_OBSERVED_ORDINARY_COMPAT_WARP3_PROFILE`, each with 15
decoded JOC objects, bed count 0, one Base LFE, ISF count 0, 15 dynamic OAMD
objects, and 16 total OAMD objects. Total index 0 must be Base LFE and indices
1 through 15 must be dynamic in declared order. The canonical typed mapping
is:

```text
joc_ordinal             = j
oamd_dynamic_ordinal    = j
oamd_total_index        = j + 1
```

The offset exists only in `DecodedJocBindingProfile`. It is not inferred from
element IDs, names, PCM content, trajectory proximity, or an opaque identity
state. Slots remain declared and are never compacted by activation state.

The compatibility variant additionally requires the exact whitelisted
deviation family and opaque raw3 shape. Raw3 remains `ReservedWarpMode(3)`
under ETSI strict policy; the opaque payload is preserved, its complete vendor
meaning is not claimed, and no raw3-specific transform is implemented.

`ResolvedWithinCarrier` means decoded JOC PCM is paired with decoded OAMD
metadata within the admitted programme/discontinuity epoch. It does not mean
original authored object identity, original UIDs, original names, original
hierarchy, original ADM master, or lossless round-trip recovery.

## Implementation boundary

- `openjoc-scene/src/binding.rs` owns the typed domains and one admission gate.
- `openjoc-scene/src/payload_decoder.rs` supplies actual JOC header count,
  decoded row count, OAMD layout, and parser profile to that gate.
- `SceneBuilder` keeps binding admission sticky across frames; a failed frame
  cannot be repaired by a later frame, and a rejected or unknown-deviation
  compatibility epoch stays unresolved.
- OAMD metadata is converted to the existing absolute scene sample timeline.
  ADM block boundaries reuse those offsets; no second QMF or latency clock is
  introduced.
- Compressed ADM preflight retains metadata events only, not full-duration PCM.
  The second pass writes bound reconstruction PCM through the existing bounded
  interleaver and keeps Base LFE separate.
- The admitted ADM path exports only finite room-coordinate position (including
  resolved screen/infinity coordinates). Active/inactive transitions, gain,
  extent, divergence, channel lock, zones, and other properties are not
  fabricated; unsupported metadata falls back in best-effort or rejects strict.

## Evidence and acceptance boundary

The sanitized vectors are covered by production mapping tests for ordinals 0,
3, and 14 plus negative tests for bed, ISF, alternative LFE, count mismatch,
actual JOC population mismatch, and authored-identity separation. Synthetic
admitted scenes cover deterministic dynamic ADM block generation, position
changes, exact sample-domain block partitioning, generated names, Base LFE
5.1 transport placeholders, strict policy, and independent ADM validation.

A clean user-owned C06 raw3-compatible carrier was present and replayed through
the internal-base capture path. It produced `resolved_within_carrier`, 15
ReconstructionBasis rows, 16 OAMD slots, and preserved raw3 as opaque and
unresolved. The other requested C00-C05/C07 media carriers were not present;
real-media dynamic ADM BWF export was not run because the independent PCM24
storage/range policy remains outside this change.

## Honest machine-readable state

```text
CLEAN_SOURCES_ONLY = YES
BINDING_RULE = joc_ordinal=j; oamd_dynamic_ordinal=j; oamd_total_index=j+1
ADMITTED_PROFILE = E_AC_3_JOC_OBSERVED_ORDINARY_PROFILE
JOC_OBJECT_COUNT = 15
OAMD_BED_COUNT = 0
BASE_LFE_COUNT = 1
OAMD_ISF_COUNT = 0
OAMD_DYNAMIC_COUNT = 15
OAMD_TOTAL_COUNT = 16
DECODED_JOC_OBJECT_BINDING = RESOLVED_WITHIN_CARRIER
ORIGINAL_AUTHORED_OBJECT_IDENTITY_RECOVERED = NO
ORIGINAL_ADM_MASTER_RECOVERED = NO
LOSSLESS_ROUND_TRIP = NO
DYNAMIC_RECONSTRUCTED_ADM = CONDITIONAL_ON_ADMITTED_PROFILE
UNSUPPORTED_PROFILES = BED_ISF_ALTERNATIVE_LFE_COUNT_ORDER_UNKNOWN_COMPAT_INACTIVE_TRANSITION
REAL_MEDIA_C06_MULTI_OBJECT_CAPTURE = PASS_SCOPED_RAW3_COMPAT
REAL_MEDIA_DYNAMIC_ADM_EXPORT = NOT_RUN_PCM24_POLICY_OUT_OF_SCOPE
```
