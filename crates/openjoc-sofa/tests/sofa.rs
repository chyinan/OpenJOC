use openjoc_render::{
    BinauralRenderer, CartesianPosition, HrirBank, SourceId, StaticBinauralSource,
    UniformPartitionedConfig, UniformPartitionedConvolver,
};
use openjoc_sofa::{
    SofaError, SofaLoadLimits, load_simple_free_field_hrir, parse_simple_free_field_hrir,
};

#[test]
fn valid_reversed_receivers_map_coordinates_and_delays() {
    let data = fixture("SimpleFreeFieldHRIR", [0.0, 1.0], false);
    let loaded =
        parse_simple_free_field_hrir(&data, SofaLoadLimits::default()).expect("valid fixture");
    assert_eq!(loaded.metadata.measurement_count, 3);
    assert_eq!(loaded.metadata.sample_rate_hz, 48_000);
    assert_eq!(loaded.bank.entries().len(), 3);
    assert_eq!(loaded.bank.entries()[0].direction(), [0.0, 1.0, 0.0]);
    assert_eq!(loaded.bank.entries()[1].direction(), [1.0, 0.0, 0.0]);
    assert_eq!(loaded.bank.entries()[2].direction(), [0.0, 0.0, 1.0]);
    assert_eq!(
        loaded.bank.entries()[0].pair().left_taps(),
        &[0.0, 3.0, 4.0]
    );
    assert_eq!(
        loaded.bank.entries()[0].pair().right_taps(),
        &[1.0, 2.0, 0.0]
    );
    assert_eq!(
        loaded.bank.entries()[1].pair().left_taps(),
        &[0.0, 7.0, 8.0]
    );
}

#[test]
fn per_measurement_equal_delays_are_accepted() {
    let data = fixture("SimpleFreeFieldHRIR", [0.0, 1.0], true);
    let loaded = parse_simple_free_field_hrir(&data, SofaLoadLimits::default())
        .expect("per-measurement delays");
    assert_eq!(
        loaded.bank.entries()[2].pair().right_taps(),
        &[9.0, 10.0, 0.0]
    );
}

#[test]
fn deterministic_load_and_renderer_integration() {
    let data = fixture("SimpleFreeFieldHRIR", [0.0, 1.0], false);
    let first = parse_simple_free_field_hrir(&data, SofaLoadLimits::default()).expect("first load");
    let second =
        parse_simple_free_field_hrir(&data, SofaLoadLimits::default()).expect("second load");
    assert_eq!(first, second);
    let bank: HrirBank = first.bank.clone();
    let sources = vec![
        StaticBinauralSource::new(
            SourceId::new(1),
            CartesianPosition::new(0.0, 1.0, 0.0),
            1.0,
            bank.entries()[0].id(),
        )
        .expect("source"),
    ];
    let _direct =
        BinauralRenderer::new(48_000, bank.clone(), sources.clone()).expect("direct integration");
    let config = UniformPartitionedConfig::new(4).expect("partition config");
    let _partitioned = UniformPartitionedConvolver::new(48_000, config, bank, sources)
        .expect("partition integration");
}

#[test]
fn malformed_and_unsupported_inputs_are_rejected() {
    assert!(matches!(
        parse_simple_free_field_hrir(b"not SOFA", SofaLoadLimits::default()),
        Err(SofaError::UnsupportedContainerOrEncoding)
    ));
    let unsupported = fixture("GeneralFIR", [0.0, 1.0], false);
    assert!(matches!(
        parse_simple_free_field_hrir(&unsupported, SofaLoadLimits::default()),
        Err(SofaError::UnsupportedSofaConvention(_))
    ));
    let fractional = fixture("SimpleFreeFieldHRIR", [0.5, 1.0], false);
    assert!(matches!(
        parse_simple_free_field_hrir(&fractional, SofaLoadLimits::default()),
        Err(SofaError::UnsupportedFractionalSofaDelay { .. })
    ));
    let limited = SofaLoadLimits {
        max_file_bytes: 16,
        ..SofaLoadLimits::default()
    };
    assert!(matches!(
        parse_simple_free_field_hrir(&fixture("SimpleFreeFieldHRIR", [0.0, 1.0], false), limited),
        Err(SofaError::ResourceLimitExceeded("file bytes"))
    ));
}

#[test]
fn hdf5_signature_is_rejected_without_native_dependencies() {
    let hdf5 = [0x89, b'H', b'D', b'F', 0x0d, 0x0a, 0x1a, 0x0a];
    assert!(matches!(
        parse_simple_free_field_hrir(&hdf5, SofaLoadLimits::default()),
        Err(SofaError::UnsupportedContainerOrEncoding)
    ));
}

#[test]
fn file_path_loader_uses_local_file_only() {
    let path = std::env::temp_dir().join(format!("openjoc-sofa-test-{}.sofa", std::process::id()));
    std::fs::write(&path, fixture("SimpleFreeFieldHRIR", [0.0, 1.0], false))
        .expect("write fixture");
    let loaded = load_simple_free_field_hrir(&path, SofaLoadLimits::default()).expect("path load");
    std::fs::remove_file(&path).expect("remove fixture");
    assert_eq!(loaded.metadata.convention_version, "1.2");
}

fn fixture(convention: &str, delays: [f64; 2], per_measurement_delay: bool) -> Vec<u8> {
    let dimensions = vec![("M", 3usize), ("R", 2), ("N", 2), ("C", 3), ("One", 1)];
    let dim_id = |name: &str| {
        dimensions
            .iter()
            .position(|(candidate, _)| *candidate == name)
            .expect("dimension")
    };
    let listener_position = [10.0, 0.0, 0.0];
    let source = [
        (1.0_f64.atan2(10.0).to_degrees(), 0.0, 101.0_f64.sqrt()),
        (0.0, 0.0, 11.0),
        (0.0, 1.0_f64.atan2(10.0).to_degrees(), 101.0_f64.sqrt()),
    ];
    let receiver = [10.1, 0.0, 0.0, 9.9, 0.0, 0.0];
    let delay_values = if per_measurement_delay {
        vec![
            delays[0], delays[1], delays[0], delays[1], delays[0], delays[1],
        ]
    } else {
        delays.to_vec()
    };
    let mut variables = vec![
        Var::new(
            "Data.IR",
            vec![dim_id("M"), dim_id("R"), dim_id("N")],
            &doubles(&[
                1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0,
            ]),
            vec![],
        ),
        Var::new(
            "Data.SamplingRate",
            vec![dim_id("One")],
            &doubles(&[48_000.0]),
            vec![text_attr("Units", "hertz")],
        ),
        Var::new(
            "Data.Delay",
            if per_measurement_delay {
                vec![dim_id("M"), dim_id("R")]
            } else {
                vec![dim_id("R")]
            },
            &delay_values,
            vec![text_attr("Units", "samples")],
        ),
        Var::new(
            "SourcePosition",
            vec![dim_id("M"), dim_id("C")],
            &doubles(
                &source
                    .iter()
                    .flat_map(|(a, e, d)| [*a, *e, *d])
                    .collect::<Vec<_>>(),
            ),
            vec![
                text_attr("Type", "spherical"),
                text_attr("Units", "degree, degree, metre"),
            ],
        ),
        Var::new(
            "ListenerPosition",
            vec![dim_id("C")],
            &doubles(&listener_position),
            vec![text_attr("Type", "cartesian"), text_attr("Units", "metre")],
        ),
        Var::new(
            "ListenerView",
            vec![dim_id("C")],
            &doubles(&[0.0, 1.0, 0.0]),
            vec![text_attr("Type", "cartesian"), text_attr("Units", "metre")],
        ),
        Var::new(
            "ListenerUp",
            vec![dim_id("C")],
            &doubles(&[0.0, 0.0, 1.0]),
            vec![text_attr("Type", "cartesian"), text_attr("Units", "metre")],
        ),
        Var::new(
            "ReceiverPosition",
            vec![dim_id("R"), dim_id("C")],
            &doubles(&receiver),
            vec![text_attr("Type", "cartesian"), text_attr("Units", "metre")],
        ),
        Var::new(
            "EmitterPosition",
            vec![dim_id("C")],
            &doubles(&[0.0, 0.0, 0.0]),
            vec![text_attr("Type", "cartesian"), text_attr("Units", "metre")],
        ),
    ];
    let globals = vec![
        text_attr("Conventions", "SOFA"),
        text_attr("SOFAConventions", convention),
        text_attr("SOFAConventionsVersion", "1.2"),
        text_attr("DataType", "FIR"),
        text_attr("RoomType", "free field"),
        text_attr("Title", "OpenJOC synthetic fixture"),
        text_attr("License", "Apache-2.0 project-owned synthetic data"),
    ];
    cdf1(&dimensions, &globals, &mut variables)
}

#[derive(Clone)]
struct Attr {
    name: String,
    value: String,
}
fn text_attr(name: &str, value: &str) -> Attr {
    Attr {
        name: name.to_string(),
        value: value.to_string(),
    }
}
struct Var {
    name: &'static str,
    dims: Vec<usize>,
    data: Vec<u8>,
    attrs: Vec<Attr>,
}
impl Var {
    fn new(name: &'static str, dims: Vec<usize>, values: &[f64], attrs: Vec<Attr>) -> Self {
        Self {
            name,
            dims,
            data: values
                .iter()
                .flat_map(|value| value.to_be_bytes())
                .collect(),
            attrs,
        }
    }
}
fn doubles(values: &[f64]) -> Vec<f64> {
    values.to_vec()
}

fn cdf1(dimensions: &[(&str, usize)], globals: &[Attr], variables: &mut [Var]) -> Vec<u8> {
    let mut header = Vec::new();
    header.extend_from_slice(b"CDF\x01");
    put_u32(&mut header, 0);
    put_u32(&mut header, 10);
    put_u32(&mut header, dimensions.len() as u32);
    for (name, length) in dimensions {
        put_string(&mut header, name);
        put_u32(&mut header, *length as u32);
    }
    put_attrs(&mut header, globals);
    put_u32(&mut header, 11);
    put_u32(&mut header, variables.len() as u32);
    let mut begin_positions = Vec::new();
    for variable in variables.iter() {
        put_string(&mut header, variable.name);
        put_u32(&mut header, variable.dims.len() as u32);
        for dim in &variable.dims {
            put_u32(&mut header, *dim as u32);
        }
        put_attrs(&mut header, &variable.attrs);
        put_u32(&mut header, 6);
        let padded = (variable.data.len() + 3) & !3;
        put_u32(&mut header, padded as u32);
        begin_positions.push(header.len());
        put_u32(&mut header, 0);
    }
    let data_start = (header.len() + 3) & !3;
    header.resize(data_start, 0);
    let mut cursor = data_start;
    for (index, variable) in variables.iter().enumerate() {
        let begin = cursor as u32;
        header[begin_positions[index]..begin_positions[index] + 4]
            .copy_from_slice(&begin.to_be_bytes());
        header.extend_from_slice(&variable.data);
        while header.len() % 4 != 0 {
            header.push(0);
        }
        cursor = header.len();
    }
    header
}
fn put_u32(target: &mut Vec<u8>, value: u32) {
    target.extend_from_slice(&value.to_be_bytes());
}
fn put_string(target: &mut Vec<u8>, value: &str) {
    put_u32(target, value.len() as u32);
    target.extend_from_slice(value.as_bytes());
    while target.len() % 4 != 0 {
        target.push(0);
    }
}
fn put_attrs(target: &mut Vec<u8>, attrs: &[Attr]) {
    if attrs.is_empty() {
        put_u32(target, 0);
        return;
    }
    put_u32(target, 12);
    put_u32(target, attrs.len() as u32);
    for attr in attrs {
        put_string(target, &attr.name);
        put_u32(target, 2);
        put_u32(target, attr.value.len() as u32);
        target.extend_from_slice(attr.value.as_bytes());
        while target.len() % 4 != 0 {
            target.push(0);
        }
    }
}
