use openjoc_joc::{ReconstructionBasis, ReconstructionOutputTimeline};
use openjoc_qmf::{QMF_BANDS, QMF_ROUNDTRIP_LATENCY_SAMPLES, ReferenceQmf64F64};

const QMF_LATENCY: usize = QMF_ROUNDTRIP_LATENCY_SAMPLES;

fn identity_qmf(signal: &[f64]) -> Vec<f64> {
    assert_eq!(signal.len() % QMF_BANDS, 0);
    let mut qmf = ReferenceQmf64F64::new();
    signal
        .chunks_exact(QMF_BANDS)
        .flat_map(|chunk| {
            let input: &[f64; QMF_BANDS] = chunk.try_into().expect("QMF block");
            let subbands = qmf.analyze(input);
            qmf.synthesize(&subbands)
        })
        .collect()
}

fn identity_qmf_with_tail(signal: &[f64]) -> (Vec<f64>, Vec<f64>) {
    assert_eq!(signal.len() % QMF_BANDS, 0);
    let mut qmf = ReferenceQmf64F64::new();
    let mut output = Vec::with_capacity(signal.len());
    for chunk in signal.chunks_exact(QMF_BANDS) {
        let input: &[f64; QMF_BANDS] = chunk.try_into().expect("QMF block");
        let subbands = qmf.analyze(input);
        output.extend_from_slice(&qmf.synthesize(&subbands));
    }
    let mut tail = Vec::with_capacity(10 * QMF_BANDS);
    let zero_pcm = [0.0; QMF_BANDS];
    for _ in 0..10 {
        let subbands = qmf.analyze(&zero_pcm);
        tail.extend_from_slice(&qmf.synthesize(&subbands));
    }
    tail.truncate(QMF_LATENCY);
    (output, tail)
}

fn cross_correlation_lag(base: &[f64], reconstruction: &[f64], max_lag: usize) -> isize {
    let mut best_lag = 0_isize;
    let mut best_score = f64::NEG_INFINITY;
    let max_lag = isize::try_from(max_lag).expect("test lag fits isize");
    for lag in -max_lag..=max_lag {
        let (base_start, reconstruction_start, length) = if lag >= 0 {
            let lag = usize::try_from(lag).expect("nonnegative lag");
            (lag, 0, base.len().min(reconstruction.len() - lag))
        } else {
            let lag = usize::try_from(-lag).expect("negative lag magnitude");
            (0, lag, reconstruction.len().min(base.len() - lag))
        };
        let score = (0..length)
            .map(|index| base[base_start + index] * reconstruction[reconstruction_start + index])
            .sum::<f64>();
        if score > best_score {
            best_score = score;
            best_lag = lag;
        }
    }
    best_lag
}

#[test]
fn pre_r2_raw_base_and_reconstruction_handoff_is_not_zero_lag() {
    let mut base = vec![0.0; 64 * 32];
    base[64 * 8 + 17] = 1.0;
    let reconstruction = identity_qmf(&base);
    let lag = cross_correlation_lag(&base, &reconstruction, 640);

    assert_eq!(
        lag,
        -isize::try_from(QMF_LATENCY).expect("QMF latency fits isize")
    );
}

fn aligned_output(source: &[f64], partition: usize) -> Vec<f64> {
    let (raw_reconstruction, tail) = identity_qmf_with_tail(source);
    let mut timeline = ReconstructionOutputTimeline::new();
    let mut output = Vec::new();
    for (frame_index, (base, reconstruction)) in source
        .chunks(partition)
        .zip(raw_reconstruction.chunks(partition))
        .enumerate()
    {
        let start = frame_index * partition;
        let end = start + base.len();
        let basis = ReconstructionBasis {
            rows: vec![reconstruction.to_vec()],
        };
        output.extend(
            timeline
                .push_frame(
                    frame_index as u64,
                    48_000,
                    start as u64,
                    end as u64,
                    &[base.to_vec()],
                    &basis,
                    None,
                    false,
                )
                .expect("aligned frame"),
        );
    }
    output.extend(
        timeline
            .finish(&ReconstructionBasis { rows: vec![tail] })
            .expect("aligned tail"),
    );
    assert_eq!(
        output
            .iter()
            .map(|frame| frame.base_full_band_pcm[0].len())
            .sum::<usize>(),
        source.len()
    );
    output
        .into_iter()
        .flat_map(|frame| {
            frame
                .reconstruction_basis
                .rows
                .into_iter()
                .next()
                .unwrap_or_default()
        })
        .collect()
}

#[test]
fn r2_timeline_aligns_correlated_source_and_preserves_tail() {
    let mut source = vec![0.0; 64 * 128];
    for position in [0, 1, 63, 64, 575, 576, 577, 578, 1535, 1536, 1537] {
        source[position] = 1.0;
    }
    for (index, sample) in source.iter_mut().enumerate() {
        *sample += ((index as f64 * 0.17).sin() * 0.1) + ((index as f64 * 0.031).cos() * 0.05);
    }
    let aligned = aligned_output(&source, 512);
    assert_eq!(aligned.len(), source.len());
    let lag = cross_correlation_lag(&source, &aligned, 8);
    assert_eq!(lag, 0);
    let (_maximum_error_index, maximum_error) = source
        .iter()
        .zip(&aligned)
        .enumerate()
        .map(|(index, (expected, actual))| (index, (expected - actual).abs()))
        .max_by(|(_, left), (_, right)| left.total_cmp(right))
        .expect("aligned samples");
    assert!(
        maximum_error <= 5.0e-4,
        "maximum aligned error {maximum_error}"
    );
}

#[test]
fn r2_timeline_is_invariant_to_valid_partitions() {
    let source = (0..64 * 128)
        .map(|index| (index as f64 * 0.013).sin() + (index as f64 * 0.071).cos())
        .collect::<Vec<_>>();
    let continuous = aligned_output(&source, 1536);
    let partitioned = aligned_output(&source, 64);
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
}

#[test]
fn r2_timeline_reset_discards_stale_tail_and_preserves_lfe_ranges() {
    let mut timeline = ReconstructionOutputTimeline::new();
    let base = vec![vec![1.0; 640]];
    let reconstruction = ReconstructionBasis {
        rows: vec![vec![2.0; 640]],
    };
    let lfe = vec![3.0; 640];

    assert!(
        timeline
            .push_frame(0, 48_000, 0, 640, &base, &reconstruction, Some(&lfe), false,)
            .expect("first frame")
            .is_empty()
    );
    let before_reset = timeline
        .push_frame(
            1,
            48_000,
            640,
            1280,
            &base,
            &reconstruction,
            Some(&lfe),
            false,
        )
        .expect("second frame");
    assert_eq!(before_reset.len(), 1);
    assert_eq!(before_reset[0].timeline.logical_start_sample, 0);
    let old_reset_epoch = before_reset[0].timeline.reset_epoch;
    let old_topology_epoch = before_reset[0].timeline.topology_epoch;

    let after_reset = timeline
        .push_frame(
            2,
            48_000,
            1280,
            1920,
            &base,
            &reconstruction,
            Some(&lfe),
            true,
        )
        .expect("discontinuity frame");
    assert!(after_reset.is_empty());
    let after_reset = timeline
        .push_frame(
            3,
            48_000,
            1920,
            2560,
            &base,
            &reconstruction,
            Some(&lfe),
            false,
        )
        .expect("post-reset frame");
    assert_eq!(after_reset.len(), 1);
    assert!(after_reset[0].timeline.reset_epoch > old_reset_epoch);
    assert!(after_reset[0].timeline.topology_epoch > old_topology_epoch);
    assert_eq!(after_reset[0].timeline.logical_start_sample, 1280);
    assert_eq!(after_reset[0].reconstruction_basis.rows[0], vec![2.0; 640]);
    assert_eq!(after_reset[0].lfe_pcm.as_ref().expect("LFE").len(), 640);

    let tail = timeline
        .finish(&ReconstructionBasis {
            rows: vec![vec![0.0; QMF_LATENCY]],
        })
        .expect("tail");
    assert_eq!(tail.len(), 1);
    assert!(tail[0].timeline.tail_flush_valid);
    assert_eq!(tail[0].timeline.logical_start_sample, 1920);
    assert_eq!(tail[0].lfe_pcm.as_ref().expect("LFE").len(), 640);
}

#[test]
fn r2_timeline_preserves_two_row_ordinals_across_frames_and_tail() {
    let frame_samples = 640;
    let programme_samples = frame_samples * 2;
    let raw_rows = [
        (0..programme_samples)
            .map(|sample| sample as f64)
            .collect::<Vec<_>>(),
        (0..programme_samples)
            .map(|sample| 10_000.0 + sample as f64 * 2.0)
            .collect::<Vec<_>>(),
    ];
    let tail_rows = [
        (0..QMF_LATENCY)
            .map(|sample| 100_000.0 + sample as f64)
            .collect::<Vec<_>>(),
        (0..QMF_LATENCY)
            .map(|sample| 200_000.0 + sample as f64 * 2.0)
            .collect::<Vec<_>>(),
    ];
    let mut timeline = ReconstructionOutputTimeline::new();
    let mut aligned = Vec::new();

    for frame_index in 0..2 {
        let start = frame_index * frame_samples;
        let end = start + frame_samples;
        aligned.extend(
            timeline
                .push_frame(
                    frame_index as u64,
                    48_000,
                    start as u64,
                    end as u64,
                    &[vec![0.0; frame_samples]],
                    &ReconstructionBasis {
                        rows: raw_rows
                            .iter()
                            .map(|row| row[start..end].to_vec())
                            .collect(),
                    },
                    None,
                    false,
                )
                .expect("aligned frame"),
        );
    }
    aligned.extend(
        timeline
            .finish(&ReconstructionBasis {
                rows: tail_rows.to_vec(),
            })
            .expect("aligned tail"),
    );

    assert_eq!(aligned.len(), 2);
    assert!(
        aligned
            .iter()
            .all(|frame| frame.reconstruction_basis.rows.len() == 2)
    );
    for row_index in 0..2 {
        let actual = aligned
            .iter()
            .flat_map(|frame| frame.reconstruction_basis.rows[row_index].iter().copied())
            .collect::<Vec<_>>();
        let expected = raw_rows[row_index][QMF_LATENCY..]
            .iter()
            .chain(&tail_rows[row_index])
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "reconstruction row {row_index}");
    }
}
