#![deny(warnings)]

use openjoc_scene::{
    NamedFallbackParameterTuple, SpatialDescriptor, SpatialDescriptorPatch, SpatialSourceClass,
};

const ADAPTER_DEFAULT: NamedFallbackParameterTuple = NamedFallbackParameterTuple::adapter_default();
const LEGACY_FIXTURE: NamedFallbackParameterTuple = NamedFallbackParameterTuple::fixture();

#[test]
fn additional_table_accessor_preserves_raw3_field_and_serialized_key() {
    for raw3 in [None, Some(vec![]), Some(vec![0, 3, 255])] {
        let descriptor = SpatialDescriptor {
            raw3,
            ..SpatialDescriptor::new(SpatialSourceClass::DynamicPoint, "dynamic-0", vec![0.5])
        };
        assert_eq!(
            descriptor.additional_table_data(),
            descriptor.raw3.as_deref()
        );
        if let Some(bytes) = descriptor.additional_table_data() {
            assert!(std::ptr::eq(
                bytes.as_ptr(),
                descriptor.raw3.as_ref().unwrap().as_ptr()
            ));
        }
        let encoded = serde_json::to_value(&descriptor).unwrap();
        assert!(encoded.get("additional_table_data").is_none());
        assert_eq!(
            encoded["raw3"],
            serde_json::to_value(&descriptor.raw3).unwrap()
        );
        let restored: SpatialDescriptor = serde_json::from_value(encoded).unwrap();
        assert_eq!(restored, descriptor);
    }
}

#[test]
fn additional_table_patch_accessor_keeps_absent_clear_and_replace_states() {
    for raw3 in [
        None,
        Some(None),
        Some(Some(vec![])),
        Some(Some(vec![0, 255])),
    ] {
        let patch = SpatialDescriptorPatch {
            raw3,
            ..SpatialDescriptorPatch::default()
        };
        assert_eq!(
            patch.additional_table_data(),
            patch.raw3.as_ref().map(Option::as_deref)
        );
        let encoded = serde_json::to_value(&patch).unwrap();
        assert!(encoded.get("additional_table_data").is_none());
        assert_eq!(encoded["raw3"], serde_json::to_value(&patch.raw3).unwrap());
        let restored: SpatialDescriptorPatch = serde_json::from_value(encoded).unwrap();
        // Existing serde behavior maps both absent and null to no patch.
        let expected = patch.raw3.filter(Option::is_some);
        assert_eq!(restored.raw3, expected);
    }
    let absent: SpatialDescriptorPatch = serde_json::from_str("{}").unwrap();
    assert_eq!(absent.additional_table_data(), None);
}

#[test]
fn adapter_default_preserves_fixture_codewords_and_const_compatibility() {
    assert_eq!(ADAPTER_DEFAULT, LEGACY_FIXTURE);
    assert_eq!(ADAPTER_DEFAULT, NamedFallbackParameterTuple::default());
    assert_eq!(
        ADAPTER_DEFAULT.codewords(),
        [
            0, 0, 0, 0, 0, -5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -51, 0,
        ]
    );
}
