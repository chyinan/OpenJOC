# J3R8 — post-export destination attestation

**Decision:** `N3_OUTPUT_DISCOVERY_SCOPE_REQUIRES_REDESIGN`

J3R8 retired the unworkable requirement to prove a Logic NSSavePanel parent
before invoking an export.  It instead tested one separately authorized,
non-scientific controller probe: a fresh S_FL disposable project, a fresh
high-entropy output stem, a bounded predeclared discovery scope, and one
durably consumed nonce.

The controller, project identity, Dolby Digital Plus with Dolby Atmos / Music
768 / Project settings, exact typed panel leaf, and single final UI action all
passed.  The producer created exactly one matching E-AC-3/JOC MP4 after
invocation, with a stable 4.096 s / 48 kHz stream.  Before moving it, the
controller froze its original path, parent identity, size, hash, nonce, and
process tuple.

The original parent was outside the frozen discovery scope.  The file was
therefore quarantined as `CONTROLLER_PROBE_ONLY`; it is not an N3 endpoint and
does not provide a null, repeatability, C1/C2/C3, semantic-binding, object, or
renderer result.  The result does show that a high-entropy leaf and
post-invocation identity freeze work, but the current scope construction is not
sufficiently predictive for future N3 batch admission.

The required next redesign is narrow: derive a bounded discovery scope from a
durably observed producer parent *before* authorizing a future probe, without
resurrecting the retired NSSavePanel folder-navigation mechanism.  No decoder,
warp, or semantic-binding behavior changed; `SemanticBindingState::Unresolved`
remains mandatory.
