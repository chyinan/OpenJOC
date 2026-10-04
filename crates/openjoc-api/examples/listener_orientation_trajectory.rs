//! Synthetic, device-independent listener-orientation trajectory.
//!
//! Usage:
//!   cargo run -p openjoc-api --example listener_orientation_trajectory -- [OUTPUT_DIR]
//!
//! The example feeds deterministic tones as fixed virtual-speaker inputs to
//! the public dynamic direct-FIR renderer. It is a renderer/API demonstration,
//! not a synthetic JOC access unit or a sensor/device playback test. It writes
//! a moving-pose WAV, a fixed-identity reference WAV, and a text comparison
//! containing actual update sample receipts.

use openjoc_api::{
    BinauralConfig, ListenerOrientation, ListenerOrientationPreparer, MAX_DYNAMIC_HRIR_TAPS,
};
use openjoc_render::{
    BinauralSourceBlock, DEFAULT_DYNAMIC_BINAURAL_TRANSITION_SAMPLES, DynamicBinauralRenderer,
    DynamicBinauralSource,
};
use std::{
    error::Error,
    f64::consts::TAU,
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

const SAMPLE_RATE_HZ: u32 = 48_000;
const BLOCK_SAMPLES: usize = 128;
const PROGRAM_SAMPLES: usize = SAMPLE_RATE_HZ as usize;
const EVENT_SAMPLES: [usize; 3] = [12_288, 24_576, 36_864];
const EVENT_YAW_DEGREES: [f64; 3] = [30.0, -30.0, 0.0];

fn main() -> Result<(), Box<dyn Error>> {
    let output_dir = std::env::args_os().nth(1).map_or_else(
        || PathBuf::from("target/orientation-evidence/trajectory"),
        PathBuf::from,
    );
    fs::create_dir_all(&output_dir)?;

    let binaural_config = BinauralConfig::builtin_generic("7.1.4");
    let preparer = ListenerOrientationPreparer::new(&binaural_config)?;
    let identity = ListenerOrientation::IDENTITY;
    let initial_update = preparer.prepare(identity, 0, 0)?;
    let sources: Vec<_> = initial_update
        .kernels()
        .iter()
        .map(|kernel| {
            DynamicBinauralSource::new(
                kernel.source_id(),
                1.0 / initial_update.kernels().len() as f64,
            )
        })
        .collect::<Result<_, _>>()?;

    let mut moving = DynamicBinauralRenderer::new(
        SAMPLE_RATE_HZ,
        initial_update.clone(),
        sources.clone(),
        MAX_DYNAMIC_HRIR_TAPS,
        BLOCK_SAMPLES,
        DEFAULT_DYNAMIC_BINAURAL_TRANSITION_SAMPLES,
    )?;
    let mut fixed = DynamicBinauralRenderer::new(
        SAMPLE_RATE_HZ,
        initial_update,
        sources.clone(),
        MAX_DYNAMIC_HRIR_TAPS,
        BLOCK_SAMPLES,
        DEFAULT_DYNAMIC_BINAURAL_TRANSITION_SAMPLES,
    )?;

    let mut moving_pcm = Vec::with_capacity((PROGRAM_SAMPLES + 8_192) * 2);
    let mut fixed_pcm = Vec::with_capacity((PROGRAM_SAMPLES + 8_192) * 2);
    let mut receipts = vec![String::from(
        "initial_sequence=0 identity_kernel_set=initialized_before_render",
    )];
    let mut next_event = 0;
    let mut reported_sequence = 0;

    for start in (0..PROGRAM_SAMPLES).step_by(BLOCK_SAMPLES) {
        if next_event < EVENT_SAMPLES.len() && start == EVENT_SAMPLES[next_event] {
            let yaw = EVENT_YAW_DEGREES[next_event];
            let angle = yaw.to_radians() * 0.5;
            let pose = ListenerOrientation::new(0.0, 0.0, angle.sin(), angle.cos())?;
            let sequence = u64::try_from(next_event + 1)?;
            let update = preparer.prepare(pose, 0, sequence)?;
            let acceptance = moving
                .apply_prepared(update)
                .map_err(|failure| failure.error)?;
            // Retired filters are released here, on this non-realtime control path.
            drop(acceptance.retired_kernels);
            println!(
                "accepted sequence={sequence} yaw={yaw:+.0}deg superseded={:?}",
                acceptance.superseded_sequence
            );
            next_event += 1;
        }

        let sample_count = BLOCK_SAMPLES.min(PROGRAM_SAMPLES - start);
        let inputs: Vec<Vec<f64>> = (0..sources.len())
            .map(|source_index| {
                let frequency_hz = 220.0 + source_index as f64 * 31.0;
                let phase = source_index as f64 * 0.41;
                (0..sample_count)
                    .map(|offset| {
                        let sample = (start + offset) as f64;
                        0.04 * (TAU * frequency_hz * sample / f64::from(SAMPLE_RATE_HZ) + phase)
                            .sin()
                    })
                    .collect()
            })
            .collect();
        let blocks: Vec<_> = sources
            .iter()
            .zip(&inputs)
            .map(|(source, samples)| BinauralSourceBlock::new(source.id(), samples))
            .collect();
        let mut moving_left = vec![0.0; sample_count];
        let mut moving_right = vec![0.0; sample_count];
        let mut fixed_left = vec![0.0; sample_count];
        let mut fixed_right = vec![0.0; sample_count];
        moving.render_block(&blocks, &mut moving_left, &mut moving_right)?;
        fixed.render_block(&blocks, &mut fixed_left, &mut fixed_right)?;
        append_stereo_f32(&mut moving_pcm, &moving_left, &moving_right);
        append_stereo_f32(&mut fixed_pcm, &fixed_left, &fixed_right);

        if let Some(receipt) = moving.last_applied_update()
            && receipt.sequence > reported_sequence
        {
            receipts.push(format!(
                "sequence={} logical_start_sample={}",
                receipt.sequence, receipt.logical_start_sample
            ));
            println!(
                "applied sequence={} logical_start_sample={}",
                receipt.sequence, receipt.logical_start_sample
            );
            reported_sequence = receipt.sequence;
        }
    }

    moving.begin_drain()?;
    fixed.begin_drain()?;
    while moving.remaining_tail_samples() > 0 || fixed.remaining_tail_samples() > 0 {
        let count = BLOCK_SAMPLES
            .min(moving.remaining_tail_samples())
            .max(BLOCK_SAMPLES.min(fixed.remaining_tail_samples()));
        let moving_count = count.min(moving.remaining_tail_samples());
        let fixed_count = count.min(fixed.remaining_tail_samples());
        if moving_count > 0 {
            let mut left = vec![0.0; moving_count];
            let mut right = vec![0.0; moving_count];
            moving.drain_tail_block(&mut left, &mut right)?;
            append_stereo_f32(&mut moving_pcm, &left, &right);
        }
        if fixed_count > 0 {
            let mut left = vec![0.0; fixed_count];
            let mut right = vec![0.0; fixed_count];
            fixed.drain_tail_block(&mut left, &mut right)?;
            append_stereo_f32(&mut fixed_pcm, &left, &right);
        }
    }

    let stable_samples = EVENT_SAMPLES[0] * 2;
    let transition_samples = (EVENT_SAMPLES[1] - EVENT_SAMPLES[0]) * 2;
    let stable_max_delta =
        max_abs_delta(&moving_pcm[..stable_samples], &fixed_pcm[..stable_samples]);
    let moving_segment_rms = rms(&moving_pcm[stable_samples..stable_samples + transition_samples]);
    let reference_segment_rms =
        rms(&fixed_pcm[stable_samples..stable_samples + transition_samples]);
    let pose_delta_rms = rms_delta(
        &moving_pcm[stable_samples..stable_samples + transition_samples],
        &fixed_pcm[stable_samples..stable_samples + transition_samples],
    );

    let trajectory_path = output_dir.join("listener-orientation-trajectory.wav");
    let reference_path = output_dir.join("listener-orientation-identity-reference.wav");
    write_float_wav(&trajectory_path, &moving_pcm)?;
    write_float_wav(&reference_path, &fixed_pcm)?;
    let summary_path = output_dir.join("listener-orientation-summary.txt");
    let summary = format!(
        "sample_rate_hz={SAMPLE_RATE_HZ}\nlayout=7.1.4\nsource_count={}\nprogram_samples={PROGRAM_SAMPLES}\nmax_render_block_samples={BLOCK_SAMPLES}\ntransition_samples={}\nidentity_prefix_max_abs_delta={stable_max_delta:.9}\nfirst_yaw_segment_trajectory_rms={moving_segment_rms:.9}\nfirst_yaw_segment_identity_reference_rms={reference_segment_rms:.9}\nfirst_yaw_segment_delta_rms={pose_delta_rms:.9}\n{}\ntrajectory_wav={}\nidentity_reference_wav={}\n",
        sources.len(),
        DEFAULT_DYNAMIC_BINAURAL_TRANSITION_SAMPLES,
        receipts.join("\n"),
        trajectory_path.display(),
        reference_path.display(),
    );
    fs::write(&summary_path, summary)?;
    println!("wrote {}", trajectory_path.display());
    println!("wrote {}", reference_path.display());
    println!("wrote {}", summary_path.display());
    println!("identity-prefix max absolute delta: {stable_max_delta:.9}");
    println!("first yaw interval dynamic-vs-identity RMS delta: {pose_delta_rms:.9}");
    Ok(())
}

fn append_stereo_f32(output: &mut Vec<f32>, left: &[f64], right: &[f64]) {
    for (&left, &right) in left.iter().zip(right) {
        output.extend([left as f32, right as f32]);
    }
}

fn max_abs_delta(left: &[f32], right: &[f32]) -> f32 {
    left.iter()
        .zip(right)
        .map(|(&left, &right)| (left - right).abs())
        .fold(0.0, f32::max)
}

fn rms(samples: &[f32]) -> f64 {
    (samples
        .iter()
        .map(|&value| f64::from(value).powi(2))
        .sum::<f64>()
        / samples.len() as f64)
        .sqrt()
}

fn rms_delta(left: &[f32], right: &[f32]) -> f64 {
    (left
        .iter()
        .zip(right)
        .map(|(&left, &right)| (f64::from(left) - f64::from(right)).powi(2))
        .sum::<f64>()
        / left.len() as f64)
        .sqrt()
}

fn write_float_wav(path: &Path, interleaved: &[f32]) -> Result<(), Box<dyn Error>> {
    let data_len = u32::try_from(std::mem::size_of_val(interleaved))?;
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(b"RIFF")?;
    writer.write_all(&(36_u32 + data_len).to_le_bytes())?;
    writer.write_all(b"WAVEfmt ")?;
    writer.write_all(&16_u32.to_le_bytes())?;
    writer.write_all(&3_u16.to_le_bytes())?; // IEEE float
    writer.write_all(&2_u16.to_le_bytes())?; // stereo
    writer.write_all(&SAMPLE_RATE_HZ.to_le_bytes())?;
    writer.write_all(&(SAMPLE_RATE_HZ * 2 * 4).to_le_bytes())?;
    writer.write_all(&8_u16.to_le_bytes())?; // block align
    writer.write_all(&32_u16.to_le_bytes())?;
    writer.write_all(b"data")?;
    writer.write_all(&data_len.to_le_bytes())?;
    for sample in interleaved {
        writer.write_all(&sample.to_le_bytes())?;
    }
    writer.flush()?;
    Ok(())
}
