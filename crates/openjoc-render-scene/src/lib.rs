//! Portable, explicit static-scene binaural rendering.
//!
//! This crate intentionally contains only caller-bound source semantics.  It
//! never accepts decoder objects, ReconstructionBasis rows, or OAMD slots.

use openjoc_render::{
    BinauralRenderer, BinauralSourceBlock, CartesianPosition, PartitionedBinauralRenderer,
    StaticBinauralSource, UniformPartitionedConfig,
};
use openjoc_sofa::{LoadedSofaHrirBank, SofaError, SofaLoadLimits, load_simple_free_field_hrir};
use openjoc_wave::{Clipping, Dither, SampleFormat, WaveEncodeOptions, WaveError, WaveWriter};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
};

pub const SCENE_SCHEMA: &str = "openjoc.render-scene.v1";
pub const RESULT_SCHEMA: &str = "openjoc.render-result.v1";
const MAX_RENDER_BLOCK_SAMPLES: usize = 65_536;
const MAX_WAV_FMT_BYTES: u64 = 4_096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderSceneLoadLimits {
    pub max_scene_bytes: u64,
    pub max_sources: usize,
    pub max_source_id_bytes: usize,
    pub max_path_bytes: usize,
    pub max_start_sample: u64,
}

impl Default for RenderSceneLoadLimits {
    fn default() -> Self {
        Self {
            max_scene_bytes: 4 * 1024 * 1024,
            max_sources: 256,
            max_source_id_bytes: 256,
            max_path_bytes: 4096,
            max_start_sample: u64::MAX - 1_000_000,
        }
    }
}

#[derive(Debug)]
pub enum RenderSceneError {
    Io(String),
    Json(String),
    InvalidScene(&'static str),
    InvalidSource(String),
    PathEscape(String),
    SourceCount,
    DuplicateSourceId(String),
    UnsupportedWav(String),
    Wav(String),
    SampleRateMismatch { expected: u32, actual: u32 },
    SourceTimelineOverflow,
    Sofa(String),
    Render(String),
    OutputExists,
    OutputFailure(String),
    NonFiniteOutput,
}
impl std::fmt::Display for RenderSceneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(v) => write!(f, "I/O error: {v}"),
            Self::Json(v) => write!(f, "scene JSON error: {v}"),
            Self::InvalidScene(v) => write!(f, "invalid render scene: {v}"),
            Self::InvalidSource(v) => write!(f, "invalid source: {v}"),
            Self::PathEscape(v) => write!(f, "source path escapes scene root: {v}"),
            Self::SourceCount => f.write_str("source count exceeds limit"),
            Self::DuplicateSourceId(v) => write!(f, "duplicate source id: {v}"),
            Self::UnsupportedWav(v) => write!(f, "unsupported WAV: {v}"),
            Self::Wav(v) => write!(f, "WAV error: {v}"),
            Self::SampleRateMismatch { expected, actual } => {
                write!(f, "sample-rate mismatch: expected {expected}, got {actual}")
            }
            Self::SourceTimelineOverflow => f.write_str("source timeline overflow"),
            Self::Sofa(v) => write!(f, "SOFA error: {v}"),
            Self::Render(v) => write!(f, "render error: {v}"),
            Self::OutputExists => f.write_str("output directory already exists"),
            Self::OutputFailure(v) => write!(f, "transactional output failed: {v}"),
            Self::NonFiniteOutput => f.write_str("non-finite output"),
        }
    }
}
impl std::error::Error for RenderSceneError {}
impl From<io::Error> for RenderSceneError {
    fn from(e: io::Error) -> Self {
        Self::Io(e.to_string())
    }
}
impl From<WaveError> for RenderSceneError {
    fn from(e: WaveError) -> Self {
        Self::Wav(e.to_string())
    }
}
impl From<openjoc_render::RenderError> for RenderSceneError {
    fn from(e: openjoc_render::RenderError) -> Self {
        Self::Render(e.to_string())
    }
}
impl From<SofaError> for RenderSceneError {
    fn from(e: SofaError) -> Self {
        Self::Sofa(e.to_string())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RenderScene {
    pub schema: String,
    pub sample_rate_hz: u32,
    pub source_semantics: String,
    pub sources: Vec<SceneSource>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SceneSource {
    pub id: String,
    pub input_wav: String,
    #[serde(default)]
    pub start_sample: u64,
    pub position: ScenePosition,
    pub gain: f64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenePosition {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderBackend {
    Direct,
    Partitioned { partition_size: usize },
}

pub struct RenderRequest {
    pub scene_path: PathBuf,
    pub sofa_path: PathBuf,
    pub output_dir: PathBuf,
    pub backend: RenderBackend,
    pub block_size: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct RenderResult {
    pub schema: &'static str,
    pub product_version: &'static str,
    pub scene_schema: &'static str,
    pub source_semantics: &'static str,
    pub joc_semantic_binding: &'static str,
    pub reconstruction_basis_input: bool,
    pub scene_sha256: String,
    pub sources: Vec<RenderSourceResult>,
    pub sofa_file: String,
    pub sofa_sha256: String,
    pub sofa_bytes: u64,
    pub sofa_convention_version: String,
    pub sofa_sample_rate_hz: u32,
    pub sofa_measurement_count: usize,
    pub resolved_measurements: Vec<usize>,
    pub backend: String,
    pub partition_size: Option<usize>,
    pub algorithmic_latency_samples: usize,
    pub scene_input_length: u64,
    pub hrir_max_tap_count: usize,
    pub tail_samples: usize,
    pub output_sample_count: u64,
    pub output_wav: &'static str,
    pub output_format: &'static str,
    pub output_bytes: u64,
    pub output_sha256: String,
    pub completion_status: &'static str,
}
#[derive(Clone, Debug, Serialize)]
pub struct RenderSourceResult {
    pub id: String,
    pub input_wav: String,
    pub start_sample: u64,
    pub sample_count: u64,
    pub gain: f64,
    pub direction: ScenePosition,
    pub sha256: String,
    pub resolved_measurement: usize,
}

pub fn load_scene(
    path: &Path,
    limits: RenderSceneLoadLimits,
) -> Result<(RenderScene, Vec<PathBuf>, String), RenderSceneError> {
    let bytes = fs::read(path)?;
    if bytes.len() as u64 > limits.max_scene_bytes {
        return Err(RenderSceneError::InvalidScene(
            "scene JSON exceeds load limit",
        ));
    }
    let scene: RenderScene =
        serde_json::from_slice(&bytes).map_err(|e| RenderSceneError::Json(e.to_string()))?;
    if scene.schema != SCENE_SCHEMA {
        return Err(RenderSceneError::InvalidScene("unsupported schema"));
    }
    if scene.source_semantics != "explicit_spatial_sources" {
        return Err(RenderSceneError::InvalidScene(
            "source_semantics must be explicit_spatial_sources",
        ));
    }
    if scene.sample_rate_hz == 0 {
        return Err(RenderSceneError::InvalidScene(
            "sample_rate_hz must be nonzero",
        ));
    }
    if scene.sources.is_empty() || scene.sources.len() > limits.max_sources {
        return Err(RenderSceneError::SourceCount);
    }
    let root = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()?;
    let mut ids = HashSet::new();
    let mut resolved = Vec::with_capacity(scene.sources.len());
    for source in &scene.sources {
        if source.id.is_empty() || source.id.len() > limits.max_source_id_bytes {
            return Err(RenderSceneError::InvalidSource("id length".into()));
        }
        if !ids.insert(source.id.clone()) {
            return Err(RenderSceneError::DuplicateSourceId(source.id.clone()));
        }
        if source.input_wav.is_empty() || source.input_wav.len() > limits.max_path_bytes {
            return Err(RenderSceneError::InvalidSource("input_wav length".into()));
        }
        if source.start_sample > limits.max_start_sample || !source.gain.is_finite() {
            return Err(RenderSceneError::InvalidSource(source.id.clone()));
        }
        if !source.position.x.is_finite()
            || !source.position.y.is_finite()
            || !source.position.z.is_finite()
            || (source.position.x * source.position.x
                + source.position.y * source.position.y
                + source.position.z * source.position.z)
                <= 0.0
        {
            return Err(RenderSceneError::InvalidSource(source.id.clone()));
        }
        let rel = Path::new(&source.input_wav);
        if rel.is_absolute()
            || rel.components().any(|c| {
                matches!(
                    c,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(RenderSceneError::PathEscape(source.input_wav.clone()));
        }
        let candidate = root.join(rel);
        let canonical = candidate
            .canonicalize()
            .map_err(|_| RenderSceneError::PathEscape(source.input_wav.clone()))?;
        if !canonical.starts_with(&root) {
            return Err(RenderSceneError::PathEscape(source.input_wav.clone()));
        }
        resolved.push(canonical);
    }
    Ok((scene, resolved, hex_hash(&bytes)))
}

pub fn inspect_sofa(path: &Path) -> Result<serde_json::Value, RenderSceneError> {
    let loaded = load_simple_free_field_hrir(path, SofaLoadLimits::default())?;
    Ok(
        serde_json::json!({"schema":"openjoc.sofa-inspect.v1", "file": path.file_name().and_then(|x| x.to_str()).unwrap_or(""), "convention_version":loaded.metadata.convention_version, "sample_rate_hz":loaded.metadata.sample_rate_hz, "measurement_count":loaded.metadata.measurement_count, "original_fir_length":loaded.metadata.original_fir_length, "expanded_max_tap_length":loaded.metadata.expanded_max_tap_length, "measurements": loaded.bank.entries().iter().enumerate().map(|(i,e)| serde_json::json!({"measurement_index":i,"direction":e.direction(),"tap_count":e.pair().tap_count()})).collect::<Vec<_>>() }),
    )
}

pub fn render(request: &RenderRequest) -> Result<RenderResult, RenderSceneError> {
    let (scene, paths, scene_hash) =
        load_scene(&request.scene_path, RenderSceneLoadLimits::default())?;
    if request.output_dir.exists() {
        return Err(RenderSceneError::OutputExists);
    }
    if request.block_size == 0 || request.block_size > MAX_RENDER_BLOCK_SAMPLES {
        return Err(RenderSceneError::InvalidScene(
            "block size outside bounded range",
        ));
    }
    let sofa_bytes = fs::read(&request.sofa_path)?;
    let sofa_hash = hex_hash(&sofa_bytes);
    let sofa = openjoc_sofa::parse_simple_free_field_hrir(&sofa_bytes, SofaLoadLimits::default())?;
    if sofa.metadata.sample_rate_hz != scene.sample_rate_hz {
        return Err(RenderSceneError::SampleRateMismatch {
            expected: scene.sample_rate_hz,
            actual: sofa.metadata.sample_rate_hz,
        });
    }
    let mut sources = Vec::with_capacity(scene.sources.len());
    let mut readers = Vec::with_capacity(scene.sources.len());
    let mut source_results = Vec::with_capacity(scene.sources.len());
    let mut scene_end = 0_u64;
    let mut max_taps = 0_usize;
    for (source, path) in scene.sources.iter().zip(paths.iter()) {
        let wav = StreamingWav::open(path, scene.sample_rate_hz)?;
        let end = source
            .start_sample
            .checked_add(wav.frames)
            .ok_or(RenderSceneError::SourceTimelineOverflow)?;
        scene_end = scene_end.max(end);
        let entry = sofa.bank.resolve_exact(CartesianPosition::new(
            source.position.x,
            source.position.y,
            source.position.z,
        ))?;
        let measurement = sofa
            .bank
            .entries()
            .iter()
            .position(|e| e.id() == entry.id())
            .unwrap_or(0);
        max_taps = max_taps.max(entry.pair().tap_count());
        let id = openjoc_render::SourceId::new(
            u64::try_from(source_results.len()).map_err(|_| RenderSceneError::SourceCount)? + 1,
        );
        sources.push(StaticBinauralSource::new(
            id,
            CartesianPosition::new(source.position.x, source.position.y, source.position.z),
            source.gain,
            entry.id(),
        )?);
        source_results.push(RenderSourceResult {
            id: source.id.clone(),
            input_wav: source.input_wav.clone(),
            start_sample: source.start_sample,
            sample_count: wav.frames,
            gain: source.gain,
            direction: source.position,
            sha256: hash_file(path)?,
            resolved_measurement: measurement,
        });
        readers.push((id, wav));
    }
    let staging = request.output_dir.with_extension("staging");
    if staging.exists() {
        return Err(RenderSceneError::OutputFailure(
            "staging directory exists".into(),
        ));
    }
    fs::create_dir_all(&staging)?;
    let result = render_to_staging(
        &scene,
        &mut readers,
        &sofa,
        &sources,
        scene_end,
        max_taps,
        request,
        &staging,
        scene_hash,
        sofa_hash,
        source_results,
    );
    match result {
        Ok(value) => {
            fs::rename(&staging, &request.output_dir)
                .map_err(|e| RenderSceneError::OutputFailure(e.to_string()))?;
            Ok(value)
        }
        Err(e) => {
            let _ = fs::remove_dir_all(&staging);
            Err(e)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_to_staging(
    scene: &RenderScene,
    readers: &mut [(openjoc_render::SourceId, StreamingWav)],
    sofa: &LoadedSofaHrirBank,
    sources: &[StaticBinauralSource],
    scene_end: u64,
    max_taps: usize,
    request: &RenderRequest,
    staging: &Path,
    scene_hash: String,
    sofa_hash: String,
    source_results: Vec<RenderSourceResult>,
) -> Result<RenderResult, RenderSceneError> {
    let options = WaveEncodeOptions {
        sample_format: SampleFormat::F32,
        clipping: Clipping::Reject,
        dither: Dither::None,
    };
    let file = File::create(staging.join("binaural.wav"))?;
    let mut writer = WaveWriter::new(file, scene.sample_rate_hz, 2, options)?;
    let block = match request.backend {
        RenderBackend::Direct => request.block_size,
        RenderBackend::Partitioned { partition_size } => partition_size,
    };
    if block == 0 || block > MAX_RENDER_BLOCK_SAMPLES {
        return Err(RenderSceneError::InvalidScene(
            "render block exceeds bounded-memory limit",
        ));
    }
    let mut left = vec![0.0; block];
    let mut right = vec![0.0; block];
    let mut pos = 0_u64;
    let mut direct = if matches!(request.backend, RenderBackend::Direct) {
        Some(BinauralRenderer::new(
            scene.sample_rate_hz,
            sofa.bank.clone(),
            sources.to_owned(),
        )?)
    } else {
        None
    };
    let mut partitioned = if let RenderBackend::Partitioned { partition_size } = request.backend {
        Some(PartitionedBinauralRenderer::new(
            scene.sample_rate_hz,
            UniformPartitionedConfig::new(partition_size)?,
            sofa.bank.clone(),
            sources.to_owned(),
        )?)
    } else {
        None
    };
    let mut source_buffers: Vec<Vec<f64>> = readers.iter().map(|_| vec![0.0; block]).collect();
    while pos < scene_end {
        let valid = (scene_end - pos).min(block as u64) as usize;
        for (i, (_id, wav)) in readers.iter_mut().enumerate() {
            source_buffers[i][..valid].fill(0.0);
            let src = &scene.sources[i];
            let block_end = pos.saturating_add(valid as u64);
            let source_end = src.start_sample.saturating_add(wav.frames);
            let overlap_start = pos.max(src.start_sample);
            let overlap_end = block_end.min(source_end);
            if overlap_end > overlap_start {
                let count = usize::try_from(overlap_end - overlap_start)
                    .map_err(|_| RenderSceneError::SourceTimelineOverflow)?;
                let destination = usize::try_from(overlap_start - pos)
                    .map_err(|_| RenderSceneError::SourceTimelineOverflow)?;
                let source_offset = overlap_start - src.start_sample;
                wav.read_samples(
                    source_offset,
                    &mut source_buffers[i][destination..destination + count],
                )?;
            }
        }
        let blocks = readers
            .iter()
            .enumerate()
            .map(|(i, (id, _))| BinauralSourceBlock::new(*id, &source_buffers[i][..valid]))
            .collect::<Vec<_>>();
        match (&mut direct, &mut partitioned) {
            (Some(renderer), None) => {
                renderer.render_block(&blocks, &mut left[..valid], &mut right[..valid])?;
            }
            (None, Some(renderer)) => {
                if valid == block {
                    renderer.render_partition(&blocks, &mut left[..valid], &mut right[..valid])?;
                } else {
                    renderer.finish_input(
                        &blocks,
                        valid,
                        &mut left[..valid],
                        &mut right[..valid],
                    )?;
                }
            }
            _ => unreachable!(),
        }
        writer.write_channels(&[&left[..valid], &right[..valid]])?;
        pos += valid as u64;
    }
    if let Some(renderer) = &mut partitioned {
        if scene_end == 0 || scene_end % block as u64 == 0 {
            let empty = readers
                .iter()
                .map(|(id, _)| BinauralSourceBlock::new(*id, &[]))
                .collect::<Vec<_>>();
            let mut unused_left = Vec::new();
            let mut unused_right = Vec::new();
            renderer.finish_input(&empty, 0, &mut unused_left, &mut unused_right)?;
        }
    }
    let tail = if let Some(renderer) = &mut direct {
        renderer.remaining_tail_samples()
    } else {
        partitioned
            .as_ref()
            .map_or(0, PartitionedBinauralRenderer::remaining_tail_samples)
    };
    let mut remaining = tail;
    while remaining > 0 {
        let count = remaining.min(block);
        if let Some(renderer) = &mut direct {
            renderer.drain_tail_block(&mut left[..count], &mut right[..count])?;
        } else if let Some(renderer) = &mut partitioned {
            renderer.drain_tail_block(&mut left[..count], &mut right[..count])?;
        }
        writer.write_channels(&[&left[..count], &right[..count]])?;
        remaining -= count;
    }
    let _ = writer.finish()?;
    let output = staging.join("binaural.wav");
    let output_bytes = fs::metadata(&output)?.len();
    let output_hash = hash_file(&output)?;
    let output_samples = scene_end
        .checked_add(max_taps.saturating_sub(1) as u64)
        .ok_or(RenderSceneError::SourceTimelineOverflow)?;
    let backend_name = match request.backend {
        RenderBackend::Direct => "direct",
        RenderBackend::Partitioned { .. } => "partitioned",
    };
    let resolved_measurements = source_results
        .iter()
        .map(|source| source.resolved_measurement)
        .collect();
    let result = RenderResult {
        schema: RESULT_SCHEMA,
        product_version: "0.3.0-dev",
        scene_schema: SCENE_SCHEMA,
        source_semantics: "explicit_caller_bound",
        joc_semantic_binding: "unresolved_not_used",
        reconstruction_basis_input: false,
        scene_sha256: scene_hash,
        sources: source_results,
        sofa_file: request
            .sofa_path
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or("")
            .to_string(),
        sofa_sha256: sofa_hash,
        sofa_bytes: fs::metadata(&request.sofa_path)?.len(),
        sofa_convention_version: sofa.metadata.convention_version.clone(),
        sofa_sample_rate_hz: sofa.metadata.sample_rate_hz,
        sofa_measurement_count: sofa.metadata.measurement_count,
        resolved_measurements,
        backend: backend_name.to_string(),
        partition_size: match request.backend {
            RenderBackend::Direct => None,
            RenderBackend::Partitioned { partition_size } => Some(partition_size),
        },
        algorithmic_latency_samples: match request.backend {
            RenderBackend::Direct => 0,
            RenderBackend::Partitioned { partition_size } => partition_size,
        },
        scene_input_length: scene_end,
        hrir_max_tap_count: max_taps,
        tail_samples: max_taps.saturating_sub(1),
        output_sample_count: output_samples,
        output_wav: "binaural.wav",
        output_format: "IEEE-float32 stereo WAV, FL then FR",
        output_bytes,
        output_sha256: output_hash,
        completion_status: "complete",
    };
    fs::write(
        staging.join("render.json"),
        serde_json::to_vec_pretty(&result).map_err(|e| RenderSceneError::Json(e.to_string()))?,
    )?;
    Ok(result)
}

fn hex_hash(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}
fn hash_file(path: &Path) -> Result<String, RenderSceneError> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut b = vec![0_u8; 65_536];
    loop {
        let n = f.read(&mut b)?;
        if n == 0 {
            break;
        }
        h.update(&b[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

struct StreamingWav {
    file: File,
    data_offset: u64,
    frames: u64,
    bytes_per_sample: u64,
    encoding: WavEncoding,
}
#[derive(Clone, Copy)]
enum WavEncoding {
    Pcm16,
    Pcm24,
    Pcm32,
    Float32,
}
impl StreamingWav {
    fn open(path: &Path, rate: u32) -> Result<Self, RenderSceneError> {
        let mut f = File::open(path)?;
        let mut head = [0_u8; 12];
        f.read_exact(&mut head)?;
        if &head[..4] != b"RIFF" || &head[8..] != b"WAVE" {
            return Err(RenderSceneError::UnsupportedWav(
                "RIFF/WAVE required".into(),
            ));
        }
        let mut fmt = None;
        let mut data = None;
        loop {
            let mut h = [0_u8; 8];
            if f.read_exact(&mut h).is_err() {
                break;
            }
            let size = u32::from_le_bytes(h[4..8].try_into().unwrap()) as u64;
            let pos = f.stream_position()?;
            if &h[..4] == b"fmt " {
                if size > MAX_WAV_FMT_BYTES {
                    return Err(RenderSceneError::UnsupportedWav(
                        "fmt chunk exceeds bounded header limit".into(),
                    ));
                }
                let mut b = vec![
                    0;
                    usize::try_from(size).map_err(|_| {
                        RenderSceneError::UnsupportedWav("format too large".into())
                    })?
                ];
                f.read_exact(&mut b)?;
                if b.len() < 16 {
                    return Err(RenderSceneError::UnsupportedWav("short fmt".into()));
                }
                let enc = u16::from_le_bytes([b[0], b[1]]);
                let ch = u16::from_le_bytes([b[2], b[3]]);
                let sr = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
                let bits = u16::from_le_bytes([b[14], b[15]]);
                if ch != 1 {
                    return Err(RenderSceneError::UnsupportedWav("mono WAV required".into()));
                }
                if sr != rate {
                    return Err(RenderSceneError::SampleRateMismatch {
                        expected: rate,
                        actual: sr,
                    });
                }
                let e = match (enc, bits) {
                    (1, 16) => WavEncoding::Pcm16,
                    (1, 24) => WavEncoding::Pcm24,
                    (1, 32) => WavEncoding::Pcm32,
                    (3, 32) => WavEncoding::Float32,
                    _ => {
                        return Err(RenderSceneError::UnsupportedWav(
                            "supported formats: mono PCM16/24/32 or float32".into(),
                        ));
                    }
                };
                fmt = Some((e, u64::from(bits / 8)));
            } else if &h[..4] == b"data" {
                data = Some((pos, size));
                f.seek(SeekFrom::Current(i64::try_from(size + size % 2).map_err(
                    |_| RenderSceneError::UnsupportedWav("data too large".into()),
                )?))?;
            } else {
                f.seek(SeekFrom::Current(i64::try_from(size + size % 2).map_err(
                    |_| RenderSceneError::UnsupportedWav("chunk too large".into()),
                )?))?;
            }
            if fmt.is_some() && data.is_some() {
                break;
            }
        }
        let (encoding, bps) =
            fmt.ok_or_else(|| RenderSceneError::UnsupportedWav("missing fmt".into()))?;
        let (data_offset, data_bytes) =
            data.ok_or_else(|| RenderSceneError::UnsupportedWav("missing data".into()))?;
        if data_bytes % bps != 0 {
            return Err(RenderSceneError::UnsupportedWav("unaligned data".into()));
        }
        Ok(Self {
            file: f,
            data_offset,
            frames: data_bytes / bps,
            bytes_per_sample: bps,
            encoding,
        })
    }
    fn read_samples(&mut self, start: u64, out: &mut [f64]) -> Result<(), RenderSceneError> {
        let byte = self
            .data_offset
            .checked_add(
                start
                    .checked_mul(self.bytes_per_sample)
                    .ok_or(RenderSceneError::SourceTimelineOverflow)?,
            )
            .ok_or(RenderSceneError::SourceTimelineOverflow)?;
        self.file.seek(SeekFrom::Start(byte))?;
        let mut buf = vec![0_u8; out.len() * self.bytes_per_sample as usize];
        self.file.read_exact(&mut buf)?;
        for (i, v) in out.iter_mut().enumerate() {
            let o = i * self.bytes_per_sample as usize;
            *v = match self.encoding {
                WavEncoding::Pcm16 => f64::from(i16::from_le_bytes([buf[o], buf[o + 1]])) / 32768.0,
                WavEncoding::Pcm24 => {
                    let mut x = i32::from(buf[o])
                        | (i32::from(buf[o + 1]) << 8)
                        | (i32::from(buf[o + 2]) << 16);
                    if x & 0x800000 != 0 {
                        x |= !0xffffff;
                    }
                    f64::from(x) / 8388608.0
                }
                WavEncoding::Pcm32 => {
                    f64::from(i32::from_le_bytes(buf[o..o + 4].try_into().unwrap())) / 2147483648.0
                }
                WavEncoding::Float32 => {
                    f64::from(f32::from_le_bytes(buf[o..o + 4].try_into().unwrap()))
                }
            };
            if !v.is_finite() {
                return Err(RenderSceneError::Wav("non-finite sample".into()));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schema_rejects_absolute_paths() {
        let dir = tempfile_dir();
        let p = dir.join("s.json");
        fs::write(&p, r#"{"schema":"openjoc.render-scene.v1","sample_rate_hz":48000,"source_semantics":"explicit_spatial_sources","sources":[{"id":"x","input_wav":"/tmp/x.wav","position":{"x":1,"y":0,"z":0},"gain":1.0}]}"#).unwrap();
        assert!(matches!(
            load_scene(&p, RenderSceneLoadLimits::default()),
            Err(RenderSceneError::PathEscape(_))
        ));
    }
    fn tempfile_dir() -> PathBuf {
        let p = std::env::temp_dir().join(format!("openjoc-scene-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&p);
        p
    }
}
