# J3R10 — Six-Run Replacement N3 Batch

J3R10 was authorized to execute six independent Logic Pro 12.3 Dolby Digital
Plus Atmos producer exports as three same-condition pairs. The batch was
fail-closed after the first endpoint exposed a provenance-controller violation:
the output appeared while its one-use nonce was still `RESERVED`, before the
durable `NONCE_CONSUMED` / `EXPORT_INVOKED` transition.

## Classification

`J3R10_N3_BATCH_PARTIAL_RECOVERY_REQUIRED`

The observed output was quarantined and is not an admitted N3 endpoint. No
automatic retry was performed and the remaining five authorizations were
revoked. Consequently, no producer pair was admitted and no producer envelope,
C1, C2, semantic binding, authored-object PCM, or renderer conclusion was
produced.

The scientific and architectural boundaries remain unchanged:

- `SemanticBindingState::Unresolved`.
- No new Logic fixture was created.
- No public media was committed.
- No warp/vendor semantic rule was added.
- Large derived-artifact generation remained frozen.

The next attempt must repair and independently test the final-action controller
ordering before any replacement producer export is authorized.
