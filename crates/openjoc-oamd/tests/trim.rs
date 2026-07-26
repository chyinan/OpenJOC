use std::num::NonZeroU8;

use openjoc_oamd::{
    GlobalTrim, OamdError, TrimConfiguration, WarpMode, decode_trim_centre,
    decode_trim_surround_or_height, decode_y_balance, parse_trim_element,
};

fn push(bits: &mut Vec<bool>, value: u64, width: u8) {
    for shift in (0..width).rev() {
        bits.push(value & (1_u64 << shift) != 0);
    }
}

fn pack(mut bits: Vec<bool>) -> Vec<u8> {
    while bits.len() % 8 != 0 {
        bits.push(false);
    }
    let mut bytes = vec![0; bits.len() / 8];
    for (index, bit) in bits.into_iter().enumerate() {
        if bit {
            bytes[index / 8] |= 0x80 >> (index % 8);
        }
    }
    bytes
}

#[test]
fn decodes_every_normative_trim_table_entry() {
    let centre = [
        6.0, 3.0, 1.5, 0.75, -0.75, -1.5, -3.0, -4.5, -6.0, -7.5, -9.0, -10.5, -12.0, -13.5, -16.0,
        -36.0,
    ];
    for (code, expected) in centre.into_iter().enumerate() {
        assert_eq!(
            decode_trim_centre(u8::try_from(code).expect("four-bit centre code")),
            Ok(expected)
        );
    }

    let surround_height = [
        -0.75, -1.5, -3.0, -4.5, -6.0, -7.5, -9.0, -10.5, -12.0, -13.5, -16.0, -36.0,
    ];
    for code in 0..4 {
        assert_eq!(
            decode_trim_surround_or_height(code),
            Err(OamdError::ReservedTrimCode { code })
        );
    }
    for (offset, expected) in surround_height.into_iter().enumerate() {
        assert_eq!(
            decode_trim_surround_or_height(
                u8::try_from(offset + 4).expect("four-bit surround code"),
            ),
            Ok(expected)
        );
    }
}

#[test]
fn decodes_every_normative_balance_sign_and_amount() {
    for sign_code in 0..=1 {
        let sign = if sign_code == 0 { -1.0 } else { 1.0 };
        for amount in 0..=15 {
            assert_eq!(
                decode_y_balance(sign_code, amount),
                Ok(sign * (f64::from(amount) + 1.0) / 16.0)
            );
        }
    }
}

#[test]
fn parses_custom_trim_and_per_object_disable_flags() {
    let mut bits = Vec::new();
    push(&mut bits, 1, 2); // double object Y
    push(&mut bits, 0, 2); // reserved
    push(&mut bits, 2, 2); // custom global trim
    push(&mut bits, 0, 1); // not default
    push(&mut bits, 0, 1); // not disabled
    push(&mut bits, 0b1_1111, 5); // all controls present
    push(&mut bits, 2, 4); // centre +1.5 dB
    push(&mut bits, 4, 4); // surround -0.75 dB
    push(&mut bits, 15, 4); // height -36 dB
    push(&mut bits, 0, 1); // top/bottom balance toward front
    push(&mut bits, 15, 4); // magnitude 1
    push(&mut bits, 1, 1); // listener balance toward back
    push(&mut bits, 7, 4); // magnitude 0.5
    push(&mut bits, 1, 1); // per-object flags present
    push(&mut bits, 1, 1);
    push(&mut bits, 0, 1);

    let trim = parse_trim_element(
        &pack(bits),
        2,
        NonZeroU8::new(1).expect("nonzero configuration count"),
    )
    .expect("trim element");
    assert_eq!(trim.warp_mode, WarpMode::DoubleY);
    let GlobalTrim::Custom(configurations) = trim.global_trim else {
        panic!("expected custom trim");
    };
    assert_eq!(configurations.len(), 1);
    let TrimConfiguration::Custom(controls) = configurations[0] else {
        panic!("expected custom controls");
    };
    assert_eq!(controls.centre_db, Some(1.5));
    assert_eq!(controls.surround_db, Some(-0.75));
    assert_eq!(controls.height_db, Some(-36.0));
    assert_eq!(controls.top_bottom_y_balance, Some(-1.0));
    assert_eq!(controls.listener_y_balance, Some(0.5));
    assert_eq!(trim.disable_trim_per_object, vec![true, false]);
}

#[test]
fn rejects_reserved_modes_bits_and_trim_codes() {
    for warp in 2..=3 {
        let mut bits = Vec::new();
        push(&mut bits, warp, 2);
        push(&mut bits, 0, 2);
        push(&mut bits, 0, 2);
        push(&mut bits, 0, 1);
        assert_eq!(
            parse_trim_element(
                &pack(bits),
                0,
                NonZeroU8::new(1).expect("nonzero configuration count")
            ),
            Err(OamdError::ReservedWarpMode {
                code: u8::try_from(warp).expect("two-bit warp code")
            })
        );
    }

    let mut reserved_bits = Vec::new();
    push(&mut reserved_bits, 0, 2);
    push(&mut reserved_bits, 1, 2);
    assert_eq!(
        parse_trim_element(
            &pack(reserved_bits),
            0,
            NonZeroU8::new(1).expect("nonzero configuration count")
        ),
        Err(OamdError::NonzeroReservedData)
    );

    let mut global = Vec::new();
    push(&mut global, 0, 2);
    push(&mut global, 0, 2);
    push(&mut global, 3, 2);
    assert_eq!(
        parse_trim_element(
            &pack(global),
            0,
            NonZeroU8::new(1).expect("nonzero configuration count")
        ),
        Err(OamdError::ReservedGlobalTrimMode)
    );
}
