use openjoc_oamd::{Gain, OamdError, decode_gain, decode_priority};

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
