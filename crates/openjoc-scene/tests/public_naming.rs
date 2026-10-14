#![deny(warnings)]

use openjoc_scene::NamedFallbackParameterTuple;

const ADAPTER_DEFAULT: NamedFallbackParameterTuple = NamedFallbackParameterTuple::adapter_default();
const LEGACY_FIXTURE: NamedFallbackParameterTuple = NamedFallbackParameterTuple::fixture();

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
