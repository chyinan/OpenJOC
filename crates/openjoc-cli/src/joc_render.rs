//! User-facing JOC-to-speaker rendering orchestration.
//!
//! The codec-coordinate topology is explicit input. This module deliberately
//! does not infer authored-object identity from OAMD order or ReconstructionBasis
//! row order.

use openjoc_eac3::{ChannelLocation, DecodedAccessUnitPcm, JocMetadataFrame};
use openjoc_emdf::JocValidationProfile;
use openjoc_scene::{
    BaseFullBandCoordinate, BridgeError, DecodedPayloadFrame, JocSpatialBridge,
    JocSpatialFrameBridge, SpatialBridgeError, SpatialCoordinateUpdate, SpatialLayout,
    SpatialLayoutChannel, SpatialLayoutNode, SpatialTopologySnapshot,
};
use openjoc_wave::{Clipping, Dither, SampleFormat, WaveEncodeOptions, WaveError, WaveWriter};
use serde::Deserialize;
use std::{
    collections::BTreeSet,
    fmt, fs, io,
    path::{Path, PathBuf},
};

pub const JOC_RENDER_CONTROL_SCHEMA: &str = "openjoc.joc-render-control.v1";
pub const JOC_RENDER_LAYOUT: &str = "5.1";
pub const JOC_RENDER_CHANNEL_ORDER: [&str; 6] = ["FL", "FR", "FC", "LFE", "Ls", "Rs"];
pub const JOC_RENDER_SUPPORTED_LAYOUTS: [&str; 4] = ["5.1", "5.1.2", "7.1", "7.1.4"];

#[derive(Debug)]
pub enum JocRenderError {
    Io(io::Error),
    Json(serde_json::Error),
    InvalidControl(String),
    UnsupportedLayout(String),
    EmptyTopology,
    TopologyCoordinateCount { expected: usize, actual: usize },
    BaseTopologyChanged,
    BaseCoordinate(ChannelLocation),
    FrameIndex { expected: u64, actual: u64 },
    SampleTimeline { expected: u64, actual: u64 },
    SampleRateMismatch { base: u32, frame: u32 },
    FrameSampleCount,
    ProfileChanged,
    UnusedUpdate { frame_index: u64 },
    Bridge(BridgeError),
    Spatial(SpatialBridgeError),
    Wave(WaveError),
    OutputExists(PathBuf),
    NoRenderedFrames,
}

impl fmt::Display for JocRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "JOC render I/O error: {error}"),
            Self::Json(error) => write!(formatter, "invalid JOC render topology JSON: {error}"),
            Self::InvalidControl(reason) => {
                write!(formatter, "invalid JOC render control: {reason}")
            }
            Self::UnsupportedLayout(layout) => {
                write!(
                    formatter,
                    "unsupported JOC render layout {layout}; supported layouts are {}",
                    JOC_RENDER_SUPPORTED_LAYOUTS.join(", ")
                )
            }
            Self::EmptyTopology => formatter.write_str("JOC render topology is empty"),
            Self::TopologyCoordinateCount { expected, actual } => write!(
                formatter,
                "JOC render topology has {actual} records; decoded Base plus ReconstructionBasis has {expected} coordinates"
            ),
            Self::BaseTopologyChanged => {
                formatter.write_str("decoded Base channel topology changed during JOC render")
            }
            Self::BaseCoordinate(location) => write!(
                formatter,
                "decoded Base channel {} is not supported by the JOC spatial bridge",
                location.label()
            ),
            Self::FrameIndex { expected, actual } => {
                write!(
                    formatter,
                    "JOC render expected frame {expected}, received {actual}"
                )
            }
            Self::SampleTimeline { expected, actual } => write!(
                formatter,
                "JOC render expected sample range to start at {expected}, received {actual}"
            ),
            Self::SampleRateMismatch { base, frame } => write!(
                formatter,
                "JOC render Base sample rate {base} Hz does not match JOC frame rate {frame} Hz"
            ),
            Self::FrameSampleCount => {
                formatter.write_str("JOC render frame sample count is invalid")
            }
            Self::ProfileChanged => {
                formatter.write_str("selected JOC validation profile changed during render")
            }
            Self::UnusedUpdate { frame_index } => write!(
                formatter,
                "JOC render topology update for unused frame {frame_index}"
            ),
            Self::Bridge(error) => write!(formatter, "JOC bridge frame error: {error}"),
            Self::Spatial(error) => write!(formatter, "JOC spatial bridge error: {error}"),
            Self::Wave(error) => write!(formatter, "JOC render WAV error: {error}"),
            Self::OutputExists(path) => {
                write!(formatter, "refusing to overwrite output {}", path.display())
            }
            Self::NoRenderedFrames => formatter.write_str("JOC render produced no audio frames"),
        }
    }
}

impl std::error::Error for JocRenderError {}

impl From<io::Error> for JocRenderError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for JocRenderError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<BridgeError> for JocRenderError {
    fn from(value: BridgeError) -> Self {
        Self::Bridge(value)
    }
}

impl From<SpatialBridgeError> for JocRenderError {
    fn from(value: SpatialBridgeError) -> Self {
        Self::Spatial(value)
    }
}

impl From<WaveError> for JocRenderError {
    fn from(value: WaveError) -> Self {
        Self::Wave(value)
    }
}

#[derive(Debug, Deserialize)]
struct RenderControlFile {
    schema: String,
    topology: SpatialTopologySnapshot,
    #[serde(default)]
    updates: Vec<FrameUpdates>,
}

#[derive(Debug, Deserialize)]
struct FrameUpdates {
    frame_index: u64,
    updates: Vec<SpatialCoordinateUpdate>,
}

#[derive(Debug)]
pub struct RenderControl {
    topology: SpatialTopologySnapshot,
    updates: Vec<FrameUpdates>,
    consumed_updates: Vec<bool>,
}

impl RenderControl {
    pub fn from_path(path: &Path) -> Result<Self, JocRenderError> {
        let file: RenderControlFile = serde_json::from_slice(&fs::read(path)?)?;
        if file.schema != JOC_RENDER_CONTROL_SCHEMA {
            return Err(JocRenderError::InvalidControl(format!(
                "expected schema {JOC_RENDER_CONTROL_SCHEMA}, got {}",
                file.schema
            )));
        }
        if file.topology.flatten().is_empty() {
            return Err(JocRenderError::EmptyTopology);
        }
        if file
            .updates
            .windows(2)
            .any(|window| window[0].frame_index >= window[1].frame_index)
        {
            return Err(JocRenderError::InvalidControl(
                "frame updates must have strictly increasing frame_index values".to_owned(),
            ));
        }
        let consumed_updates = vec![false; file.updates.len()];
        Ok(Self {
            topology: file.topology,
            updates: file.updates,
            consumed_updates,
        })
    }

    fn mark_update_for(&mut self, frame_index: u64) -> Option<usize> {
        let index = self
            .updates
            .iter()
            .position(|update| update.frame_index == frame_index)?;
        self.consumed_updates[index] = true;
        Some(index)
    }

    fn finish(&self) -> Result<(), JocRenderError> {
        if let Some((index, _)) = self
            .updates
            .iter()
            .enumerate()
            .find(|(index, _)| !self.consumed_updates[*index])
        {
            return Err(JocRenderError::UnusedUpdate {
                frame_index: self.updates[index].frame_index,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderedBlock {
    pub sample_rate: u32,
    pub channels: Vec<Vec<f64>>,
}

/// Data-only speaker preset consumed by the generic spatial bridge.
///
/// The preset owns public output order and the non-LFE projection geometry;
/// it does not implement a layout-specific renderer. Height presets use the
/// same normalized horizontal/height axes and tensor interpolation already
/// implemented by `SpatialLayout`.
#[derive(Clone, Debug)]
struct SpeakerLayoutPreset {
    name: &'static str,
    labels: Vec<&'static str>,
    lfe_index: Option<usize>,
    layout: SpatialLayout,
}

impl SpeakerLayoutPreset {
    fn for_name(name: &str) -> Result<Self, JocRenderError> {
        match name {
            JOC_RENDER_LAYOUT => five_point_one_preset(),
            "5.1.2" => five_point_one_two_preset(),
            "7.1" => seven_point_one_preset(),
            "7.1.4" => seven_point_one_four_preset(),
            other => Err(JocRenderError::UnsupportedLayout(other.to_owned())),
        }
    }

    fn new(
        name: &'static str,
        labels: Vec<&'static str>,
        lfe_index: Option<usize>,
        layout: SpatialLayout,
    ) -> Self {
        Self {
            name,
            labels,
            lfe_index,
            layout,
        }
    }

    fn channel_labels(&self) -> Vec<&'static str> {
        self.labels.clone()
    }

    fn channel_count(&self) -> usize {
        self.labels.len()
    }

    const fn lfe_index(&self) -> Option<usize> {
        self.lfe_index
    }
}

#[derive(Debug)]
pub struct JocSpeakerRenderer {
    frame_bridge: JocSpatialFrameBridge,
    bridge: JocSpatialBridge,
    preset: SpeakerLayoutPreset,
    control: RenderControl,
    expected_coordinates: usize,
    expected_frame: u64,
    expected_sample: u64,
    base_coordinates: Option<Vec<BaseFullBandCoordinate>>,
    selected_profile: Option<JocValidationProfile>,
    deviations: BTreeSet<String>,
}

impl JocSpeakerRenderer {
    pub fn new(layout: &str, control: RenderControl) -> Result<Self, JocRenderError> {
        let preset = SpeakerLayoutPreset::for_name(layout)?;
        let expected_coordinates = control.topology.flatten().len();
        if expected_coordinates == 0 {
            return Err(JocRenderError::EmptyTopology);
        }
        Ok(Self {
            frame_bridge: JocSpatialFrameBridge,
            bridge: JocSpatialBridge::new(),
            preset,
            control,
            expected_coordinates,
            expected_frame: 0,
            expected_sample: 0,
            base_coordinates: None,
            selected_profile: None,
            deviations: BTreeSet::new(),
        })
    }

    pub fn render_frame(
        &mut self,
        frame_index: usize,
        frame: &DecodedPayloadFrame,
        base: &DecodedAccessUnitPcm,
    ) -> Result<RenderedBlock, JocRenderError> {
        let frame_index =
            u64::try_from(frame_index).map_err(|_| JocRenderError::FrameSampleCount)?;
        if frame_index != self.expected_frame || frame.frame_index != frame_index {
            return Err(JocRenderError::FrameIndex {
                expected: self.expected_frame,
                actual: frame_index,
            });
        }
        if frame.sample_range.start_sample != self.expected_sample {
            return Err(JocRenderError::SampleTimeline {
                expected: self.expected_sample,
                actual: frame.sample_range.start_sample,
            });
        }
        if base.sample_rate != frame.sample_rate {
            return Err(JocRenderError::SampleRateMismatch {
                base: base.sample_rate,
                frame: frame.sample_rate,
            });
        }
        let base_coordinates = base
            .channel_locations
            .iter()
            .copied()
            .map(base_coordinate)
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(previous) = &self.base_coordinates {
            if previous != &base_coordinates {
                return Err(JocRenderError::BaseTopologyChanged);
            }
        } else {
            self.base_coordinates = Some(base_coordinates.clone());
        }
        let expected = base_coordinates
            .len()
            .checked_add(frame.decoded.reconstruction_basis.rows.len())
            .ok_or(JocRenderError::FrameSampleCount)?;
        if expected != self.expected_coordinates {
            return Err(JocRenderError::TopologyCoordinateCount {
                expected,
                actual: self.expected_coordinates,
            });
        }
        let bridge_frame = self.frame_bridge.frame(
            frame,
            &base_coordinates,
            &base.channels,
            base.lfe.as_deref(),
        )?;
        let sample_count = usize::try_from(bridge_frame.sample_range.len())
            .map_err(|_| JocRenderError::FrameSampleCount)?;
        if sample_count == 0 {
            return Err(JocRenderError::FrameSampleCount);
        }
        if usize::from(base.samples) != sample_count {
            return Err(JocRenderError::FrameSampleCount);
        }
        let mut active = vec![vec![0.0; sample_count]; self.preset.layout.active_channel_count()];
        let mut output_planes = active.iter_mut().map(Vec::as_mut_slice).collect::<Vec<_>>();
        let update_index = self.control.mark_update_for(frame_index);
        let topology = (self.expected_frame == 0).then_some(&self.control.topology);
        let updates = update_index.map(|index| self.control.updates[index].updates.as_slice());
        self.bridge.render_codec_basis_frame(
            &bridge_frame,
            topology,
            updates,
            &self.preset.layout,
            u64::try_from(sample_count).map_err(|_| JocRenderError::FrameSampleCount)?,
            &mut output_planes,
        )?;

        let mut channels = vec![vec![0.0; sample_count]; self.preset.channel_count()];
        let mut active_index = 0;
        for (output_index, channel) in self.preset.layout.channels().iter().enumerate() {
            if channel.lfe {
                if let Some(lfe) = base.lfe.as_deref() {
                    channels[output_index].copy_from_slice(lfe);
                }
            } else {
                channels[output_index].copy_from_slice(&active[active_index]);
                active_index += 1;
            }
        }
        self.expected_frame = self
            .expected_frame
            .checked_add(1)
            .ok_or(JocRenderError::FrameSampleCount)?;
        self.expected_sample = self
            .expected_sample
            .checked_add(u64::try_from(sample_count).map_err(|_| JocRenderError::FrameSampleCount)?)
            .ok_or(JocRenderError::FrameSampleCount)?;
        Ok(RenderedBlock {
            sample_rate: frame.sample_rate,
            channels,
        })
    }

    pub fn record_profile(&mut self, metadata: &JocMetadataFrame) -> Result<(), JocRenderError> {
        if let Some(selected) = self.selected_profile {
            if selected != metadata.validation_profile {
                return Err(JocRenderError::ProfileChanged);
            }
        } else {
            self.selected_profile = Some(metadata.validation_profile);
        }
        for deviation in &metadata.deviations {
            self.deviations.insert(format!(
                "payload {} {}={} expected_by_etsi={}",
                deviation.payload_id, deviation.field, deviation.actual, deviation.expected_by_etsi
            ));
        }
        Ok(())
    }

    pub fn finish(&self) -> Result<(), JocRenderError> {
        self.control.finish()
    }

    pub fn diagnostics(
        &self,
        requested_layout: &str,
        requested_profile: crate::eac3_decode::ValidationProfileRequest,
        selected_profile: JocValidationProfile,
        summary: &openjoc_scene::StreamingSceneSummary,
        output: &Path,
    ) -> String {
        let selected = self.selected_profile.unwrap_or(selected_profile);
        let deviations = if self.deviations.is_empty() {
            "none".to_owned()
        } else {
            self.deviations
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        };
        format!(
            "feature: JocSpatialBridge\nimplementation maturity: Experimental\nsemantic binding: Unresolved\nrequested layout: {requested_layout}\nselected layout: {}\nchannel count: {}\nLFE index: {:?}\nrequested profile: {}\nselected profile: {}\ncompatibility deviations: {}\noutput: {}\nsample rate: {} Hz\nframes: {}\nsamples: {}\noutput channel order: {}\nraw3: preserved and excluded from projection arithmetic",
            self.preset.name,
            self.preset.channel_count(),
            self.preset.lfe_index(),
            requested_profile.as_str(),
            selected.as_str(),
            deviations,
            output.display(),
            summary.sample_rate,
            summary.frames,
            summary.duration_samples,
            self.preset.channel_labels().join(", "),
        )
    }
}

fn base_coordinate(location: ChannelLocation) -> Result<BaseFullBandCoordinate, JocRenderError> {
    Ok(match location {
        ChannelLocation::Left => BaseFullBandCoordinate::Left,
        ChannelLocation::Right => BaseFullBandCoordinate::Right,
        ChannelLocation::Centre => BaseFullBandCoordinate::Centre,
        ChannelLocation::LeftSurround => BaseFullBandCoordinate::LeftSurround,
        ChannelLocation::RightSurround => BaseFullBandCoordinate::RightSurround,
        ChannelLocation::LeftBack => BaseFullBandCoordinate::LeftBack,
        ChannelLocation::RightBack => BaseFullBandCoordinate::RightBack,
        ChannelLocation::TopFrontLeft => BaseFullBandCoordinate::TopFrontLeft,
        ChannelLocation::TopFrontRight => BaseFullBandCoordinate::TopFrontRight,
        ChannelLocation::Other(value) => BaseFullBandCoordinate::Other(value),
        ChannelLocation::Lfe(_) => return Err(JocRenderError::BaseCoordinate(location)),
    })
}

fn five_point_one_layout() -> Result<SpatialLayout, JocRenderError> {
    let channels = vec![
        SpatialLayoutChannel {
            identity: "FL".to_owned(),
            enabled: true,
            lfe: false,
        },
        SpatialLayoutChannel {
            identity: "FR".to_owned(),
            enabled: true,
            lfe: false,
        },
        SpatialLayoutChannel {
            identity: "FC".to_owned(),
            enabled: true,
            lfe: false,
        },
        SpatialLayoutChannel {
            identity: "LFE".to_owned(),
            enabled: true,
            lfe: true,
        },
        SpatialLayoutChannel {
            identity: "Ls".to_owned(),
            enabled: true,
            lfe: false,
        },
        SpatialLayoutChannel {
            identity: "Rs".to_owned(),
            enabled: true,
            lfe: false,
        },
    ];
    let one_hot = |index: usize| {
        let mut vector = vec![0.0; 5];
        vector[index] = 1.0;
        vector
    };
    SpatialLayout::new(
        channels,
        vec![vec![0.0, 0.25, 0.5, 0.75, 1.0]],
        (0..5)
            .map(|index| SpatialLayoutNode {
                knot_indices: vec![index],
                vector: one_hot([3, 0, 2, 1, 4][index]),
            })
            .collect(),
        Vec::new(),
    )
    .map_err(|error| JocRenderError::InvalidControl(error.to_string()))
}

fn five_point_one_preset() -> Result<SpeakerLayoutPreset, JocRenderError> {
    Ok(SpeakerLayoutPreset::new(
        JOC_RENDER_LAYOUT,
        JOC_RENDER_CHANNEL_ORDER.to_vec(),
        Some(3),
        five_point_one_layout()?,
    ))
}

fn five_point_one_two_preset() -> Result<SpeakerLayoutPreset, JocRenderError> {
    let active_count = 7;
    let horizontal_knots = vec![0.0, 0.25, 0.5, 0.75, 1.0];
    let lower = [3, 0, 2, 1, 4]
        .into_iter()
        .map(|index| one_hot(active_count, index))
        .collect();
    let upper = vec![
        one_hot(active_count, 5),
        one_hot(active_count, 5),
        mixed(active_count, 5, 6),
        one_hot(active_count, 6),
        one_hot(active_count, 6),
    ];
    two_layer_preset(
        "5.1.2",
        vec!["FL", "FR", "FC", "LFE", "Ls", "Rs", "TFL", "TFR"],
        Some(3),
        horizontal_knots,
        lower,
        upper,
    )
}

fn seven_point_one_preset() -> Result<SpeakerLayoutPreset, JocRenderError> {
    let active_count = 7;
    one_layer_preset(
        "7.1",
        vec!["FL", "FR", "FC", "LFE", "Ls", "Rs", "Lb", "Rb"],
        Some(3),
        vec![0.0, 1.0 / 6.0, 1.0 / 3.0, 0.5, 2.0 / 3.0, 5.0 / 6.0, 1.0],
        vec![5, 3, 0, 2, 1, 4, 6]
            .into_iter()
            .map(|index| one_hot(active_count, index))
            .collect(),
    )
}

fn seven_point_one_four_preset() -> Result<SpeakerLayoutPreset, JocRenderError> {
    let active_count = 11;
    let horizontal_knots = vec![0.0, 1.0 / 6.0, 1.0 / 3.0, 0.5, 2.0 / 3.0, 5.0 / 6.0, 1.0];
    let lower = [5, 3, 0, 2, 1, 4, 6]
        .into_iter()
        .map(|index| one_hot(active_count, index))
        .collect();
    let upper = vec![
        one_hot(active_count, 9),
        one_hot(active_count, 9),
        one_hot(active_count, 7),
        mixed(active_count, 7, 8),
        one_hot(active_count, 8),
        one_hot(active_count, 10),
        one_hot(active_count, 10),
    ];
    two_layer_preset(
        "7.1.4",
        vec![
            "FL", "FR", "FC", "LFE", "Ls", "Rs", "Lb", "Rb", "TFL", "TFR", "TBL", "TBR",
        ],
        Some(3),
        horizontal_knots,
        lower,
        upper,
    )
}

fn one_layer_preset(
    name: &'static str,
    labels: Vec<&'static str>,
    lfe_index: Option<usize>,
    horizontal_knots: Vec<f64>,
    vectors: Vec<Vec<f64>>,
) -> Result<SpeakerLayoutPreset, JocRenderError> {
    if horizontal_knots.len() != vectors.len() {
        return Err(JocRenderError::InvalidControl(
            "speaker preset knot/vector count mismatch".to_owned(),
        ));
    }
    let nodes = vectors
        .into_iter()
        .enumerate()
        .map(|(index, vector)| SpatialLayoutNode {
            knot_indices: vec![index],
            vector,
        })
        .collect();
    let layout = SpatialLayout::new(
        layout_channels(&labels, lfe_index),
        vec![horizontal_knots],
        nodes,
        Vec::new(),
    )
    .map_err(|error| JocRenderError::InvalidControl(error.to_string()))?;
    Ok(SpeakerLayoutPreset::new(name, labels, lfe_index, layout))
}

fn two_layer_preset(
    name: &'static str,
    labels: Vec<&'static str>,
    lfe_index: Option<usize>,
    horizontal_knots: Vec<f64>,
    lower: Vec<Vec<f64>>,
    upper: Vec<Vec<f64>>,
) -> Result<SpeakerLayoutPreset, JocRenderError> {
    if lower.len() != horizontal_knots.len() || upper.len() != horizontal_knots.len() {
        return Err(JocRenderError::InvalidControl(
            "height speaker preset knot/vector count mismatch".to_owned(),
        ));
    }
    let mut nodes = Vec::with_capacity(horizontal_knots.len() * 2);
    for (height, layer) in [lower, upper].into_iter().enumerate() {
        for (horizontal, vector) in layer.into_iter().enumerate() {
            nodes.push(SpatialLayoutNode {
                knot_indices: vec![horizontal, height],
                vector,
            });
        }
    }
    let layout = SpatialLayout::new(
        layout_channels(&labels, lfe_index),
        vec![horizontal_knots, vec![0.0, 1.0]],
        nodes,
        Vec::new(),
    )
    .map_err(|error| JocRenderError::InvalidControl(error.to_string()))?;
    Ok(SpeakerLayoutPreset::new(name, labels, lfe_index, layout))
}

fn layout_channels(labels: &[&'static str], lfe_index: Option<usize>) -> Vec<SpatialLayoutChannel> {
    labels
        .iter()
        .enumerate()
        .map(|(index, identity)| SpatialLayoutChannel {
            identity: (*identity).to_owned(),
            enabled: true,
            lfe: lfe_index == Some(index),
        })
        .collect()
}

fn one_hot(length: usize, index: usize) -> Vec<f64> {
    let mut vector = vec![0.0; length];
    vector[index] = 1.0;
    vector
}

fn mixed(length: usize, first: usize, second: usize) -> Vec<f64> {
    let mut vector = vec![0.0; length];
    vector[first] = 0.5;
    vector[second] = 0.5;
    vector
}

pub struct JocWavOutput {
    output: PathBuf,
    staging: PathBuf,
    format: SampleFormat,
    writer: Option<WaveWriter<fs::File>>,
    sample_rate: Option<u32>,
    channels: Option<usize>,
}

impl JocWavOutput {
    pub fn new(output: &Path, format: SampleFormat) -> Result<Self, JocRenderError> {
        if output.exists() {
            return Err(JocRenderError::OutputExists(output.to_owned()));
        }
        let parent = output
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let name = output
            .file_name()
            .ok_or_else(|| JocRenderError::InvalidControl("output has no filename".to_owned()))?
            .to_string_lossy();
        let staging = parent.join(format!(".{name}.openjoc-partial"));
        if staging.exists() {
            return Err(JocRenderError::OutputExists(staging));
        }
        Ok(Self {
            output: output.to_owned(),
            staging,
            format,
            writer: None,
            sample_rate: None,
            channels: None,
        })
    }

    pub fn write_block(&mut self, block: &RenderedBlock) -> Result<(), JocRenderError> {
        if block.channels.is_empty() {
            return Err(JocRenderError::NoRenderedFrames);
        }
        let channels = block.channels.len();
        if let Some(expected) = self.sample_rate {
            if expected != block.sample_rate || self.channels != Some(channels) {
                return Err(JocRenderError::InvalidControl(
                    "render output format changed during stream".to_owned(),
                ));
            }
        } else {
            let file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&self.staging)?;
            self.writer = Some(WaveWriter::new(
                file,
                block.sample_rate,
                channels,
                WaveEncodeOptions {
                    sample_format: self.format,
                    clipping: Clipping::Reject,
                    dither: Dither::None,
                },
            )?);
            self.sample_rate = Some(block.sample_rate);
            self.channels = Some(channels);
        }
        let references = block.channels.iter().map(Vec::as_slice).collect::<Vec<_>>();
        self.writer
            .as_mut()
            .ok_or(JocRenderError::NoRenderedFrames)?
            .write_channels(&references)?;
        Ok(())
    }

    pub fn finish(mut self) -> Result<(), JocRenderError> {
        let writer = self.writer.take().ok_or(JocRenderError::NoRenderedFrames)?;
        if let Err(error) = writer.finish() {
            let _ = fs::remove_file(&self.staging);
            return Err(error.into());
        }
        if let Err(error) = fs::rename(&self.staging, &self.output) {
            let _ = fs::remove_file(&self.staging);
            return Err(error.into());
        }
        Ok(())
    }

    pub fn abort(&mut self) {
        self.writer.take();
        let _ = fs::remove_file(&self.staging);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FrameUpdates, JOC_RENDER_CHANNEL_ORDER, JOC_RENDER_SUPPORTED_LAYOUTS, JocRenderError,
        JocSpeakerRenderer, JocWavOutput, RenderControl, RenderedBlock, SpeakerLayoutPreset,
        five_point_one_layout,
    };
    use openjoc_eac3::{ChannelLocation, DecodedAccessUnitPcm};
    use openjoc_joc::{DecodedJocFrame, JocFrame, JocHeader, ReconstructionBasis};
    use openjoc_oamd::{ContentDescription, OamdContentPrefix, OamdPayload, ObjectClass};
    use openjoc_scene::{
        DecodedPayloadFrame, JocSpatialBridge, ProgrammeLayout, SampleRange, SpatialBindingRecord,
        SpatialDescriptor, SpatialSourceClass, SpatialTopologySnapshot,
    };
    use openjoc_wave::{SampleFormat, decode};
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn record(identity: &str) -> SpatialBindingRecord {
        SpatialBindingRecord {
            descriptor: SpatialDescriptor {
                source_class: SpatialSourceClass::ExplicitChannel,
                identity: identity.to_owned(),
                coordinates: vec![0.5],
                spread: None,
                paired: None,
                raw3: Some(vec![3]),
            },
            scalar: 1.0,
            active: true,
        }
    }

    fn control(with_updates: bool, record_count: usize) -> RenderControl {
        let identities = ["FL", "FR", "FC", "Ls", "Rs", "FC"];
        control_with_identities(with_updates, &identities[..record_count])
    }

    fn control_with_identities(with_updates: bool, identities: &[&str]) -> RenderControl {
        let records = identities.iter().map(|identity| record(identity)).collect();
        RenderControl {
            topology: SpatialTopologySnapshot {
                explicit_groups: Vec::new(),
                fixed_layout: Vec::new(),
                dynamic_records: records,
            },
            updates: if with_updates {
                vec![FrameUpdates {
                    frame_index: 1,
                    updates: vec![openjoc_scene::SpatialCoordinateUpdate {
                        ordinal: 5,
                        descriptor: None,
                        scalar: Some(0.0),
                        active: None,
                    }],
                }]
            } else {
                Vec::new()
            },
            consumed_updates: vec![false; usize::from(with_updates)],
        }
    }

    fn decoded_frame(frame_index: u64, start: u64, samples: usize) -> DecodedPayloadFrame {
        let prefix = OamdContentPrefix {
            syntax_version: 0,
            object_count: 1,
            content: ContentDescription::DynamicOnly { lfe_present: false },
            alternate_object_data_present: false,
            element_count: 0,
            consumed_bits: 0,
        };
        let oamd = OamdPayload {
            prefix: prefix.clone(),
            object_classes: vec![ObjectClass::Dynamic],
            elements: Vec::new(),
            consumed_bits: 0,
        };
        DecodedPayloadFrame {
            frame_index,
            sample_rate: 48_000,
            sample_range: SampleRange::new(start, start + samples as u64).unwrap(),
            joc: JocFrame {
                header: JocHeader {
                    downmix_index: 0,
                    channel_count: 5,
                    object_count_bits: 0,
                    object_count: 1,
                    extension_index: 0,
                },
                clip_gain_x_bits: 0,
                clip_gain_y_bits: 0,
                sequence_count: u16::try_from(frame_index).unwrap(),
                objects: Vec::new(),
            },
            oamd,
            decoded: DecodedJocFrame {
                reconstruction_qmf: Vec::new(),
                reconstruction_basis: ReconstructionBasis {
                    rows: vec![vec![6.0; samples]],
                },
                stages: Vec::new(),
                state_reset: frame_index == 0,
            },
            programme_layout: ProgrammeLayout::from_prefix(&prefix).unwrap(),
        }
    }

    fn base(samples: usize, value: f64) -> DecodedAccessUnitPcm {
        DecodedAccessUnitPcm {
            sample_rate: 48_000,
            samples: samples as u16,
            channel_locations: vec![
                ChannelLocation::Left,
                ChannelLocation::Right,
                ChannelLocation::Centre,
                ChannelLocation::LeftSurround,
                ChannelLocation::RightSurround,
            ],
            channels: (0..5)
                .map(|index| vec![value + index as f64; samples])
                .collect(),
            lfe_location: Some(ChannelLocation::Lfe(0)),
            lfe: Some(vec![99.0; samples]),
        }
    }

    #[test]
    fn standard_layout_has_deterministic_wav_order() {
        let layout = five_point_one_layout().unwrap();
        assert_eq!(
            JOC_RENDER_CHANNEL_ORDER,
            ["FL", "FR", "FC", "LFE", "Ls", "Rs"]
        );
        assert_eq!(
            layout
                .channels()
                .iter()
                .map(|channel| channel.identity.as_str())
                .collect::<Vec<_>>(),
            JOC_RENDER_CHANNEL_ORDER
        );
    }

    #[test]
    fn canonical_layout_presets_have_explicit_public_contracts() {
        assert_eq!(
            JOC_RENDER_SUPPORTED_LAYOUTS,
            ["5.1", "5.1.2", "7.1", "7.1.4"]
        );
        let expected = [
            ("5.1", vec!["FL", "FR", "FC", "LFE", "Ls", "Rs"]),
            (
                "5.1.2",
                vec!["FL", "FR", "FC", "LFE", "Ls", "Rs", "TFL", "TFR"],
            ),
            ("7.1", vec!["FL", "FR", "FC", "LFE", "Ls", "Rs", "Lb", "Rb"]),
            (
                "7.1.4",
                vec![
                    "FL", "FR", "FC", "LFE", "Ls", "Rs", "Lb", "Rb", "TFL", "TFR", "TBL", "TBR",
                ],
            ),
        ];
        for (name, labels) in expected {
            let preset = SpeakerLayoutPreset::for_name(name).unwrap();
            assert_eq!(preset.channel_labels(), labels);
            assert_eq!(preset.lfe_index(), Some(3));
            assert_eq!(preset.channel_count(), labels.len());
        }
    }

    #[test]
    fn unknown_layout_reports_supported_values_without_fallback() {
        let error = JocSpeakerRenderer::new("9.1.6", control(false, 6)).unwrap_err();
        assert!(matches!(error, JocRenderError::UnsupportedLayout(_)));
        assert!(error.to_string().contains("7.1.4"));
        assert!(!error.to_string().contains("fallback"));
    }

    #[test]
    fn generic_presets_set_output_dimension_and_lfe_index() {
        for name in JOC_RENDER_SUPPORTED_LAYOUTS {
            let mut renderer = JocSpeakerRenderer::new(name, control(false, 6)).unwrap();
            let block = renderer
                .render_frame(0, &decoded_frame(0, 0, 2), &base(2, 1.0))
                .unwrap();
            let preset = SpeakerLayoutPreset::for_name(name).unwrap();
            assert_eq!(block.channels.len(), preset.channel_count());
            assert_eq!(block.channels[preset.lfe_index().unwrap()], vec![99.0; 2]);
        }
    }

    #[test]
    fn height_layout_routes_a_basis_row_to_a_height_channel() {
        let mut renderer = JocSpeakerRenderer::new(
            "7.1.4",
            control_with_identities(false, &["FL", "FR", "FC", "Ls", "Rs", "TFL"]),
        )
        .unwrap();
        let block = renderer
            .render_frame(0, &decoded_frame(0, 0, 2), &base(2, 1.0))
            .unwrap();
        assert_eq!(block.channels.len(), 12);
        assert_eq!(block.channels[8], vec![6.0; 2]);
        assert_eq!(block.channels[3], vec![99.0; 2]);
    }

    #[test]
    fn non_five_one_layout_is_partition_invariant() {
        let preset = SpeakerLayoutPreset::for_name("7.1.4").unwrap();
        let topology = SpatialTopologySnapshot {
            dynamic_records: vec![SpatialBindingRecord {
                descriptor: SpatialDescriptor::new(
                    SpatialSourceClass::DynamicPoint,
                    "height-point",
                    vec![1.0 / 3.0, 1.0],
                ),
                scalar: 1.0,
                active: true,
            }],
            ..SpatialTopologySnapshot::default()
        };
        let input = vec![0.25, 0.5, 0.75, 1.0, 0.5, 0.25, 0.0, 0.125];
        let coordinates = [input.as_slice()];
        let active_count = preset.layout.active_channel_count();
        let mut whole = vec![vec![0.0; input.len()]; active_count];
        let mut whole_refs = whole.iter_mut().map(Vec::as_mut_slice).collect::<Vec<_>>();
        JocSpatialBridge::new()
            .render_coordinates(
                &coordinates,
                Some(&topology),
                None,
                &preset.layout,
                0,
                48_000,
                &mut whole_refs,
            )
            .unwrap();

        let mut split = vec![vec![0.0; input.len()]; active_count];
        let mut bridge = JocSpatialBridge::new();
        {
            let first_coordinates = [&input[..3]];
            let mut first_refs = split
                .iter_mut()
                .map(|channel| &mut channel[..3])
                .collect::<Vec<_>>();
            bridge
                .render_coordinates(
                    &first_coordinates,
                    Some(&topology),
                    None,
                    &preset.layout,
                    0,
                    48_000,
                    &mut first_refs,
                )
                .unwrap();
        }
        {
            let second_coordinates = [&input[3..]];
            let mut second_refs = split
                .iter_mut()
                .map(|channel| &mut channel[3..])
                .collect::<Vec<_>>();
            bridge
                .render_coordinates(
                    &second_coordinates,
                    None,
                    None,
                    &preset.layout,
                    0,
                    48_000,
                    &mut second_refs,
                )
                .unwrap();
        }
        assert_eq!(whole, split);
    }

    #[test]
    fn decoded_base_and_basis_reach_bridge_and_lfe_stays_separate() {
        let mut renderer = JocSpeakerRenderer::new("5.1", control(false, 6)).unwrap();
        let block = renderer
            .render_frame(0, &decoded_frame(0, 0, 2), &base(2, 1.0))
            .unwrap();
        assert_eq!(block.channels.len(), 6);
        assert_eq!(block.channels[0], vec![1.0; 2]);
        assert_eq!(block.channels[1], vec![2.0; 2]);
        assert_eq!(block.channels[2], vec![9.0; 2]);
        assert_eq!(block.channels[3], vec![99.0; 2]);
        assert_eq!(block.channels[4], vec![4.0; 2]);
        assert_eq!(block.channels[5], vec![5.0; 2]);
    }

    #[test]
    fn topology_count_mismatch_is_rejected_without_row_guessing() {
        let mut renderer = JocSpeakerRenderer::new("5.1", control(false, 5)).unwrap();
        let error = renderer
            .render_frame(0, &decoded_frame(0, 0, 2), &base(2, 1.0))
            .unwrap_err();
        assert!(matches!(
            error,
            JocRenderError::TopologyCoordinateCount {
                expected: 6,
                actual: 5
            }
        ));
    }

    #[test]
    fn metadata_update_changes_only_subsequent_render_state() {
        let mut renderer = JocSpeakerRenderer::new("5.1", control(true, 6)).unwrap();
        let first = renderer
            .render_frame(0, &decoded_frame(0, 0, 2), &base(2, 1.0))
            .unwrap();
        let second = renderer
            .render_frame(1, &decoded_frame(1, 2, 2), &base(2, 1.0))
            .unwrap();
        assert_eq!(first.channels[2], vec![9.0; 2]);
        assert_eq!(second.channels[2], vec![3.0; 2]);
        renderer.finish().unwrap();
    }

    #[test]
    fn unsupported_layout_is_explicit() {
        let error = JocSpeakerRenderer::new("9.1.6", control(false, 6)).unwrap_err();
        assert!(matches!(error, JocRenderError::UnsupportedLayout(_)));
    }

    #[test]
    fn rendered_block_is_channel_major() {
        let block = RenderedBlock {
            sample_rate: 48_000,
            channels: vec![vec![1.0], vec![2.0]],
        };
        assert_eq!(block.channels[0][0], 1.0);
        assert_eq!(block.channels[1][0], 2.0);
    }

    #[test]
    fn frame_updates_are_not_silently_dropped() {
        let mut renderer = JocSpeakerRenderer::new("5.1", control(true, 6)).unwrap();
        renderer
            .render_frame(0, &decoded_frame(0, 0, 2), &base(2, 1.0))
            .unwrap();
        let error = renderer.finish().unwrap_err();
        assert!(matches!(
            error,
            JocRenderError::UnusedUpdate { frame_index: 1 }
        ));
    }

    #[test]
    fn timeline_and_sample_rate_mismatches_are_explicit() {
        let mut renderer = JocSpeakerRenderer::new("5.1", control(false, 6)).unwrap();
        let error = renderer
            .render_frame(0, &decoded_frame(0, 1, 2), &base(2, 1.0))
            .unwrap_err();
        assert!(matches!(
            error,
            JocRenderError::SampleTimeline {
                expected: 0,
                actual: 1
            }
        ));

        let mut mismatched_base = base(2, 1.0);
        mismatched_base.sample_rate = 44_100;
        let error = renderer
            .render_frame(0, &decoded_frame(0, 0, 2), &mismatched_base)
            .unwrap_err();
        assert!(matches!(
            error,
            JocRenderError::SampleRateMismatch {
                base: 44_100,
                frame: 48_000
            }
        ));
    }

    #[test]
    fn wav_output_finalizes_float32_with_stable_channel_order() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "openjoc-joc-render-{}-{nonce}.wav",
            std::process::id()
        ));
        let mut output = JocWavOutput::new(&path, SampleFormat::F32).unwrap();
        output
            .write_block(&RenderedBlock {
                sample_rate: 48_000,
                channels: vec![
                    vec![0.1, 0.2],
                    vec![0.3, 0.4],
                    vec![0.5, 0.6],
                    vec![0.7, 0.8],
                    vec![0.9, 0.0],
                    vec![-0.1, -0.2],
                ],
            })
            .unwrap();
        output.finish().unwrap();

        let pcm = decode(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(pcm.sample_rate, 48_000);
        assert_eq!(pcm.channels.len(), JOC_RENDER_CHANNEL_ORDER.len());
        assert_eq!(
            pcm.channels.iter().map(Vec::len).collect::<Vec<_>>(),
            vec![2; 6]
        );
        assert!((pcm.channels[0][0] - 0.1).abs() < 1e-6);
        assert!((pcm.channels[5][1] + 0.2).abs() < 1e-6);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn wav_output_supports_twelve_channel_preset_dimension() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "openjoc-joc-render-12ch-{}-{nonce}.wav",
            std::process::id()
        ));
        let channels = (0..12)
            .map(|index| vec![index as f64 / 20.0, -(index as f64) / 20.0])
            .collect::<Vec<_>>();
        let mut output = JocWavOutput::new(&path, SampleFormat::F32).unwrap();
        output
            .write_block(&RenderedBlock {
                sample_rate: 48_000,
                channels,
            })
            .unwrap();
        output.finish().unwrap();

        let pcm = decode(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(pcm.sample_rate, 48_000);
        assert_eq!(pcm.channels.len(), 12);
        assert_eq!(
            pcm.channels.iter().map(Vec::len).collect::<Vec<_>>(),
            vec![2; 12]
        );
        assert!((pcm.channels[0][0] - 0.0).abs() < 1e-6);
        assert!((pcm.channels[11][1] + 0.55).abs() < 1e-6);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn wav_output_supports_twenty_four_channel_pcm_capacity() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "openjoc-joc-render-24ch-{}-{nonce}.wav",
            std::process::id()
        ));
        let channels = (0..24)
            .map(|index| vec![index as f64 / 32.0])
            .collect::<Vec<_>>();
        let mut output = JocWavOutput::new(&path, SampleFormat::F32).unwrap();
        output
            .write_block(&RenderedBlock {
                sample_rate: 48_000,
                channels,
            })
            .unwrap();
        output.finish().unwrap();

        let pcm = decode(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(pcm.channels.len(), 24);
        assert!((pcm.channels[23][0] - 23.0 / 32.0).abs() < 1e-6);
        fs::remove_file(path).unwrap();
    }
}
