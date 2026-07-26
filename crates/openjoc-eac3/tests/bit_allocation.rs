use openjoc_eac3::{
    BitAllocationParameters, Eac3Error, FixedBitAllocationParameters,
    decode_bit_allocation_parameters, exponents_to_psd,
};

#[test]
fn maps_every_legal_exponent_to_normative_log_psd() {
    let exponents = (0_u8..=24).collect::<Vec<_>>();
    let expected = (0_i16..=24)
        .map(|exponent| 3_072 - (exponent << 7))
        .collect::<Vec<_>>();

    assert_eq!(exponents_to_psd(&exponents), Ok(expected));
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
