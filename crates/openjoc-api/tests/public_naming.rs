#![deny(warnings)]

use openjoc_api::{OpenJocConfig, OpenJocDiagnostics, OpenJocPacket, OpenJocSession};

const DIAGNOSTICS: OpenJocDiagnostics = OpenJocDiagnostics {
    profile: None,
    downmix_index: None,
    object_count: Some(17),
    complexity_index: None,
};
const OAMD_TOTAL: Option<u16> = DIAGNOSTICS.oamd_total_object_count();

#[test]
fn oamd_total_accessor_preserves_legacy_field_and_const_access() {
    assert_eq!(OAMD_TOTAL, Some(17));
    for object_count in [None, Some(0), Some(u16::MAX)] {
        let diagnostics = OpenJocDiagnostics {
            object_count,
            ..DIAGNOSTICS
        };
        assert_eq!(diagnostics.oamd_total_object_count(), object_count);
        assert_eq!(diagnostics.object_count, object_count);
    }
}

#[test]
fn oamd_total_accessor_follows_session_metadata_and_reset() {
    let mut session = OpenJocSession::new(OpenJocConfig::default()).unwrap();
    assert_eq!(session.diagnostics().oamd_total_object_count(), None);
    session
        .push_packet(OpenJocPacket {
            data: &include_bytes!("fixtures/timestamps.ec3")[..4096],
            pts_samples: None,
            discontinuity: false,
            preroll: false,
        })
        .unwrap();
    let diagnostics = session.diagnostics();
    assert!(diagnostics.object_count.is_some());
    assert_eq!(
        diagnostics.oamd_total_object_count(),
        diagnostics.object_count
    );
    session.reset();
    assert_eq!(session.diagnostics().oamd_total_object_count(), None);
}
