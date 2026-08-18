//! Minimal packet-oriented Rust integration example.
//!
//! Usage: cargo run -p openjoc-api --example rust_stream -- INPUT.eac3

use openjoc_api::{OpenJocConfig, OpenJocPacket, OpenJocSession, OpenJocStatus};
use std::{env, fs, process};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: rust_stream INPUT.eac3");
        process::exit(2);
    });
    let bytes = fs::read(input)?;
    let mut session = OpenJocSession::new(OpenJocConfig::default())?;
    println!("output: {:?}", session.output_info());

    // A real demuxer should pass one complete JOC access unit per call. The
    // example uses the repository's existing indexed/framed raw stream reader
    // only to keep this demonstration short.
    let frames = openjoc_eac3::index_syncframes(&bytes)?;
    let units = openjoc_eac3::group_access_units(&frames)?;
    for unit in units {
        let start = frames[unit.first_frame].offset;
        let end = if let Some(last) = frames.get(unit.first_frame + unit.frame_count - 1) {
            last.offset + last.header.frame_size
        } else {
            start
        };
        let status = session.push_packet(OpenJocPacket {
            data: &bytes[start..end],
            pts_samples: None,
            discontinuity: false,
            preroll: false,
        })?;
        while let Some(frame) = session.receive_frame() {
            println!(
                "PCM: {} Hz, {} channels, {} samples, pts={:?}",
                frame.sample_rate, frame.channel_count, frame.sample_count, frame.pts_samples
            );
        }
        if status == OpenJocStatus::OutputPending {
            return Err("session requested output drain".into());
        }
    }
    let status = session.drain()?;
    while let Some(frame) = session.receive_frame() {
        println!("drain PCM: {} samples", frame.sample_count);
    }
    println!("drain status: {status:?}");
    Ok(())
}
