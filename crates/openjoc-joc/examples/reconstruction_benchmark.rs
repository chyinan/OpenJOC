use num_complex::Complex64;
use openjoc_joc::{
    HuffmanCodeword, JocDataPoint, JocDecoderState, JocFrame, JocHeader, JocObjectFrame,
    JocPayloadData, QuantMode, ReconstructionStageTiming, Slope,
};
use std::{
    env,
    hint::black_box,
    time::{Duration, Instant},
};

const CHANNELS: usize = 5;
const OBJECTS: usize = 15;
const TIMESLOTS: usize = 24;
const SUBBANDS: usize = 64;

fn frame() -> JocFrame {
    let objects = (0..OBJECTS)
        .map(|object| JocObjectFrame {
            present: true,
            band_index: Some(0),
            band_count: Some(23),
            sparse: Some(false),
            quant_mode: Some(QuantMode::Fine192),
            slope: Some(Slope::Smooth),
            data_points: vec![JocDataPoint {
                offset_timeslot: None,
                payload: JocPayloadData::Full {
                    matrix_symbols: (0..CHANNELS)
                        .map(|channel| {
                            (0..23)
                                .map(|band| HuffmanCodeword {
                                    bits: Vec::new(),
                                    symbol: ((object * 17 + channel * 11 + band) % 192) as u16,
                                })
                                .collect()
                        })
                        .collect(),
                },
            }],
        })
        .collect();
    JocFrame {
        header: JocHeader {
            downmix_index: 0,
            channel_count: CHANNELS as u8,
            object_count_bits: 0,
            object_count: OBJECTS as u8,
            extension_index: 0,
        },
        clip_gain_x_bits: 0,
        clip_gain_y_bits: 0,
        sequence_count: 1,
        objects,
    }
}

fn qmf_inputs() -> Vec<Vec<[Complex64; SUBBANDS]>> {
    (0..CHANNELS)
        .map(|channel| {
            (0..TIMESLOTS)
                .map(|timeslot| {
                    let mut block = [Complex64::ZERO; SUBBANDS];
                    for (subband, value) in block.iter_mut().enumerate() {
                        let phase = (channel * 13 + timeslot * 7 + subband) as f64;
                        *value = Complex64::new(phase.sin(), (phase * 0.37).cos());
                    }
                    block
                })
                .collect()
        })
        .collect()
}

fn pcm_inputs() -> Vec<Vec<f64>> {
    qmf_inputs()
        .into_iter()
        .map(|timeslots| {
            timeslots
                .into_iter()
                .flat_map(|block| block.into_iter().map(|value| value.re))
                .collect()
        })
        .collect()
}

fn percentile(values: &[Duration], percentile: usize) -> Duration {
    let index = ((values.len() - 1) * percentile + 50) / 100;
    values[index]
}

fn milliseconds(value: Duration) -> f64 {
    value.as_secs_f64() * 1_000.0
}

fn main() {
    let frames = env::args()
        .nth(1)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(16);
    let mode = env::args().nth(2).unwrap_or_else(|| "qmf".to_owned());
    let frame = frame();
    let inputs = qmf_inputs();
    let pcm = pcm_inputs();
    let mut state = JocDecoderState::new();
    state.enable_reconstruction_timing();
    let mut timings = Vec::with_capacity(frames);
    let mut stages = ReconstructionStageTiming::default();
    let mut checksum = 0.0;

    for index in 0..frames {
        let mut current = frame.clone();
        current.sequence_count = ((index % 1023) + 1) as u16;
        let start = Instant::now();
        let decoded = if mode == "pcm" {
            state
                .decode_pcm_frame(&current, &pcm)
                .expect("PCM reconstruction")
        } else {
            state
                .decode_frame(&current, &inputs)
                .expect("QMF reconstruction")
        };
        timings.push(start.elapsed());
        stages.add_assign(&state.take_reconstruction_timing());
        checksum += decoded.reconstruction_basis.rows[0][0];
        black_box(&decoded);
    }

    timings.sort_by_key(|value| *value);
    let total: Duration = timings.iter().sum();
    let average = total / u32::try_from(frames).expect("bounded frame count");
    println!("mode={mode}");
    println!("access_units={frames}");
    println!("wall_ms={:.3}", milliseconds(total));
    println!("ms_per_au={:.3}", milliseconds(average));
    println!("p50_ms={:.3}", milliseconds(percentile(&timings, 50)));
    println!("p95_ms={:.3}", milliseconds(percentile(&timings, 95)));
    println!("p99_ms={:.3}", milliseconds(percentile(&timings, 99)));
    println!(
        "max_ms={:.3}",
        milliseconds(*timings.last().expect("timing"))
    );
    println!("stage_ms={stages:?}");
    println!("checksum={checksum:.17e}");
}
