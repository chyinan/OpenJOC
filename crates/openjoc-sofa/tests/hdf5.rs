// Functional Core: deterministic in-memory HDF5 fixtures and parser regressions.
use hdf5_pure::{AttrValue, FileBuilder};
use openjoc_sofa::{SofaError, SofaLoadLimits, parse_simple_free_field_hrir};

// SOFA SimpleFreeFieldHRIR: ReceiverPosition uses RCI and fixed Data.Delay uses IR.
// https://www.sofaconventions.org/mediawiki/index.php/SimpleFreeFieldHRIR
fn fixture(receiver_shape: &[u64], delay_shape: &[u64], chunks: &[u64]) -> Vec<u8> {
    fixture_with_precision(receiver_shape, delay_shape, chunks, false)
}

fn fixture_with_precision(
    receiver_shape: &[u64],
    delay_shape: &[u64],
    chunks: &[u64],
    use_f32: bool,
) -> Vec<u8> {
    fixture_with_metadata(
        receiver_shape,
        delay_shape,
        chunks,
        use_f32,
        &Metadata::default(),
    )
}

struct Metadata<'a> {
    convention_version: &'a str,
    position: [f64; 3],
    view: [f64; 3],
    up_type: Option<&'a str>,
    up_units: Option<&'a str>,
    delay_units: Option<&'a str>,
}

impl Default for Metadata<'_> {
    fn default() -> Self {
        Self {
            convention_version: "1.1",
            position: [0., 0., 0.],
            view: [1., 0., 0.],
            up_type: Some("cartesian"),
            up_units: Some("metre"),
            delay_units: Some("samples"),
        }
    }
}

fn fixture_with_metadata(
    receiver_shape: &[u64],
    delay_shape: &[u64],
    chunks: &[u64],
    use_f32: bool,
    metadata: &Metadata<'_>,
) -> Vec<u8> {
    let mut file = FileBuilder::new();
    for (name, value) in [
        ("Conventions", "SOFA"),
        ("SOFAConventions", "SimpleFreeFieldHRIR"),
        ("SOFAConventionsVersion", metadata.convention_version),
        ("DataType", "FIR"),
        ("RoomType", "free field"),
    ] {
        file.set_attr(name, AttrValue::String(value.into()));
    }
    let ir = file.create_dataset("Data.IR");
    if use_f32 {
        ir.with_f32_data(&[1., 2., 3., 4., 5., 6., 7., 8.]);
    } else {
        ir.with_f64_data(&[1., 2., 3., 4., 5., 6., 7., 8.]);
    }
    ir.with_shape(&[2, 2, 2])
        .with_chunks(chunks)
        .with_shuffle()
        .with_deflate(6);
    let rate = file.create_dataset("Data.SamplingRate");
    if use_f32 {
        rate.with_f32_data(&[48000.]);
    } else {
        rate.with_f64_data(&[48000.]);
    }
    rate.set_attr("Units", AttrValue::String("hertz".into()));
    let delay = file.create_dataset("Data.Delay");
    delay.with_f64_data(&[0., 1.]).with_shape(delay_shape);
    if let Some(units) = metadata.delay_units {
        delay.set_attr("Units", AttrValue::String(units.into()));
    }
    for (name, data, shape, kind, units) in [
        (
            "ListenerPosition",
            metadata.position.to_vec(),
            vec![1, 3],
            "cartesian",
            "metre",
        ),
        (
            "ListenerView",
            metadata.view.to_vec(),
            vec![1, 3],
            "cartesian",
            "metre",
        ),
        (
            "ListenerUp",
            vec![0., 0., 1.],
            vec![1, 3],
            "cartesian",
            "metre",
        ),
        (
            "ReceiverPosition",
            vec![0., 0.09, 0., 0., -0.09, 0.],
            receiver_shape.to_vec(),
            "cartesian",
            "metre",
        ),
        (
            "SourcePosition",
            vec![0., 0., 1., 90., 0., 1.],
            vec![2, 3],
            "spherical",
            "degree, degree, metre",
        ),
    ] {
        let dataset = file.create_dataset(name);
        dataset.with_f64_data(&data).with_shape(&shape);
        let (kind, units) = if name == "ListenerUp" {
            (metadata.up_type, metadata.up_units)
        } else {
            (Some(kind), Some(units))
        };
        if let Some(kind) = kind {
            dataset.set_attr("Type", AttrValue::String(kind.into()));
        }
        if let Some(units) = units {
            dataset.set_attr("Units", AttrValue::String(units.into()));
        }
    }
    file.finish().expect("HDF5 fixture")
}

#[test]
fn standard_singleton_dimensions_preserve_taps_and_fixed_delays() {
    let data = fixture(&[2, 3, 1], &[1, 2], &[1, 2, 2]);
    let loaded = parse_simple_free_field_hrir(&data, SofaLoadLimits::default()).unwrap();
    assert_eq!(loaded.bank.entries()[0].pair().left_taps(), &[1., 2., 0.]);
    assert_eq!(loaded.bank.entries()[0].pair().right_taps(), &[0., 3., 4.]);
    assert_eq!(loaded.bank.entries()[1].pair().right_taps(), &[0., 7., 8.]);
    let legacy = parse_simple_free_field_hrir(
        &fixture(&[2, 3], &[2], &[1, 2, 2]),
        SofaLoadLimits::default(),
    )
    .unwrap();
    for (standard, legacy) in loaded.bank.entries().iter().zip(legacy.bank.entries()) {
        for (actual, expected) in standard
            .pair()
            .left_taps()
            .iter()
            .chain(standard.pair().right_taps())
            .zip(
                legacy
                    .pair()
                    .left_taps()
                    .iter()
                    .chain(legacy.pair().right_taps()),
            )
        {
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
    }
}

#[test]
fn compressed_chunk_cannot_exceed_coefficient_budget() {
    let data = fixture(&[2, 3], &[2], &[16, 2, 2]);
    let limits = SofaLoadLimits {
        max_total_coefficients: 32,
        ..SofaLoadLimits::default()
    };
    assert!(matches!(
        parse_simple_free_field_hrir(&data, limits),
        Err(SofaError::ResourceLimitExceeded("HDF5 chunk bytes"))
    ));
}

#[test]
fn compressed_chunk_cannot_exceed_file_budget() {
    let data = fixture(&[2, 3], &[2], &[4096, 2, 2]);
    let limits = SofaLoadLimits {
        max_file_bytes: data.len() as u64,
        ..SofaLoadLimits::default()
    };
    assert!(matches!(
        parse_simple_free_field_hrir(&data, limits),
        Err(SofaError::ResourceLimitExceeded("HDF5 chunk bytes"))
    ));
    // The file itself is valid and can be read with an adequate budget.
    assert!(parse_simple_free_field_hrir(&data, SofaLoadLimits::default()).is_ok());
}

#[test]
fn compressed_chunk_cannot_exceed_default_scratch_cap() {
    // 16 MiB + 32 bytes: enough to test the fixed cap without large allocations.
    let data = fixture(&[2, 3], &[2], &[524_289, 2, 2]);
    assert!(data.len() < 64 * 1024);
    assert!(matches!(
        parse_simple_free_field_hrir(&data, SofaLoadLimits::default()),
        Err(SofaError::ResourceLimitExceeded("HDF5 chunk bytes"))
    ));
}

#[test]
fn f32_hdf5_rows_and_scalar_values_are_widened_without_changing_taps() {
    let f32_data = fixture_with_precision(&[2, 3, 1], &[1, 2], &[1, 2, 2], true);
    let f64_data = fixture(&[2, 3, 1], &[1, 2], &[1, 2, 2]);
    let f32_bank = parse_simple_free_field_hrir(&f32_data, SofaLoadLimits::default()).unwrap();
    let f64_bank = parse_simple_free_field_hrir(&f64_data, SofaLoadLimits::default()).unwrap();
    assert_eq!(f32_bank, f64_bank);
}

#[test]
fn standard_optional_attributes_preserve_raw_taps_and_delays() {
    let reference = parse_simple_free_field_hrir(
        &fixture(&[2, 3, 1], &[1, 2], &[1, 2, 2]),
        SofaLoadLimits::default(),
    )
    .unwrap();
    for metadata in [
        Metadata {
            convention_version: "1.0",
            up_type: None,
            up_units: None,
            delay_units: None,
            ..Metadata::default()
        },
        Metadata {
            convention_version: "1.2",
            up_type: None,
            up_units: None,
            delay_units: None,
            ..Metadata::default()
        },
        Metadata {
            up_type: None,
            up_units: None,
            ..Metadata::default()
        },
        Metadata {
            delay_units: None,
            ..Metadata::default()
        },
        Metadata {
            up_type: None,
            up_units: None,
            delay_units: None,
            ..Metadata::default()
        },
    ] {
        let data = fixture_with_metadata(&[2, 3, 1], &[1, 2], &[1, 2, 2], false, &metadata);
        let loaded = parse_simple_free_field_hrir(&data, SofaLoadLimits::default()).unwrap();
        assert_eq!(loaded.bank, reference.bank);
    }
}

#[test]
fn explicit_invalid_optional_attributes_still_fail_closed() {
    for metadata in [
        Metadata {
            up_type: Some("spherical"),
            ..Metadata::default()
        },
        Metadata {
            up_units: Some("seconds"),
            ..Metadata::default()
        },
        Metadata {
            delay_units: Some("seconds"),
            ..Metadata::default()
        },
        Metadata {
            delay_units: Some("not samples"),
            ..Metadata::default()
        },
    ] {
        let data = fixture_with_metadata(&[2, 3, 1], &[1, 2], &[1, 2, 2], false, &metadata);
        assert!(matches!(
            parse_simple_free_field_hrir(&data, SofaLoadLimits::default()),
            Err(SofaError::InvalidCoordinate(_))
        ));
    }
}

#[test]
fn listener_pose_does_not_rotate_or_translate_local_receiver_ears() {
    // Receiver 0 is always the left ear (+Y in SOFA's listener-local frame).
    // The world source at [1,0,0] must still transform with the listener pose.
    for (position, view, expected_direction) in [
        ([0., 0., 0.], [1., 0., 0.], [0., 1., 0.]),
        ([0., 0., 0.], [-1., 0., 0.], [0., -1., 0.]),
        ([0., 0., 0.], [0., 1., 0.], [1., 0., 0.]),
        (
            [0., -1., 0.],
            [1., 0., 0.],
            [
                -std::f64::consts::FRAC_1_SQRT_2,
                std::f64::consts::FRAC_1_SQRT_2,
                0.,
            ],
        ),
        (
            [0., -1., 0.],
            [0., 1., 0.],
            [
                std::f64::consts::FRAC_1_SQRT_2,
                std::f64::consts::FRAC_1_SQRT_2,
                0.,
            ],
        ),
    ] {
        let metadata = Metadata {
            position,
            view,
            ..Metadata::default()
        };
        let data = fixture_with_metadata(&[2, 3, 1], &[1, 2], &[1, 2, 2], false, &metadata);
        let loaded = parse_simple_free_field_hrir(&data, SofaLoadLimits::default()).unwrap();
        let entry = &loaded.bank.entries()[0];
        assert_eq!(entry.pair().left_taps(), &[1., 2., 0.]);
        assert_eq!(entry.pair().right_taps(), &[0., 3., 4.]);
        for (actual, expected) in entry.direction().into_iter().zip(expected_direction) {
            assert!((actual - expected).abs() < 1e-12);
        }
    }
}
