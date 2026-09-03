// pattern: Imperative Shell

use openjoc_api::{
    BinauralConfig, DialnormMode, OpenJocConfig, OpenJocPacket, OpenJocSession, RenderMode,
};
use openjoc_eac3::{AccessUnitParse, parse_access_unit_bounds};
use std::{env, ffi::OsStr, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args_os().skip(1);
    let input = PathBuf::from(arguments.next().ok_or("missing input .ec3 path")?);
    let output = PathBuf::from(arguments.next().ok_or("missing output PCM path")?);
    let metadata_output = PathBuf::from(arguments.next().ok_or("missing output metadata path")?);
    let mut renderer = RenderMode::Stereo;
    let mut dialnorm = DialnormMode::Default;
    for argument in arguments {
        match argument.as_os_str() {
            value if value == OsStr::new("--stereo") => renderer = RenderMode::Stereo,
            value if value == OsStr::new("--binaural") => renderer = RenderMode::Binaural,
            value if value == OsStr::new("--calibrated") => dialnorm = DialnormMode::Default,
            value if value == OsStr::new("--unity") => dialnorm = DialnormMode::Analog,
            _ => {
                return Err(
                    "usage: dump-native-pcm <input.ec3> <output.f32le> <output.meta> [--stereo|--binaural] [--calibrated|--unity]".into(),
                );
            }
        }
    }

    let bytes = fs::read(input)?;
    let mut pending = bytes;
    let mut session = OpenJocSession::new(renderer_config(renderer, dialnorm))?;
    let mut frames = Vec::new();
    let mut access_units = 0;
    let mut eos = false;
    while !pending.is_empty() {
        match parse_access_unit_bounds(&pending, eos)? {
            AccessUnitParse::NeedMore => {
                eos = true;
            }
            AccessUnitParse::Complete(length) => {
                let packet: Vec<u8> = pending.drain(..length).collect();
                session.push_packet(OpenJocPacket {
                    data: &packet,
                    pts_samples: None,
                    discontinuity: false,
                    preroll: false,
                })?;
                access_units += 1;
                while let Some(frame) = session.receive_frame() {
                    frames.push(frame);
                }
                eos = false;
            }
        }
    }
    session.drain()?;
    while let Some(frame) = session.receive_frame() {
        frames.push(frame);
    }

    let sample_rate = frames
        .first()
        .map(|frame| frame.sample_rate)
        .ok_or("native fixture produced no PCM")?;
    let channels = frames
        .first()
        .map(|frame| frame.channel_count)
        .ok_or("native fixture produced no PCM")?;
    if sample_rate != 48_000 || channels != 2 {
        return Err(format!(
            "native renderer contract mismatch: {sample_rate} Hz, {channels} channels"
        )
        .into());
    }

    let pcm = frames
        .iter()
        .flat_map(|frame| frame.interleaved_f32.iter().copied())
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<u8>>();
    fs::write(output, pcm)?;
    let diagnostics = session.diagnostics();
    let total_samples = frames.iter().map(|frame| frame.sample_count).sum::<usize>();
    let metadata = format!(
        "sample_rate={sample_rate}\nchannels={channels}\nframe_count={}\nsamples={total_samples}\nduration_ms={:.12}\naccess_units={access_units}\nprofile={}\ndownmix_index={}\nobject_count={}\ncomplexity_index={}\nrenderer={}\ndialnorm={}\nvirtual_layout={}\nhrtf={}\nlatency_samples={}\n",
        frames.len(),
        total_samples as f64 * 1000.0 / f64::from(sample_rate),
        diagnostics.profile.map_or("none", |value| value),
        diagnostics
            .downmix_index
            .map_or_else(|| "none".to_owned(), |value| value.to_string()),
        diagnostics
            .object_count
            .map_or_else(|| "none".to_owned(), |value| value.to_string()),
        diagnostics
            .complexity_index
            .map_or_else(|| "none".to_owned(), |value| value.to_string()),
        if renderer == RenderMode::Binaural {
            "binaural"
        } else {
            "stereo"
        },
        if dialnorm == DialnormMode::Analog {
            "unity"
        } else {
            "calibrated"
        },
        if renderer == RenderMode::Binaural {
            "7.1.4"
        } else {
            "none"
        },
        if renderer == RenderMode::Binaural {
            "Built-in SADIE II D1"
        } else {
            "none"
        },
        session.latency_samples(),
    );
    fs::write(metadata_output, metadata)?;
    println!(
        "sample_rate={sample_rate} channels={channels} samples={total_samples} frames={} access_units={access_units}",
        frames.len()
    );
    Ok(())
}

fn renderer_config(renderer: RenderMode, dialnorm: DialnormMode) -> OpenJocConfig {
    OpenJocConfig {
        render_mode: renderer,
        speaker_layout: if renderer == RenderMode::Binaural {
            String::from("7.1.4")
        } else {
            String::from("2.0")
        },
        binaural: (renderer == RenderMode::Binaural)
            .then(|| BinauralConfig::builtin_generic("7.1.4")),
        dialnorm,
        ..OpenJocConfig::default()
    }
}
