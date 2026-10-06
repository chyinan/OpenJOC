# E-AC-3 coupling, SPX, and inventory corrections

## Normative basis and compatibility boundary

These corrections derive from public specifications:

- [ETSI TS 102 366 V1.4.1](https://www.etsi.org/deliver/etsi_ts/102300_102399/102366/01.04.01_60/ts_102366v010401p.pdf),
  clauses 4.4.3.11–13 and Table E.1.12: the default standard-coupling table is
  indexed by absolute subband, while the parser's active structure is relative
  to `cplbegf`. The first active entry must be zero even when the corresponding
  absolute table entry is one. Both initial and following-strategy parsing use
  the same conversion. Explicitly transmitted structures retain their relative
  indexing and are unchanged.
- ETSI Table E.2.12 and clause E.2.6.4.2.3, independently corroborated by
  [ATSC A/52:2012](https://www.atsc.org/wp-content/uploads/2015/03/A52-201212-17.pdf)
  Table E3.14: SPX attenuation cells `(code, tap)` are corrected to
  `(18, 2) = 0.071793647`, `(23, 2) = 0.035896824`,
  `(27, 2) = 0.020617311`, and `(28, 1) = 0.068551561`.
- ETSI clause 4.4.2.3, Table 4.2: diagnostic channel inventories for `acmod=5`
  follow L, C, R, S; `acmod=6` follows L, R, Ls, Rs. The existing access-unit
  channel mapping was already correct; only opt-in inventory labels change.

The coupling correction can admit valid streams that previously failed or
change their decoded spectrum/PCM. The four SPX corrections deliberately
change affected notch coefficients and resulting PCM. They are correctness
changes, not bit-exact performance optimizations. No perceptual significance
or full decoder conformance is claimed. The frozen performance oracle
`15aefe1` is not reset, and performance comparisons must not weaken their
existing equality checks to accommodate these corrections. Unaffected PCM
must continue to pass the established gates.

## Regression contract

- Exhaustive legal four-bit `cplbegf`/`cplendf` pairs exercise both parser
  entry points against an independently transcribed default table. The tests
  also exercise explicit overrides and assert coordinate bit consumption.
- Public FFmpeg-generated four-channel silence fixtures exercise the failing
  nonzero-origin case through all six blocks and PCM synthesis. For begin 11
  and end 12, active flags are `[0, 0, 1, 1]` with two bands. The old prefix
  indexing instead consumed four bands and failed with exponent 26.
- Coupling-off fixtures independently verify diagnostic labels in every block.
  Fixture generation commands and hashes are in the
  [fixture README](../../crates/openjoc-eac3/tests/fixtures/coupling/README.md).
- SPX tests use a valid five-bit blend code, independently build expected
  spectra with both boundary and wrap-point notches, and compare spectra and
  windowed PCM for all four corrected rows.

## Remaining scope

Following-block reuse of an explicitly transmitted coupling structure when
`cplbndstrce=0` is a separate state-retention issue under E.1.3.3.15. This change
corrects default-table origin only; it does not claim to solve retention across
strategy/range changes or inactive gaps. A complete retention correction needs
its own stateful regression contract. Current support boundaries remain owned
by [CAPABILITIES](../CAPABILITIES.md) and [KNOWN_LIMITATIONS](../KNOWN_LIMITATIONS.md).
