# Following-block channel exponent boundary

`explicit-fresh-crc.eac3` is an independently constructed, CRC-valid 4096-byte
E-AC-3 syncframe: 48 kHz, six blocks, L/C/R, no LFE, standard coupling, and
fresh D15 channel and coupling exponents in every block. It contains generated
zero-BAP syntax, no third-party recording. It is a bounded parser regression,
not an encoder-generated conformance or audible quality vector.

SHA-256: `a7d3e997500716fc707c0f80e10e4d0169e5d8726aed9ed1053e3f80d50a18e1`.

The generator is `frame([active(true, A); 6], 1)` in
`tests/following_coupling_exponents.rs`. The fixture equality assertion checks
its reproduction including CRC16 (polynomial 0x8005, initial value zero over
bytes after the syncword, with the final remainder stored big-endian).
Tests require no external decoder. D25/D45 and retained-structure variants
are generated in the same test from explicit syntax and expected boundaries.

## Normative boundary and independent bit trace

ETSI TS 102 366 V1.4.1 [E.2.3.3, printed page 146](https://www.etsi.org/deliver/etsi_ts/102300_102399/102366/01.04.01_60/ts_102366v010401p.pdf)
refers standard coupling to 6.1.3 (printed pages 53–54). Coupled channel
exponents stop at `cplstrtmant = 37 + 12*cplbegf`, rather than the coupling
channel's exclusive end. Here `cplbegf=7`, `cplendf=9`: channel end 121,
coupling end 181. Thus D15 needs 40 channel exponent groups, D25 needs 20,
and D45 needs 10. The coupling D15 payload itself has 20 groups for bins
121 through 180.

For block 1 (zero based), independent MSB-first offsets in the checked-in
fixture are:

- block starts at bit 1267
- `cplbegf`: [1273,1277) = 7; `cplendf`: [1277,1281) = 9
- coupling absolute exponent: [1367,1371) = 5, followed by 20 groups of 62
- channel 0 absolute exponent: [1511,1515) = 10
- channel 0's 40 groups: [1515,1795); gain range: [1795,1797) = 0
- channel 1 absolute exponent: [1797,1801); gain range: [2081,2083) = 1
- channel 2 absolute exponent: [2083,2087); gain range: [2367,2369) = 2
- converter SNR presence: bit 2369; [2370,2380) = 342
- coupling leak presence: bit 2380; fast=3, slow=5; block ends at 2387

The pre-fix parser requested 60 channel groups. Its excess group 43 consumed
next-channel syntax and produced `ExponentOutOfRange { actual: 26 }`.
All three new tests reproduce that error on b86c85e before the production
change. Correct block ends are 1267, 2387, 3507, 4627, 5747, 6867.

## Independent decoder check

FFmpeg 7.1.5 accepts the fixture including strict CRC checking:

```sh
ffmpeg -v verbose -err_detect crccheck+explode \
  -i crates/openjoc-eac3/tests/fixtures/coupling/explicit-fresh-crc.eac3 \
  -f f32le -y /tmp/explicit-fresh.pcm
```

Verified with FFmpeg 7.1.5 for all six generated variants (D15/D25/D45, each
with explicit or retained band structure): one decoded frame, zero errors,
1536 samples/channel, three channels, 18432 output bytes. The tests also check six decoded blocks, finite PCM,
1536 samples/channel, and no LFE through OpenJOC's public API. They do not
claim cross-decoder PCM identity.

## Compatibility scope

This is an intentional decoder-correctness change for affected E-AC-3 standard
coupling streams: valid fresh exponent payloads previously rejected can now
produce PCM, and channel BAP/mantissa boundaries use the independent channel
range even when exponents are reused. Existing reuse and ordinary AC-3 tests
remain unchanged. The enhanced coupling branch and phase-flag behavior are
outside this fix. No public API, dependency, or lockfile changes are needed.
The frozen `15aefe1` PCM gate and its expectations remain untouched; this new
correctness fixture is not substituted into that compatibility baseline.
