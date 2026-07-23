# Codex Goal: Build OpenJOC end-to-end

Read `OPENJOC_ENGINEERING_SPEC.md` completely before writing code.

Your goal is to implement a clean-room, reference-quality E-AC-3 JOC object-audio decoder in Rust, based on ETSI TS 103 420 V1.2.1 and ETSI TS 102 366 V1.4.1.

## Hard constraints

- Do not use or copy proprietary Dolby source code.
- Do not infer missing normative behavior from vendor implementations.
- Treat ETSI TS 103 420 as the normative source for OAMD/JOC/QMF.
- Treat ETSI TS 102 366 as the normative source for E-AC-3/EMDF frontend behavior.
- Historical MTK/Broadcom/Pixel research is cross-check evidence only.
- Do not stop at “it compiles”. Run the complete test/validation loop.
- Do not claim success until all Mandatory Definition-of-Done items in the engineering spec pass.
- Reference implementation first; optimization only after correctness is demonstrated.

## Local reference files expected

Place official files under `references/etsi/`:

- `ts_103420v010201p.pdf`
- `ts_103420v010201p0.zip`
- `ts_102366v010401p.pdf`

Expected SHA-256 for the provided TS 103 420 companion ZIP:

`a79cf108c4529b7d9ca9525c871183a70b1732ed6df03a3d85b2f31be46eeced`

Expected extracted `ts_103420_tables.c` SHA-256:

`4db8ae83e3c2e9269e88365be92a1a3ed6a9e6ee3851afac8ca03902723b1fcd`

Expected attachment contents:

- `joc_huff_code_coarse_generic`: 95 nodes
- `joc_huff_code_fine_generic`: 191 nodes
- `joc_huff_code_coarse_coeff_sparse`: 95 nodes
- `joc_huff_code_fine_coeff_sparse`: 191 nodes
- `joc_huff_code_5ch_pos_index_sparse`: 4 nodes
- `joc_huff_code_7ch_pos_index_sparse`: 6 nodes
- `prot64`: 640 float coefficients

## Required workflow

1. Create `RESEARCH_NOTES.md` and `REQUIREMENTS_MATRIX.md` before implementation.
2. Map every relevant ETSI clause to code and tests.
3. Build the Rust workspace described in the engineering spec.
4. Implement and test in this order:
   - bit reader
   - ETSI table importer
   - Huffman decoder + exhaustive tree tests
   - reference 64-band/640-tap complex QMF
   - JOC syntax/semantics parser
   - sparse/full differential reconstruction
   - 96/192-step dequantization
   - Table 54 subband mapping
   - temporal interpolation and cross-frame state
   - QMF-domain object reconstruction
   - inverse QMF
   - OAMD parser and timed metadata model
   - ObjectScene + JSON + per-object WAV export
   - EMDF/E-AC-3 frontend
   - end-to-end CLI
   - fuzzing + CI
5. Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-features` continuously.
6. Never disable a failing test to make the build green.
7. Add debug dumps for each reconstruction stage so mismatches are localizable.
8. End by writing `IMPLEMENTATION_REPORT.md` containing:
   - completed requirements
   - remaining gaps
   - all test results
   - numerical QMF reconstruction metrics
   - real JOC vector results
   - known limitations

## Core architecture invariant

The implementation must preserve this boundary:

```text
E-AC-3/EMDF frontend
        ↓
channel downmix + OAMD payload + JOC payload
        ↓
OAMD parser ───────────────► timed metadata
JOC parser → Huffman → differential → dequant → band map → interpolate
                                                ↓
downmix PCM → 64-band complex QMF ───────► reconstruction matrix multiply
                                                ↓
                                          object QMF essences
                                                ↓
                                           inverse QMF
                                                ↓
                                    ObjectScene + object PCM
```

Do not add a headphone/speaker renderer to the codec core. Decoder fidelity and renderer fidelity must remain separate.

## Success criterion

The project is only complete when a real, legally generated E-AC-3 JOC test vector can be decoded from the `.ec3/.eac3` file into:

- inspectable OAMD metadata timeline,
- reconstructed per-object PCM/WAV stems,
- `scene.json`,
- and debug data sufficient to trace JOC matrix reconstruction frame-by-frame,

with all Mandatory items in `OPENJOC_ENGINEERING_SPEC.md` passing.
