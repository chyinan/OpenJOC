use openjoc_qmf::{QMF_BANDS, QMF_ROUNDTRIP_LATENCY_SAMPLES, ReferenceQmf64F64};

const SAMPLE_RATE_HZ: f64 = 48_000.0;
const CLEAN_QMF_DELAY: usize = QMF_ROUNDTRIP_LATENCY_SAMPLES;
const GUARD_BLOCKS: usize = 20;
const TOLERANCE: f64 = 5.0e-4;

// This is the clean clause 7 reference identity contract: analysis, identity
// subband mapping, and synthesis must be one positive near-unity delayed path.
// It supersedes the former regression values that preserved non-transparent
// coloration from the old synthesis phase convention.

fn process(signal: &[f64]) -> Vec<f64> {
    assert_eq!(
        signal.len() % QMF_BANDS,
        0,
        "input must contain full QMF blocks"
    );
    let mut qmf = ReferenceQmf64F64::new();
    signal
        .chunks_exact(QMF_BANDS)
        .flat_map(|block| {
            let input: &[f64; QMF_BANDS] = block.try_into().expect("exact QMF block");
            let subbands = qmf.analyze(input);
            qmf.synthesize(&subbands)
        })
        .collect()
}

fn process_partitioned(signal: &[f64], partition_blocks: &[usize]) -> Vec<f64> {
    assert!(!partition_blocks.is_empty());
    assert_eq!(
        signal.len() % QMF_BANDS,
        0,
        "input must contain full QMF blocks"
    );

    let mut qmf = ReferenceQmf64F64::new();
    let blocks = signal.chunks_exact(QMF_BANDS).collect::<Vec<_>>();
    let mut output = Vec::with_capacity(signal.len());
    let mut block_index = 0;
    let mut partition_index = 0;
    while block_index < blocks.len() {
        let partition_len = partition_blocks[partition_index % partition_blocks.len()]
            .min(blocks.len() - block_index);
        for block in &blocks[block_index..block_index + partition_len] {
            let input: &[f64; QMF_BANDS] = (*block).try_into().expect("exact QMF block");
            let subbands = qmf.analyze(input);
            output.extend_from_slice(&qmf.synthesize(&subbands));
        }
        block_index += partition_len;
        partition_index += 1;
    }
    output
}

fn sine(frequency_hz: f64, length: usize) -> Vec<f64> {
    (0..length)
        .map(|index| {
            (2.0 * std::f64::consts::PI * frequency_hz * index as f64 / SAMPLE_RATE_HZ).sin()
        })
        .collect()
}

fn deterministic_uniform_noise(length: usize) -> Vec<f64> {
    let mut state = 0x004f_5045_4e4a_4f43_u64;
    (0..length)
        .map(|_| {
            state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut value = state;
            value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            value ^= value >> 31;
            (value as f64 / u64::MAX as f64) * 2.0 - 1.0
        })
        .collect()
}

fn deterministic_random_pcm(length: usize) -> Vec<f64> {
    let mut state = 0x004f_5045_4e4a_4f43_u64;
    (0..length)
        .map(|_| {
            state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut value = state;
            value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            value ^= value >> 31;
            let unsigned = ((value >> 40) & 0x00ff_ffff) as i32;
            f64::from(unsigned - 0x0080_0000) / 8_388_608.0
        })
        .collect()
}

fn multi_tone(length: usize) -> Vec<f64> {
    [80.0, 1_000.0, 5_900.0, 12_100.0, 20_000.0]
        .into_iter()
        .map(|frequency| sine(frequency, length))
        .fold(vec![0.0; length], |mut sum, tone| {
            for (sample, contribution) in sum.iter_mut().zip(tone) {
                *sample += contribution;
            }
            sum
        })
}

#[derive(Debug)]
struct ReconstructionMetrics {
    gain: f64,
    maximum_error: f64,
    rms_error: f64,
}

fn measure_reconstruction(signal: &[f64]) -> ReconstructionMetrics {
    assert!(signal.len() > 2 * GUARD_BLOCKS * QMF_BANDS);
    let mut input = signal.to_vec();
    input.extend(std::iter::repeat_n(0.0, 24 * QMF_BANDS));
    let output = process(&input);
    let start = GUARD_BLOCKS * QMF_BANDS;
    let end = signal.len() - GUARD_BLOCKS * QMF_BANDS;
    let energy = (start..end)
        .map(|index| signal[index] * signal[index])
        .sum::<f64>();
    let gain = (start..end)
        .map(|index| output[index + CLEAN_QMF_DELAY] * signal[index])
        .sum::<f64>()
        / energy;
    let mut maximum_error = 0.0_f64;
    let mut squared_error = 0.0_f64;
    for index in start..end {
        let error = output[index + CLEAN_QMF_DELAY] / gain - signal[index];
        maximum_error = maximum_error.max(error.abs());
        squared_error += error * error;
    }
    let rms_error = (squared_error / (end - start) as f64).sqrt();
    assert!(gain.is_finite() && maximum_error.is_finite() && rms_error.is_finite());
    ReconstructionMetrics {
        gain,
        maximum_error,
        rms_error,
    }
}

fn assert_clean_metrics(signal: &[f64], label: &str) -> ReconstructionMetrics {
    let metrics = measure_reconstruction(signal);
    let peak = signal.iter().map(|sample| sample.abs()).fold(0.0, f64::max);
    let scale = peak.max(1.0);
    assert!(
        (metrics.gain - 1.0).abs() <= TOLERANCE,
        "{label}: gain {metrics:?}"
    );
    assert!(
        metrics.maximum_error <= TOLERANCE * scale,
        "{label}: maximum error {metrics:?}"
    );
    assert!(
        metrics.rms_error <= 1.0e-4 * scale,
        "{label}: RMS error {metrics:?}"
    );
    eprintln!("{label}: scale={scale}, {metrics:?}");
    metrics
}

fn impulse_result(position: usize) -> (usize, f64, f64) {
    let length = ((position + 24 * QMF_BANDS).div_ceil(QMF_BANDS)) * QMF_BANDS;
    let mut input = vec![0.0; length];
    input[position] = 1.0;
    let output = process(&input);
    let (peak_index, peak) = output
        .iter()
        .copied()
        .enumerate()
        .max_by(|(_, left), (_, right)| left.abs().total_cmp(&right.abs()))
        .expect("impulse output");
    let off_peak = output
        .iter()
        .copied()
        .enumerate()
        .filter(|(index, _)| *index != peak_index)
        .map(|(_, sample)| sample.abs())
        .fold(0.0, f64::max);
    (peak_index, peak, off_peak)
}

#[test]
fn qmf_identity_has_fixed_577_sample_latency_across_impulse_positions() {
    for position in [0, 1, 7, 31, 32, 63, 64, 65, 127, 128, 257, 511] {
        let (peak_index, peak, off_peak) = impulse_result(position);
        eprintln!(
            "impulse position {position}: delay={}, peak_gain={peak}, off_peak={off_peak}",
            peak_index - position
        );
        assert_eq!(
            peak_index,
            position + CLEAN_QMF_DELAY,
            "impulse position {position}"
        );
        assert!(peak > 0.0, "impulse position {position}: {peak}");
        assert!(
            (peak - 1.0).abs() <= TOLERANCE,
            "impulse position {position}: {peak}"
        );
        assert!(
            off_peak <= 2.5e-4,
            "impulse position {position}: off-peak residual {off_peak}"
        );
    }
}

#[test]
fn qmf_identity_signal_suite_is_near_unity_and_frequency_independent() {
    let length = 128 * QMF_BANDS;
    for (frequency, label) in [
        (80.0, "80 Hz sine"),
        (1_000.0, "1 kHz sine"),
        (20_000.0, "20 kHz sine"),
    ] {
        assert_clean_metrics(&sine(frequency, length), label);
    }
    assert_clean_metrics(&multi_tone(length), "multi-tone");
    assert_clean_metrics(
        &deterministic_uniform_noise(length),
        "deterministic white noise",
    );
    assert_clean_metrics(
        &deterministic_random_pcm(length),
        "deterministic random PCM",
    );
}

#[test]
fn qmf_identity_frequency_gain_is_flat_over_supported_tone_set() {
    let length = 128 * QMF_BANDS;
    let frequencies = [
        80.0, 250.0, 1_000.0, 3_000.0, 5_900.0, 6_100.0, 11_900.0, 12_100.0, 18_000.0, 20_000.0,
        22_000.0,
    ];
    let gains = frequencies
        .into_iter()
        .map(|frequency| {
            let metrics = measure_reconstruction(&sine(frequency, length));
            eprintln!("{frequency} Hz: {metrics:?}");
            assert!((metrics.gain - 1.0).abs() <= TOLERANCE);
            metrics.gain
        })
        .collect::<Vec<_>>();
    let minimum = gains.iter().copied().fold(f64::INFINITY, f64::min);
    let maximum = gains.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    eprintln!("frequency gain spread: {}", maximum - minimum);
    assert!(
        maximum - minimum <= TOLERANCE,
        "gain spread {maximum} - {minimum}"
    );
}

#[test]
fn qmf_processing_is_partition_invariant() {
    let mut input = deterministic_random_pcm(160 * QMF_BANDS);
    input.extend(std::iter::repeat_n(0.0, 24 * QMF_BANDS));
    let continuous = process(&input);
    let partitioned = process_partitioned(&input, &[1, 2, 3, 5, 8, 13]);
    assert_eq!(continuous.len(), partitioned.len());
    let maximum_difference = continuous
        .iter()
        .zip(&partitioned)
        .map(|(left, right)| (left - right).abs())
        .fold(0.0, f64::max);
    assert!(
        maximum_difference <= 1.0e-12,
        "maximum difference {maximum_difference}"
    );

    let (continuous_peak, _, _) = impulse_result(257);
    let mut impulse = vec![0.0; (257 + 24 * QMF_BANDS).div_ceil(QMF_BANDS) * QMF_BANDS];
    impulse[257] = 1.0;
    let partitioned_impulse = process_partitioned(&impulse, &[1, 2, 3, 5, 8, 13]);
    let (partitioned_peak, _) = partitioned_impulse
        .iter()
        .copied()
        .enumerate()
        .max_by(|(_, left), (_, right)| left.abs().total_cmp(&right.abs()))
        .expect("partitioned impulse output");
    assert_eq!(continuous_peak, partitioned_peak);
}

#[test]
fn qmf_reset_is_zero_and_matches_a_fresh_instance() {
    let signal = deterministic_random_pcm(96 * QMF_BANDS);
    let mut qmf = ReferenceQmf64F64::new();
    for block in signal.chunks_exact(QMF_BANDS).take(8) {
        let input: &[f64; QMF_BANDS] = block.try_into().expect("exact QMF block");
        let subbands = qmf.analyze(input);
        let _ = qmf.synthesize(&subbands);
    }
    qmf.reset();

    let zero = [0.0; QMF_BANDS];
    let zero_subbands = qmf.analyze(&zero);
    assert!(
        zero_subbands
            .iter()
            .all(|sample| *sample == num_complex::Complex64::ZERO)
    );
    assert!(
        qmf.synthesize(&zero_subbands)
            .iter()
            .all(|sample| *sample == 0.0)
    );

    qmf.reset();
    let mut reset_output = Vec::with_capacity(signal.len());
    for block in signal.chunks_exact(QMF_BANDS) {
        let input: &[f64; QMF_BANDS] = block.try_into().expect("exact QMF block");
        let subbands = qmf.analyze(input);
        reset_output.extend_from_slice(&qmf.synthesize(&subbands));
    }
    let fresh_output = process(&signal);
    let maximum_difference = reset_output
        .iter()
        .zip(&fresh_output)
        .map(|(left, right)| (left - right).abs())
        .fold(0.0, f64::max);
    assert!(
        maximum_difference <= 1.0e-12,
        "maximum difference {maximum_difference}"
    );
}

#[test]
fn qmf_identity_preserves_sign_and_linear_superposition() {
    let input = multi_tone(128 * QMF_BANDS);
    let negative = input.iter().map(|sample| -sample).collect::<Vec<_>>();
    let half = input.iter().map(|sample| 0.5 * sample).collect::<Vec<_>>();
    let original = process(&input);
    let negative_output = process(&negative);
    let half_output = process(&half);
    let maximum_sign_error = original
        .iter()
        .zip(&negative_output)
        .map(|(left, right)| (left + right).abs())
        .fold(0.0, f64::max);
    let maximum_scale_error = original
        .iter()
        .zip(&half_output)
        .map(|(left, right)| (0.5 * left - right).abs())
        .fold(0.0, f64::max);
    assert!(
        maximum_sign_error <= 5.0e-12,
        "sign error {maximum_sign_error}"
    );
    assert!(
        maximum_scale_error <= 5.0e-12,
        "scale error {maximum_scale_error}"
    );
}
