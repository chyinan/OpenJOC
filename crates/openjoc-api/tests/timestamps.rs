use openjoc_api::{
    BinauralConfig, OpenJocConfig, OpenJocError, OpenJocPacket, OpenJocPcmFrame, OpenJocSession,
    RenderMode,
};

// Public, valid six-block JOC AUs. Timestamp coverage never changes codec bytes.
const FIXTURE: &[u8] = include_bytes!("fixtures/timestamps.ec3");
const AU_BYTES: usize = 4096;
const AU_SAMPLES: i64 = 1536;

fn packet(index: usize, pts_samples: Option<i64>, discontinuity: bool) -> OpenJocPacket<'static> {
    OpenJocPacket {
        data: &FIXTURE[index * AU_BYTES..(index + 1) * AU_BYTES],
        pts_samples,
        discontinuity,
        preroll: false,
    }
}

fn receive(session: &mut OpenJocSession, pull: bool) -> Vec<OpenJocPcmFrame> {
    let mut frames = Vec::new();
    loop {
        let next = if pull {
            session.receive_binaural_frame().unwrap()
        } else {
            session.receive_frame()
        };
        match next {
            Some(frame) => frames.push(frame),
            None => return frames,
        }
    }
}

fn check_segment(session: &mut OpenJocSession, pull: bool, origin: i64, anchor_au: usize) {
    let mut logical_start = 0_i64;
    let mut prior_frames = Vec::new();
    for index in 0..6 {
        // Exercise omitted PTS both before and after anchoring, with a later
        // explicit timestamp proving that continuation was not shifted.
        let pts = (index == anchor_au || index == 5)
            .then_some(origin + i64::try_from(index).unwrap() * AU_SAMPLES);
        session.push_packet(packet(index, pts, false)).unwrap();
        for frame in receive(session, pull) {
            assert_eq!(
                frame.pts_samples,
                (index >= anchor_au).then_some(origin + logical_start)
            );
            logical_start += i64::try_from(frame.sample_count).unwrap();
            if index < anchor_au {
                prior_frames.push(frame);
            }
        }
    }
    session.drain().unwrap();
    for frame in receive(session, pull) {
        assert_eq!(frame.pts_samples, Some(origin + logical_start));
        logical_start += i64::try_from(frame.sample_count).unwrap();
    }
    assert!(session.is_drained());
    assert!(logical_start >= 6 * AU_SAMPLES);
    // Retained owned frames cannot acquire timestamps retroactively.
    assert!(prior_frames.iter().all(|frame| frame.pts_samples.is_none()));
    if anchor_au >= 3 {
        assert!(!matches!(prior_frames.as_slice(), []));
    }
}

#[test]
fn first_and_late_packet_pts_preserve_sample_domain_for_signed_origins() {
    for origin in [i64::MIN, -48_000, 0, 12_345] {
        for anchor_au in [0, 1, 3] {
            let mut session = OpenJocSession::new(OpenJocConfig::default()).unwrap();
            check_segment(&mut session, false, origin, anchor_au);
        }
    }
}

#[test]
fn late_pts_applies_to_static_and_pull_binaural_output_and_tails() {
    for pull in [false, true] {
        let config = OpenJocConfig {
            render_mode: RenderMode::Binaural,
            binaural: Some(BinauralConfig::builtin_generic("5.1")),
            ..OpenJocConfig::default()
        };
        let mut session = if pull {
            OpenJocSession::new_with_listener_orientation_pull(config, 128).unwrap()
        } else {
            OpenJocSession::new(config).unwrap()
        };
        check_segment(&mut session, pull, -12_345, 3);
    }
}

#[test]
fn unrepresentable_late_origin_rejects_without_committing_and_allows_retry() {
    let mut session = OpenJocSession::new(OpenJocConfig::default()).unwrap();
    session.push_packet(packet(0, None, false)).unwrap();
    assert!(matches!(receive(&mut session, false).as_slice(), []));
    let info = session.output_info();
    let error = session
        .push_packet(packet(1, Some(i64::MIN), false))
        .unwrap_err();
    assert!(
        matches!(error, OpenJocError::InvalidPacket(message) if message.contains("segment origin"))
    );
    assert_eq!(session.output_info(), info);
    assert!(matches!(receive(&mut session, false).as_slice(), []));
    session.push_packet(packet(1, Some(1536), false)).unwrap();
    let first = receive(&mut session, false);
    assert_eq!(first[0].pts_samples, Some(0));
    session.push_packet(packet(2, Some(3072), false)).unwrap();
    assert_eq!(receive(&mut session, false)[0].pts_samples, Some(1536));
}

#[test]
fn expected_packet_timestamp_overflow_is_not_saturated() {
    let mut session = OpenJocSession::new(OpenJocConfig::default()).unwrap();
    session
        .push_packet(packet(0, Some(i64::MAX), false))
        .unwrap();
    assert!(matches!(receive(&mut session, false).as_slice(), []));
    for _ in 0..2 {
        let error = session
            .push_packet(packet(1, Some(i64::MAX), false))
            .unwrap_err();
        assert!(
            matches!(error, OpenJocError::InvalidPacket(message) if message.contains("expected packet timestamp"))
        );
    }
    session.reset();
    check_segment(&mut session, false, 0, 1);
}

#[test]
fn drain_output_timestamp_overflow_is_reported_and_reset_recovers() {
    let mut session = OpenJocSession::new(OpenJocConfig::default()).unwrap();
    session
        .push_packet(packet(0, Some(i64::MAX), false))
        .unwrap();
    let error = session.drain().unwrap_err();
    assert!(matches!(error, OpenJocError::Render(message) if message.contains("output timestamp")));
    assert!(session.is_drained());
    assert!(session.receive_frame().is_none());
    session.reset();
    check_segment(&mut session, false, 42, 1);
}

#[test]
fn reset_and_discontinuity_discard_the_previous_anchor() {
    let mut session = OpenJocSession::new(OpenJocConfig::default()).unwrap();
    check_segment(&mut session, false, 12_345, 1);
    session.reset();
    check_segment(&mut session, false, -48_000, 3);
    session.reset();
    session.push_packet(packet(0, Some(99_999), false)).unwrap();
    assert!(matches!(receive(&mut session, false).as_slice(), []));
    // Untimestamped discontinuity must erase the old origin, then allow a new
    // late anchor for this new segment's second AU.
    session.push_packet(packet(0, None, true)).unwrap();
    assert!(matches!(receive(&mut session, false).as_slice(), []));
    session.push_packet(packet(1, Some(1536), false)).unwrap();
    assert_eq!(receive(&mut session, false)[0].pts_samples, Some(0));
    session.push_packet(packet(2, Some(3072), false)).unwrap();
    assert_eq!(receive(&mut session, false)[0].pts_samples, Some(1536));
}
