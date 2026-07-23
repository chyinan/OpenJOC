use openjoc_qmf::{QMF_BANDS, ReferenceQmf64F64};

fn process(signal: &[f64]) -> Vec<f64> {
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

fn measured_delay_and_gain() -> (usize, f64) {
    let mut impulse = vec![0.0; 64 * 40];
    impulse[0] = 1.0;
    let output = process(&impulse);
    output
        .iter()
        .copied()
        .enumerate()
        .max_by(|(_, left), (_, right)| left.abs().total_cmp(&right.abs()))
        .expect("impulse output")
}

#[derive(Debug)]
struct ReconstructionMetrics {
    delay: usize,
    gain: f64,
    maximum_error: f64,
    rms_error: f64,
}

fn measure_reconstruction(mut signal: Vec<f64>) -> ReconstructionMetrics {
    let original_len = signal.len();
    signal.extend(std::iter::repeat_n(0.0, 64 * 20));
    let output = process(&signal);
    let (delay, _) = measured_delay_and_gain();
    let compared = original_len.min(output.len() - delay);
    let margin = 64 * 20;
    let range = margin..compared - margin;
    let gain = range
        .clone()
        .map(|index| output[index + delay] * signal[index])
        .sum::<f64>()
        / signal[range.clone()]
            .iter()
            .map(|sample| sample * sample)
            .sum::<f64>();
    assert!(gain.abs() > 1.0e-12, "invalid measured gain {gain}");
    let mut squared = 0.0;
    let mut maximum = 0.0_f64;
    for index in range.clone() {
        let error = output[index + delay] / gain - signal[index];
        maximum = maximum.max(error.abs());
        squared += error * error;
    }
    let sample_count = u32::try_from(range.len()).expect("bounded test signal length");
    let rms = (squared / f64::from(sample_count)).sqrt();
    ReconstructionMetrics {
        delay,
        gain,
        maximum_error: maximum,
        rms_error: rms,
    }
}

fn assert_metrics(actual: &ReconstructionMetrics, expected: [f64; 3], label: &str) {
    assert_eq!(actual.delay, 514, "{label}: {actual:?}");
    for (actual, expected) in [actual.gain, actual.maximum_error, actual.rms_error]
        .into_iter()
        .zip(expected)
    {
        assert!(
            (actual - expected).abs() < 1.0e-9,
            "{label}: expected {expected}, got {actual} in {actual:?}"
        );
    }
}

#[test]
fn reset_clears_analysis_and_synthesis_state() {
    let mut qmf = ReferenceQmf64F64::new();
    let mut impulse = [0.0; QMF_BANDS];
    impulse[0] = 1.0;
    let subbands = qmf.analyze(&impulse);
    let _ = qmf.synthesize(&subbands);
    qmf.reset();

    let zero = [0.0; QMF_BANDS];
    assert!(qmf.analyze(&zero).iter().all(|sample| sample.norm() == 0.0));
    let subbands = qmf.analyze(&zero);
    assert!(
        qmf.synthesize(&subbands)
            .iter()
            .all(|sample| *sample == 0.0)
    );
}

#[test]
fn analysis_synthesis_reconstructs_required_signal_suite() {
    let length = 64 * 80;
    assert_metrics(
        &measure_reconstruction(vec![1.0; length]),
        [
            0.024_545_259_564_847_457,
            0.005_761_979_114_730_531,
            0.001_050_469_777_645_912_8,
        ],
        "DC",
    );
    for (frequency, expected) in [
        (
            1_000.0,
            [
                0.843_001_219_829_438_8,
                0.565_237_535_104_961_2,
                0.399_013_005_850_974_8,
            ],
        ),
        (
            5_900.0,
            [
                0.684_604_683_082_347_8,
                0.876_282_578_026_095_7,
                0.618_816_360_749_615_9,
            ],
        ),
        (
            6_100.0,
            [
                0.684_253_420_317_710_9,
                0.876_732_419_315_586,
                0.619_028_398_305_945,
            ],
        ),
        (
            11_900.0,
            [
                0.684_426_641_031_943_7,
                0.876_510_528_163_516_2,
                0.618_923_858_891_203_4,
            ],
        ),
        (
            12_100.0,
            [
                0.684_426_641_031_956_2,
                0.876_510_528_163_540_5,
                0.618_923_858_891_184_6,
            ],
        ),
    ] {
        let signal = (0_u32..)
            .take(length)
            .map(|index| {
                (2.0 * std::f64::consts::PI * frequency * f64::from(index) / 48_000.0).sin()
            })
            .collect();
        assert_metrics(
            &measure_reconstruction(signal),
            expected,
            &format!("{frequency} Hz sine"),
        );
    }
    let mut state = 0x1234_5678_u64;
    let noise = (0..length)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            (f64::from((state >> 32) as u32) / f64::from(u32::MAX)) * 2.0 - 1.0
        })
        .collect();
    assert_metrics(
        &measure_reconstruction(noise),
        [
            0.573_756_034_231_724_8,
            1.157_622_611_525_434_7,
            0.564_744_870_581_192_7,
        ],
        "deterministic white noise",
    );
}
