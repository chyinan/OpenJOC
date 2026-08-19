use openjoc_api::{OpenJocConfig, OpenJocPacket, OpenJocSession, RenderMode};
use openjoc_eac3::{group_access_units, index_syncframes};
use sha2::{Digest, Sha256};
use std::{env, fmt::Write as _, fs, process::ExitCode, time::Instant};

fn run() -> Result<(), String> {
    let mut arguments = env::args().skip(1);
    let input = arguments
        .next()
        .ok_or("usage: direct_checksum INPUT.ec3 LAYOUT")?;
    let layout = arguments
        .next()
        .ok_or("usage: direct_checksum INPUT.ec3 LAYOUT")?;
    if arguments.next().is_some() {
        return Err("usage: direct_checksum INPUT.ec3 LAYOUT".to_owned());
    }
    let stream = fs::read(input).map_err(|error| error.to_string())?;
    let frames = index_syncframes(&stream).map_err(|error| error.to_string())?;
    let units = group_access_units(&frames).map_err(|error| error.to_string())?;
    let mut session = OpenJocSession::new(OpenJocConfig {
        render_mode: RenderMode::Speaker,
        speaker_layout: layout,
        ..OpenJocConfig::default()
    })
    .map_err(|error| error.to_string())?;
    let started = Instant::now();
    let mut digest = Sha256::new();
    let mut output_frames = 0_u64;
    let mut output_samples = 0_u64;
    let mut pts = 0_i64;
    for unit in units {
        let first = frames[unit.first_frame];
        let last = frames[unit.first_frame + unit.frame_count - 1];
        let end = last.offset + last.header.frame_size;
        session
            .push_packet(OpenJocPacket {
                data: &stream[first.offset..end],
                pts_samples: Some(pts),
                discontinuity: false,
                preroll: false,
            })
            .map_err(|error| error.to_string())?;
        pts = pts
            .checked_add(i64::from(unit.samples))
            .ok_or("PTS overflow")?;
        while let Some(frame) = session.receive_frame() {
            for sample in frame.interleaved_f32 {
                digest.update(sample.to_le_bytes());
            }
            output_frames = output_frames.saturating_add(1);
            output_samples = output_samples.saturating_add(frame.sample_count as u64);
        }
    }
    let _ = session.drain().map_err(|error| error.to_string())?;
    while let Some(frame) = session.receive_frame() {
        for sample in frame.interleaved_f32 {
            digest.update(sample.to_le_bytes());
        }
        output_frames = output_frames.saturating_add(1);
        output_samples = output_samples.saturating_add(frame.sample_count as u64);
    }
    let checksum = digest
        .finalize()
        .iter()
        .fold(String::new(), |mut output, byte| {
            let _ = write!(output, "{byte:02x}");
            output
        });
    println!(
        "direct-openjoc frames={output_frames} samples_per_channel={output_samples} elapsed_seconds={:.6} pcm-openjoc-order-f32le-sha256={checksum}",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
