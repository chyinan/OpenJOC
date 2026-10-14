//! Legacy terminal-error behavior, using fixed errors and admitted codec bytes.
//! No malformed input or integer-limit state is needed to exercise this contract.

use super::*;
use std::{io::Write as _, path::Path};

fn failures() -> [(OpenJocError, &'static str); 11] {
    [
        (
            OpenJocError::InvalidConfig("fixed configuration failure".to_owned()),
            "invalid OpenJOC configuration: fixed configuration failure",
        ),
        (
            OpenJocError::InvalidPacket("fixed packet failure".to_owned()),
            "invalid OpenJOC packet: fixed packet failure",
        ),
        (
            OpenJocError::Decode("fixed decode failure".to_owned()),
            "OpenJOC decode error: fixed decode failure",
        ),
        (
            OpenJocError::Render("fixed render failure".to_owned()),
            "OpenJOC render error: fixed render failure",
        ),
        (
            OpenJocError::FormatChanged {
                expected: 48_000,
                actual: 44_100,
            },
            "sample rate changed from 48000 Hz to 44100 Hz",
        ),
        (
            OpenJocError::TimestampDiscontinuity {
                expected: 1536,
                actual: 3072,
            },
            "timestamp discontinuity: expected 1536, received 3072",
        ),
        (
            OpenJocError::ProfileChanged,
            "validation profile changed within a session",
        ),
        (
            OpenJocError::EmptyStream,
            "cannot drain an empty OpenJOC stream",
        ),
        (
            OpenJocError::AlreadyDrained,
            "OpenJOC session is already drained",
        ),
        (
            OpenJocError::OutputPending,
            "output must be received before pushing more input",
        ),
        (
            OpenJocError::Unsupported("fixed operation failure".to_owned()),
            "unsupported OpenJOC operation: fixed operation failure",
        ),
    ]
}

fn session(pull: bool) -> OpenJocSession {
    if pull {
        OpenJocSession::new_with_listener_orientation_pull(
            OpenJocConfig {
                render_mode: RenderMode::Binaural,
                speaker_layout: "7.1.4".to_owned(),
                binaural: Some(BinauralConfig::builtin_generic("7.1.4")),
                ..OpenJocConfig::default()
            },
            128,
        )
        .unwrap()
    } else {
        OpenJocSession::new(OpenJocConfig::default()).unwrap()
    }
}

fn packet(index: usize, discontinuity: bool) -> OpenJocPacket<'static> {
    let bytes = include_bytes!("../../openjoc-wasm/testdata/joc.lifecycle.ec3");
    let frames = index_syncframes(bytes).unwrap();
    let units = group_access_units(&frames).unwrap();
    let unit = units[index];
    let first = &frames[unit.first_frame];
    let last = &frames[unit.first_frame + unit.frame_count - 1];
    let pts = units[..index]
        .iter()
        .map(|unit| i64::from(unit.samples))
        .sum();
    OpenJocPacket {
        data: &bytes[first.offset..last.offset + last.header.frame_size],
        pts_samples: Some(pts),
        discontinuity,
        preroll: false,
    }
}

#[derive(Default, Debug, Eq, PartialEq)]
struct Capture {
    events: String,
    pcm: Vec<u8>,
}

impl Capture {
    fn frame(&mut self, mut frame: OpenJocPcmFrame) {
        for sample in std::mem::take(&mut frame.interleaved_f32) {
            self.pcm.extend_from_slice(&sample.to_bits().to_le_bytes());
        }
        writeln!(self.events, "frame={frame:?}").unwrap();
    }

    fn save(&self, name: &str) {
        if let Some(directory) = std::env::var_os("OPENJOC_TERMINAL_CAPTURE") {
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

fn receive(session: &mut OpenJocSession, pull: bool, capture: &mut Capture) {
    while let Some(frame) = if pull {
        session.receive_binaural_frame().unwrap()
    } else {
        session.receive_frame()
    } {
        capture.frame(frame);
    }
}

fn recovery(session: &mut OpenJocSession, pull: bool) -> Capture {
    let mut capture = Capture::default();
    for index in 0..4 {
        let status = session.push_packet(packet(index, false)).unwrap();
        assert_ne!(status, OpenJocStatus::OutputPending);
        writeln!(capture.events, "push={status:?}").unwrap();
        receive(session, pull, &mut capture);
    }
    let status = session.drain().unwrap();
    writeln!(capture.events, "drain={status:?}").unwrap();
    receive(session, pull, &mut capture);
    assert_eq!(session.drain(), Ok(OpenJocStatus::EndOfStream));
    assert!(session.is_drained());
    assert_ne!(capture.pcm, [] as [u8; 0]);
    capture
}

fn assert_terminal(
    session: &mut OpenJocSession,
    error: &OpenJocError,
    display: &str,
    capture: &mut Capture,
) {
    // Freeze the original input category and complete text before recording it.
    let original = error.clone();
    assert_eq!(error.to_string(), display);
    session.fail_closed(error);
    assert_eq!(*error, original);
    writeln!(capture.events, "first={error:?};display={error}").unwrap();

    assert!(session.output_queue.is_empty());
    assert!(session.pending_binaural_inputs.is_empty());
    assert!(session.current_binaural_input.is_none());
    assert_eq!(session.pending_binaural_input_samples, 0);
    assert!(session.drained);
    assert!(session.is_drained());
    assert!(session.receive_frame().is_none());
    let terminal_debug = format!("{:?}", session.terminal_error);
    assert_eq!(terminal_debug, format!("{:?}", Some(display.to_owned())));
    // OpenJocSession's derived Debug must keep the former Option<String> shape.
    assert!(format!("{session:?}").ends_with(&format!("terminal_error: {terminal_debug} }}")));
    writeln!(
        capture.events,
        "terminal={terminal_debug};queues=0,0,None,0;drained=true"
    )
    .unwrap();

    let repeated = OpenJocError::Render(display.to_owned());
    let repeated_display = format!("OpenJOC render error: {display}");
    for discontinuity in [false, true] {
        // Terminal state precedes drained state, disabled pull, and a requested
        // discontinuity. Repeating calls must not accumulate more prefixes.
        for (operation, actual) in [
            (
                "push",
                session.push_packet(packet(0, discontinuity)).unwrap_err(),
            ),
            ("pull", session.receive_binaural_frame().unwrap_err()),
            ("drain", session.drain().unwrap_err()),
        ] {
            assert_eq!(actual, repeated);
            assert_eq!(actual.to_string(), repeated_display);
            writeln!(
                capture.events,
                "{operation}[{discontinuity}]={actual:?};display={actual}"
            )
            .unwrap();
        }
        assert!(session.receive_frame().is_none());
        assert_eq!(format!("{:?}", session.terminal_error), terminal_debug);
        assert!(session.is_drained());
    }
}

fn reset_and_recover(session: &mut OpenJocSession, pull: bool, capture: &mut Capture) {
    let epoch = session.listener_orientation_stream_epoch();
    let result = session.try_reset();
    assert_eq!(result, Ok(()));
    assert!(session.terminal_error.is_none());
    assert!(!session.drained);
    assert!(!session.is_drained());
    assert_eq!(session.pending_binaural_input_samples, 0);
    assert_eq!(
        session.listener_orientation_stream_epoch(),
        epoch.map(|value| value + 1)
    );
    writeln!(
        capture.events,
        "reset={result:?};terminal=None;drained=false;epoch={:?}",
        session.listener_orientation_stream_epoch()
    )
    .unwrap();
    let recovered = recovery(session, pull);
    assert_eq!(recovered, recovery(&mut self::session(pull), pull));
    capture.events.push_str(&recovered.events);
    capture.pcm.extend(recovered.pcm);
}

#[test]
fn fresh_terminal_variants_preserve_legacy_errors_and_reset() {
    for pull in [false, true] {
        for (index, (error, display)) in failures().iter().enumerate() {
            let mut session = session(pull);
            let mut capture = Capture::default();
            assert_terminal(&mut session, error, display, &mut capture);
            reset_and_recover(&mut session, pull, &mut capture);
            capture.save(&format!("fresh-{pull}-{index}"));
        }
    }
}

#[test]
fn terminal_failure_clears_admitted_pcm_in_each_queue_state() {
    for (name, pull, partial, drain) in [
        ("speaker-queued", false, false, false),
        ("pull-queued", true, false, false),
        ("pull-partial", true, true, false),
        ("speaker-draining", false, false, true),
        ("pull-draining", true, false, true),
    ] {
        let mut session = session(pull);
        let mut capture = Capture::default();
        for index in 0..6 {
            session.push_packet(packet(index, false)).unwrap();
            if !session.output_queue.is_empty() || session.pending_binaural_input_samples > 0 {
                break;
            }
        }
        assert!((!session.output_queue.is_empty() || session.pending_binaural_input_samples > 0));
        if drain {
            receive(&mut session, pull, &mut capture);
            assert_eq!(session.drain(), Ok(OpenJocStatus::FrameAvailable));
            assert!(session.drained);
            assert!(!session.is_drained());
        }
        if partial {
            capture.frame(session.receive_binaural_frame().unwrap().unwrap());
            assert!(session.current_binaural_input.is_some());
            assert!(session.pending_binaural_input_samples > 0);
        } else if pull {
            assert_ne!(session.pending_binaural_inputs.len(), 0);
            assert!(session.pending_binaural_input_samples > 0);
        } else {
            assert_ne!(session.output_queue.len(), 0);
        }
        writeln!(
            capture.events,
            "before-failure=output:{},pending:{},current:{},samples:{},drained:{}",
            session.output_queue.len(),
            session.pending_binaural_inputs.len(),
            session.current_binaural_input.is_some(),
            session.pending_binaural_input_samples,
            session.drained
        )
        .unwrap();
        assert_terminal(
            &mut session,
            &OpenJocError::Render("fixed render failure".to_owned()),
            "OpenJOC render error: fixed render failure",
            &mut capture,
        );
        reset_and_recover(&mut session, pull, &mut capture);
        capture.save(name);
    }
}

#[test]
fn repeated_fail_closed_replaces_the_previous_error() {
    let mut session = session(false);
    let mut capture = Capture::default();
    for (error, display) in failures() {
        assert_terminal(&mut session, &error, display, &mut capture);
    }
    // The infallible reset alias must also clear the terminal record.
    session.reset();
    assert!(session.terminal_error.is_none());
    assert!(!session.is_drained());
    let recovered = recovery(&mut session, false);
    assert_eq!(recovered, recovery(&mut self::session(false), false));
    capture.events.push_str(&recovered.events);
    capture.pcm.extend(recovered.pcm);
    capture.save("replacement");
}
