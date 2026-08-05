use openjoc_eac3::{
    AudioBlockPrefix, AudioPcmSynthesizer, DecodedAudioBlock, Eac3Error, inverse_transform,
    inverse_transform_with_trace, overlap_add, overlap_add_with_trace,
};

#[test]
fn inverse_transform_rejects_wrong_coefficient_dimensions() {
    assert_eq!(
        inverse_transform(&[0.0; 255], false),
        Err(Eac3Error::InvalidTransformCoefficientLength {
            expected: 256,
            actual: 255,
        })
    );
    assert_eq!(
        inverse_transform(&[0.0; 255], true),
        Err(Eac3Error::InvalidTransformCoefficientLength {
            expected: 256,
            actual: 255,
        })
    );
}

#[test]
fn inverse_transform_rejects_nonfinite_coefficients_at_the_entry_point() {
    let mut coefficients = [0.0; 256];
    coefficients[17] = f64::NAN;
    assert_eq!(
        inverse_transform(&coefficients, false),
        Err(Eac3Error::NonFiniteTransformCoefficient { index: 17 })
    );
}

#[test]
fn inverse_transform_zero_coefficients_are_zero_for_both_block_modes() {
    for block_switch in [false, true] {
        let output = inverse_transform(&[0.0; 256], block_switch).expect("zero transform");
        assert_eq!(output.len(), 512);
        assert!(output.iter().all(|value| *value == 0.0));
    }
}

#[test]
fn inverse_transform_dc_coefficient_matches_the_rendered_etsi_equations() {
    let mut coefficients = [0.0; 256];
    coefficients[0] = 1.0;
    let long = inverse_transform(&coefficients, false).expect("long IMDCT");
    let short = inverse_transform(&coefficients, true).expect("short IMDCT");
    let expected_long = [
        -0.00009869077125262666,
        -0.00016813651054637958,
        -0.0002575855386518413,
        -0.0003527972217657205,
    ];
    let expected_short = [
        -0.00013999736453956417,
        -0.00023995933963099763,
        -0.0003698258844754045,
        -0.0005095296411538492,
    ];
    for (actual, expected) in long[..4].iter().zip(expected_long) {
        assert!((actual - expected).abs() < 1.0e-15);
    }
    for (actual, expected) in short[..4].iter().zip(expected_short) {
        assert!((actual - expected).abs() < 1.0e-15);
    }
}

#[test]
fn overlap_add_emits_one_half_and_retains_the_next_half() {
    let windowed = (0..512).map(f64::from).collect::<Vec<_>>();
    let mut delay = vec![1.0; 256];
    let pcm = overlap_add(&windowed, &mut delay).expect("overlap/add");
    assert_eq!(pcm.len(), 256);
    assert_eq!(pcm[0], 2.0);
    assert_eq!(pcm[255], 512.0);
    assert_eq!(delay[0], 256.0);
    assert_eq!(delay[255], 511.0);
}

#[test]
fn overlap_add_contribution_identity_is_explicit_and_stateful() {
    let windowed = (0..512)
        .map(|index| f64::from(index) / 17.0)
        .collect::<Vec<_>>();
    let mut delay = (0..256)
        .map(|index| f64::from(index) / 23.0)
        .collect::<Vec<_>>();
    let trace = overlap_add_with_trace(&windowed, &mut delay).expect("trace");
    assert_eq!(trace.carry_in.len(), 256);
    assert_eq!(trace.current_head.len(), 256);
    assert_eq!(trace.output_sum.len(), 256);
    assert_eq!(trace.output.len(), 256);
    assert_eq!(trace.carry_out.len(), 256);
    for (index, delayed) in delay.iter().enumerate() {
        assert_eq!(
            trace.output_sum[index],
            trace.carry_in[index] + trace.current_head[index]
        );
        assert_eq!(trace.output[index], 2.0 * trace.output_sum[index]);
        assert_eq!(trace.carry_out[index], *delayed);
    }
}

#[test]
fn twelve_continuous_blocks_equal_two_framed_six_block_runs() {
    let coefficients = (0..12)
        .map(|block| {
            (0..256)
                .map(|bin| {
                    let x = (block * 257 + bin * 17) as f64;
                    (x.sin() * 0.25) + (x.cos() * 0.03125)
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let switches = [
        false, false, true, true, false, true, true, false, false, true, false, true,
    ];

    let render = |start: usize, end: usize, delay: &mut Vec<f64>| {
        let mut output = Vec::new();
        for block in start..end {
            let transform = inverse_transform_with_trace(&coefficients[block], switches[block])
                .expect("transform");
            output.extend(overlap_add(&transform.windowed, delay).expect("overlap/add"));
        }
        output
    };

    let mut continuous_delay = vec![0.0; 256];
    let continuous = render(0, 12, &mut continuous_delay);
    let mut framed_delay = vec![0.0; 256];
    let mut framed = render(0, 6, &mut framed_delay);
    framed.extend(render(6, 12, &mut framed_delay));
    assert_eq!(continuous, framed);
    assert_eq!(continuous_delay, framed_delay);
}

#[test]
fn traced_inverse_transform_has_512_samples_and_symmetric_window() {
    let mut coefficients = [0.0; 256];
    coefficients[17] = 0.75;
    let trace = inverse_transform_with_trace(&coefficients, true).expect("trace");
    assert_eq!(trace.pre_window.len(), 512);
    assert_eq!(trace.window_coefficients.len(), 512);
    assert_eq!(trace.windowed.len(), 512);
    for index in 0..512 {
        assert_eq!(
            trace.windowed[index],
            trace.pre_window[index] * trace.window_coefficients[index]
        );
        assert_eq!(
            trace.window_coefficients[index],
            trace.window_coefficients[511 - index]
        );
    }
}

fn synthetic_block(block_index: usize, block_switch: bool) -> DecodedAudioBlock {
    let coefficients = (0..256)
        .map(|bin| {
            let phase = (block_index * 257 + bin * 19) as f64;
            phase.sin() * 0.125 + phase.cos() * 0.015625
        })
        .collect();
    DecodedAudioBlock {
        block_index,
        prefix: AudioBlockPrefix {
            block_switch: vec![block_switch],
            dither: vec![false],
            dynamic_range: None,
            dynamic_range_2: None,
            spectral_extension: None,
            coupling: None,
            rematrix_flags: Vec::new(),
            channel_bandwidth_codes: vec![None],
            coupling_exponents: None,
            channel_exponents: vec![None],
            lfe_exponents: None,
            bit_allocation_parameters: None,
            snr_offsets: None,
            fast_gain_codes: None,
            converter_snr_offset: None,
            coupling_leak: None,
            delta_bit_allocation: None,
            skip_field: None,
            skip_field_start_offset_bits: None,
            next_offset_bits: 0,
        },
        channel_baps: vec![Vec::new()],
        channel_mantissas: vec![coefficients],
        coupling_bap: None,
        coupling_mantissas: None,
        enhanced_coupling: None,
        lfe_bap: None,
        lfe_mantissas: None,
        channel_aht: vec![None],
        coupling_aht: None,
        lfe_aht: None,
        mantissa_end_offset_bits: 0,
    }
}

#[test]
fn synthesizer_trace_proves_carry_out_equals_next_frame_carry_in() {
    let switches = [
        false, false, true, false, true, true, false, true, false, false, true, false,
    ];
    let blocks = switches
        .iter()
        .enumerate()
        .map(|(index, switch)| synthetic_block(index, *switch))
        .collect::<Vec<_>>();
    let mut continuous_trace = Vec::new();
    let mut continuous = AudioPcmSynthesizer::new();
    let continuous_pcm = continuous
        .synthesize_with_trace(&blocks, &mut |trace| continuous_trace.push(trace))
        .expect("continuous synthesis");
    let direct_pcm = AudioPcmSynthesizer::new()
        .synthesize(&blocks)
        .expect("direct synthesis");
    assert_eq!(direct_pcm, continuous_pcm);

    let mut framed_trace = Vec::new();
    let mut framed = AudioPcmSynthesizer::new();
    let first_pcm = framed
        .synthesize_with_trace(&blocks[..6], &mut |trace| framed_trace.push(trace))
        .expect("first frame synthesis");
    let second_pcm = framed
        .synthesize_with_trace(&blocks[6..], &mut |trace| framed_trace.push(trace))
        .expect("second frame synthesis");
    let mut framed_samples = first_pcm.channels[0].clone();
    framed_samples.extend_from_slice(&second_pcm.channels[0]);
    assert_eq!(continuous_pcm.channels[0], framed_samples);
    assert_eq!(continuous_trace[5].carry_out, framed_trace[5].carry_out);
    assert_eq!(continuous_trace[6].carry_in, framed_trace[6].carry_in);
    assert_eq!(framed_trace[6].carry_in, framed_trace[5].carry_out);
    assert_eq!(framed_trace[6].previous_block_switch, switches[5]);
    assert_eq!(framed_trace[6].block_switch, switches[6]);
}

#[test]
fn synthesizer_clone_replay_and_failed_call_are_isolated() {
    let good = synthetic_block(0, false);
    let mut bad = synthetic_block(1, false);
    bad.channel_mantissas.push(vec![0.0; 256]);
    bad.prefix.block_switch.push(false);

    let mut staged = AudioPcmSynthesizer::new();
    let mut snapshot = staged.clone();
    let first = staged
        .synthesize(std::slice::from_ref(&good))
        .expect("first block");
    let replay = snapshot
        .synthesize(std::slice::from_ref(&good))
        .expect("snapshot replay");
    assert_eq!(first, replay);

    let mut before_failure = staged.clone();
    assert!(staged.synthesize(std::slice::from_ref(&bad)).is_err());
    let after_failure = staged
        .synthesize(std::slice::from_ref(&good))
        .expect("retry after failed call");
    let expected_after_failure = before_failure
        .synthesize(std::slice::from_ref(&good))
        .expect("fresh clone after failed call");
    assert_eq!(after_failure, expected_after_failure);
}
