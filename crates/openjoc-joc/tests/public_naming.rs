#![deny(warnings)]

use num_complex::Complex64;
use openjoc_joc::{
    QuantMode, decode_full_coefficients, decode_sparse_coefficients, reconstruct_basis_qmf,
    reconstruct_full, reconstruct_objects, reconstruct_sparse,
};

#[test]
fn legacy_coefficient_names_preserve_values_and_errors_without_warnings() {
    for mode in [QuantMode::Coarse96, QuantMode::Fine192] {
        for channels in [5, 7] {
            for (initial, deltas, symbols) in [
                (0, vec![], vec![0]),
                (1, vec![1, 2], vec![0, 1, mode.steps() - 1]),
                (channels, vec![], vec![0]),
                (0, vec![1], vec![0]),
                (0, vec![], vec![mode.steps()]),
            ] {
                assert_eq!(
                    reconstruct_sparse(channels, mode, initial, &deltas, &symbols),
                    decode_sparse_coefficients(channels, mode, initial, &deltas, &symbols)
                );
            }
        }
        for symbols in [
            vec![vec![0, 1, mode.steps() - 1], vec![1, 0, 1]],
            vec![],
            vec![vec![]],
            vec![vec![0], vec![0, 1]],
            vec![vec![mode.steps()]],
        ] {
            assert_eq!(
                reconstruct_full(mode, &symbols),
                decode_full_coefficients(mode, &symbols)
            );
        }
    }
}

#[test]
fn legacy_object_name_preserves_basis_qmf_bits_and_errors_without_warnings() {
    let inputs = vec![
        vec![
            [Complex64::new(0.25, -0.0); 64],
            [Complex64::new(-2.0, 3.0); 64],
        ],
        vec![
            [Complex64::new(-7.0, 0.5); 64],
            [Complex64::new(0.0, -0.125); 64],
        ],
    ];
    let matrices = vec![
        vec![vec![[0.5; 64], [-0.25; 64]], vec![[-0.125; 64], [2.0; 64]]],
        vec![vec![[0.0; 64]; 2]; 2],
        vec![vec![[-1.0; 64], [0.5; 64]], vec![[0.25; 64], [-0.0; 64]]],
    ];
    let legacy = reconstruct_objects(&inputs, &matrices).expect("valid matrices");
    let canonical = reconstruct_basis_qmf(&inputs, &matrices).expect("valid matrices");
    assert_eq!(legacy.len(), canonical.len());
    for (legacy_row, canonical_row) in legacy.iter().zip(&canonical) {
        assert_eq!(legacy_row.len(), canonical_row.len());
        for (legacy_slot, canonical_slot) in legacy_row.iter().zip(canonical_row) {
            for (legacy_sample, canonical_sample) in legacy_slot.iter().zip(canonical_slot) {
                assert_eq!(legacy_sample.re.to_bits(), canonical_sample.re.to_bits());
                assert_eq!(legacy_sample.im.to_bits(), canonical_sample.im.to_bits());
            }
        }
    }
    for (input, matrices) in [
        (vec![], vec![]),
        (inputs.clone(), vec![vec![]]),
        (vec![inputs[0].clone(), vec![]], matrices),
    ] {
        assert_eq!(
            reconstruct_objects(&input, &matrices).unwrap_err(),
            reconstruct_basis_qmf(&input, &matrices).unwrap_err()
        );
    }
}
