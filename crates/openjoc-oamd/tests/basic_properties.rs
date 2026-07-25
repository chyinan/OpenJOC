use openjoc_oamd::{
    Extent3, Gain, OamdError, ZoneConstraint, decode_gain, decode_priority, decode_size,
    decode_zone_constraints,
};

#[test]
fn decodes_every_explicit_gain_code_and_index_semantics() {
    assert_eq!(decode_gain(0, None, None), Ok(Gain::Decibels(0)));
    assert_eq!(decode_gain(1, None, None), Ok(Gain::NegativeInfinity));
    for bits in 0_u8..=63 {
        let expected = if bits <= 14 {
            15 - i16::from(bits)
        } else {
            14 - i16::from(bits)
        };
        assert_eq!(
            decode_gain(2, Some(bits), None),
            Ok(Gain::Decibels(expected))
        );
    }
    assert_eq!(decode_gain(3, None, None), Ok(Gain::Decibels(0)));
    assert_eq!(
        decode_gain(3, None, Some(Gain::Decibels(-12))),
        Ok(Gain::Decibels(-12))
    );
    assert_eq!(decode_gain(2, None, None), Err(OamdError::MissingGainBits));
}

#[test]
fn decodes_default_and_all_signalled_priorities() {
    assert_eq!(decode_priority(true, None), Ok(1.0));
    for bits in 0_u8..=31 {
        assert_eq!(
            decode_priority(false, Some(bits)),
            Ok(f64::from(bits) / 32.0)
        );
    }
    assert_eq!(
        decode_priority(false, None),
        Err(OamdError::MissingPriorityBits)
    );
}

#[test]
fn decodes_all_size_modes_and_boundaries() {
    assert_eq!(decode_size(0, None, None), Ok(Extent3::ZERO));
    assert_eq!(
        decode_size(1, Some(31), None),
        Ok(Extent3 {
            width: 1.0,
            depth: 1.0,
            height: 1.0
        })
    );
    assert_eq!(
        decode_size(2, None, Some([0, 15, 31])),
        Ok(Extent3 {
            width: 0.0,
            depth: 15.0 / 31.0,
            height: 1.0
        })
    );
    assert_eq!(
        decode_size(3, None, None),
        Err(OamdError::ReservedSizeIndex)
    );
    assert!(decode_size(1, Some(32), None).is_err());
}

#[test]
fn decodes_every_zone_table_entry_and_elevation_flag() {
    let include = ZoneConstraint::Include;
    let exclude = ZoneConstraint::Exclude;
    let expected = [
        [include, include, include, include, include],
        [include, include, include, exclude, include],
        [include, exclude, include, include, include],
        [exclude, exclude, exclude, exclude, include],
        [include, exclude, exclude, exclude, exclude],
        [exclude, exclude, include, exclude, exclude],
    ];
    for (index, horizontal) in expected.into_iter().enumerate() {
        let zones =
            decode_zone_constraints(u8::try_from(index).expect("index"), false).expect("zone");
        assert_eq!(zones[..5], horizontal);
        assert_eq!(zones[5], exclude);
        assert_eq!(
            decode_zone_constraints(u8::try_from(index).expect("index"), true).expect("zone")[5],
            include
        );
    }
    assert!(decode_zone_constraints(6, true).is_err());
    assert!(decode_zone_constraints(7, true).is_err());
}
