//! Complete file serialization and publication contract, independent of DSP.

use super::{JOC_RENDER_SUPPORTED_LAYOUTS, JocPcmOutput, JocRenderError, RenderedBlock};
use openjoc_scene::SpeakerLayoutPreset;
use openjoc_wave::SampleFormat;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write as _,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "openjoc-output-contract-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn block(channels: usize, start: usize, samples: usize) -> RenderedBlock {
    let values = [
        0_u64,
        0x8000_0000_0000_0000, // preserve negative zero for floating-point output
        0x3fb9_9999_9999_999a,
        0xbfb9_9999_9999_999a,
        0x3fe0_0000_0000_0001, // distinguish the F64 input from an F32 roundtrip
        0xbfe0_0000_0000_0001,
        0x3fd0_0000_0000_0000,
    ];
    RenderedBlock {
        sample_rate: 48_000,
        channels: (0..channels)
            .map(|channel| {
                (start..start + samples)
                    .map(|sample| f64::from_bits(values[(sample + channel) % values.len()]))
                    .collect()
            })
            .collect(),
    }
}

#[test]
fn complete_output_files_preserve_frozen_bytes_and_publish_only_on_finish() {
    let scratch = Scratch::new();
    let mut records = BTreeMap::new();
    for layout in JOC_RENDER_SUPPORTED_LAYOUTS.into_iter().chain(["binaural"]) {
        for extension in ["wav", "caf"] {
            if extension == "wav" && matches!(layout, "7.1.6" | "9.1" | "9.1.2" | "9.1.4" | "9.1.6")
            {
                continue;
            }
            for (name, format) in [
                ("f32", SampleFormat::F32),
                ("f64", SampleFormat::F64),
                ("s24", SampleFormat::S24),
                ("s16", SampleFormat::S16),
            ] {
                let filename = format!("{layout}-{name}.{extension}");
                let path = scratch.0.join(&filename);
                let staging = scratch.0.join(format!(".{filename}.openjoc-partial"));
                let (mut writer, channels) = if layout == "binaural" {
                    (
                        JocPcmOutput::new_for_binaural(&path, format, false).unwrap(),
                        2,
                    )
                } else {
                    (
                        JocPcmOutput::new_for_speaker_layout(&path, format, false, layout).unwrap(),
                        SpeakerLayoutPreset::for_name(layout)
                            .unwrap()
                            .channel_count(),
                    )
                };
                assert!(!path.exists() && !staging.exists());
                writer.write_block(&block(channels, 0, 7)).unwrap();
                assert!(!path.exists() && staging.exists());
                assert_eq!(writer.frames(), 7);
                writer.write_block(&block(channels, 7, 10)).unwrap();
                assert!(!path.exists() && staging.exists());
                assert_eq!(writer.frames(), 17);
                writer.finish().unwrap();
                assert!(path.exists() && !staging.exists());
                assert_eq!(writer.frames(), 0);
                assert_eq!(
                    writer.finish().unwrap_err().to_string(),
                    "JOC render produced no audio frames"
                );
                let bytes = fs::read(&path).unwrap();
                records.insert(
                    filename.clone(),
                    serde_json::json!({
                        "bytes": bytes.len(), "sha256": format!("{:x}", Sha256::digest(&bytes))
                    }),
                );
                if let Some(directory) = std::env::var_os("OPENJOC_CLI_OUTPUT_CAPTURE") {
                    fs::File::create_new(PathBuf::from(directory).join(filename))
                        .unwrap()
                        .write_all(&bytes)
                        .unwrap();
                }
            }
        }
    }
    assert_eq!(records.len(), 92);
    let frozen: BTreeMap<String, serde_json::Value> =
        serde_json::from_str(include_str!("../tests/fixtures/output-contract.json")).unwrap();
    assert_eq!(records, frozen, "complete pre-extraction container bytes");
    if let Some(directory) = std::env::var_os("OPENJOC_CLI_OUTPUT_CAPTURE") {
        fs::File::create_new(PathBuf::from(directory).join("manifest.json"))
            .unwrap()
            .write_all(serde_json::to_string_pretty(&records).unwrap().as_bytes())
            .unwrap();
    }
}

#[test]
fn output_publication_failure_and_abort_preserve_existing_files() {
    let scratch = Scratch::new();
    for extension in ["wav", "caf"] {
        let path = scratch.0.join(format!("race.{extension}"));
        let staging = scratch.0.join(format!(".race.{extension}.openjoc-partial"));
        let mut writer = JocPcmOutput::new_for_binaural(&path, SampleFormat::F64, false).unwrap();
        writer.write_block(&block(2, 0, 7)).unwrap();
        fs::write(&path, b"late-existing-output").unwrap();
        let error = writer.finish().unwrap_err();
        assert!(
            matches!(&error, JocRenderError::Io(error) if error.kind() == std::io::ErrorKind::AlreadyExists)
        );
        assert_eq!(
            error.to_string(),
            format!(
                "JOC render I/O error: refusing to overwrite output {}",
                path.display()
            )
        );
        assert_eq!(fs::read(&path).unwrap(), b"late-existing-output");
        assert!(!staging.exists());

        let mut writer = JocPcmOutput::new_for_binaural(&path, SampleFormat::F64, true).unwrap();
        writer.write_block(&block(2, 0, 7)).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"late-existing-output");
        writer.abort();
        assert!(!staging.exists());
        assert_eq!(fs::read(&path).unwrap(), b"late-existing-output");

        let mut writer = JocPcmOutput::new_for_binaural(&path, SampleFormat::F64, true).unwrap();
        writer.write_block(&block(2, 0, 7)).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"late-existing-output");
        writer.finish().unwrap();
        assert_ne!(fs::read(&path).unwrap(), b"late-existing-output");
        assert!(!staging.exists());

        let blocked = scratch.0.join(format!("blocked.{extension}"));
        let partial = scratch
            .0
            .join(format!(".blocked.{extension}.openjoc-partial"));
        fs::write(&partial, b"preexisting-staging-file").unwrap();
        let result = JocPcmOutput::new_for_binaural(&blocked, SampleFormat::F32, true);
        let Err(error) = result else {
            panic!("existing staging file must be refused")
        };
        assert!(matches!(error, JocRenderError::OutputExists(ref value) if value == &partial));
        assert_eq!(fs::read(&partial).unwrap(), b"preexisting-staging-file");
        assert!(!blocked.exists());
    }
}
