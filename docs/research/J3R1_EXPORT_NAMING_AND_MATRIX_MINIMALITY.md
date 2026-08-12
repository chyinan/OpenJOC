# J3R1 — Export Naming and N3 Matrix Minimality

This recovery closes the incident-bounded naming question without starting a
new Logic interaction or performing N3 analysis. `SemanticBindingState` remains
`Unresolved`.

## Incident boundary

The original J3R1 rehearsal selected the `naming-probe` folder row and sent
Return. In this Logic/NSSavePanel workflow, Return confirmed the export rather
than navigating into the folder. The producer created
`S_FL_CANCEL_ONLY_ec3.mp4` (400055 bytes; SHA-256
`d6c5b7289c8a6416f46dc2befe93bd0ab1573c45c6d8547b0683ea510e8cc030`). It remains quarantined and is admitted
only as filename/path incident evidence; its audio contents were not decoded.

Permanent controller rule:

> `RETURN_OR_ENTER_IS_NEVER_A_FOLDER_NAVIGATION_ACTION_IN_LOGIC_NSSAVEPANEL`

Folder navigation must use an explicit semantic folder-selection control or a
separately authorized, proven mechanism. Return must never be used to activate
a selected folder row.

## Naming contract

J2R15 observed `carrier.mp4` → `carrier_ec3.mp4`. The J3R1 incident observed
`S_FL_CANCEL_ONLY.mp4` → `S_FL_CANCEL_ONLY_ec3.mp4`. These are two independent
real producer executions under the same Logic Pro 12.3 DD+ Atmos configuration
(Music, 768 kbps, Project scope, ADM companion off).

The admitted operational contract is directory-scoped, not a universal
filename law:

```text
approved parent = unique and empty before invocation
authorized stem = X
allowed final leaf = X_ec3.mp4
exactly one new media file appears after invocation
```

The final leaf must equal the authorized stem plus `_ec3.mp4`; a wildcard such
as `X*.mp4`, or “whatever single file appears,” is not acceptable. Any output
outside the approved parent, a second output, a pre-existing output, a
symlink/alias, a colon-encoded path artifact, or a grammar mismatch is a
failure and must be quarantined.

The panel input leaf and panel-resolved URL are pre-invocation values. The
producer-final leaf and producer-final URL are post-invocation observations;
typed leaf equality is not required.

Classification: `PRODUCER_OUTPUT_DIRECTORY_SCOPED_CONTRACT_ADMITTED`.
No further sacrificial filename probe is required for this scoped contract,
but the suffix is not generalized beyond the tested configuration.

## N3 matrix minimality

Design 6 is `S_FL×2, S_FR×2, D_SWAP×2`. One D_SWAP producer pair contains both
the D_PRE and D_POST reciprocal windows, so it supplies one exact-condition
producer-repeatability observation for each window while remaining one
independent pair. This supports the primary scoped C1 and C2 questions.

Design 8 adds `D_SWAP×4`, i.e. a second independent D_SWAP pair. In the frozen
J2R15 queue that pair was revoked before invocation, so its observed value is
**not available**. It would strengthen producer-repeatability characterization
and is relevant to a stricter C3 admission, but it is not necessary for C1/C2.

Decision:

```text
SIX_EXPORT_MATRIX_SUFFICIENT_FOR_PRIMARY_SCOPED_CONTEXT_TEST
selected_future_export_count = 6
c1_supported = true
c2_supported = true
c3_supported = false
dual_independent_pair_count = 1
```

This is a matrix-minimality result, not evidence that N3 exports were
performed. No C1/C2/C3 N3 analysis was run in this recovery.

## Recovery and storage boundary

Recovery used only frozen J2R13/J2R14/J2R15 records and the J3R1 incident
record. No Logic launch, producer export, new media, storage GC, decoder
semantic change, or canonical-corpus modification occurred. The accidental
and J2R15 outputs remain excluded private quarantine evidence. The storage
GC result remains frozen at 5,058,596,864 bytes (exact-AU), 4,078,161,920
bytes (TDAC), 9,136,758,784 bytes combined; no additional GC was performed.

Generated from deterministic private evidence hashes:

```text
accident_forensic = 0a385d9fcf1ae2682a1f2b7d1488f451c394e550fa4550d1229c628913e562f9
naming_inventory = 3462abd5ba6cf3ed9c163057596eadc4c0f35d5c4017eb0dbfca872c70f0acf0
naming_contract = 58ed54200f5e561a65f9cd5e51333ab0703c37504854ea4acbad2b5bdff18274
matrix_minimality = ce60aac6be59a4363c4faf78c25f1e9e2b3f92d5bee07b3d801e386fc6f8d1f5
recovery_verification = 192e8a507b56f9f51ab2d7c6b762bfc6a26049937045c2548f66de90ef3e1795
public_json = 0d363e00e3f99c10b282b2df210cbb6208845a6c567aa6ace2fc9e418e131d8e
```
