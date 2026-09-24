// pattern: Functional Core

use openjoc_api::{BinauralConfig, DialnormMode, OpenJocConfig, RenderMode};
use openjoc_wasm::{
    BINAURAL_HRTF_SOURCE, BINAURAL_VIRTUAL_LAYOUT, Decoder, DecoderStatus, WasmRenderer,
};

#[test]
fn binaural_decoder_uses_fixed_browser_renderer_contract() {
    let decoder =
        Decoder::new_with_dialnorm_and_renderer(DialnormMode::Default, WasmRenderer::Binaural)
            .expect("binaural decoder");
    let status = decoder.status();

    assert_eq!(status.renderer, "Binaural (Headphones)");
    assert_eq!(status.virtual_layout, Some(BINAURAL_VIRTUAL_LAYOUT));
    assert_eq!(status.hrtf, Some(BINAURAL_HRTF_SOURCE));
    assert_eq!(status.sample_rate, None);
    assert_eq!(status.output_channels, 2);
    assert_eq!(status.latency_samples, 577);
    assert!(!status.native_dolby_decoder_used);
}

#[test]
fn unknown_renderer_modes_are_not_constructible_through_the_public_enum() {
    assert_ne!(WasmRenderer::Stereo.code(), WasmRenderer::Binaural.code());
}

#[test]
fn non_binaural_modes_reject_binaural_configuration() {
    let config = OpenJocConfig {
        render_mode: RenderMode::Stereo,
        speaker_layout: String::from("2.0"),
        binaural: Some(BinauralConfig::builtin_generic("7.1.4")),
        ..OpenJocConfig::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn custom_sofa_decoder_uses_the_existing_strict_sofa_parser() {
    let result = Decoder::new_with_dialnorm_renderer_and_custom_sofa(
        DialnormMode::Default,
        WasmRenderer::Binaural,
        b"not a SOFA file",
    );

    assert!(result.is_err(), "malformed custom SOFA must fail closed");
}

#[test]
fn custom_sofa_decoder_accepts_supported_local_sofa_bytes() {
    let sofa = custom_sofa_fixture();
    let decoder = Decoder::new_with_dialnorm_renderer_and_custom_sofa(
        DialnormMode::Default,
        WasmRenderer::Binaural,
        &sofa,
    )
    .expect("compatible local SOFA should initialize the existing binaural path");

    assert_eq!(decoder.status().hrtf, Some("Custom SOFA"));
    assert_eq!(decoder.status().output_channels, 2);
}

#[test]
fn custom_sofa_decoder_rejects_expanded_hrir_banks_over_the_wasm_memory_budget() {
    let sofa = custom_sofa_fixture_with_delay(2000.0);
    let result = Decoder::new_with_dialnorm_renderer_and_custom_sofa(
        DialnormMode::Default,
        WasmRenderer::Binaural,
        &sofa,
    );

    assert!(
        result.is_err(),
        "an expanded f64 HRIR bank over the WASM limit must fail closed"
    );
}

#[test]
fn custom_sofa_decoder_renders_real_joc_frames_to_finite_stereo_pcm() {
    let sofa = custom_sofa_fixture();
    let mut decoder = Decoder::new_with_dialnorm_renderer_and_custom_sofa(
        DialnormMode::Default,
        WasmRenderer::Binaural,
        &sofa,
    )
    .expect("compatible local SOFA should initialize");
    let fixture = include_bytes!("../testdata/joc.ec3");
    let mut frames = Vec::new();
    for chunk in fixture.chunks(97) {
        assert_ne!(decoder.push_bytes(chunk), DecoderStatus::Error);
        while let Some(frame) = decoder.receive_pcm() {
            frames.push(frame);
        }
    }
    loop {
        let status = decoder.flush().expect("custom SOFA bridge drain");
        while let Some(frame) = decoder.receive_pcm() {
            frames.push(frame);
        }
        if status == DecoderStatus::EndOfStream {
            break;
        }
    }
    assert!(!frames.is_empty());
    assert!(
        frames
            .iter()
            .any(|frame| frame.interleaved_f32.iter().any(|sample| *sample != 0.0))
    );
    assert!(frames.iter().all(|frame| {
        frame.sample_rate == 48_000
            && frame.channel_count == 2
            && frame
                .interleaved_f32
                .iter()
                .all(|sample| sample.is_finite())
    }));
}

fn custom_sofa_fixture() -> Vec<u8> {
    custom_sofa_fixture_with_delay(0.0)
}

fn custom_sofa_fixture_with_delay(delay: f64) -> Vec<u8> {
    let mut source_positions = Vec::new();
    let mut impulse_responses = Vec::new();
    for elevation in (-75..=75).step_by(15) {
        for azimuth in (-180..180).step_by(15) {
            source_positions.extend([azimuth as f64, elevation as f64, 1.0]);
            impulse_responses.extend([1.0, 0.0, 0.75, 0.0]);
        }
    }
    source_positions.extend([0.0, 90.0, 1.0]);
    impulse_responses.extend([1.0, 0.0, 0.75, 0.0]);
    let measurements = source_positions.len() / 3;
    let dimensions = [
        ("M", measurements),
        ("R", 2),
        ("N", 2),
        ("C", 3),
        ("One", 1),
    ];
    let dimension = |name: &str| {
        dimensions
            .iter()
            .position(|(dimension_name, _)| *dimension_name == name)
            .expect("fixture dimension")
    };
    let mut variables =
        vec![
            FixtureVariable::new(
                "Data.IR",
                vec![dimension("M"), dimension("R"), dimension("N")],
                &impulse_responses,
            ),
            FixtureVariable::new("Data.SamplingRate", vec![dimension("One")], &[48_000.0])
                .attribute(fixture_attribute("Units", "hertz")),
            FixtureVariable::new("Data.Delay", vec![dimension("R")], &[delay, delay])
                .attribute(fixture_attribute("Units", "samples")),
            FixtureVariable::new(
                "SourcePosition",
                vec![dimension("M"), dimension("C")],
                &source_positions,
            )
            .attributes(vec![
                fixture_attribute("Type", "spherical"),
                fixture_attribute("Units", "degree, degree, metre"),
            ]),
            FixtureVariable::new("ListenerPosition", vec![dimension("C")], &[0.0, 0.0, 0.0])
                .attributes(vec![
                    fixture_attribute("Type", "cartesian"),
                    fixture_attribute("Units", "metre"),
                ]),
            FixtureVariable::new("ListenerView", vec![dimension("C")], &[0.0, 1.0, 0.0])
                .attributes(vec![
                    fixture_attribute("Type", "cartesian"),
                    fixture_attribute("Units", "metre"),
                ]),
            FixtureVariable::new("ListenerUp", vec![dimension("C")], &[0.0, 0.0, 1.0]).attributes(
                vec![
                    fixture_attribute("Type", "cartesian"),
                    fixture_attribute("Units", "metre"),
                ],
            ),
            FixtureVariable::new(
                "ReceiverPosition",
                vec![dimension("R"), dimension("C")],
                &[0.09, 0.0, 0.0, -0.09, 0.0, 0.0],
            )
            .attributes(vec![
                fixture_attribute("Type", "cartesian"),
                fixture_attribute("Units", "metre"),
            ]),
            FixtureVariable::new("EmitterPosition", vec![dimension("C")], &[0.0, 0.0, 0.0])
                .attributes(vec![
                    fixture_attribute("Type", "cartesian"),
                    fixture_attribute("Units", "metre"),
                ]),
        ];
    let globals = [
        fixture_attribute("Conventions", "SOFA"),
        fixture_attribute("SOFAConventions", "SimpleFreeFieldHRIR"),
        fixture_attribute("SOFAConventionsVersion", "1.2"),
        fixture_attribute("DataType", "FIR"),
        fixture_attribute("RoomType", "free field"),
    ];
    make_cdf1(&dimensions, &globals, &mut variables)
}

struct FixtureAttribute {
    name: String,
    value: String,
}

fn fixture_attribute(name: &str, value: &str) -> FixtureAttribute {
    FixtureAttribute {
        name: name.into(),
        value: value.into(),
    }
}

struct FixtureVariable {
    name: &'static str,
    dimensions: Vec<usize>,
    bytes: Vec<u8>,
    attributes: Vec<FixtureAttribute>,
}

impl FixtureVariable {
    fn new(name: &'static str, dimensions: Vec<usize>, values: &[f64]) -> Self {
        Self {
            name,
            dimensions,
            bytes: values
                .iter()
                .flat_map(|value| value.to_be_bytes())
                .collect(),
            attributes: Vec::new(),
        }
    }

    fn attribute(mut self, attribute: FixtureAttribute) -> Self {
        self.attributes.push(attribute);
        self
    }

    fn attributes(mut self, attributes: Vec<FixtureAttribute>) -> Self {
        self.attributes.extend(attributes);
        self
    }
}

fn make_cdf1(
    dimensions: &[(&str, usize)],
    globals: &[FixtureAttribute],
    variables: &mut [FixtureVariable],
) -> Vec<u8> {
    let mut output = Vec::new();
    output.extend_from_slice(b"CDF\x01");
    put_u32(&mut output, 0);
    put_u32(&mut output, 10);
    put_u32(&mut output, dimensions.len() as u32);
    for (name, length) in dimensions {
        put_string(&mut output, name);
        put_u32(&mut output, *length as u32);
    }
    put_attributes(&mut output, globals);
    put_u32(&mut output, 11);
    put_u32(&mut output, variables.len() as u32);
    let mut begin_offsets = Vec::new();
    for variable in variables.iter() {
        put_string(&mut output, variable.name);
        put_u32(&mut output, variable.dimensions.len() as u32);
        for dimension in &variable.dimensions {
            put_u32(&mut output, *dimension as u32);
        }
        put_attributes(&mut output, &variable.attributes);
        put_u32(&mut output, 6);
        let padded_length = (variable.bytes.len() + 3) & !3;
        put_u32(&mut output, padded_length as u32);
        begin_offsets.push(output.len());
        put_u32(&mut output, 0);
    }
    output.resize((output.len() + 3) & !3, 0);
    for (index, variable) in variables.iter().enumerate() {
        let begin = output.len() as u32;
        output[begin_offsets[index]..begin_offsets[index] + 4]
            .copy_from_slice(&begin.to_be_bytes());
        output.extend_from_slice(&variable.bytes);
        while output.len() % 4 != 0 {
            output.push(0);
        }
    }
    output
}

fn put_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn put_string(output: &mut Vec<u8>, value: &str) {
    put_u32(output, value.len() as u32);
    output.extend_from_slice(value.as_bytes());
    while output.len() % 4 != 0 {
        output.push(0);
    }
}

fn put_attributes(output: &mut Vec<u8>, attributes: &[FixtureAttribute]) {
    if attributes.is_empty() {
        put_u32(output, 0);
        return;
    }
    put_u32(output, 12);
    put_u32(output, attributes.len() as u32);
    for attribute in attributes {
        put_string(output, &attribute.name);
        put_u32(output, 2);
        put_u32(output, attribute.value.len() as u32);
        output.extend_from_slice(attribute.value.as_bytes());
        while output.len() % 4 != 0 {
            output.push(0);
        }
    }
}
