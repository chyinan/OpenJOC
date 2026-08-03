use openjoc_eac3::{
    Eac3Error, StandardCouplingCoordinates, StandardCouplingInformation,
    reconstruct_standard_coupling,
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
