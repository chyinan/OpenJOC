use super::*;
use openjoc_render::{DynamicBinauralRenderer, DynamicBinauralSource};
use openjoc_scene::SpeakerGeometry;

fn binaural_probe_frame(channels: usize, count: usize, start: u64) -> RenderedBlock {
    RenderedBlock {
        sample_rate: 48_000,
        logical_start_sample: start,
        sample_count: count,
        channels: (0..channels)
            .map(|channel| {
                (0..count)
                    .map(|sample| {
                        ((sample as u64 + start + channel as u64 * 17) % 127) as f64 / 127.0 - 0.5
                    })
                    .collect()
            })
            .collect(),
    }
}

fn listener_orientation_session_at_epoch_max() -> OpenJocSession {
    let config = OpenJocConfig {
        render_mode: RenderMode::Binaural,
        speaker_layout: "7.1.4".to_owned(),
        binaural: Some(BinauralConfig::builtin_generic("7.1.4")),
        ..OpenJocConfig::default()
    };
    let mut session = OpenJocSession::new_with_listener_orientation(config).unwrap();
    let state = session.binaural.as_mut().unwrap();
    let preparer = state.orientation_preparer.clone().unwrap();
    let initial = preparer
        .prepare(ListenerOrientation::IDENTITY, u64::MAX, 0)
        .unwrap();
    let sources = initial
        .kernels()
        .iter()
        .map(|kernel| DynamicBinauralSource::new(kernel.source_id(), 1.0).unwrap())
        .collect();
    state.dynamic_engine = Some(
        DynamicBinauralRenderer::new(
            48_000,
            initial,
            sources,
            preparer.max_filter_taps(),
            MAX_DYNAMIC_BINAURAL_BLOCK_SAMPLES,
            DEFAULT_DYNAMIC_BINAURAL_TRANSITION_SAMPLES,
        )
        .unwrap(),
    );
    session
}

#[test]
fn binaural_block_adapter_preserves_exact_direct_pcm_tail_and_reset() {
    for hrtf in [BuiltinHrtf::SadieD1Ku100, BuiltinHrtf::SadieD2Kemar] {
        for layout in ["7.1.4", "9.1.6", "22.2"] {
            for lfe_policy in [
                BinauralLfePolicy::Exclude,
                BinauralLfePolicy::EqualPowerDualMono,
            ] {
                let mut config = BinauralConfig::builtin(hrtf, layout);
                config.lfe_policy = lfe_policy;
                let mut state = BinauralState::new(&config, None, None).unwrap();
                let mut reference = state.ensure_engine(48_000).unwrap().clone();
                let channels = SpeakerLayoutPreset::for_name(layout).unwrap().labels.len();
                for _ in 0..2 {
                    let mut start = 0;
                    for count in [1, 97, 256, 1536, 17] {
                        let frame = binaural_probe_frame(channels, count, start);
                        let blocks: Vec<_> = state
                            .mappings
                            .iter()
                            .map(|mapping| {
                                BinauralSourceBlock::new(
                                    mapping.source_id,
                                    &frame.channels[mapping.channel_index],
                                )
                            })
                            .collect();
                        let mut left = vec![0.0; count];
                        let mut right = vec![0.0; count];
                        reference
                            .render_block(&blocks, &mut left, &mut right)
                            .unwrap();
                        if lfe_policy == BinauralLfePolicy::EqualPowerDualMono {
                            for i in 0..count {
                                let lfe = frame.channels[state.lfe_index.unwrap()][i]
                                    * std::f64::consts::FRAC_1_SQRT_2;
                                left[i] += lfe;
                                right[i] += lfe;
                            }
                        }
                        let actual = state.render(&frame).unwrap();
                        for (actual, expected) in actual
                            .channels
                            .iter()
                            .flatten()
                            .zip(left.iter().chain(&right))
                        {
                            assert_eq!(actual.to_bits(), expected.to_bits());
                        }
                        assert_eq!(actual.logical_start_sample, start);
                        assert_eq!(actual.sample_count, count);
                        start += count as u64;
                    }
                    let count = reference.remaining_tail_samples();
                    let mut left = vec![0.0; count];
                    let mut right = vec![0.0; count];
                    reference.drain_tail_block(&mut left, &mut right).unwrap();
                    let tail = state.drain_tail(48_000, start).unwrap();
                    for (channel, expected) in [left, right].iter().enumerate() {
                        let actual: Vec<_> = tail
                            .iter()
                            .flat_map(|block| block.channels[channel].iter().copied())
                            .collect();
                        assert_eq!(actual.len(), expected.len());
                        for (actual, expected) in actual.iter().zip(expected) {
                            assert_eq!(actual.to_bits(), expected.to_bits());
                        }
                    }
                    state.reset().unwrap();
                    reference.reset();
                }
            }
        }
    }
}

#[test]
fn dynamic_binaural_range_render_matches_static_across_pull_partitions() {
    for hrtf in [BuiltinHrtf::SadieD1Ku100, BuiltinHrtf::SadieD2Kemar] {
        for layout in ["7.1.4", "9.1.6", "22.2"] {
            for lfe_policy in [
                BinauralLfePolicy::Exclude,
                BinauralLfePolicy::EqualPowerDualMono,
            ] {
                let mut config = BinauralConfig::builtin(hrtf, layout);
                config.lfe_policy = lfe_policy;
                let mut static_state = BinauralState::new(&config, None, None).unwrap();
                let mut dynamic_state =
                    BinauralState::new_with_listener_orientation(&config, None, None).unwrap();
                let channels = SpeakerLayoutPreset::for_name(layout).unwrap().labels.len();
                let input = binaural_probe_frame(channels, 1536, 20_000);
                let reference = static_state.render(&input).unwrap();

                let mut actual_left = Vec::new();
                let mut actual_right = Vec::new();
                let partitions = [1, 17, 97, 128, 256];
                let mut offset = 0;
                let mut partition_index = 0;
                while offset < input.sample_count {
                    let count = partitions[partition_index % partitions.len()]
                        .min(input.sample_count - offset);
                    let actual = dynamic_state.render_range(&input, offset, count).unwrap();
                    assert_eq!(
                        actual.logical_start_sample,
                        input.logical_start_sample + offset as u64
                    );
                    actual_left.extend_from_slice(&actual.channels[0]);
                    actual_right.extend_from_slice(&actual.channels[1]);
                    offset += count;
                    partition_index += 1;
                }
                for (actual, expected) in actual_left.iter().zip(&reference.channels[0]) {
                    assert_eq!(actual.to_bits(), expected.to_bits());
                }
                for (actual, expected) in actual_right.iter().zip(&reference.channels[1]) {
                    assert_eq!(actual.to_bits(), expected.to_bits());
                }

                let static_tail = static_state.drain_tail(48_000, 21_536).unwrap();
                let mut dynamic_tail = Vec::new();
                let mut tail_start = 21_536;
                while let Some(block) = dynamic_state.drain_tail_chunk(128, tail_start).unwrap() {
                    tail_start += block.sample_count as u64;
                    dynamic_tail.push(block);
                }
                for channel in 0..2 {
                    let expected: Vec<_> = static_tail
                        .iter()
                        .flat_map(|block| block.channels[channel].iter().copied())
                        .collect();
                    let actual: Vec<_> = dynamic_tail
                        .iter()
                        .flat_map(|block| block.channels[channel].iter().copied())
                        .collect();
                    assert_eq!(actual.len(), expected.len());
                    for (actual, expected) in actual.iter().zip(expected) {
                        assert_eq!(actual.to_bits(), expected.to_bits());
                    }
                }
            }
        }
    }
}

#[test]
fn listener_orientation_is_explicit_opt_in_and_fingerprints_session_mode() {
    let config = OpenJocConfig {
        render_mode: RenderMode::Binaural,
        speaker_layout: "7.1.4".to_owned(),
        binaural: Some(BinauralConfig::builtin_generic("7.1.4")),
        ..OpenJocConfig::default()
    };
    let static_session = OpenJocSession::new(config.clone()).unwrap();
    assert!(static_session.listener_orientation_preparer().is_none());
    assert_eq!(
        static_session.effective_config_descriptor(),
        config.effective_config_descriptor()
    );
    let enabled = OpenJocSession::new_with_listener_orientation(config.clone()).unwrap();
    assert!(enabled.listener_orientation_preparer().is_some());
    assert_eq!(enabled.listener_orientation_stream_epoch(), Some(0));
    assert!(
        enabled
            .effective_config_descriptor()
            .contains("listener_orientation=enabled")
    );
    assert_ne!(
        enabled.effective_config_fingerprint(),
        static_session.effective_config_fingerprint()
    );

    assert!(matches!(
        OpenJocSession::new_with_listener_orientation_pull(config.clone(), 0),
        Err(OpenJocError::InvalidConfig(_))
    ));
    assert!(matches!(
        OpenJocSession::new_with_listener_orientation_pull(
            config.clone(),
            MAX_LISTENER_ORIENTATION_PULL_SAMPLES + 1
        ),
        Err(OpenJocError::InvalidConfig(_))
    ));
    let mut pull = OpenJocSession::new_with_listener_orientation_pull(config.clone(), 128).unwrap();
    assert!(
        pull.effective_config_descriptor()
            .contains("listener_orientation_pull_max_samples=128")
    );
    assert!(pull.receive_frame().is_none());
    assert!(pull.receive_binaural_frame().unwrap().is_none());

    let preparer = enabled.listener_orientation_preparer().unwrap();
    let update = preparer
        .prepare(ListenerOrientation::IDENTITY, 0, 1)
        .unwrap();
    let mut enabled = enabled;
    let receipt = enabled.apply_prepared_listener_orientation(update).unwrap();
    assert_eq!(receipt.accepted_sequence, 1);
    assert_eq!(enabled.drain().unwrap(), OpenJocStatus::EndOfStream);
    let post_drain_update = preparer
        .prepare(ListenerOrientation::IDENTITY, 0, 2)
        .unwrap();
    let failure = enabled
        .apply_prepared_listener_orientation(post_drain_update)
        .unwrap_err();
    assert_eq!(
        failure.error,
        openjoc_render::RenderError::BinauralInputAfterTailStart
    );
    assert_eq!(failure.update.sequence(), 2);
    enabled.try_flush().unwrap();
    assert_eq!(enabled.listener_orientation_stream_epoch(), Some(1));
    assert_eq!(enabled.last_applied_listener_orientation(), None);
    assert!(matches!(
        OpenJocSession::new_with_listener_orientation(OpenJocConfig::default()),
        Err(OpenJocError::InvalidConfig(_))
    ));
}

#[test]
fn failed_orientation_reset_clears_queued_pcm_and_fails_closed() {
    let mut session = listener_orientation_session_at_epoch_max();
    session.output_queue.push_back(OpenJocPcmFrame {
        sample_format: PCM_SAMPLE_FORMAT,
        sample_rate: 48_000,
        channel_count: 2,
        channel_labels: vec!["Left Ear".to_owned(), "Right Ear".to_owned()],
        layout_name: "Binaural stereo".to_owned(),
        render_mode: RenderMode::Binaural,
        sample_count: 1,
        pts_samples: Some(0),
        interleaved_f32: vec![0.0, 0.0],
    });
    let error = session.try_flush().unwrap_err();
    assert!(error.to_string().contains("epoch overflowed"));
    assert!(session.receive_frame().is_none());
    assert!(session.is_drained());
    assert!(session.terminal_error.is_some());

    let preparer = session.listener_orientation_preparer().unwrap();
    let rejected = preparer
        .prepare(ListenerOrientation::IDENTITY, u64::MAX, 1)
        .unwrap();
    let failure = session
        .apply_prepared_listener_orientation(rejected)
        .unwrap_err();
    assert_eq!(
        failure.error,
        openjoc_render::RenderError::BinauralRequiresReset
    );
    assert_eq!(failure.update.sequence(), 1);
}

#[test]
fn pull_failure_clears_deferred_pcm_and_rejects_further_receives() {
    let config = OpenJocConfig {
        render_mode: RenderMode::Binaural,
        speaker_layout: "7.1.4".to_owned(),
        binaural: Some(BinauralConfig::builtin_generic("7.1.4")),
        ..OpenJocConfig::default()
    };
    let mut session = OpenJocSession::new_with_listener_orientation_pull(config, 128).unwrap();
    session
        .pending_binaural_inputs
        .push_back(binaural_probe_frame(12, 4, 0));
    session.pending_binaural_input_samples = 4;
    session.current_binaural_input = Some((binaural_probe_frame(12, 8, 4), 0));
    session.pending_binaural_input_samples += 8;

    let failure = OpenJocError::Render("injected terminal pull failure".to_owned());
    session.fail_closed(&failure);
    assert_eq!(session.pending_binaural_input_samples(), Some(0));
    assert!(session.pending_binaural_inputs.is_empty());
    assert!(session.current_binaural_input.is_none());
    assert!(session.is_drained());
    assert!(
        session
            .receive_binaural_frame()
            .unwrap_err()
            .to_string()
            .contains("injected terminal pull failure")
    );
    assert!(session.receive_frame().is_none());
}

#[test]
fn dynamic_session_identity_pcm_and_applied_receipts_match_static_timeline() {
    let mut config = BinauralConfig::builtin(BuiltinHrtf::SadieD1Ku100, "7.1.4");
    config.lfe_policy = BinauralLfePolicy::EqualPowerDualMono;
    let mut static_state = BinauralState::new(&config, None, None).unwrap();
    let mut dynamic_state =
        BinauralState::new_with_listener_orientation(&config, None, None).unwrap();
    let channels = SpeakerLayoutPreset::for_name("7.1.4").unwrap().labels.len();

    let initial = binaural_probe_frame(channels, 128, 100_000);
    let static_output = static_state.render(&initial).unwrap();
    let dynamic_output = dynamic_state.render(&initial).unwrap();
    for (dynamic, static_sample) in dynamic_output
        .channels
        .iter()
        .flatten()
        .zip(static_output.channels.iter().flatten())
    {
        assert_eq!(dynamic.to_bits(), static_sample.to_bits());
    }
    assert_eq!(
        dynamic_state.last_applied_listener_orientation(),
        Some(AppliedBinauralUpdate {
            sequence: 0,
            logical_start_sample: 100_000,
        })
    );

    let preparer = dynamic_state.listener_orientation_preparer().unwrap();
    let first_angle = 5.0_f64.to_radians();
    let first = preparer
        .prepare(
            ListenerOrientation::new(
                0.0,
                0.0,
                (first_angle * 0.5).sin(),
                (first_angle * 0.5).cos(),
            )
            .unwrap(),
            0,
            1,
        )
        .unwrap();
    let accepted = dynamic_state
        .apply_prepared_listener_orientation(first)
        .unwrap();
    drop(accepted.retired_kernels);
    let first_rotated = binaural_probe_frame(channels, 128, 100_128);
    let _ = dynamic_state.render(&first_rotated).unwrap();
    assert_eq!(
        dynamic_state.last_applied_listener_orientation(),
        Some(AppliedBinauralUpdate {
            sequence: 1,
            logical_start_sample: 100_128,
        })
    );

    let second_angle = 10.0_f64.to_radians();
    let second = preparer
        .prepare(
            ListenerOrientation::new(
                0.0,
                0.0,
                (second_angle * 0.5).sin(),
                (second_angle * 0.5).cos(),
            )
            .unwrap(),
            0,
            2,
        )
        .unwrap();
    let accepted = dynamic_state
        .apply_prepared_listener_orientation(second)
        .unwrap();
    drop(accepted.retired_kernels);
    let second_rotated = binaural_probe_frame(channels, 256, 100_256);
    let _ = dynamic_state.render(&second_rotated).unwrap();
    assert_eq!(
        dynamic_state.last_applied_listener_orientation(),
        Some(AppliedBinauralUpdate {
            sequence: 2,
            logical_start_sample: 100_368,
        })
    );

    dynamic_state.reset().unwrap();
    assert_eq!(dynamic_state.listener_orientation_stream_epoch(), Some(1));
    assert_eq!(dynamic_state.last_applied_listener_orientation(), None);
    let stale = preparer
        .prepare(ListenerOrientation::IDENTITY, 0, 3)
        .unwrap();
    let failure = dynamic_state
        .apply_prepared_listener_orientation(stale)
        .unwrap_err();
    assert!(matches!(
        failure.error,
        openjoc_render::RenderError::BinauralUpdateEpochMismatch {
            expected: 1,
            actual: 0,
        }
    ));
    assert_eq!(failure.update.sequence(), 3);
}

#[test]
fn binaural_state_reconstruction_tail_keeps_waiting_pose_pending() {
    let config = BinauralConfig::builtin(BuiltinHrtf::SadieD1Ku100, "7.1.4");
    let mut state = BinauralState::new_with_listener_orientation(&config, None, None).unwrap();
    let preparer = state.listener_orientation_preparer().unwrap();
    let target = preparer
        .prepare(
            ListenerOrientation::new(
                0.0,
                0.0,
                (2.0_f64.to_radians() * 0.5).sin(),
                (2.0_f64.to_radians() * 0.5).cos(),
            )
            .unwrap(),
            0,
            1,
        )
        .unwrap();
    let accepted = state.apply_prepared_listener_orientation(target).unwrap();
    drop(accepted.retired_kernels);
    assert_eq!(state.pending_listener_orientation_sequence(), Some(1));

    state.begin_drain().unwrap();
    let channels = SpeakerLayoutPreset::for_name("7.1.4").unwrap().labels.len();
    let reconstruction_tail = binaural_probe_frame(channels, 128, 8_192);
    let _ = state.render(&reconstruction_tail).unwrap();
    assert_eq!(state.pending_listener_orientation_sequence(), Some(1));
    assert_eq!(
        state.last_applied_listener_orientation(),
        Some(AppliedBinauralUpdate {
            sequence: 0,
            logical_start_sample: 8_192,
        })
    );
}

fn timing_percentile_ns(samples: &mut [u128], percentile: usize) -> u128 {
    samples.sort_unstable();
    let rank = samples.len().saturating_mul(percentile).div_ceil(100);
    samples[rank.saturating_sub(1).min(samples.len() - 1)]
}

fn print_timing_distribution(label: &str, samples: &mut [u128]) {
    let total_ns: u128 = samples.iter().sum();
    let average_ns = total_ns / samples.len() as u128;
    let p50_ns = timing_percentile_ns(samples, 50);
    let p95_ns = timing_percentile_ns(samples, 95);
    let p99_ns = timing_percentile_ns(samples, 99);
    let max_ns = *samples.last().unwrap_or(&0);
    eprintln!(
        "{label}: avg={:.3} us p50={:.3} us p95={:.3} us p99={:.3} us max={:.3} us",
        average_ns as f64 / 1_000.0,
        p50_ns as f64 / 1_000.0,
        p95_ns as f64 / 1_000.0,
        p99_ns as f64 / 1_000.0,
        max_ns as f64 / 1_000.0,
    );
}

fn print_setup_timing_summary(label: &str, samples: &mut [u128]) {
    samples.sort_unstable();
    let min_ns = samples.first().copied().unwrap_or(0);
    let p50_ns = timing_percentile_ns(samples, 50);
    let max_ns = samples.last().copied().unwrap_or(0);
    eprintln!(
        "{label}: n={} min={:.3} us p50={:.3} us max={:.3} us (descriptive setup sample only)",
        samples.len(),
        min_ns as f64 / 1_000.0,
        p50_ns as f64 / 1_000.0,
        max_ns as f64 / 1_000.0,
    );
}

#[test]
#[ignore = "manual release-mode timing probe; no wall-clock CI threshold"]
fn binaural_adapter_benchmark() {
    const PREP_RUNS: usize = 7;
    const MEASURED_BLOCKS: usize = 1_024;
    const INDEPENDENT_RUNS: usize = 3;
    const BLOCK_SIZE_OPTIONS: [usize; 2] = [128, 256];

    for hrtf in [BuiltinHrtf::SadieD1Ku100, BuiltinHrtf::SadieD2Kemar] {
        for layout in ["7.1.4", "9.1.6"] {
            let config = BinauralConfig::builtin(hrtf, layout);
            let channels = SpeakerLayoutPreset::for_name(layout).unwrap().labels.len();
            let mut prepare_ns = Vec::with_capacity(PREP_RUNS);
            let mut prepared_payload_bytes = 0;
            for _ in 0..PREP_RUNS {
                let start = std::time::Instant::now();
                let state = BinauralState::new(&config, None, None).unwrap();
                prepare_ns.push(start.elapsed().as_nanos());
                prepared_payload_bytes = state.bank.as_ref().map_or(0, |bank| {
                    bank.entries()
                        .iter()
                        .map(|entry| {
                            (entry.pair().left_taps().len() + entry.pair().right_taps().len())
                                * std::mem::size_of::<f64>()
                        })
                        .sum()
                });
            }
            print_setup_timing_summary(
                &format!("{hrtf:?} {layout} prepare_binaural_state"),
                &mut prepare_ns,
            );
            eprintln!(
                "{hrtf:?} {layout}: selected-HRIR tap payload={prepared_payload_bytes} bytes (excludes transient full f32 bank, loader/allocator overhead, and peak init memory)"
            );

            for block_samples in BLOCK_SIZE_OPTIONS {
                for run in 0..INDEPENDENT_RUNS {
                    let mut state = BinauralState::new(&config, None, None).unwrap();
                    let first_frame = binaural_probe_frame(channels, block_samples, 0);
                    let first_start = std::time::Instant::now();
                    let _ = state.render(&first_frame).unwrap();
                    let first_render_ns = first_start.elapsed().as_nanos();
                    let renderer = state.engine.as_ref().expect("first render initializes FIR");
                    let kernel_storage_bytes = renderer.hrir_kernel_storage_bytes();
                    let history_storage_bytes = renderer.hrir_history_storage_bytes();
                    let mut block_ns = Vec::with_capacity(MEASURED_BLOCKS);
                    let mut digest = 0xcbf2_9ce4_8422_2325_u64;
                    for block in 0..MEASURED_BLOCKS {
                        let start_sample = ((block + 1) * block_samples) as u64;
                        let frame = binaural_probe_frame(channels, block_samples, start_sample);
                        let start = std::time::Instant::now();
                        let output = std::hint::black_box(
                            state.render(std::hint::black_box(&frame)).unwrap(),
                        );
                        block_ns.push(start.elapsed().as_nanos());
                        for sample in output.channels.iter().flatten() {
                            digest =
                                (digest ^ sample.to_bits()).wrapping_mul(0x0000_0100_0000_01b3);
                        }
                    }
                    let average_ns: u128 = block_ns.iter().sum::<u128>() / block_ns.len() as u128;
                    let block_period_ns =
                        (block_samples as u128 * 1_000_000_000_u128) / 48_000_u128;
                    let rtf = average_ns as f64 / block_period_ns as f64;
                    let half_period_exceedances = block_ns
                        .iter()
                        .filter(|&&elapsed_ns| elapsed_ns > block_period_ns / 2)
                        .count();
                    let period_exceedances = block_ns
                        .iter()
                        .filter(|&&elapsed_ns| elapsed_ns > block_period_ns)
                        .count();
                    print_timing_distribution(
                        &format!(
                            "{hrtf:?} {layout} {block_samples}-sample run {} render_block",
                            run + 1
                        ),
                        &mut block_ns,
                    );
                    eprintln!(
                        "{hrtf:?} {layout} {block_samples}-sample run {}: first_render={:.3} us block_RTF={rtf:.4} block_period={:.3} us >50%_budget={half_period_exceedances}/{MEASURED_BLOCKS} >period={period_exceedances}/{MEASURED_BLOCKS} (timing exceedances, not device underruns) kernel_storage={kernel_storage_bytes} bytes history_storage={history_storage_bytes} bytes PCM digest={digest:016x}",
                        run + 1,
                        first_render_ns as f64 / 1_000.0,
                        block_period_ns as f64 / 1_000.0,
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "manual release-mode comparison of static and dynamic direct FIR; no wall-clock CI threshold"]
fn dynamic_binaural_vs_static_release_probe() {
    const BLOCKS: usize = 256;
    const ACTIVE_BLOCKS: usize = 64;
    const BLOCK_SIZES: [usize; 2] = [128, 256];
    const FADE_SAMPLES: usize = 240;

    for hrtf in [BuiltinHrtf::SadieD1Ku100, BuiltinHrtf::SadieD2Kemar] {
        for layout in ["7.1.4", "9.1.6"] {
            let config = BinauralConfig::builtin(hrtf, layout);
            let preparer = ListenerOrientationPreparer::new(&config).unwrap();
            let channels = SpeakerLayoutPreset::for_name(layout).unwrap().labels.len();
            let source_count = preparer.source_count();

            let mut prepared_updates = Vec::with_capacity(ACTIVE_BLOCKS);
            let mut prepare_ns = Vec::with_capacity(ACTIVE_BLOCKS);
            for sequence in 1..=ACTIVE_BLOCKS {
                let angle = sequence as f64 * 0.25_f64.to_radians();
                let orientation =
                    ListenerOrientation::new(0.0, 0.0, (angle * 0.5).sin(), (angle * 0.5).cos())
                        .unwrap();
                let start = std::time::Instant::now();
                let update = preparer.prepare(orientation, 0, sequence as u64).unwrap();
                prepare_ns.push(start.elapsed().as_nanos());
                prepared_updates.push(update);
            }
            print_timing_distribution(
                &format!("{hrtf:?} {layout} HRIR preparation for active-transition probe"),
                &mut prepare_ns,
            );

            for block_samples in BLOCK_SIZES {
                let initial = preparer
                    .prepare(ListenerOrientation::IDENTITY, 0, 0)
                    .unwrap();
                let dynamic_sources = initial
                    .kernels()
                    .iter()
                    .map(|kernel| DynamicBinauralSource::new(kernel.source_id(), 1.0).unwrap())
                    .collect::<Vec<_>>();
                assert_eq!(dynamic_sources.len(), source_count);
                let mut static_state = BinauralState::new(&config, None, None).unwrap();
                let mappings = static_state.mappings.clone();
                let static_engine = static_state.ensure_engine(48_000).unwrap();
                let mut dynamic_engine = DynamicBinauralRenderer::new(
                    48_000,
                    initial,
                    dynamic_sources,
                    preparer.max_filter_taps(),
                    block_samples,
                    FADE_SAMPLES,
                )
                .unwrap();
                let frame = binaural_probe_frame(channels, block_samples, 0);
                let blocks = mappings
                    .iter()
                    .map(|mapping| {
                        BinauralSourceBlock::new(
                            mapping.source_id,
                            &frame.channels[mapping.channel_index],
                        )
                    })
                    .collect::<Vec<_>>();
                let mut static_left = vec![0.0; block_samples];
                let mut static_right = vec![0.0; block_samples];
                let mut dynamic_left = vec![0.0; block_samples];
                let mut dynamic_right = vec![0.0; block_samples];

                static_engine
                    .render_block(&blocks, &mut static_left, &mut static_right)
                    .unwrap();
                dynamic_engine
                    .render_block(&blocks, &mut dynamic_left, &mut dynamic_right)
                    .unwrap();
                assert_eq!(
                    static_left, dynamic_left,
                    "identity left PCM {hrtf:?} {layout}"
                );
                assert_eq!(
                    static_right, dynamic_right,
                    "identity right PCM {hrtf:?} {layout}"
                );

                let mut static_ns = Vec::with_capacity(BLOCKS);
                let mut dynamic_ns = Vec::with_capacity(BLOCKS);
                let mut static_digest = 0xcbf2_9ce4_8422_2325_u64;
                let mut dynamic_digest = 0xcbf2_9ce4_8422_2325_u64;
                for _ in 0..BLOCKS {
                    let start = std::time::Instant::now();
                    static_engine
                        .render_block(
                            std::hint::black_box(&blocks),
                            std::hint::black_box(&mut static_left),
                            std::hint::black_box(&mut static_right),
                        )
                        .unwrap();
                    static_ns.push(start.elapsed().as_nanos());
                    for sample in static_left.iter().chain(&static_right) {
                        static_digest =
                            (static_digest ^ sample.to_bits()).wrapping_mul(0x0000_0100_0000_01b3);
                    }

                    let start = std::time::Instant::now();
                    dynamic_engine
                        .render_block(
                            std::hint::black_box(&blocks),
                            std::hint::black_box(&mut dynamic_left),
                            std::hint::black_box(&mut dynamic_right),
                        )
                        .unwrap();
                    dynamic_ns.push(start.elapsed().as_nanos());
                    for sample in dynamic_left.iter().chain(&dynamic_right) {
                        dynamic_digest =
                            (dynamic_digest ^ sample.to_bits()).wrapping_mul(0x0000_0100_0000_01b3);
                    }
                }
                print_timing_distribution(
                    &format!("{hrtf:?} {layout} {block_samples} direct static FIR"),
                    &mut static_ns,
                );
                print_timing_distribution(
                    &format!("{hrtf:?} {layout} {block_samples} direct dynamic FIR, stable kernel"),
                    &mut dynamic_ns,
                );
                eprintln!(
                    "{hrtf:?} {layout} {block_samples}: sources={source_count} taps_bound={} static_kernel_bytes={} dynamic_kernel_bytes={} dynamic_history_bytes={} static_digest={static_digest:016x} dynamic_digest={dynamic_digest:016x}",
                    preparer.max_filter_taps(),
                    static_engine.hrir_kernel_storage_bytes(),
                    dynamic_engine.hrir_kernel_storage_bytes(),
                    dynamic_engine.hrir_history_storage_bytes(),
                );

                let mut active_ns = Vec::with_capacity(ACTIVE_BLOCKS);
                for update in prepared_updates.iter().cloned() {
                    let accepted = dynamic_engine.apply_prepared(update).unwrap();
                    drop(accepted.retired_kernels);
                    let start = std::time::Instant::now();
                    dynamic_engine
                        .render_block(
                            std::hint::black_box(&blocks),
                            std::hint::black_box(&mut dynamic_left),
                            std::hint::black_box(&mut dynamic_right),
                        )
                        .unwrap();
                    active_ns.push(start.elapsed().as_nanos());
                }
                print_timing_distribution(
                    &format!(
                        "{hrtf:?} {layout} {block_samples} direct dynamic FIR, active transitions"
                    ),
                    &mut active_ns,
                );
                let block_period_ns = (block_samples as u128 * 1_000_000_000_u128) / 48_000_u128;
                let over_period = active_ns
                    .iter()
                    .filter(|&&elapsed| elapsed > block_period_ns)
                    .count();
                eprintln!(
                    "{hrtf:?} {layout} {block_samples} active transitions: >period={over_period}/{ACTIVE_BLOCKS}, block_period={:.3} us (direct FIR only; no decode/device scheduling)",
                    block_period_ns as f64 / 1_000.0
                );
            }
        }
    }
}

fn push_bits(bytes: &mut [u8], cursor: &mut usize, value: u64, width: usize) {
    for shift in (0..width).rev() {
        if value & (1_u64 << shift) != 0 {
            bytes[*cursor / 8] |= 0x80 >> (*cursor % 8);
        }
        *cursor += 1;
    }
}

fn indexed_syncframe(stream_type: u8, size: usize, marker: u8) -> Vec<u8> {
    let mut bytes = vec![0_u8; size];
    let mut cursor = 0;
    push_bits(&mut bytes, &mut cursor, 0x0b77, 16);
    push_bits(&mut bytes, &mut cursor, u64::from(stream_type), 2);
    push_bits(&mut bytes, &mut cursor, 0, 3);
    push_bits(
        &mut bytes,
        &mut cursor,
        u64::try_from(size / 2 - 1).expect("frame words"),
        11,
    );
    push_bits(&mut bytes, &mut cursor, 0, 2);
    push_bits(&mut bytes, &mut cursor, 3, 2);
    push_bits(&mut bytes, &mut cursor, 2, 3);
    push_bits(&mut bytes, &mut cursor, 0, 1);
    push_bits(&mut bytes, &mut cursor, 16, 5);
    bytes[size - 1] = marker;
    bytes
}

#[test]
fn default_config_is_headless_and_has_stable_output_contract() {
    assert_eq!(OpenJocConfig::default().dialnorm, DialnormMode::Default);
    let session = OpenJocSession::new(OpenJocConfig::default()).expect("default config");
    let info = session.output_info();
    assert_eq!(info.sample_format, PcmSampleFormat::F32);
    assert_eq!(info.layout_name, "5.1");
    assert_eq!(info.channel_labels, ["FL", "FR", "FC", "LFE", "Ls", "Rs"]);
    assert_eq!(
        info.latency_samples,
        577 + FINAL_LINKED_GAIN_LATENCY_SAMPLES
    );
}

#[test]
fn native_twenty_two_two_is_available_through_the_rust_session_api() {
    let session = OpenJocSession::new(OpenJocConfig {
        speaker_layout: "22.2".to_owned(),
        ..OpenJocConfig::default()
    })
    .expect("22.2 session");
    let info = session.output_info();
    assert_eq!(info.layout_name, "22.2");
    assert_eq!(info.channel_labels.len(), 24);
    assert_eq!(info.channel_labels[3], "LFE1");
    assert_eq!(info.channel_labels[9], "LFE2");
}

#[test]
fn rust_session_accepts_custom_layout_without_json_or_cli() {
    let layout = SpeakerLayout::custom(
        "rust-studio",
        vec![
            SpeakerGeometry::full_range("A", -42.0, 0.0),
            SpeakerGeometry::full_range("B", 7.0, 9.0),
            SpeakerGeometry::full_range("C", 51.0, -3.0),
            SpeakerGeometry::lfe("Sub", 0.0, -20.0),
        ],
    )
    .expect("custom layout");
    let config = OpenJocConfig::default().with_speaker_layout(layout);
    let descriptor = config.effective_config_descriptor();
    assert!(descriptor.contains("custom_layout_channels=4:1:A:1:B:1:C:3:Sub"));
    let session = OpenJocSession::new(config).expect("custom session");
    let info = session.output_info();
    assert_eq!(info.layout_name, "rust-studio");
    assert_eq!(info.channel_count, 4);
    assert_eq!(info.channel_labels, ["A", "B", "C", "Sub"]);
}

#[test]
fn resampled_lfe_delay_survives_short_input_tail_and_reset() {
    let mut config = BinauralConfig::builtin_generic("5.1");
    config.lfe_policy = BinauralLfePolicy::EqualPowerDualMono;
    let mut state = BinauralState::new(&config, None, None).unwrap();
    state.lfe_delay = openjoc_render::SampleDelay::new(33);
    for _ in 0..2 {
        let mut actual = Vec::new();
        for start in [0, 1] {
            let mut channels = vec![vec![0.0]; 6];
            channels[3][0] = 1.0;
            let block = state
                .render(&RenderedBlock {
                    sample_rate: 48_000,
                    logical_start_sample: start,
                    sample_count: 1,
                    channels,
                })
                .unwrap();
            actual.extend(block.channels[0].iter().copied());
        }
        for block in state.drain_tail(48_000, 2).unwrap() {
            assert_eq!(block.channels[0], block.channels[1]);
            actual.extend(block.channels[0].iter().copied());
        }
        assert!(actual.len() >= 35);
        for (index, sample) in actual.into_iter().enumerate() {
            let expected = if index == 33 || index == 34 {
                std::f64::consts::FRAC_1_SQRT_2
            } else {
                0.0
            };
            assert!((sample - expected).abs() < 1e-12);
        }
        state.reset().unwrap();
    }
}

#[test]
fn builtin_generic_binaural_is_available_without_sofa_bytes() {
    assert_eq!(
        BinauralConfig::builtin_generic("7.1.4").builtin_hrtf,
        BuiltinHrtf::SadieD1Ku100
    );
    let config = OpenJocConfig {
        render_mode: RenderMode::Binaural,
        speaker_layout: "7.1.4".to_owned(),
        binaural: Some(BinauralConfig::builtin_generic("7.1.4")),
        ..OpenJocConfig::default()
    };
    let session = OpenJocSession::new(config).expect("built-in generic HRTF session");
    assert_eq!(session.latency_samples(), QMF_LATENCY_SAMPLES);
    assert!(!session.speaker.common_profile_stereo_enabled);
    let info = session.output_info();
    assert_eq!(info.layout_name, "Binaural stereo");
    assert_eq!(info.channel_labels, ["Left Ear", "Right Ear"]);
}

#[test]
fn external_builtin_asset_constructs_a_binaural_session() {
    let asset = openjoc_sofa::builtin_hrtf_asset_bytes(BuiltinHrtf::SadieD1Ku100)
        .expect("packaged D1 asset");
    let config = OpenJocConfig {
        render_mode: RenderMode::Binaural,
        speaker_layout: "7.1.4".to_owned(),
        binaural: Some(BinauralConfig::builtin(BuiltinHrtf::SadieD1Ku100, "7.1.4")),
        ..OpenJocConfig::default()
    };

    let session = OpenJocSession::new_with_hrtf_asset(config, asset)
        .expect("externally supplied built-in asset");
    assert_eq!(session.output_info().render_mode, RenderMode::Binaural);
}

#[test]
fn external_builtin_asset_rejects_corruption_before_session_creation() {
    let mut asset = openjoc_sofa::builtin_hrtf_asset_bytes(BuiltinHrtf::SadieD1Ku100)
        .expect("packaged D1 asset")
        .to_vec();
    *asset.last_mut().expect("asset payload") ^= 1;
    let config = OpenJocConfig {
        render_mode: RenderMode::Binaural,
        speaker_layout: "7.1.4".to_owned(),
        binaural: Some(BinauralConfig::builtin(BuiltinHrtf::SadieD1Ku100, "7.1.4")),
        ..OpenJocConfig::default()
    };

    assert!(OpenJocSession::new_with_hrtf_asset(config, &asset).is_err());
}

#[test]
fn non_default_built_in_presets_are_valid_session_configurations() {
    let config = OpenJocConfig {
        render_mode: RenderMode::Binaural,
        speaker_layout: "7.1.4".to_owned(),
        binaural: Some(BinauralConfig::builtin(BuiltinHrtf::SadieD2Kemar, "7.1.4")),
        ..OpenJocConfig::default()
    };
    let session = OpenJocSession::new(config).expect("built-in preset session");
    assert_eq!(
        session.output_info().channel_labels,
        ["Left Ear", "Right Ear"]
    );
}

#[test]
fn common_profile_stereo_composition_is_disabled_for_binaural_only() {
    let physical = OpenJocSession::new(OpenJocConfig {
        render_mode: RenderMode::Stereo,
        speaker_layout: "2.0".to_owned(),
        ..OpenJocConfig::default()
    })
    .expect("physical Stereo session");
    assert!(physical.speaker.common_profile_stereo_enabled);

    let binaural = OpenJocSession::new(OpenJocConfig {
        render_mode: RenderMode::Binaural,
        speaker_layout: "2.0".to_owned(),
        binaural: Some(BinauralConfig::builtin_generic("2.0")),
        ..OpenJocConfig::default()
    })
    .expect("virtual-2.0 binaural session");
    assert!(!binaural.speaker.common_profile_stereo_enabled);
}

#[test]
fn effective_config_fingerprint_ignores_non_effective_binaural_speaker_layout() {
    let cli_config = OpenJocConfig {
        render_mode: RenderMode::Binaural,
        speaker_layout: "7.1.4".to_owned(),
        binaural: Some(BinauralConfig::builtin_generic("7.1.4")),
        ..OpenJocConfig::default()
    };
    let gst_config = OpenJocConfig {
        render_mode: RenderMode::Binaural,
        speaker_layout: "5.1".to_owned(),
        binaural: Some(BinauralConfig::builtin_generic("7.1.4")),
        ..OpenJocConfig::default()
    };
    assert_eq!(
        cli_config.effective_config_descriptor(),
        gst_config.effective_config_descriptor()
    );
    assert_eq!(
        cli_config.effective_config_fingerprint(),
        gst_config.effective_config_fingerprint()
    );
}

#[test]
fn access_unit_trace_records_exact_bytes_counts_and_sample_pts() {
    let stream = [
        indexed_syncframe(0, 16, 0x10),
        indexed_syncframe(1, 16, 0x20),
        indexed_syncframe(0, 16, 0x30),
        indexed_syncframe(1, 16, 0x40),
    ]
    .concat();
    let trace = trace_access_units(&stream, Some(1000)).expect("trace access units");
    assert_eq!(trace.len(), 2);
    assert_eq!(trace[0].byte_length, 32);
    assert_eq!(trace[0].pts_samples, Some(1000));
    assert_eq!(trace[0].independent_frame_count, 1);
    assert_eq!(trace[0].dependent_frame_count, 1);
    assert_eq!(trace[1].pts_samples, Some(2536));
    assert_eq!(trace[1].sha256.len(), 64);
    assert_ne!(trace[0].sha256, trace[1].sha256);
}

#[test]
fn invalid_and_pending_lifecycle_is_structured() {
    let mut session = OpenJocSession::new(OpenJocConfig::default()).expect("session");
    assert_eq!(
        session.drain().expect("empty drain"),
        OpenJocStatus::EndOfStream
    );
    assert_eq!(
        session.drain().expect("second drain"),
        OpenJocStatus::EndOfStream
    );
    let error = session
        .push_packet(OpenJocPacket {
            data: &[0x0b],
            pts_samples: None,
            discontinuity: false,
            preroll: false,
        })
        .expect_err("drained session rejects input");
    assert_eq!(error, OpenJocError::AlreadyDrained);
}
