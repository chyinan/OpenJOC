# J3R13 — Exact N3 context analysis

J3R12 supplied three independently executed, exact-condition producer null
pairs: static Front Left, static Front Right, and the reciprocal dual-object
swap. In each pair the stream-copied raw EC-3 bytes are identical. J3R13 uses
those pairs as the N3 full-complex envelope and reuses the frozen J2R7 target
identities for C1/C2; it does not generate media or reopen Logic.

## Scope and method

The decoder source is unchanged from the frozen J2R8 analysis baseline through
the current documentation HEAD. The estimator is the previously validated
simultaneous sine/cosine least-squares fit at exactly 48 kHz and 997/2003 Hz,
with complex coefficient `cosine - i*sine`. Comparisons permit one common
complex gauge per labeled partition. The labeled components are Base full-band
(L, R, C, Ls, Rs), Base LFE separately, ReconstructionBasis rows 000–014, and
their labeled concatenation. No free row permutation or object identity is
introduced.

The frozen steady-state spans are 60,000–84,000 samples (1.25–1.75 s) and
132,000–156,000 samples (2.75–3.25 s). J2R11 fixes the authored-state mapping:
D_PRE is 0–2 s with 997 Hz authored Front Left; D_POST is 2–4 s with 997 Hz
authored Front Right. D_PRE and D_POST are windows from one D_SWAP producer
pair, not independent producer samples.

## Results

The exact byte-identical producer null has zero residual at the carrier and
deterministic decoder boundary for Base, ReconstructionBasis, and the labeled
joint descriptor. The frozen target reproductions are:

| contrast | Base full-band | ReconstructionBasis | joint encoded descriptor |
| --- | ---: | ---: | ---: |
| C1 static FL 997 vs D_PRE 997 | 0.0034848789188314825 | 0.0031748106398913228 | 0.2974217716899793 |
| C2 static FR 997 vs D_POST 997 | 0.0035417176701007364 | 0.9999992742845898 | 0.857715331578793 |

Both contrasts exceed the exact N3 envelope. C1/C2 are therefore admitted as
scoped context differences under the frozen target definitions, with the
explicit caveat that this does not identify the causal hidden variable. The
FR/RB asymmetry is reported, not promoted to slot, position, or object
semantics.

## Boundaries preserved

- `SemanticBindingState::Unresolved` remains unchanged.
- ReconstructionBasis rows remain a numerical basis, not authored objects or
  object PCM.
- RcLfe and Base LFE remain separate from the ReconstructionBasis descriptor.
- C3 is not analyzed; one reciprocal dual contrast cannot support a universal
  frequency claim.
- `warp_mode` raw 3 remains `ETSI_STRICT` `ReservedWarpMode { raw: 3 }`.
- No vendor rule, JOC reconstruction, ObjectScene, renderer, Logic operation,
  new fixture, or new media was introduced.

This milestone establishes only the following bounded statement: under the
three J3R12 exact producer-null conditions, the frozen C1/C2 full-complex
contrasts exceed the admitted exact-condition null envelope. Authored-object
identity, slot identity, universal context dependence, and final rendering
semantics remain unresolved.

See `J3R13_EXACT_N3_CONTEXT_ANALYSIS.json` and `J3R13_N3_CALIBRATION.json` for
the machine-readable calibration and result contract.
