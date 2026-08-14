use openjoc_render_scene::{RESULT_SCHEMA, RenderBackend, RenderRequest, SCENE_SCHEMA, render};
use openjoc_sofa::{SofaLoadLimits, parse_simple_free_field_hrir};
use std::{fs, path::Path, path::PathBuf};

#[test]
fn direct_and_partitioned_render_same_scene_with_complete_tail() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let source = root.join("source.wav");
    let sofa = root.join("listener.sofa");
    let scene = root.join("scene.json");
    write_pcm16_mono(&source, 48_000, &[0.25, -0.5, 0.75, -0.25, 0.125]);
    let sofa_bytes = sofa_fixture();
    let direction = parse_simple_free_field_hrir(&sofa_bytes, SofaLoadLimits::default())
        .unwrap()
        .bank
        .entries()[0]
        .direction();
    fs::write(&sofa, sofa_bytes).unwrap();
    fs::write(
        &scene,
        format!(
            r#"{{"schema":"{SCENE_SCHEMA}","sample_rate_hz":48000,"source_semantics":"explicit_spatial_sources","sources":[{{"id":"voice","input_wav":"source.wav","start_sample":2,"position":{{"x":{},"y":{},"z":{}}},"gain":1.0}}]}}"#,
            direction[0], direction[1], direction[2]
        ),
    )
    .unwrap();

    let direct_dir = root.join("direct");
    let direct_repeat_dir = root.join("direct-repeat");
    let partitioned_dir = root.join("partitioned");
    let direct = render(&RenderRequest {
        scene_path: scene.clone(),
        sofa_path: sofa.clone(),
        output_dir: direct_dir.clone(),
        backend: RenderBackend::Direct,
        block_size: 2,
    })
    .unwrap();
    let direct_repeat = render(&RenderRequest {
        scene_path: scene.clone(),
        sofa_path: sofa.clone(),
        output_dir: direct_repeat_dir.clone(),
        backend: RenderBackend::Direct,
        block_size: 3,
    })
    .unwrap();
    let partitioned = render(&RenderRequest {
        scene_path: scene,
        sofa_path: sofa,
        output_dir: partitioned_dir.clone(),
        backend: RenderBackend::Partitioned { partition_size: 4 },
        block_size: 99,
    })
    .unwrap();

    assert_eq!(direct.schema, RESULT_SCHEMA);
    assert_eq!(direct.output_sample_count, 9); // start 2 + 5 input + 2-tap tail
    assert_eq!(
        direct.output_sample_count,
        direct_repeat.output_sample_count
    );
    assert_eq!(direct.output_sample_count, partitioned.output_sample_count);
    assert_eq!(direct.output_sha256, direct_repeat.output_sha256);
    assert_eq!(direct.tail_samples, 2);
    assert_eq!(partitioned.tail_samples, 2);
    assert_eq!(direct.sources[0].sample_count, 5);
    assert_eq!(direct.sources[0].start_sample, 2);

    let direct_samples = read_f32_stereo(&direct_dir.join("binaural.wav"));
    let partitioned_samples = read_f32_stereo(&partitioned_dir.join("binaural.wav"));
    assert_eq!(direct_samples.len(), partitioned_samples.len());
    assert!(
        direct_samples
            .iter()
            .zip(partitioned_samples)
            .all(|(a, b)| (a - b).abs() < 1.0e-5)
    );
    assert!(direct_dir.join("render.json").is_file());
    assert!(direct_dir.join("binaural.wav").is_file());
    assert!(!root.join("direct.staging").exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn scene_unknown_fields_and_parent_paths_are_rejected() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let path = root.join("scene.json");
    fs::write(
        &path,
        format!(
            r#"{{"schema":"{SCENE_SCHEMA}","sample_rate_hz":48000,"source_semantics":"explicit_spatial_sources","unexpected":true,"sources":[]}}"#
        ),
    )
    .unwrap();
    assert!(
        openjoc_render_scene::load_scene(
            &path,
            openjoc_render_scene::RenderSceneLoadLimits::default()
        )
        .is_err()
    );
    fs::write(
        &path,
        format!(
            r#"{{"schema":"{SCENE_SCHEMA}","sample_rate_hz":48000,"source_semantics":"explicit_spatial_sources","sources":[{{"id":"x","input_wav":"../source.wav","position":{{"x":0,"y":1,"z":0}},"gain":1}}]}}"#
        ),
    )
    .unwrap();
    assert!(
        openjoc_render_scene::load_scene(
            &path,
            openjoc_render_scene::RenderSceneLoadLimits::default()
        )
        .is_err()
    );
    let _ = fs::remove_dir_all(root);
}

fn temp_root() -> PathBuf {
    std::env::temp_dir().join(format!(
        "openjoc-render-scene-test-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ))
}

fn write_pcm16_mono(path: &Path, rate: u32, samples: &[f64]) {
    let mut data = Vec::with_capacity(samples.len() * 2);
    for &sample in samples {
        data.extend_from_slice(&((sample.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    let riff_size = 36 + data.len() as u32;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&riff_size.to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(&data);
    fs::write(path, wav).unwrap();
}

fn read_f32_stereo(path: &Path) -> Vec<f32> {
    let bytes = fs::read(path).unwrap();
    let data_len = u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize;
    bytes[44..44 + data_len]
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
        .collect()
}

fn sofa_fixture() -> Vec<u8> {
    let dimensions = vec![("M", 1usize), ("R", 2), ("N", 3), ("C", 3), ("One", 1)];
    let dim_id = |name: &str| dimensions.iter().position(|(n, _)| *n == name).unwrap();
    let mut variables = vec![
        Var::new(
            "Data.IR",
            vec![dim_id("M"), dim_id("R"), dim_id("N")],
            &[1.0, 0.5, 0.0, 0.25, 0.0, 0.0],
        ),
        Var::new("Data.SamplingRate", vec![dim_id("One")], &[48_000.0])
            .attr(text_attr("Units", "hertz")),
        Var::new("Data.Delay", vec![dim_id("R")], &[0.0, 0.0]).attr(text_attr("Units", "samples")),
        Var::new(
            "SourcePosition",
            vec![dim_id("M"), dim_id("C")],
            &[0.0, 0.0, 1.0],
        )
        .attrs(vec![
            text_attr("Type", "spherical"),
            text_attr("Units", "degree, degree, metre"),
        ]),
        Var::new("ListenerPosition", vec![dim_id("C")], &[0.0, 0.0, 0.0]).attrs(vec![
            text_attr("Type", "cartesian"),
            text_attr("Units", "metre"),
        ]),
        Var::new("ListenerView", vec![dim_id("C")], &[0.0, 1.0, 0.0]).attrs(vec![
            text_attr("Type", "cartesian"),
            text_attr("Units", "metre"),
        ]),
        Var::new("ListenerUp", vec![dim_id("C")], &[0.0, 0.0, 1.0]).attrs(vec![
            text_attr("Type", "cartesian"),
            text_attr("Units", "metre"),
        ]),
        Var::new(
            "ReceiverPosition",
            vec![dim_id("R"), dim_id("C")],
            &[0.1, 0.0, 0.0, -0.1, 0.0, 0.0],
        )
        .attrs(vec![
            text_attr("Type", "cartesian"),
            text_attr("Units", "metre"),
        ]),
        Var::new("EmitterPosition", vec![dim_id("C")], &[0.0, 0.0, 0.0]).attrs(vec![
            text_attr("Type", "cartesian"),
            text_attr("Units", "metre"),
        ]),
    ];
    let globals = vec![
        text_attr("Conventions", "SOFA"),
        text_attr("SOFAConventions", "SimpleFreeFieldHRIR"),
        text_attr("SOFAConventionsVersion", "1.2"),
        text_attr("DataType", "FIR"),
        text_attr("RoomType", "free field"),
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
        name: name.into(),
        value: value.into(),
    }
}
struct Var {
    name: &'static str,
    dims: Vec<usize>,
    data: Vec<u8>,
    attrs: Vec<Attr>,
}
impl Var {
    fn new(name: &'static str, dims: Vec<usize>, values: &[f64]) -> Self {
        Self {
            name,
            dims,
            data: values.iter().flat_map(|v| v.to_be_bytes()).collect(),
            attrs: Vec::new(),
        }
    }
    fn attr(mut self, attr: Attr) -> Self {
        self.attrs.push(attr);
        self
    }
    fn attrs(mut self, attrs: Vec<Attr>) -> Self {
        self.attrs.extend(attrs);
        self
    }
}
fn cdf1(dimensions: &[(&str, usize)], globals: &[Attr], variables: &mut [Var]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"CDF\x01");
    put_u32(&mut out, 0);
    put_u32(&mut out, 10);
    put_u32(&mut out, dimensions.len() as u32);
    for (name, len) in dimensions {
        put_string(&mut out, name);
        put_u32(&mut out, *len as u32);
    }
    put_attrs(&mut out, globals);
    put_u32(&mut out, 11);
    put_u32(&mut out, variables.len() as u32);
    let mut begins = Vec::new();
    for var in variables.iter() {
        put_string(&mut out, var.name);
        put_u32(&mut out, var.dims.len() as u32);
        for dim in &var.dims {
            put_u32(&mut out, *dim as u32);
        }
        put_attrs(&mut out, &var.attrs);
        put_u32(&mut out, 6);
        let padded = (var.data.len() + 3) & !3;
        put_u32(&mut out, padded as u32);
        begins.push(out.len());
        put_u32(&mut out, 0);
    }
    out.resize((out.len() + 3) & !3, 0);
    let mut cursor = out.len();
    for (i, var) in variables.iter().enumerate() {
        let begin = cursor as u32;
        out[begins[i]..begins[i] + 4].copy_from_slice(&begin.to_be_bytes());
        out.extend_from_slice(&var.data);
        while out.len() % 4 != 0 {
            out.push(0);
        }
        cursor = out.len();
    }
    out
}
fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}
fn put_string(out: &mut Vec<u8>, value: &str) {
    put_u32(out, value.len() as u32);
    out.extend_from_slice(value.as_bytes());
    while out.len() % 4 != 0 {
        out.push(0);
    }
}
fn put_attrs(out: &mut Vec<u8>, attrs: &[Attr]) {
    if attrs.is_empty() {
        put_u32(out, 0);
        return;
    }
    put_u32(out, 12);
    put_u32(out, attrs.len() as u32);
    for attr in attrs {
        put_string(out, &attr.name);
        put_u32(out, 2);
        put_u32(out, attr.value.len() as u32);
        out.extend_from_slice(attr.value.as_bytes());
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }
}
