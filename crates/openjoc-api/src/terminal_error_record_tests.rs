use super::*;

#[test]
fn terminal_record_retains_original_categories_and_fields() {
    let mut session = OpenJocSession::new(OpenJocConfig::default()).unwrap();
    for error in [
        OpenJocError::InvalidConfig("fixed configuration failure".to_owned()),
        OpenJocError::InvalidPacket("fixed packet failure".to_owned()),
        OpenJocError::Decode("fixed decode failure".to_owned()),
        OpenJocError::Render("fixed render failure".to_owned()),
        OpenJocError::FormatChanged {
            expected: 48_000,
            actual: 44_100,
        },
        OpenJocError::TimestampDiscontinuity {
            expected: 1536,
            actual: 3072,
        },
        OpenJocError::ProfileChanged,
        OpenJocError::EmptyStream,
        OpenJocError::AlreadyDrained,
        OpenJocError::OutputPending,
        OpenJocError::Unsupported("fixed operation failure".to_owned()),
    ] {
        session.fail_closed(&error);
        let record = session.terminal_error.as_ref().unwrap();
        assert_eq!(record.original, error);
        assert_eq!(
            record.legacy_error(),
            OpenJocError::Render(error.to_string())
        );
    }
    session.try_reset().unwrap();
    assert!(session.terminal_error.is_none());
}
