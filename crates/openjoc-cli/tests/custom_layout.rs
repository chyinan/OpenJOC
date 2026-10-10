use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const FRAMES: usize = 6 * 1536 + 32;

struct Fixture {
    root: PathBuf,
    input: PathBuf,
    topology: PathBuf,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "openjoc-custom-layout-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let input = root.join("input.ec3");
        // Complete, unchanged public AUs with ordinary sequence counts 1..=6.
        // No compressed metadata, decoded PCM, or reference oracle is modified.
        fs::write(
            &input,
            include_bytes!("../../openjoc-api/tests/fixtures/timestamps.ec3"),
        )
        .unwrap();
        let topology = root.join("topology.json");
        let records: Vec<_> = ["FL", "FR", "FC", "Ls", "Rs", "FC"]
            .into_iter()
            .map(|identity| {
                json!({
                    "descriptor": {
                        "source_class": "explicit_channel", "identity": identity,
                        "coordinates": [0.5, 0.5, 0.0], "raw3": [3], "channel_lock": false
                    },
                    "active": true, "scalar": 1.0
                })
            })
            .collect();
        fs::write(
            &topology,
            serde_json::to_vec(&json!({
                "schema": "openjoc.joc-render-control.v1",
                "topology": {"explicit_groups": [], "fixed_layout": [], "dynamic_records": records},
                "route_vectors": [], "updates": []
            }))
            .unwrap(),
        )
        .unwrap();
        Self {
            root,
            input,
            topology,
        }
    }

    fn layout(&self, filename: &str, document: &Value) -> PathBuf {
        let path = self.root.join(filename);
        fs::write(&path, serde_json::to_vec(document).unwrap()).unwrap();
        path
    }

    fn render(&self, layout: &Path, label: &str, extension: &str, options: &[&str]) -> Rendered {
        let output = self.root.join(format!("{label}.{extension}"));
        let mut command = Command::new(env!("CARGO_BIN_EXE_openjoc"));
        command
            .arg("render-joc")
            .arg(&self.input)
            .arg("--layout-file")
            .arg(layout)
            .arg("--output")
            .arg(&output)
            .args(["--validation-profile", "etsi-strict", "--no-progress"]);
        for option in options {
            match *option {
                "--topology" => {
                    command.arg(option).arg(&self.topology);
                }
                "--performance-report" => {
                    command
                        .arg(option)
                        .arg(self.root.join(format!("{label}-{extension}.json")));
                }
                _ => {
                    command.arg(option);
                }
            }
        }
        let result = command.output().expect("run render-joc");
        assert!(
            result.status.success(),
            "{command:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let stdout = String::from_utf8(result.stdout).unwrap();
        assert!(stdout.contains("selected profile: ETSI_STRICT"), "{stdout}");
        assert!(stdout.contains(&format!("frames: {FRAMES}")), "{stdout}");
        let bytes = fs::read(output).unwrap();
        if options.contains(&"--performance-report") {
            let report: Value = serde_json::from_slice(
                &fs::read(self.root.join(format!("{label}-{extension}.json"))).unwrap(),
            )
            .unwrap();
            assert_eq!(report["output_frames"], FRAMES);
            assert_eq!(report["output_bytes"], bytes.len());
            assert_eq!(report["selected_validation_profile"], "ETSI_STRICT");
        }
        let (channels, descriptions) = if extension == "wav" {
            let wave = openjoc_wave::decode(&bytes).unwrap();
            assert_eq!(wave.sample_rate, 48_000);
            assert_eq!(wave.channel_mask, None, "custom WAV must remain unmasked");
            (wave.channels, Vec::new())
        } else {
            let desc = caf_chunk(&bytes, *b"desc");
            assert_eq!(f64::from_be_bytes(desc[..8].try_into().unwrap()), 48_000.0);
            assert_eq!(&desc[8..12], b"lpcm");
            assert_eq!(u32::from_be_bytes(desc[28..32].try_into().unwrap()), 32);
            let count = u32::from_be_bytes(desc[24..28].try_into().unwrap()) as usize;
            let chan = caf_chunk(&bytes, *b"chan");
            assert_eq!(
                u32::from_be_bytes(chan[8..12].try_into().unwrap()) as usize,
                count
            );
            let descriptions = chan[12..].chunks_exact(20).map(<[u8]>::to_vec).collect();
            let data = caf_chunk(&bytes, *b"data");
            assert_eq!(&data[..4], &[0; 4]);
            assert_eq!(data[4..].len(), FRAMES * count * 4);
            let mut channels = vec![Vec::new(); count];
            for frame in data[4..].chunks_exact(count * 4) {
                for (channel, sample) in channels.iter_mut().zip(frame.chunks_exact(4)) {
                    channel.push(f64::from(f32::from_le_bytes(sample.try_into().unwrap())));
                }
            }
            (channels, descriptions)
        };
        for channel in &channels {
            assert_eq!(channel.len(), FRAMES);
            assert!(channel.iter().all(|sample| sample.is_finite()));
        }
        assert!(
            channels.iter().flatten().any(|sample| *sample != 0.0),
            "non-silent fixture"
        );
        Rendered {
            bytes,
            channels,
            descriptions,
            stdout,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct Rendered {
    bytes: Vec<u8>,
    channels: Vec<Vec<f64>>,
    descriptions: Vec<Vec<u8>>,
    stdout: String,
}

fn caf_chunk(bytes: &[u8], name: [u8; 4]) -> &[u8] {
    assert_eq!(&bytes[..4], b"caff");
    let mut position = 8;
    while position < bytes.len() {
        let size = usize::try_from(i64::from_be_bytes(
            bytes[position + 4..position + 12].try_into().unwrap(),
        ))
        .unwrap();
        let start = position + 12;
        if bytes[position..position + 4] == name {
            return &bytes[start..start + size];
        }
        position = start + size;
    }
    panic!("missing CAF chunk {name:?}");
}

fn irregular() -> Value {
    serde_json::from_str(include_str!(
        "../../../fixtures/speaker-layouts/studio-irregular.json"
    ))
    .unwrap()
}

fn six_channels() -> Value {
    let mut layout = irregular();
    layout["name"] = json!("six-channel-studio");
    layout["speakers"].as_array_mut().unwrap().extend([
        json!({"name": "Rear-left", "azimuth": -110.0, "elevation": 0.0}),
        json!({"name": "Rear-right", "azimuth": 110.0, "elevation": 0.0}),
    ]);
    layout
}

fn assert_channels_bits_eq(actual: &[Vec<f64>], expected: &[Vec<f64>], order: &[usize]) {
    assert_eq!(actual.len(), order.len());
    for (actual_index, &expected_index) in order.iter().enumerate() {
        let actual = &actual[actual_index];
        let expected = &expected[expected_index];
        assert_eq!(actual.len(), expected.len());
        for (sample, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "channel {actual_index}, sample {sample}"
            );
        }
    }
}

#[test]
fn custom_layout_performance_report_preserves_session_output_bytes() {
    let fixture = Fixture::new("performance");
    for (index, mut document) in [irregular(), irregular(), six_channels()]
        .into_iter()
        .enumerate()
    {
        if index != 0 {
            document["name"] = json!("5.1");
        }
        let count = document["speakers"].as_array().unwrap().len();
        let layout = fixture.layout(&format!("layout-{index}.json"), &document);
        for extension in ["wav", "caf"] {
            let session = fixture.render(&layout, &format!("session-{index}"), extension, &[]);
            assert!(session.stdout.contains("render session: OpenJocSession"));
            let legacy = fixture.render(
                &layout,
                &format!("legacy-{index}"),
                extension,
                &["--performance-report"],
            );
            assert_eq!(legacy.channels.len(), count);
            // Independent embedded-session path; compare entire containers,
            // including every PCM bit, channel metadata, and final gain drain.
            assert!(
                legacy.bytes == session.bytes,
                "complete output bytes differ for {index}/{extension}"
            );
        }
    }
}

#[test]
fn custom_layout_stereo_display_name_preserves_speaker_mode() {
    let fixture = Fixture::new("stereo-name");
    let mut document = irregular();
    let reference_layout = fixture.layout("reference.json", &document);
    document["name"] = json!("2.0");
    let named_layout = fixture.layout("named-stereo.json", &document);
    for normalize in [false, true] {
        let options = if normalize {
            vec!["--normalize-peak", "-10"]
        } else {
            Vec::new()
        };
        for extension in ["wav", "caf"] {
            let reference = fixture.render(
                &reference_layout,
                &format!("reference-{normalize}"),
                extension,
                &options,
            );
            let named = fixture.render(
                &named_layout,
                &format!("named-{normalize}"),
                extension,
                &options,
            );
            assert_eq!(named.channels.len(), 4);
            assert!(
                named.bytes == reference.bytes,
                "display-name change altered output for {normalize}/{extension}"
            );
            assert!(
                named
                    .stdout
                    .contains("output channel order: FL, FC, FR, LFE-A")
            );
        }
    }
}

#[test]
fn custom_layout_sidecar_and_diagnostics_preserve_wav_caf_channel_order() {
    let fixture = Fixture::new("containers");
    let mut document = irregular();
    let layout = fixture.layout("studio.json", &document);
    document["name"] = json!("5.1");
    let collision_layout = fixture.layout("collision.json", &document);
    for (index, options) in [
        vec!["--topology"],
        vec!["--diagnostic-contribution", "base-only"],
        vec!["--diagnostic-contribution", "reconstruction-only"],
    ]
    .iter()
    .enumerate()
    {
        let wav = fixture.render(&layout, &format!("wav-{index}"), "wav", options);
        let caf = fixture.render(&layout, &format!("caf-{index}"), "caf", options);
        for (extension, expected) in [("wav", &wav), ("caf", &caf)] {
            let collision = fixture.render(
                &collision_layout,
                &format!("collision-{index}"),
                extension,
                options,
            );
            // A four-channel custom display name must not select six-channel
            // preset PCM, even for WAV where the writer accepts either count.
            assert_eq!(collision.channels.len(), 4);
            assert!(
                collision.bytes == expected.bytes,
                "display-name collision altered output for {index}/{extension}"
            );
        }
        assert_channels_bits_eq(&caf.channels, &wav.channels, &[0, 1, 2, 3]);
        assert!(
            caf.stdout
                .contains("output channel order: FL, FC, FR, LFE-A")
        );
        assert!(caf.stdout.contains("LFE index: Some(3)"));
        let labels: Vec<_> = caf
            .descriptions
            .iter()
            .map(|item| u32::from_be_bytes(item[..4].try_into().unwrap()))
            .collect();
        assert_eq!(labels, [100, 100, 100, 4]);
    }
}

#[test]
fn custom_layout_preset_name_collision_preserves_reordered_lfe_with_and_without_normalization() {
    let fixture = Fixture::new("collision");
    let reference_document = six_channels();
    let reference_layout = fixture.layout("reference.json", &reference_document);
    let mut moved_document = reference_document;
    moved_document["name"] = json!("5.1");
    let speakers = moved_document["speakers"].as_array_mut().unwrap();
    let lfe = speakers.remove(3);
    speakers.insert(0, lfe);
    let moved_layout = fixture.layout("collision.json", &moved_document);
    for (index, options) in [
        vec!["--topology"],
        vec!["--topology", "--diagnostic-contribution", "base-only"],
        vec![
            "--topology",
            "--diagnostic-contribution",
            "reconstruction-only",
        ],
        vec!["--diagnostic-contribution", "base-only"],
        vec!["--diagnostic-contribution", "reconstruction-only"],
    ]
    .iter()
    .enumerate()
    {
        for normalize in [false, true] {
            let mut options = options.clone();
            if normalize {
                options.extend(["--normalize-peak", "-10"]);
            }
            for extension in ["wav", "caf"] {
                let reference = fixture.render(
                    &reference_layout,
                    &format!("reference-{index}-{normalize}"),
                    extension,
                    &options,
                );
                let moved = fixture.render(
                    &moved_layout,
                    &format!("moved-{index}-{normalize}"),
                    extension,
                    &options,
                );
                // Only move LFE; preserve the relative order of all projection
                // vertices, so every full-band sample must remain bit-exact.
                let order = [3, 0, 1, 2, 4, 5];
                assert_channels_bits_eq(&moved.channels, &reference.channels, &order);
                assert!(moved.stdout.contains("LFE index: Some(0)"));
                assert!(
                    moved
                        .stdout
                        .contains("output channel order: LFE-A, FL, FC, FR, Rear-left, Rear-right")
                );
                assert!(
                    moved.channels[1..].iter().any(|full_band| {
                        moved.channels[0]
                            .iter()
                            .zip(full_band)
                            .any(|(lfe, full_band)| lfe.to_bits() != full_band.to_bits())
                    }),
                    "LFE and full-band PCM must be distinguishable for {index}/{normalize}/{extension}"
                );
                assert!(
                    moved.channels[1..]
                        .iter()
                        .flatten()
                        .any(|sample| *sample != 0.0)
                );
                if extension == "caf" {
                    for (actual, expected) in moved.descriptions.iter().zip(order) {
                        assert_eq!(*actual, reference.descriptions[expected]);
                    }
                    assert_eq!(&moved.descriptions[0][..4], &4_u32.to_be_bytes());
                }
            }
        }
    }
}
