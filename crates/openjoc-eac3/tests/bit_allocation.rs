use openjoc_eac3::{Eac3Error, exponents_to_psd};

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
