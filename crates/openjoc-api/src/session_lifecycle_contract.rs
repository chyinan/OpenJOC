//! Fixed, reachable session transitions using complete admitted access units.
//! Capture files are create-only and freeze full PCM bytes and observable state.

use super::*;
use crate::BinauralConfig;
use std::{io::Write as _, path::Path};

#[derive(Clone, Copy, Debug)]
enum Mode {
    Speaker,
    Binaural,
    Pull128,
    Pull256,
}

const MODES: [Mode; 4] = [Mode::Speaker, Mode::Binaural, Mode::Pull128, Mode::Pull256];

fn session(mode: Mode, epoch: u64) -> OpenJocSession {
    if matches!(mode, Mode::Speaker) {
        return OpenJocSession::new(OpenJocConfig::default()).unwrap();
    }
    let config = OpenJocConfig {
        render_mode: RenderMode::Binaural,
        speaker_layout: "7.1.4".to_owned(),
        binaural: Some(BinauralConfig::builtin_generic("7.1.4")),
        ..OpenJocConfig::default()
    };
    match mode {
        Mode::Speaker => unreachable!(),
        Mode::Binaural => OpenJocSession::new(config).unwrap(),
        Mode::Pull128 | Mode::Pull256 => {
            let size = if matches!(mode, Mode::Pull128) {
                128
            } else {
                256
            };
            OpenJocSession::new_with_listener_orientation_pull_at_epoch(config, size, epoch)
                .unwrap()
        }
    }
}

fn packet(index: usize, origin: i64, discontinuity: bool) -> OpenJocPacket<'static> {
    let bytes = include_bytes!("../../openjoc-wasm/testdata/joc.lifecycle.ec3");
    let frames = index_syncframes(bytes).unwrap();
    let units = group_access_units(&frames).unwrap();
    let unit = units[index];
    let first = &frames[unit.first_frame];
    let last = &frames[unit.first_frame + unit.frame_count - 1];
    OpenJocPacket {
        data: &bytes[first.offset..last.offset + last.header.frame_size],
        pts_samples: Some(
            origin
                + units[..index]
                    .iter()
                    .map(|unit| i64::from(unit.samples))
                    .sum::<i64>(),
        ),
        discontinuity,
        preroll: false,
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
struct Capture {
    events: String,
    pcm: Vec<u8>,
}

impl Capture {
    fn state(&mut self, name: &str, session: &OpenJocSession) {
        let pending = !session.output_queue.is_empty()
            || !session.pending_binaural_inputs.is_empty()
            || session.current_binaural_input.is_some();
        assert_eq!(session.has_pending_output(), pending);
        let remaining = session
            .pending_binaural_inputs
            .iter()
            .map(|frame| frame.sample_count)
            .sum::<usize>()
            + session
                .current_binaural_input
                .as_ref()
                .map_or(0, |(frame, offset)| frame.sample_count - offset);
        assert_eq!(session.pending_binaural_input_samples, remaining);
        let tail = session.binaural_pull_samples.is_some()
            && session.drained
            && session
                .binaural
                .as_ref()
                .is_some_and(|state| !state.has_no_tail());
        assert_eq!(session.has_unrendered_pull_tail(), tail);
        assert_eq!(
            session.is_drained(),
            session.drained && (session.terminal_error.is_some() || (!pending && !tail))
        );
        writeln!(self.events,
            "{name}:output={},deferred={},current={:?},samples={},started={},complete={},tail={},terminal={:?},epoch={:?},origin={:?},next={},end={}",
            session.output_queue.len(), session.pending_binaural_inputs.len(),
            session.current_binaural_input.as_ref().map(|(frame, offset)| (frame.sample_count, *offset)),
            remaining, session.drained, session.is_drained(), tail, session.terminal_error,
            session.listener_orientation_stream_epoch(), session.segment_pts,
            session.next_input_sample, session.last_output_end).unwrap();
    }

    fn frame(&mut self, mut frame: OpenJocPcmFrame) {
        assert_eq!(
            frame.interleaved_f32.len(),
            frame.sample_count * frame.channel_count
        );
        for sample in std::mem::take(&mut frame.interleaved_f32) {
            self.pcm.extend_from_slice(&sample.to_bits().to_le_bytes());
        }
        writeln!(self.events, "frame={frame:?}").unwrap();
    }

    fn save(&self, name: &str) {
        if let Some(directory) = std::env::var_os("OPENJOC_LIFECYCLE_CAPTURE") {
            let directory = Path::new(&directory);
            for (extension, bytes) in [
                ("txt", self.events.as_bytes()),
                ("f32le", self.pcm.as_slice()),
            ] {
                std::fs::File::create_new(directory.join(format!("{name}.{extension}")))
                    .unwrap()
                    .write_all(bytes)
                    .unwrap();
            }
        }
    }
}

fn receive_one(session: &mut OpenJocSession, capture: &mut Capture) -> bool {
    let frame = if session.binaural_pull_samples.is_some() {
        assert!(session.receive_frame().is_none());
        session.receive_binaural_frame().unwrap()
    } else {
        session.receive_frame()
    };
    let Some(frame) = frame else { return false };
    if let Some(size) = session.binaural_pull_samples {
        assert!(frame.sample_count <= size);
    }
    capture.frame(frame);
    capture.state("receive", session);
    true
}

fn receive_all(session: &mut OpenJocSession, capture: &mut Capture) {
    while receive_one(session, capture) {}
}

fn pending_backpressure(session: &mut OpenJocSession, capture: &mut Capture) {
    assert!(session.has_pending_output());
    let mut before = Capture::default();
    before.state("backpressure", session);
    let expected_push = if session.drained {
        Err(OpenJocError::AlreadyDrained)
    } else {
        Ok(OpenJocStatus::OutputPending)
    };
    // A requested discontinuity and a different, ordinary PTS do not bypass
    // pending output. Once drain starts, AlreadyDrained takes priority instead.
    let push = session.push_packet(packet(0, 48_000, true));
    assert_eq!(push, expected_push);
    let drain = session.drain();
    assert_eq!(drain, Ok(OpenJocStatus::OutputPending));
    let mut after = Capture::default();
    after.state("backpressure", session);
    assert_eq!(before, after);
    writeln!(capture.events, "blocked:push={push:?};drain={drain:?}").unwrap();
    capture.events.push_str(&after.events);
}

fn finish(session: &mut OpenJocSession, capture: &mut Capture) {
    assert_eq!(session.drain(), Ok(OpenJocStatus::FrameAvailable));
    assert!(session.drained);
    assert!(!session.is_drained());
    capture.state("drain-started", session);
    pending_backpressure(session, capture);
    while session.has_pending_output() {
        assert!(receive_one(session, capture));
    }
    if session.binaural_pull_samples.is_some() {
        assert!(session.has_unrendered_pull_tail());
        assert!(!session.is_drained());
        assert_eq!(session.drain(), Ok(OpenJocStatus::FrameAvailable));
        capture.state("tail-only", session);
    }
    receive_all(session, capture);
    assert!(session.is_drained());
    assert_eq!(session.drain(), Ok(OpenJocStatus::EndOfStream));
    assert_eq!(session.drain(), Ok(OpenJocStatus::EndOfStream));
    assert_eq!(
        session.push_packet(packet(0, 48_000, true)),
        Err(OpenJocError::AlreadyDrained)
    );
    capture.state("complete", session);
}

fn recovery(session: &mut OpenJocSession) -> Capture {
    let mut capture = Capture::default();
    for index in 0..4 {
        let status = session.push_packet(packet(index, 24_000, false)).unwrap();
        assert_ne!(status, OpenJocStatus::OutputPending);
        writeln!(capture.events, "push={status:?}").unwrap();
        capture.state("push", session);
        receive_all(session, &mut capture);
    }
    finish(session, &mut capture);
    assert_ne!(capture.pcm, [] as [u8; 0]);
    capture
}

#[test]
fn safe_pending_and_drain_transition_table() {
    for mode in MODES {
        let mut session = session(mode, 7);
        let mut capture = Capture::default();
        capture.state("fresh", &session);
        assert!(!session.drained);
        assert!(!session.is_drained());
        if session.binaural_pull_samples.is_none() {
            let result = session.receive_binaural_frame();
            assert_eq!(
                result,
                Err(OpenJocError::InvalidConfig(
                    "bounded binaural pull is not enabled for this session".to_owned()
                ))
            );
            writeln!(capture.events, "disabled-pull={result:?}").unwrap();
        }
        for index in 0..4 {
            let status = session.push_packet(packet(index, 24_000, false)).unwrap();
            assert_ne!(status, OpenJocStatus::OutputPending);
            writeln!(capture.events, "push={status:?}").unwrap();
            capture.state("push", &session);
            if session.has_pending_output() {
                pending_backpressure(&mut session, &mut capture);
                if session.binaural_pull_samples.is_some() {
                    assert!(receive_one(&mut session, &mut capture));
                    assert!(session.current_binaural_input.is_some());
                    pending_backpressure(&mut session, &mut capture);
                }
            }
            receive_all(&mut session, &mut capture);
            assert!(!session.has_pending_output());
            assert!(!session.is_drained());
        }
        finish(&mut session, &mut capture);
        capture.save(&format!("transitions-{mode:?}"));

        let mut empty = self::session(mode, 7);
        let mut capture = Capture::default();
        capture.state("empty", &empty);
        assert_eq!(empty.drain(), Ok(OpenJocStatus::EndOfStream));
        assert!(empty.is_drained());
        assert_eq!(empty.drain(), Ok(OpenJocStatus::EndOfStream));
        assert!(!receive_one(&mut empty, &mut capture));
        assert_eq!(
            empty.push_packet(packet(0, 24_000, true)),
            Err(OpenJocError::AlreadyDrained)
        );
        capture.state("empty-complete", &empty);
        capture.save(&format!("empty-{mode:?}"));
    }
}

#[derive(Clone, Copy, Debug)]
enum Phase {
    Queued,
    Partial,
    Draining,
    Tail,
}

fn prepare(mode: Mode, phase: Phase, capture: &mut Capture) -> OpenJocSession {
    let mut session = session(mode, 7);
    for index in 0..4 {
        session.push_packet(packet(index, 24_000, false)).unwrap();
        if session.has_pending_output() {
            break;
        }
    }
    assert!(session.has_pending_output());
    if matches!(phase, Phase::Partial) {
        assert!(receive_one(&mut session, capture));
        assert!(session.current_binaural_input.is_some());
    }
    if matches!(phase, Phase::Draining | Phase::Tail) {
        receive_all(&mut session, capture);
        assert_eq!(session.drain(), Ok(OpenJocStatus::FrameAvailable));
        assert!(session.drained);
        assert!(!session.is_drained());
    }
    if matches!(phase, Phase::Tail) {
        while session.has_pending_output() {
            assert!(receive_one(&mut session, capture));
        }
        assert!(session.has_unrendered_pull_tail());
    }
    capture.state("before-clear", &session);
    session
}

#[test]
fn safe_clear_paths_preserve_recovery_and_epochs() {
    for mode in MODES {
        for phase in [Phase::Queued, Phase::Partial, Phase::Draining, Phase::Tail] {
            if matches!(mode, Mode::Speaker | Mode::Binaural)
                && matches!(phase, Phase::Partial | Phase::Tail)
            {
                continue;
            }
            for operation in ["try-flush", "reset", "fail-closed"] {
                let mut capture = Capture::default();
                let mut session = prepare(mode, phase, &mut capture);
                let epoch = session.listener_orientation_stream_epoch();
                match operation {
                    "try-flush" => session.try_flush().unwrap(),
                    "reset" => session.reset(),
                    "fail-closed" => {
                        session.fail_closed(&OpenJocError::Render(
                            "fixed lifecycle failure".to_owned(),
                        ));
                        assert!(session.is_drained());
                        assert!(!session.has_pending_output());
                        assert_eq!(session.pending_binaural_input_samples, 0);
                        assert_eq!(session.listener_orientation_stream_epoch(), epoch);
                        capture.state("failed", &session);
                        let expected = OpenJocError::Render(
                            "OpenJOC render error: fixed lifecycle failure".to_owned(),
                        );
                        assert_eq!(
                            session.push_packet(packet(0, 48_000, true)),
                            Err(expected.clone())
                        );
                        assert_eq!(session.drain(), Err(expected.clone()));
                        assert_eq!(session.receive_binaural_frame(), Err(expected));
                        session.try_reset().unwrap();
                    }
                    _ => unreachable!(),
                }
                assert!(!session.has_pending_output());
                assert_eq!(session.pending_binaural_input_samples, 0);
                assert!(!session.drained);
                assert!(!session.is_drained());
                assert!(session.terminal_error.is_none());
                assert_eq!(session.sample_rate, None);
                assert_eq!(session.segment_pts, None);
                assert_eq!(session.next_input_sample, 0);
                assert_eq!(session.last_output_end, 0);
                assert_eq!(
                    session.listener_orientation_stream_epoch(),
                    epoch.map(|epoch| epoch + 1)
                );
                capture.state("cleared", &session);
                let actual = recovery(&mut session);
                let mut fresh = self::session(mode, epoch.map_or(7, |epoch| epoch + 1));
                assert_eq!(actual, recovery(&mut fresh));
                capture.events.push_str(&actual.events);
                capture.pcm.extend(actual.pcm);
                capture.save(&format!("clear-{mode:?}-{phase:?}-{operation}"));
            }
        }
    }
}

#[test]
fn safe_discontinuity_resets_only_after_backpressure_clears() {
    for mode in MODES {
        let mut capture = Capture::default();
        let mut session = prepare(mode, Phase::Queued, &mut capture);
        pending_backpressure(&mut session, &mut capture);
        receive_all(&mut session, &mut capture);
        let epoch = session.listener_orientation_stream_epoch();
        let status = session.push_packet(packet(0, 24_000, true)).unwrap();
        assert_ne!(status, OpenJocStatus::OutputPending);
        assert_eq!(session.segment_pts, Some(24_000));
        assert_eq!(
            session.listener_orientation_stream_epoch(),
            epoch.map(|epoch| epoch + 1)
        );
        capture.state("discontinuity", &session);
        let mut actual = Capture::default();
        receive_all(&mut session, &mut actual);
        finish(&mut session, &mut actual);

        let mut fresh = self::session(mode, epoch.map_or(7, |epoch| epoch + 1));
        assert_eq!(fresh.push_packet(packet(0, 24_000, false)), Ok(status));
        let mut expected = Capture::default();
        receive_all(&mut fresh, &mut expected);
        finish(&mut fresh, &mut expected);
        assert_eq!(actual, expected);
        capture.events.push_str(&actual.events);
        capture.pcm.extend(actual.pcm);
        capture.save(&format!("discontinuity-{mode:?}"));
    }
}
