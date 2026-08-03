use openjoc_eac3::{
    CouplingInformation, Eac3Error, StandardCouplingCoordinates, StandardCouplingInformation,
    apply_dynamic_range_gains, dynamic_range_gain, reconstruct_standard_coupling,
    rematrix_channels,
};

#[test]
fn reconstructs_standard_coupling_coordinates_and_right_phase_flags() {
    let info = StandardCouplingInformation {
        channel_in_use: vec![true, true],
        phase_flags_in_use: true,
        begin_frequency_code: 0,
        end_frequency_code: 0,
        subband_count: 3,
        band_structure: [false; 18],
        band_count: 3,
        coordinates: vec![
            Some(StandardCouplingCoordinates {
                master: 0,
                bands: vec![(0, 0), (0, 0), (0, 0)],
            }),
            Some(StandardCouplingCoordinates {
                master: 0,
                bands: vec![(0, 0), (0, 0), (0, 0)],
            }),
        ],
        phase_flags: vec![true, false, false],
    };
    let channels = vec![vec![0.0; 37], vec![0.0; 37]];
    let coupling = vec![0.25; 36];
    let reconstructed =
        reconstruct_standard_coupling(&info, &coupling, &channels).expect("standard coupling");
    assert_eq!(reconstructed.len(), 2);
    assert!(reconstructed[0][37..73].iter().all(|value| *value == 1.0));
    assert!(reconstructed[1][37..49].iter().all(|value| *value == -1.0));
    assert!(reconstructed[1][49..73].iter().all(|value| *value == 1.0));
}

#[test]
fn rejects_nonfinite_standard_coupling_input() {
    let info = StandardCouplingInformation {
        channel_in_use: vec![true],
        phase_flags_in_use: false,
        begin_frequency_code: 0,
        end_frequency_code: 0,
        subband_count: 3,
        band_structure: [false; 18],
        band_count: 3,
        coordinates: vec![Some(StandardCouplingCoordinates {
            master: 0,
            bands: vec![(0, 0), (0, 0), (0, 0)],
        })],
        phase_flags: Vec::new(),
    };
    let mut coupling = vec![0.0; 36];
    coupling[4] = f64::INFINITY;
    let error = reconstruct_standard_coupling(&info, &coupling, &[vec![0.0; 37]])
        .expect_err("non-finite coupling");
    assert_eq!(error, Eac3Error::NonFiniteCouplingCoefficient);
}

#[test]
fn rematrix_restores_sum_and_difference_only_inside_each_flagged_band() {
    let left = (0..80).map(f64::from).collect::<Vec<_>>();
    let right = (100..180).map(f64::from).collect::<Vec<_>>();
    let restored = rematrix_channels(&[left, right], &[true, false, false, false], None, None)
        .expect("rematrix");

    for index in 0..13 {
        assert_eq!(restored[0][index], index as f64);
        assert_eq!(restored[1][index], (100 + index) as f64);
    }
    for index in 13..25 {
        assert_eq!(restored[0][index], (100 + 2 * index) as f64);
        assert_eq!(restored[1][index], -100.0);
    }
    for index in 25..80 {
        assert_eq!(restored[0][index], index as f64);
        assert_eq!(restored[1][index], (100 + index) as f64);
    }
}

#[test]
fn rematrix_standard_coupling_band_ends_at_coupling_start() {
    let coupling = CouplingInformation::Standard(StandardCouplingInformation {
        channel_in_use: vec![true, true],
        phase_flags_in_use: false,
        begin_frequency_code: 2,
        end_frequency_code: 2,
        subband_count: 3,
        band_structure: [false; 18],
        band_count: 3,
        coordinates: vec![None, None],
        phase_flags: Vec::new(),
    });
    let left = vec![1.0; 80];
    let right = vec![2.0; 80];
    let restored = rematrix_channels(&[left, right], &[false, false, true], Some(&coupling), None)
        .expect("rematrix");
    assert_eq!(restored[0][36], 1.0);
    assert_eq!(restored[1][36], 2.0);
    assert_eq!(restored[0][37], 3.0);
    assert_eq!(restored[1][37], -1.0);
    assert_eq!(restored[0][60], 3.0);
    assert_eq!(restored[1][60], -1.0);
    assert_eq!(restored[0][61], 1.0);
    assert_eq!(restored[1][61], 2.0);
}

#[test]
fn rematrix_clips_to_the_lower_channel_bandwidth() {
    let restored = rematrix_channels(
        &[vec![1.0; 80], vec![2.0; 20]],
        &[true, false, false, false],
        None,
        None,
    )
    .expect("rematrix");
    assert_eq!(restored[0].len(), 80);
    assert_eq!(restored[1].len(), 20);
    assert_eq!(restored[0][19], 3.0);
    assert_eq!(restored[1][19], -1.0);
}

#[test]
fn rematrix_rejects_wrong_flag_count_and_nonfinite_coefficients() {
    let channels = [vec![0.0; 80], vec![0.0; 80]];
    assert_eq!(
        rematrix_channels(&channels, &[false], None, None),
        Err(Eac3Error::InvalidRematrixFlagCount {
            expected: 4,
            actual: 1,
        })
    );
    let mut left = vec![0.0; 80];
    left[13] = f64::NAN;
    assert_eq!(
        rematrix_channels(
            &[left, vec![0.0; 80]],
            &[true, false, false, false],
            None,
            None
        ),
        Err(Eac3Error::NonFiniteRematrixCoefficient {
            channel: 0,
            index: 13,
        })
    );
}

#[test]
fn dynamic_range_gain_matches_the_rendered_table_and_fraction() {
    assert_eq!(dynamic_range_gain(None), 1.0);
    assert_eq!(dynamic_range_gain(Some(0x00)), 1.0);
    assert_eq!(dynamic_range_gain(Some(0x60)), 8.0);
    assert_eq!(dynamic_range_gain(Some(0x7f)), 15.75);
    assert_eq!(dynamic_range_gain(Some(0x80)), 0.0625);
    assert_eq!(dynamic_range_gain(Some(0xff)), 63.0 / 64.0);
}

#[test]
fn dynamic_range_applies_independent_linear_gains_without_mutating_input() {
    let input = vec![vec![1.0, -2.0], vec![3.0, 4.0]];
    let output = apply_dynamic_range_gains(&input, &[2.0, 0.5]).expect("dynamic range");
    assert_eq!(output, vec![vec![2.0, -4.0], vec![1.5, 2.0]]);
    assert_eq!(input, vec![vec![1.0, -2.0], vec![3.0, 4.0]]);
}
