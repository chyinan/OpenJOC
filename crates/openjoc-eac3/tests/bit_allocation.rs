use openjoc_eac3::{
    BitAllocationBand, BitAllocationParameters, DeltaBitAllocationElement,
    DeltaBitAllocationSegment, Eac3Error, FixedBitAllocationParameters, apply_delta_bit_allocation,
    bit_allocation_band, bit_allocation_band_for_bin, bit_allocation_pointer, calc_lowcomp,
    compute_bap, compute_excitation, compute_masking_curve, decode_bit_allocation_parameters,
    exponents_to_psd, high_efficiency_bit_allocation_pointer, integrate_psd, log_add,
};

fn stage_parameters() -> FixedBitAllocationParameters {
    FixedBitAllocationParameters {
        slow_decay: 0,
        fast_decay: 0,
        slow_gain: 0,
        db_per_bit: 0,
        floor: 0,
        fast_gain: 0,
    }
}

#[test]
fn calc_lowcomp_uses_the_structured_branch_interpretation() {
    assert_eq!(calc_lowcomp(0, 100, 356, 0), 384);
    assert_eq!(calc_lowcomp(500, 100, 356, 0), 384);
    assert_eq!(calc_lowcomp(320, 500, 400, 7), 256);
    assert_eq!(calc_lowcomp(100, 500, 400, 20), 0);
}

#[test]
fn computes_uncoupled_excitation_over_the_active_bands() {
    let psd = vec![1000_i16; 7];
    let bndpsd = vec![1000_i16; 50];
    let excite = compute_excitation(&psd, &bndpsd, 0, 7, stage_parameters(), None)
        .expect("valid uncoupled range");
    assert_eq!(&excite[..7], &[1000; 7]);
    assert!(excite[7..].iter().all(|value| *value == 0));
}

#[test]
fn computes_masking_curve_with_hearing_threshold_and_knee() {
    let bndpsd = vec![100, 200, 400];
    let excite = vec![100, 50, 400];
    let mask = compute_masking_curve(&bndpsd, &excite, 0, 3, 0, 300).expect("valid mask range");
    assert_eq!(&mask[..3], &[0x04d0, 0x04d0, 0x0440]);
}

#[test]
fn applies_delta_segments_to_the_masking_curve() {
    let mask = vec![0_i16; 50];
    let delta = DeltaBitAllocationElement {
        strategy: 1,
        segments: vec![DeltaBitAllocationSegment {
            offset: 2,
            length: 2,
            delta: 5,
        }],
    };
    let adjusted = apply_delta_bit_allocation(&mask, &delta).expect("valid delta segment");
    assert_eq!(&adjusted[0..2], &[0, 0]);
    assert_eq!(&adjusted[2..4], &[256, 256]);
}

#[test]
fn computes_bap_from_fine_psd_and_band_mask() {
    let psd = vec![1024_i16; 253];
    let mask = vec![0_i16; 50];
    let bap = compute_bap(&psd, &mask, 0, 1, 0, 0).expect("valid bap range");
    assert_eq!(bap[0], 10);
    assert!(bap[1..].iter().all(|value| *value == 0));
}

#[test]
fn maps_every_legal_exponent_to_normative_log_psd() {
    let exponents = (0_u8..=24).collect::<Vec<_>>();
    let expected = (0_i16..=24)
        .map(|exponent| 3_072 - (exponent << 7))
        .collect::<Vec<_>>();

    assert_eq!(exponents_to_psd(&exponents), Ok(expected));
}

#[test]
fn log_add_uses_the_normative_difference_address_and_latab() {
    assert_eq!(log_add(100, 50), 142);
    assert_eq!(log_add(50, 100), 142);
    assert_eq!(log_add(0, 0), 64);
    assert_eq!(log_add(3_072, 0), 3_072);
}

#[test]
fn integrates_fine_psd_into_the_exact_nonuniform_bands() {
    let psd = vec![0_i16; 253];
    let integrated = integrate_psd(&psd, 0, 253).expect("valid PSD range");
    let mut expected = vec![0_i16; 50];
    for (index, expected_value) in expected.iter_mut().enumerate() {
        let size = bit_allocation_band(u8::try_from(index).expect("50 bands fit u8"))
            .expect("normative band")
            .size;
        for _ in 1..size {
            *expected_value = log_add(*expected_value, 0);
        }
    }
    assert_eq!(integrated, expected);
}

#[test]
fn rejects_invalid_psd_integration_ranges() {
    assert_eq!(
        integrate_psd(&[0; 4], 2, 2),
        Err(Eac3Error::InvalidPsdRange { start: 2, end: 2 })
    );
    assert_eq!(
        integrate_psd(&[0; 4], 0, 5),
        Err(Eac3Error::InvalidPsdRange { start: 0, end: 5 })
    );
}

#[test]
fn rejects_exponents_outside_the_normative_range() {
    assert_eq!(
        exponents_to_psd(&[0, 24, 25]),
        Err(Eac3Error::ExponentOutOfRange { actual: 25 })
    );
}

#[test]
fn maps_every_normative_bit_allocation_parameter_table_entry() {
    let slow_decay = [0x0f, 0x11, 0x13, 0x15];
    let fast_decay = [0x3f, 0x53, 0x67, 0x7b];
    let slow_gain = [0x540, 0x4d8, 0x478, 0x410];
    let db_per_bit = [0x000, 0x700, 0x900, 0xb00];
    let floor = [0x2f0_i16, 0x2b0, 0x270, 0x230, 0x1f0, 0x170, 0x0f0, -0x800];
    let fast_gain = [0x080, 0x100, 0x180, 0x200, 0x280, 0x300, 0x380, 0x400];

    for slow_decay_code in 0..4 {
        for fast_decay_code in 0..4 {
            for slow_gain_code in 0..4 {
                for db_per_bit_code in 0..4 {
                    for floor_code in 0..8 {
                        for fast_gain_code in 0..8 {
                            assert_eq!(
                                decode_bit_allocation_parameters(
                                    BitAllocationParameters {
                                        slow_decay_code,
                                        fast_decay_code,
                                        slow_gain_code,
                                        db_per_bit_code,
                                        floor_code,
                                    },
                                    fast_gain_code,
                                ),
                                Ok(FixedBitAllocationParameters {
                                    slow_decay: slow_decay[usize::from(slow_decay_code)],
                                    fast_decay: fast_decay[usize::from(fast_decay_code)],
                                    slow_gain: slow_gain[usize::from(slow_gain_code)],
                                    db_per_bit: db_per_bit[usize::from(db_per_bit_code)],
                                    floor: floor[usize::from(floor_code)],
                                    fast_gain: fast_gain[usize::from(fast_gain_code)],
                                })
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn rejects_out_of_range_bit_allocation_parameter_codes() {
    let valid = BitAllocationParameters {
        slow_decay_code: 0,
        fast_decay_code: 0,
        slow_gain_code: 0,
        db_per_bit_code: 0,
        floor_code: 0,
    };

    for (parameter, actual, codes, fast_gain_code) in [
        (
            "slow decay",
            4,
            BitAllocationParameters {
                slow_decay_code: 4,
                ..valid
            },
            0,
        ),
        (
            "fast decay",
            4,
            BitAllocationParameters {
                fast_decay_code: 4,
                ..valid
            },
            0,
        ),
        (
            "slow gain",
            4,
            BitAllocationParameters {
                slow_gain_code: 4,
                ..valid
            },
            0,
        ),
        (
            "dB per bit",
            4,
            BitAllocationParameters {
                db_per_bit_code: 4,
                ..valid
            },
            0,
        ),
        (
            "floor",
            8,
            BitAllocationParameters {
                floor_code: 8,
                ..valid
            },
            0,
        ),
        ("fast gain", 8, valid, 8),
    ] {
        assert_eq!(
            decode_bit_allocation_parameters(codes, fast_gain_code),
            Err(Eac3Error::InvalidBitAllocationParameterCode { parameter, actual })
        );
    }
}

#[test]
fn maps_every_normative_bit_allocation_band_and_bin() {
    let starts = [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
        25, 26, 27, 28, 31, 34, 37, 40, 43, 46, 49, 55, 61, 67, 73, 79, 85, 97, 109, 121, 133, 157,
        181, 205, 229,
    ];
    let sizes = [
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 3, 3,
        3, 3, 3, 3, 3, 6, 6, 6, 6, 6, 6, 12, 12, 12, 12, 24, 24, 24, 24, 24,
    ];

    for (index, (&start, &size)) in starts.iter().zip(&sizes).enumerate() {
        let band_index = u8::try_from(index).expect("50 bands fit u8");
        assert_eq!(
            bit_allocation_band(band_index),
            Ok(BitAllocationBand { start, size })
        );
        for bin in start..start + u16::from(size) {
            assert_eq!(bit_allocation_band_for_bin(bin), Ok(band_index));
        }
    }
}

#[test]
fn rejects_bit_allocation_band_and_bin_outside_table_six_twelve() {
    assert_eq!(
        bit_allocation_band(50),
        Err(Eac3Error::InvalidBitAllocationTableIndex {
            table: "band",
            actual: 50,
        })
    );
    assert_eq!(
        bit_allocation_band_for_bin(253),
        Err(Eac3Error::InvalidBitAllocationTableIndex {
            table: "bin",
            actual: 253,
        })
    );
}

#[test]
fn maps_every_normative_conventional_bit_allocation_pointer() {
    let expected = [
        0, 1, 1, 1, 1, 1, 2, 2, 3, 3, 3, 4, 4, 5, 5, 6, 6, 6, 6, 7, 7, 7, 7, 8, 8, 8, 8, 9, 9, 9,
        9, 10, 10, 10, 10, 11, 11, 11, 11, 12, 12, 12, 12, 13, 13, 13, 13, 14, 14, 14, 14, 14, 14,
        14, 14, 15, 15, 15, 15, 15, 15, 15, 15, 15,
    ];

    for (address, pointer) in expected.into_iter().enumerate() {
        assert_eq!(
            bit_allocation_pointer(u8::try_from(address).expect("64 addresses fit u8")),
            Ok(pointer)
        );
    }
    assert_eq!(
        bit_allocation_pointer(64),
        Err(Eac3Error::InvalidBitAllocationTableIndex {
            table: "pointer",
            actual: 64,
        })
    );
}

#[test]
fn maps_every_normative_high_efficiency_bit_allocation_pointer() {
    let expected = [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 8, 8, 8, 9, 9, 9, 10, 10, 10, 10, 11, 11, 11, 11, 12, 12, 12,
        12, 13, 13, 13, 13, 14, 14, 14, 14, 15, 15, 15, 15, 16, 16, 16, 16, 17, 17, 17, 17, 18, 18,
        18, 18, 18, 18, 18, 18, 19, 19, 19, 19, 19, 19, 19, 19, 19,
    ];

    for (address, pointer) in expected.into_iter().enumerate() {
        assert_eq!(
            high_efficiency_bit_allocation_pointer(
                u8::try_from(address).expect("64 addresses fit u8")
            ),
            Ok(pointer)
        );
    }
    assert_eq!(
        high_efficiency_bit_allocation_pointer(64),
        Err(Eac3Error::InvalidBitAllocationTableIndex {
            table: "high-efficiency pointer",
            actual: 64,
        })
    );
}
