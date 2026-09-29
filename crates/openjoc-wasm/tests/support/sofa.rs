// pattern: Functional Core

pub fn custom_sofa_fixture() -> Vec<u8> {
    custom_sofa_fixture_with_delay(0.0)
}

pub fn custom_sofa_fixture_with_delay(delay: f64) -> Vec<u8> {
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
