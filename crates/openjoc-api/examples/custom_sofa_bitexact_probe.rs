//! Evidence-only public-API custom-SOFA capture.
//! Output uses the existing pcm_regression_probe manifest/PCM representation.
#[allow(dead_code)]
#[path = "../../openjoc-wasm/tests/support/sofa.rs"]
mod sofa_fixture;

use openjoc_api::{
    BinauralConfig, BinauralLfePolicy, OpenJocConfig, OpenJocPacket, OpenJocPcmFrame,
    OpenJocSession, OpenJocStatus, RenderMode,
};
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs::{self, File},
    io::{BufWriter, Write},
    path::PathBuf,
};

const INPUT_SHA256: &str = "a44fc36470d07f98c68053c9015e3cb169a21af927b1b3b93bd5712a42234671";
const INPUT_BYTES: usize = 3_842_048;
const INPUT_SAMPLE_RATE: u32 = 48_000;
const VIRTUAL_LAYOUT: &str = "5.1";

#[derive(Clone, Copy)]
struct InputUnit {
    start: usize,
    end: usize,
    samples: u16,
    sample_rate: u32,
    independent_count: usize,
    dependent_count: usize,
}

#[derive(Default)]
struct ChannelAudit {
    fnv64: u64,
    nonzero: u64,
    finite: u64,
    nonfinite: u64,
    min: f32,
    max: f32,
    initialized: bool,
}

impl ChannelAudit {
    fn update(&mut self, value: f32) {
        if self.initialized {
            self.min = self.min.min(value);
            self.max = self.max.max(value);
        } else {
            self.min = value;
            self.max = value;
            self.initialized = true;
        }
        if value != 0.0 {
            self.nonzero = self.nonzero.saturating_add(1);
        }
        if value.is_finite() {
            self.finite = self.finite.saturating_add(1);
        } else {
            self.nonfinite = self.nonfinite.saturating_add(1);
        }
        if self.fnv64 == 0 {
            self.fnv64 = 0xcbf2_9ce4_8422_2325;
        }
        for byte in value.to_bits().to_le_bytes() {
            self.fnv64 ^= u64::from(byte);
            self.fnv64 = self.fnv64.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

struct Reports {
    manifest: BufWriter<File>,
    pcm: BufWriter<File>,
    pcm_bytes: u64,
    frame_count: u64,
    sample_count: u64,
    tail_samples: u64,
    channels: Option<Vec<ChannelAudit>>,
}

impl Reports {
    fn new(prefix: &str) -> Result<Self, Box<dyn Error>> {
        let prefix = PathBuf::from(prefix);
        if let Some(parent) = prefix.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(Self {
            manifest: BufWriter::new(File::create(format!("{}.manifest.tsv", prefix.display()))?),
            pcm: BufWriter::new(File::create(format!("{}.pcm32le", prefix.display()))?),
            pcm_bytes: 0,
            frame_count: 0,
            sample_count: 0,
            tail_samples: 0,
            channels: None,
        })
    }

    fn write_header(
        &mut self,
        input_sha256: &str,
        sofa_identity: (&str, usize),
        descriptor: &str,
        fingerprint: &str,
        latency_samples: usize,
        units: &[InputUnit],
    ) -> Result<(), Box<dyn Error>> {
        let (sofa_sha256, sofa_bytes) = sofa_identity;
        writeln!(self.manifest, "openjoc-pcm-probe\t1")?;
        writeln!(self.manifest, "H\tinput_sha256\t{input_sha256}")?;
        writeln!(self.manifest, "H\tsofa_sha256\t{sofa_sha256}")?;
        writeln!(self.manifest, "H\tsofa_bytes\t{sofa_bytes}")?;
        writeln!(self.manifest, "H\tmode\tcustom-sofa")?;
        writeln!(
            self.manifest,
            "H\tlayout\t{}",
            hex(VIRTUAL_LAYOUT.as_bytes())
        )?;
        writeln!(self.manifest, "H\tlatency_samples\t{latency_samples}")?;
        writeln!(self.manifest, "H\tconfig_fingerprint\t{fingerprint}")?;
        writeln!(
            self.manifest,
            "H\tconfig_descriptor_hex\t{}",
            hex(descriptor.as_bytes())
        )?;
        writeln!(self.manifest, "H\tinput_unit_count\t{}", units.len())?;
        for (index, unit) in units.iter().enumerate() {
            writeln!(
                self.manifest,
                "AU\t{index}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                unit.start,
                unit.end - unit.start,
                unit.samples,
                unit.sample_rate,
                index * usize::from(unit.samples),
                unit.independent_count,
                unit.dependent_count,
            )?;
        }
        Ok(())
    }

    fn write_frame(&mut self, frame: &OpenJocPcmFrame, phase: &str) -> Result<(), Box<dyn Error>> {
        if frame.interleaved_f32.len() != frame.sample_count * frame.channel_count {
            return Err("frame samples do not match its channel/sample dimensions".into());
        }
        let channels = self.channels.get_or_insert_with(|| {
            (0..frame.channel_count)
                .map(|_| ChannelAudit::default())
                .collect()
        });
        if channels.len() != frame.channel_count {
            return Err("output channel count changed within capture".into());
        }
        if frame.sample_rate != INPUT_SAMPLE_RATE || frame.channel_count != 2 {
            return Err(format!(
                "unexpected custom-SOFA output format: {} Hz, {} channels",
                frame.sample_rate, frame.channel_count
            )
            .into());
        }
        let labels = frame
            .channel_labels
            .iter()
            .map(|label| hex(label.as_bytes()))
            .collect::<Vec<_>>()
            .join(",");
        let byte_length = u64::try_from(frame.interleaved_f32.len())?
            .checked_mul(4)
            .ok_or("PCM byte count overflow")?;
        writeln!(
            self.manifest,
            "F\t{}\t{phase}\t{}\t{}\t{}\t{}\t{}\t{}\t{:?}\t{}\t{}",
            self.frame_count,
            frame.sample_rate,
            frame.channel_count,
            frame.sample_count,
            frame
                .pts_samples
                .map_or_else(|| "none".to_owned(), |pts| pts.to_string()),
            hex(frame.layout_name.as_bytes()),
            labels,
            frame.render_mode,
            frame.sample_format as u8,
            self.pcm_bytes,
        )?;
        for (index, sample) in frame.interleaved_f32.iter().copied().enumerate() {
            channels[index % frame.channel_count].update(sample);
            self.pcm.write_all(&sample.to_bits().to_le_bytes())?;
        }
        self.pcm_bytes = self
            .pcm_bytes
            .checked_add(byte_length)
            .ok_or("PCM byte count overflow")?;
        self.sample_count = self
            .sample_count
            .checked_add(u64::try_from(frame.sample_count)?)
            .ok_or("sample count overflow")?;
        if phase == "drain" {
            self.tail_samples = self
                .tail_samples
                .checked_add(u64::try_from(frame.sample_count)?)
                .ok_or("tail sample count overflow")?;
        }
        self.frame_count = self.frame_count.saturating_add(1);
        Ok(())
    }

    fn finish(
        mut self,
        unit_count: usize,
        diagnostics: &str,
        terminal_state: &str,
    ) -> Result<(u64, u64, u64, u64), Box<dyn Error>> {
        self.pcm.flush()?;
        let channels = self.channels.as_deref().unwrap_or_default();
        let channel_count = channels.len();
        let expected_bytes = self
            .sample_count
            .checked_mul(u64::try_from(channel_count)?)
            .and_then(|count| count.checked_mul(4))
            .ok_or("expected PCM length overflow")?;
        if self.frame_count == 0
            || self.sample_count == 0
            || self.pcm_bytes == 0
            || self.pcm_bytes != expected_bytes
            || self.tail_samples == 0
            || channels.iter().map(|channel| channel.nonzero).sum::<u64>() == 0
            || channels.iter().any(|channel| channel.nonfinite != 0)
        {
            return Err(
                "custom-SOFA capture was empty, silent, non-finite, or wrong-length".into(),
            );
        }
        for (index, audit) in channels.iter().enumerate() {
            writeln!(
                self.manifest,
                "C\t{index}\t{}\t{}\t{}\t{}\t{:08x}\t{:08x}\t{:016x}",
                audit.nonzero,
                audit.finite,
                audit.nonfinite,
                audit.initialized,
                audit.min.to_bits(),
                audit.max.to_bits(),
                audit.fnv64,
            )?;
        }
        let distinct_channel_fingerprints = channels
            .iter()
            .map(|channel| channel.fnv64)
            .collect::<std::collections::HashSet<_>>()
            .len();
        writeln!(self.manifest, "R\tinput_units\t{unit_count}")?;
        writeln!(self.manifest, "R\toutput_frames\t{}", self.frame_count)?;
        writeln!(self.manifest, "R\tpcm_bytes\t{}", self.pcm_bytes)?;
        writeln!(self.manifest, "R\tsample_count\t{}", self.sample_count)?;
        writeln!(self.manifest, "R\ttail_samples\t{}", self.tail_samples)?;
        writeln!(self.manifest, "R\tchannel_count\t{channel_count}")?;
        writeln!(
            self.manifest,
            "R\tdistinct_channel_fingerprints\t{distinct_channel_fingerprints}"
        )?;
        writeln!(
            self.manifest,
            "R\tdiagnostics_hex\t{}",
            hex(diagnostics.as_bytes())
        )?;
        writeln!(self.manifest, "R\tterminal_state\t{terminal_state}")?;
        self.manifest.flush()?;
        Ok((
            self.frame_count,
            self.sample_count,
            self.pcm_bytes,
            self.tail_samples,
        ))
    }
}

fn generate_fixture(path: &str) -> Result<(), Box<dyn Error>> {
    let bytes = sofa_fixture::custom_hdf5_sofa_fixture(INPUT_SAMPLE_RATE, false);
    fs::write(path, &bytes)?;
    println!(
        "sofa_bytes={}\nsofa_sha256={}",
        bytes.len(),
        sha256_hex(&bytes)
    );
    Ok(())
}

fn capture(input_path: &str, sofa_path: &str, output_prefix: &str) -> Result<(), Box<dyn Error>> {
    let input = fs::read(input_path)?;
    let input_sha256 = sha256_hex(&input);
    if input.len() != INPUT_BYTES || input_sha256 != INPUT_SHA256 {
        return Err(format!(
            "input corpus identity mismatch: {} bytes, {input_sha256}",
            input.len()
        )
        .into());
    }
    let sofa_bytes = fs::read(sofa_path)?;
    if sofa_bytes.is_empty() {
        return Err("SOFA bytes are empty".into());
    }
    let sofa_sha256 = sha256_hex(&sofa_bytes);
    let units = index_units(&input)?;
    if units.is_empty()
        || units
            .iter()
            .any(|unit| unit.sample_rate != INPUT_SAMPLE_RATE)
    {
        return Err("custom-SOFA input must contain nonempty 48-kHz JOC access units".into());
    }

    let config = OpenJocConfig {
        render_mode: RenderMode::Binaural,
        speaker_layout: VIRTUAL_LAYOUT.to_owned(),
        binaural: Some(BinauralConfig::from_sofa_bytes(
            sofa_bytes.clone(),
            VIRTUAL_LAYOUT,
            BinauralLfePolicy::EqualPowerDualMono,
        )),
        ..OpenJocConfig::default()
    };
    let mut session = OpenJocSession::new(config)?;
    let descriptor = session.effective_config_descriptor();
    let expected_hrtf =
        format!("binaural_hrtf_source=custom-sofa-bytes\nbinaural_hrtf_sha256={sofa_sha256}");
    if !descriptor.contains(&expected_hrtf) {
        return Err("effective config descriptor does not identify the custom SOFA hash".into());
    }
    let fingerprint = session.effective_config_fingerprint();
    let mut reports = Reports::new(output_prefix)?;
    reports.write_header(
        &input_sha256,
        (&sofa_sha256, sofa_bytes.len()),
        &descriptor,
        &fingerprint,
        session.latency_samples(),
        &units,
    )?;

    let mut sample_offset = 0_u64;
    for unit in &units {
        let pts = i64::try_from(sample_offset)?;
        let packet = OpenJocPacket {
            data: &input[unit.start..unit.end],
            pts_samples: Some(pts),
            discontinuity: false,
            preroll: false,
        };
        let status = session.push_packet(packet)?;
        if status == OpenJocStatus::OutputPending {
            return Err("unexpected backpressure before AU output was received".into());
        }
        receive_programme_frames(&mut session, &mut reports)?;
        sample_offset = sample_offset
            .checked_add(u64::from(unit.samples))
            .ok_or("input PTS range overflow")?;
    }

    loop {
        let status = session.drain()?;
        receive_drain_frames(&mut session, &mut reports)?;
        if session.is_drained() {
            if status == OpenJocStatus::EndOfStream {
                break;
            }
            continue;
        }
        if status == OpenJocStatus::EndOfStream {
            return Err("drain reported EOS before the terminal state".into());
        }
    }
    let repeated = session.drain()?;
    if repeated != OpenJocStatus::EndOfStream || !session.is_drained() {
        return Err("repeated drain did not preserve terminal EOS".into());
    }
    let diagnostics = format!("{:?}", session.diagnostics());
    let (output_frames, output_samples, pcm_bytes, tail_samples) =
        reports.finish(units.len(), &diagnostics, "drained-repeated-drain-eos")?;
    println!(
        "input_sha256={input_sha256}\nsofa_sha256={sofa_sha256}\ninput_units={}\noutput_frames={output_frames}\noutput_samples={output_samples}\npcm_bytes={pcm_bytes}\ntail_samples={tail_samples}",
        units.len(),
    );
    Ok(())
}

fn receive_programme_frames(
    session: &mut OpenJocSession,
    reports: &mut Reports,
) -> Result<(), Box<dyn Error>> {
    while let Some(frame) = session.receive_frame() {
        reports.write_frame(&frame, "programme")?;
    }
    Ok(())
}

fn receive_drain_frames(
    session: &mut OpenJocSession,
    reports: &mut Reports,
) -> Result<(), Box<dyn Error>> {
    while let Some(frame) = session.receive_frame() {
        reports.write_frame(&frame, "drain")?;
    }
    Ok(())
}

fn index_units(input: &[u8]) -> Result<Vec<InputUnit>, Box<dyn Error>> {
    let frames = openjoc_eac3::index_syncframes(input)?;
    let units = openjoc_eac3::group_access_units(&frames)?;
    units
        .into_iter()
        .map(|unit| {
            let first = *frames
                .get(unit.first_frame)
                .ok_or("invalid access-unit start")?;
            let last_index = unit
                .first_frame
                .checked_add(unit.frame_count)
                .and_then(|end| end.checked_sub(1))
                .ok_or("invalid access-unit frame range")?;
            let last = *frames.get(last_index).ok_or("invalid access-unit end")?;
            let start = first.offset;
            let end = last
                .offset
                .checked_add(last.header.frame_size)
                .ok_or("access-unit byte range overflow")?;
            let independent_count = frames[unit.first_frame..=last_index]
                .iter()
                .filter(|frame| frame.header.stream_type != openjoc_eac3::StreamType::Dependent)
                .count();
            Ok(InputUnit {
                start,
                end,
                samples: unit.samples,
                sample_rate: unit.sample_rate,
                independent_count,
                dependent_count: unit.frame_count - independent_count,
            })
        })
        .collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    format!("{:x}", digest.finalize())
}

fn hex(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(result, "{byte:02x}");
    }
    result
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [mode, path] if mode == "generate-sofa" => generate_fixture(path),
        [mode, input, sofa, output_prefix] if mode == "capture" => {
            capture(input, sofa, output_prefix)
        }
        _ => Err("usage: custom_sofa_bitexact_probe generate-sofa SOFA.h5 | capture INPUT.ec3 SOFA.h5 OUTPUT_PREFIX".into()),
    }
}
