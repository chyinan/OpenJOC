use super::*;

const FIXTURE: &[u8] = include_bytes!("../../openjoc-wasm/testdata/joc.lifecycle.ec3");

#[test]
fn selected_metadata_retains_identical_ordinary_payload_for_all_profiles() {
    let frames = index_syncframes(FIXTURE).unwrap();
    let units = group_access_units(&frames).unwrap();
    assert_eq!(units.len(), 128);
    for requested in [
        ValidationProfile::Auto,
        ValidationProfile::EtsiStrict,
        ValidationProfile::ObservedVendorCompat,
    ] {
        let session = OpenJocSession::new(OpenJocConfig {
            validation_profile: requested,
            ..OpenJocConfig::default()
        })
        .unwrap();
        for unit in units.iter().copied() {
            let selected = session.select_metadata(FIXTURE, &frames, unit).unwrap();
            let expected_profile = match requested {
                ValidationProfile::Auto | ValidationProfile::EtsiStrict => {
                    JocValidationProfile::EtsiStrict
                }
                ValidationProfile::ObservedVendorCompat => {
                    JocValidationProfile::ObservedVendorCompat
                }
            };
            assert_eq!(selected.profile, expected_profile);
            let expected_metadata =
                extract_joc_access_unit_for_profile(FIXTURE, &frames, unit, expected_profile)
                    .unwrap()
                    .unwrap();
            assert_eq!(selected.metadata, expected_metadata);
            let reparsed = crate::parse_oamd_for_profile(
                &selected.metadata.oamd,
                session.config.oamd,
                selected.oamd_profile,
            )
            .unwrap();
            // Metadata structural equality is supplementary; audio is checked
            // separately with complete raw-bit before/after captures.
            assert_eq!(selected.oamd_proof.parsed(), &reparsed);
        }
    }
}
