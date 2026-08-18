//! User-facing JOC-to-speaker rendering orchestration.
//!
//! The codec-coordinate topology is explicit input. This module deliberately
//! does not infer authored-object identity from OAMD order or ReconstructionBasis
//! row order.

use crate::performance::RenderStageTiming;
use openjoc_eac3::{
    ChannelLocation, DecodedAccessUnitPcm, DialnormState, DownmixMetadata, JocMetadataFrame,
    StereoDownmixMode, stereo_downmix_matrix,
};
use openjoc_emdf::JocValidationProfile;
use openjoc_joc::{
    AlignedReconstructionOutput, ReconstructionBasis, ReconstructionOutputTimeline,
    ReconstructionTimelineError,
};
use openjoc_render::{
    BinauralRenderer, BinauralSourceBlock, CartesianPosition, FINAL_LINKED_GAIN_BLOCK_SAMPLES,
    FinalLinkedGain, FinalLinkedGainError, HrirBank, HrirEntry, HrirEntryId,
    PartitionedBinauralRenderer, SourceId, StaticBinauralSource, UniformPartitionedConfig,
};
use openjoc_scene::{
    BaseFullBandCoordinate, BridgeControlAssembler, BridgeControlAssemblyError, BridgeControlFrame,
    BridgeError, DecodedPayloadFrame, JocSpatialBridge, JocSpatialFrameBridge,
    SPEAKER_LAYOUT_PRESET_NAMES, SemanticChannelLayout, SpatialBridgeError,
    SpatialContributionMode, SpatialCoordinateUpdate, SpatialRouteVector, SpatialTopologySnapshot,
    SpeakerLayoutPreset, SpeakerLayoutPresetError,
};
#[cfg(test)]
use openjoc_scene::{SPEAKER_LAYOUT_5_1_CHANNELS, SpatialLayout};
use openjoc_sofa::{SofaError, resolve_hrir};
use openjoc_wave::{
    CafChannelDescription, CafError, CafWriter, Clipping, Dither, SampleFormat, WaveEncodeOptions,
    WaveError, WaveWriter,
};
use serde::Deserialize;
use std::{
    collections::{BTreeSet, VecDeque},
    fmt, fs, io,
    path::{Path, PathBuf},
    time::Instant,
};

pub const JOC_RENDER_CONTROL_SCHEMA: &str = "openjoc.joc-render-control.v1";
#[cfg(test)]
pub const JOC_RENDER_LAYOUT: &str = "5.1";
#[cfg(test)]
pub const JOC_RENDER_CHANNEL_ORDER: [&str; 6] = SPEAKER_LAYOUT_5_1_CHANNELS;
pub const JOC_RENDER_SUPPORTED_LAYOUTS: [&str; 12] = SPEAKER_LAYOUT_PRESET_NAMES;
/// Product default for the internal virtual speaker field used by binaural.
/// This does not change physical speaker output semantics.
pub const DEFAULT_BINAURAL_VIRTUAL_LAYOUT: &str = "7.1.4";

/// Channel-based stereo downmix policy for the admitted 2.0 speaker output.
/// This is intentionally separate from the binaural/SOFA output mode.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum StereoDownmixPolicy {
    #[default]
    Auto,
    LoRo,
    LtRt,
}

impl StereoDownmixPolicy {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::LoRo => "loro",
            Self::LtRt => "ltrt",
        }
    }
}

#[derive(Debug)]
pub enum JocRenderError {
    Io(io::Error),
    Json(serde_json::Error),
    InvalidControl(String),
    BridgeControl(BridgeControlAssemblyError),
    Timeline(ReconstructionTimelineError),
    UnsupportedLayout(String),
    EmptyTopology,
    TopologyCoordinateCount {
        expected: usize,
        actual: usize,
    },
    BaseTopologyChanged,
    BaseCoordinate(ChannelLocation),
    FrameIndex {
        expected: u64,
        actual: u64,
    },
    SampleTimeline {
        expected: u64,
        actual: u64,
    },
    SampleRateMismatch {
        base: u32,
        frame: u32,
    },
    FrameSampleCount,
    ProfileChanged,
    UnusedUpdate {
        frame_index: u64,
    },
    Bridge(BridgeError),
    Spatial(SpatialBridgeError),
    Sofa(SofaError),
    Binaural(openjoc_render::RenderError),
    FinalLinkedGain(FinalLinkedGainError),
    BinauralHrirCoverage {
        layout: String,
        missing: Vec<String>,
    },
    BinauralLayoutNotReady {
        layout: String,
        missing: Vec<String>,
    },
    BinauralSampleRateMismatch {
        expected: u32,
        actual: u32,
    },
    BinauralLfePolicyRequired {
        layout: String,
    },
    BinauralOutput(String),
    Wave(WaveError),
    Caf(CafError),
    WavLayoutNotExactlyRepresentable {
        layout: String,
    },
    UnsupportedOutputExtension(PathBuf),
    UnsupportedCafSpeaker {
        layout: String,
        label: String,
    },
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
            Self::BridgeControl(error) => {
                write!(formatter, "automatic JOC bridge-control error: {error}")
            }
            Self::Timeline(error) => {
                write!(formatter, "JOC reconstruction timeline error: {error}")
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
            Self::Sofa(error) => write!(formatter, "JOC binaural SOFA error: {error}"),
            Self::Binaural(error) => write!(formatter, "JOC binaural render error: {error}"),
            Self::FinalLinkedGain(error) => {
                write!(formatter, "JOC final linked gain error: {error}")
            }
            Self::BinauralHrirCoverage { layout, missing } => write!(
                formatter,
                "SOFA cannot resolve exact or safely interpolated HRIR directions for virtual layout {layout}: {}",
                missing.join(", ")
            ),
            Self::BinauralLayoutNotReady { layout, missing } => write!(
                formatter,
                "binaural rendering is not currently admitted for semantic layout {layout}: no public direction mapping exists for {}; physical speaker output is independent",
                missing.join(", ")
            ),
            Self::BinauralSampleRateMismatch { expected, actual } => write!(
                formatter,
                "binaural SOFA sample rate mismatch: decoded JOC is {expected} Hz, SOFA is {actual} Hz"
            ),
            Self::BinauralLfePolicyRequired { layout } => write!(
                formatter,
                "binaural layout {layout} contains LFE; select --lfe-policy exclude or equal-power-dual-mono"
            ),
            Self::BinauralOutput(reason) => {
                write!(formatter, "JOC binaural output error: {reason}")
            }
            Self::Wave(error) => write!(formatter, "JOC render WAV error: {error}"),
            Self::Caf(error) => write!(formatter, "JOC render CAF error: {error}"),
            Self::WavLayoutNotExactlyRepresentable { layout } => write!(
                formatter,
                "semantic layout {layout} cannot be represented exactly by WAV/WAVEFORMATEXTENSIBLE; no channel identities were substituted; use .caf for semantic multichannel output"
            ),
            Self::UnsupportedOutputExtension(path) => write!(
                formatter,
                "unsupported output extension for {}; use .wav or .caf",
                path.display()
            ),
            Self::UnsupportedCafSpeaker { layout, label } => write!(
                formatter,
                "CAF cannot represent semantic speaker {label} in layout {layout} using public Core Audio descriptions"
            ),
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

impl From<BridgeControlAssemblyError> for JocRenderError {
    fn from(value: BridgeControlAssemblyError) -> Self {
        Self::BridgeControl(value)
    }
}

impl From<ReconstructionTimelineError> for JocRenderError {
    fn from(value: ReconstructionTimelineError) -> Self {
        Self::Timeline(value)
    }
}

impl From<SpatialBridgeError> for JocRenderError {
    fn from(value: SpatialBridgeError) -> Self {
        Self::Spatial(value)
    }
}

impl From<SpeakerLayoutPresetError> for JocRenderError {
    fn from(value: SpeakerLayoutPresetError) -> Self {
        match value {
            SpeakerLayoutPresetError::UnsupportedLayout(layout) => Self::UnsupportedLayout(layout),
            SpeakerLayoutPresetError::Projection(error) => Self::InvalidControl(error.to_string()),
            SpeakerLayoutPresetError::ChannelMask(error) => Self::InvalidControl(error.to_string()),
        }
    }
}

impl From<SofaError> for JocRenderError {
    fn from(value: SofaError) -> Self {
        Self::Sofa(value)
    }
}

impl From<openjoc_render::RenderError> for JocRenderError {
    fn from(value: openjoc_render::RenderError) -> Self {
        Self::Binaural(value)
    }
}

impl From<FinalLinkedGainError> for JocRenderError {
    fn from(error: FinalLinkedGainError) -> Self {
        Self::FinalLinkedGain(error)
    }
}

impl From<WaveError> for JocRenderError {
    fn from(value: WaveError) -> Self {
        Self::Wave(value)
    }
}

impl From<CafError> for JocRenderError {
    fn from(value: CafError) -> Self {
        Self::Caf(value)
    }
}

#[derive(Debug, Deserialize)]
struct RenderControlFile {
    schema: String,
    topology: SpatialTopologySnapshot,
    #[serde(default)]
    route_vectors: Vec<SpatialRouteVector>,
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
    route_vectors: Vec<SpatialRouteVector>,
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
            route_vectors: file.route_vectors,
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

    #[allow(dead_code)]
    fn reset(&mut self) {
        self.consumed_updates.fill(false);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderedBlock {
    pub sample_rate: u32,
    pub channels: Vec<Vec<f64>>,
}

#[derive(Debug)]
struct PendingRenderFrame {
    frame: DecodedPayloadFrame,
    channel_locations: Vec<ChannelLocation>,
    lfe_location: Option<ChannelLocation>,
    downmix: DownmixMetadata,
    dialnorm: DialnormState,
}

#[derive(Debug)]
pub struct JocSpeakerRenderer {
    frame_bridge: JocSpatialFrameBridge,
    bridge: JocSpatialBridge,
    preset: SpeakerLayoutPreset,
    control: Option<RenderControl>,
    assembler: Option<BridgeControlAssembler>,
    expected_coordinates: Option<usize>,
    next_input_frame: u64,
    expected_frame: u64,
    expected_sample: u64,
    timeline: ReconstructionOutputTimeline,
    pending_frames: VecDeque<PendingRenderFrame>,
    base_coordinates: Option<Vec<BaseFullBandCoordinate>>,
    selected_profile: Option<JocValidationProfile>,
    deviations: BTreeSet<String>,
    contribution_mode: SpatialContributionMode,
    downmix_policy: StereoDownmixPolicy,
    stage_timings: RenderStageTiming,
    stage_timing_enabled: bool,
    final_linked_gain: Option<FinalLinkedGain>,
    linked_gain_enabled: bool,
}

impl JocSpeakerRenderer {
    pub fn new(layout: &str, control: RenderControl) -> Result<Self, JocRenderError> {
        Self::new_with_contribution(layout, control, SpatialContributionMode::Full)
    }

    /// Returns the renderer-owned semantic channel identity and order for the
    /// selected canonical layout. Containers consume this record without
    /// changing renderer behavior.
    pub(crate) fn semantic_channel_layout(&self) -> SemanticChannelLayout {
        self.preset.semantic_channel_layout()
    }

    /// Creates a renderer with an expert-only PCM contribution diagnostic.
    pub fn new_with_contribution(
        layout: &str,
        control: RenderControl,
        contribution_mode: SpatialContributionMode,
    ) -> Result<Self, JocRenderError> {
        Self::new_with_contribution_and_linked_gain(layout, control, contribution_mode, true)
    }

    fn new_with_contribution_and_linked_gain(
        layout: &str,
        control: RenderControl,
        contribution_mode: SpatialContributionMode,
        linked_gain_enabled: bool,
    ) -> Result<Self, JocRenderError> {
        let mut preset = SpeakerLayoutPreset::for_name(layout)?;
        preset.layout = preset
            .layout
            .with_route_vectors(control.route_vectors.clone())
            .map_err(|error| JocRenderError::InvalidControl(error.to_string()))?;
        let expected_coordinates = control.topology.flatten().len();
        if expected_coordinates == 0 {
            return Err(JocRenderError::EmptyTopology);
        }
        Ok(Self {
            frame_bridge: JocSpatialFrameBridge,
            bridge: JocSpatialBridge::new(),
            preset,
            control: Some(control),
            assembler: None,
            expected_coordinates: Some(expected_coordinates),
            next_input_frame: 0,
            expected_frame: 0,
            expected_sample: 0,
            timeline: ReconstructionOutputTimeline::new(),
            pending_frames: VecDeque::new(),
            base_coordinates: None,
            selected_profile: None,
            deviations: BTreeSet::new(),
            contribution_mode,
            downmix_policy: StereoDownmixPolicy::Auto,
            stage_timings: RenderStageTiming::default(),
            stage_timing_enabled: false,
            final_linked_gain: None,
            linked_gain_enabled,
        })
    }

    /// Creates a renderer whose bridge control is assembled from each decoded
    /// real JOC/OAMD frame. The explicit topology sidecar remains available
    /// through [`Self::new`] for overrides and synthetic fixtures.
    pub fn new_automatic(layout: &str) -> Result<Self, JocRenderError> {
        Self::new_automatic_with_contribution(layout, SpatialContributionMode::Full)
    }

    /// Creates an automatic-control renderer with diagnostic PCM selection.
    pub fn new_automatic_with_contribution(
        layout: &str,
        contribution_mode: SpatialContributionMode,
    ) -> Result<Self, JocRenderError> {
        Self::new_automatic_with_contribution_and_linked_gain(layout, contribution_mode, true)
    }

    fn new_automatic_with_contribution_and_linked_gain(
        layout: &str,
        contribution_mode: SpatialContributionMode,
        linked_gain_enabled: bool,
    ) -> Result<Self, JocRenderError> {
        let preset = SpeakerLayoutPreset::for_name(layout)?;
        let dimensions = preset.layout.coordinate_dimension_count();
        let base_projection_enabled = preset.name != "2.0";
        Ok(Self {
            frame_bridge: JocSpatialFrameBridge,
            bridge: JocSpatialBridge::new(),
            preset,
            control: None,
            assembler: Some(BridgeControlAssembler::new_with_base_projection(
                64,
                dimensions,
                base_projection_enabled,
            )),
            expected_coordinates: None,
            next_input_frame: 0,
            expected_frame: 0,
            expected_sample: 0,
            timeline: ReconstructionOutputTimeline::new(),
            pending_frames: VecDeque::new(),
            base_coordinates: None,
            selected_profile: None,
            deviations: BTreeSet::new(),
            contribution_mode,
            downmix_policy: StereoDownmixPolicy::Auto,
            stage_timings: RenderStageTiming::default(),
            stage_timing_enabled: false,
            final_linked_gain: None,
            linked_gain_enabled,
        })
    }

    /// Selects the channel-based stereo policy for the 2.0 speaker preset.
    /// Other speaker layouts reject an explicit stereo matrix policy.
    pub fn set_downmix_policy(
        &mut self,
        policy: StereoDownmixPolicy,
    ) -> Result<(), JocRenderError> {
        if self.preset.name != "2.0" && policy != StereoDownmixPolicy::Auto {
            return Err(JocRenderError::InvalidControl(
                "--downmix is only meaningful with --layout 2.0".to_owned(),
            ));
        }
        self.downmix_policy = policy;
        Ok(())
    }

    /// Compatibility entry point for already aligned/synthetic bridge frames.
    /// The normal `render-joc` path uses [`Self::render_frame_aligned`] so raw
    /// causal QMF output cannot bypass reconstruction timeline ownership.
    #[allow(dead_code)]
    pub fn render_frame(
        &mut self,
        frame_index: usize,
        frame: &DecodedPayloadFrame,
        base: &DecodedAccessUnitPcm,
    ) -> Result<RenderedBlock, JocRenderError> {
        self.render_aligned_frame(frame_index, frame, base)
    }

    /// Queues raw reconstruction output and returns all complete logical
    /// intervals available after the declared QMF delay.
    pub fn render_frame_aligned(
        &mut self,
        frame_index: usize,
        frame: &DecodedPayloadFrame,
        base: &DecodedAccessUnitPcm,
    ) -> Result<Vec<RenderedBlock>, JocRenderError> {
        let frame_index =
            u64::try_from(frame_index).map_err(|_| JocRenderError::FrameSampleCount)?;
        if frame_index != self.next_input_frame || frame.frame_index != frame_index {
            return Err(JocRenderError::FrameIndex {
                expected: self.next_input_frame,
                actual: frame_index,
            });
        }
        let state_reset = frame.decoded.state_reset;
        if state_reset {
            self.timeline.reset();
            self.pending_frames.clear();
            if let Some(assembler) = self.assembler.as_mut() {
                assembler.reset();
            }
            if let Some(control) = self.control.as_mut() {
                control.reset();
            }
            self.base_coordinates = None;
            self.expected_coordinates = None;
            self.expected_frame = frame_index;
            self.expected_sample = frame.sample_range.start_sample;
        }
        let aligned = self.timeline.push_frame(
            frame.frame_index,
            frame.sample_rate,
            frame.sample_range.start_sample,
            frame.sample_range.end_sample,
            &base.channels,
            &frame.decoded.reconstruction_basis,
            base.lfe.as_deref(),
            false,
        )?;
        self.pending_frames.push_back(PendingRenderFrame {
            frame: frame.clone(),
            channel_locations: base.channel_locations.clone(),
            lfe_location: base.lfe_location,
            downmix: base.downmix,
            dialnorm: base.dialnorm,
        });
        self.next_input_frame = self
            .next_input_frame
            .checked_add(1)
            .ok_or(JocRenderError::FrameSampleCount)?;
        self.render_aligned_outputs(aligned)
    }

    fn render_aligned_frame(
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
        let calibrated_base = base.with_dialnorm_applied();
        let mut calibrated_frame = frame.clone();
        for row in &mut calibrated_frame.decoded.reconstruction_basis.rows {
            base.dialnorm.apply_to_samples(row);
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
        if let Some(actual) = self.expected_coordinates {
            if expected != actual {
                return Err(JocRenderError::TopologyCoordinateCount { expected, actual });
            }
        } else {
            self.expected_coordinates = Some(expected);
        }
        let bridge_frame = self.frame_bridge.frame(
            &calibrated_frame,
            &base_coordinates,
            &calibrated_base.channels,
            calibrated_base.lfe.as_deref(),
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
        let stereo_speaker = self.preset.name == "2.0";
        let automatic_frame = if let Some(assembler) = self.assembler.as_mut() {
            let start = self.stage_timing_enabled.then(Instant::now);
            let frame = assembler.assemble_frame(&calibrated_frame, &base_coordinates, None)?;
            if let Some(start) = start {
                self.stage_timings.bridge_control_assembly += start.elapsed();
            }
            Some(frame)
        } else {
            None
        };
        if let Some(control_frame) = automatic_frame {
            let start = self.stage_timing_enabled.then(Instant::now);
            self.render_automatic_segments(
                &bridge_frame,
                &control_frame,
                sample_count,
                &calibrated_base,
                &mut active,
                stereo_speaker,
            )?;
            if let Some(start) = start {
                self.stage_timings.spatial_bridge_render += start.elapsed();
            }
        } else {
            let mut output_planes = active.iter_mut().map(Vec::as_mut_slice).collect::<Vec<_>>();
            let control = self.control.as_mut().ok_or_else(|| {
                JocRenderError::InvalidControl("missing explicit control source".to_owned())
            })?;
            let update_index = control.mark_update_for(frame_index);
            let topology = (self.expected_frame == 0 || calibrated_frame.decoded.state_reset)
                .then_some(&control.topology);
            let updates = update_index.map(|index| control.updates[index].updates.as_slice());
            let start = self.stage_timing_enabled.then(Instant::now);
            let bridge_contribution = if stereo_speaker {
                SpatialContributionMode::ReconstructionOnly
            } else {
                self.contribution_mode
            };
            if !stereo_speaker || self.contribution_mode.includes_reconstruction() {
                self.bridge.render_codec_basis_frame_with_contribution(
                    &bridge_frame,
                    bridge_contribution,
                    topology,
                    updates,
                    &self.preset.layout,
                    u64::try_from(sample_count).map_err(|_| JocRenderError::FrameSampleCount)?,
                    &mut output_planes,
                )?;
            }
            if let Some(start) = start {
                self.stage_timings.spatial_bridge_render += start.elapsed();
            }
            if stereo_speaker && self.contribution_mode.includes_base() {
                add_stereo_base_downmix(&mut active, &calibrated_base, self.downmix_policy)?;
            }
        }

        let mut channels = vec![vec![0.0; sample_count]; self.preset.channel_count()];
        let mut active_index = 0;
        for (output_index, channel) in self.preset.layout.channels().iter().enumerate() {
            if channel.lfe {
                if self.contribution_mode.includes_base() {
                    if let Some(lfe) = calibrated_base.lfe.as_deref() {
                        channels[output_index].copy_from_slice(lfe);
                    }
                }
            } else {
                channels[output_index].copy_from_slice(&active[active_index]);
                active_index += 1;
            }
        }

        self.apply_final_linked_gain(
            frame.sample_rate,
            &mut channels,
            calibrated_base.lfe.as_deref(),
        )?;
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

    fn apply_final_linked_gain(
        &mut self,
        sample_rate: u32,
        channels: &mut [Vec<f64>],
        lfe: Option<&[f64]>,
    ) -> Result<(), JocRenderError> {
        if !self.linked_gain_enabled {
            return Ok(());
        }
        let sample_count = channels.first().map_or(0, Vec::len);
        // The public E-AC-3 adapter supplies 1536-sample frames, which are
        // split into the admitted 32-sample linked-gain blocks. Short blocks
        // are retained for the renderer's existing synthetic compatibility
        // entry points; they are not an admitted adapter processing call.
        if sample_count != 1536
            && sample_count != FINAL_LINKED_GAIN_BLOCK_SAMPLES
            && sample_count != 40
        {
            return Ok(());
        }
        let active_lfe = lfe.is_some_and(|samples| !samples.is_empty());
        let active_channels = self
            .preset
            .layout
            .channels()
            .iter()
            .map(|channel| if channel.lfe { active_lfe } else { true })
            .collect::<Vec<_>>();
        let linked_gain = if let Some(linked_gain) = self.final_linked_gain.as_mut() {
            linked_gain.reconfigure(
                sample_rate,
                FINAL_LINKED_GAIN_BLOCK_SAMPLES,
                &active_channels,
            )?;
            linked_gain
        } else {
            self.final_linked_gain = Some(FinalLinkedGain::new(
                sample_rate,
                FINAL_LINKED_GAIN_BLOCK_SAMPLES,
                &active_channels,
            )?);
            self.final_linked_gain
                .as_mut()
                .expect("linked gain was just initialized")
        };
        linked_gain.process(channels)?;
        Ok(())
    }

    fn render_aligned_outputs(
        &mut self,
        aligned: Vec<AlignedReconstructionOutput>,
    ) -> Result<Vec<RenderedBlock>, JocRenderError> {
        let mut rendered = Vec::with_capacity(aligned.len());
        for aligned in aligned {
            let pending = self.pending_frames.pop_front().ok_or_else(|| {
                JocRenderError::InvalidControl("aligned frame queue underflow".to_owned())
            })?;
            if pending.frame.frame_index != aligned.frame_index {
                return Err(JocRenderError::FrameIndex {
                    expected: pending.frame.frame_index,
                    actual: aligned.frame_index,
                });
            }
            let aligned_base = aligned_base_pcm(&pending, &aligned);
            let mut aligned_frame = pending.frame;
            aligned_frame.decoded.reconstruction_basis = aligned.reconstruction_basis;
            rendered.push(
                self.render_aligned_frame(
                    usize::try_from(aligned_frame.frame_index)
                        .map_err(|_| JocRenderError::FrameSampleCount)?,
                    &aligned_frame,
                    &aligned_base,
                )?,
            );
        }
        Ok(rendered)
    }

    fn render_automatic_segments(
        &mut self,
        bridge_frame: &openjoc_scene::JocSpatialReconstructionFrame<'_>,
        control_frame: &BridgeControlFrame,
        sample_count: usize,
        base: &DecodedAccessUnitPcm,
        active: &mut [Vec<f64>],
        stereo_speaker: bool,
    ) -> Result<(), JocRenderError> {
        let mut boundaries = vec![0_usize, sample_count];
        for event in &control_frame.events {
            let start = usize::try_from(
                event
                    .quantum
                    .checked_mul(32)
                    .ok_or(JocRenderError::FrameSampleCount)?,
            )
            .map_err(|_| JocRenderError::FrameSampleCount)?;
            if start < sample_count {
                boundaries.push(start);
            }
        }
        boundaries.sort_unstable();
        boundaries.dedup();
        let zero_storage = (stereo_speaker
            || !matches!(self.contribution_mode, SpatialContributionMode::Full))
        .then(|| vec![0.0; sample_count]);
        let zero_pcm = zero_storage.as_deref().unwrap_or(&[]);
        let mut coordinates = Vec::with_capacity(
            bridge_frame.basis.base_full_band_pcm.len()
                + bridge_frame.basis.reconstruction_basis.rows.len(),
        );
        coordinates.extend(bridge_frame.basis.base_full_band_pcm.iter().map(|pcm| {
            if !stereo_speaker && self.contribution_mode.includes_base() {
                pcm.as_slice()
            } else {
                zero_pcm
            }
        }));
        coordinates.extend(
            bridge_frame
                .basis
                .reconstruction_basis
                .rows
                .iter()
                .map(|pcm| {
                    if self.contribution_mode.includes_reconstruction() {
                        pcm.as_slice()
                    } else {
                        zero_pcm
                    }
                }),
        );
        for window in boundaries.windows(2) {
            let start = window[0];
            let end = window[1];
            if start == end {
                continue;
            }
            let event = control_frame.events.iter().find(|event| {
                usize::try_from(event.quantum.saturating_mul(32)).ok() == Some(start)
            });
            let updates = event.map(|event| event.updates.as_slice());
            let ramp_duration = event.map_or(0, |event| u64::from(event.ramp_duration));
            let sliced_coordinates = coordinates
                .iter()
                .map(|coordinate| &coordinate[start..end])
                .collect::<Vec<_>>();
            let mut output_planes = active
                .iter_mut()
                .map(|channel| &mut channel[start..end])
                .collect::<Vec<_>>();
            let topology = (start == 0)
                .then_some(control_frame.initial_topology.as_ref())
                .flatten();
            self.bridge.render_coordinates(
                &sliced_coordinates,
                topology,
                updates,
                &self.preset.layout,
                ramp_duration,
                bridge_frame.sample_rate,
                &mut output_planes,
            )?;
        }
        if stereo_speaker && self.contribution_mode.includes_base() {
            add_stereo_base_downmix(active, base, self.downmix_policy)?;
        }
        Ok(())
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
        if let Some(control) = &self.control {
            control.finish()
        } else {
            Ok(())
        }
    }

    /// Flushes reconstruction-owned QMF tail state and renders all pending
    /// logical frames on the common Base/ReconstructionBasis timeline.
    pub fn finish_with_reconstruction_tail(
        &mut self,
        reconstruction_tail: &ReconstructionBasis,
    ) -> Result<Vec<RenderedBlock>, JocRenderError> {
        let aligned = self.timeline.finish(reconstruction_tail)?;
        let mut rendered = self.render_aligned_outputs(aligned)?;
        if !self.pending_frames.is_empty() {
            return Err(JocRenderError::InvalidControl(
                "reconstruction timeline left pending frames".to_owned(),
            ));
        }
        self.finish()?;
        if self.linked_gain_enabled {
            if let Some(linked_gain) = self.final_linked_gain.as_mut() {
                let sample_rate = linked_gain.sample_rate();
                let channels = linked_gain.drain()?;
                rendered.push(RenderedBlock {
                    sample_rate,
                    channels,
                });
            }
        }
        Ok(rendered)
    }

    pub(crate) fn take_stage_timings(&mut self) -> RenderStageTiming {
        std::mem::take(&mut self.stage_timings)
    }

    pub(crate) fn enable_stage_timing(&mut self) {
        self.stage_timing_enabled = true;
    }

    /// Resets bridge, automatic assembly, timeline, and explicit-update state.
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.bridge.reset();
        if let Some(assembler) = self.assembler.as_mut() {
            assembler.reset();
        }
        if let Some(control) = self.control.as_mut() {
            control.reset();
        }
        self.timeline.reset();
        self.pending_frames.clear();
        self.next_input_frame = 0;
        self.expected_frame = 0;
        self.expected_sample = 0;
        self.base_coordinates = None;
        self.selected_profile = None;
        self.deviations.clear();
        self.stage_timings = RenderStageTiming::default();
        if let Some(linked_gain) = self.final_linked_gain.as_mut() {
            linked_gain.reset();
        }
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
        let contribution_diagnostic = if self.contribution_mode == SpatialContributionMode::Full {
            String::new()
        } else {
            format!(
                "\ndiagnostic contribution: {} (experimental fidelity isolation)",
                self.contribution_mode.as_str()
            )
        };
        let (output_layout, output_order, speaker_identities) =
            speaker_output_presentation(&self.preset);
        format!(
            "feature: JocSpatialBridge\nimplementation maturity: Experimental\nsemantic binding: Unresolved\nrequested layout: {requested_layout}\nselected layout: {}\noutput layout: {}\nchannel count: {}\nLFE index: {:?}\ndownmix policy: {}\nrequested profile: {}\nselected profile: {}\ncompatibility deviations: {}\nQMF round-trip latency: {} samples\noutput: {}\nsample rate: {} Hz\nframes: {}\nsamples: {}\noutput channel order: {}\nspeaker identities: {}\nraw3: preserved and excluded from projection arithmetic{}",
            self.preset.name,
            output_layout,
            self.preset.channel_count(),
            self.preset.lfe_index(),
            self.downmix_policy.as_str(),
            requested_profile.as_str(),
            selected.as_str(),
            deviations,
            ReconstructionOutputTimeline::qmf_latency_samples(),
            output.display(),
            summary.sample_rate,
            summary.frames,
            summary.duration_samples,
            output_order,
            speaker_identities,
            contribution_diagnostic,
        )
    }
}

/// Selects the existing static binaural convolution implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinauralBackend {
    Direct,
    Partitioned { partition_size: usize },
}

impl BinauralBackend {
    fn name(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Partitioned { .. } => "partitioned",
        }
    }
}

/// Explicit renderer policy for the virtual speaker layout's LFE channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinauralLfePolicy {
    /// Do not include the layout LFE in the binaural signal.
    Exclude,
    /// Add the layout LFE equally to both ears with -3.0103 dB gain.
    EqualPowerDualMono,
}

impl BinauralLfePolicy {
    fn name(self) -> &'static str {
        match self {
            Self::Exclude => "exclude",
            Self::EqualPowerDualMono => "equal-power-dual-mono",
        }
    }
}

/// Deterministic public-speaker to static-HRIR binding used by JOC binaural.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BinauralSpeakerMapping {
    pub label: &'static str,
    pub channel_index: usize,
    pub direction: CartesianPosition,
    pub source_id: SourceId,
    pub hrir_entry: HrirEntryId,
    pub interpolated: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BinauralRenderedBlock {
    pub sample_rate: u32,
    pub left: Vec<f64>,
    pub right: Vec<f64>,
}

enum BinauralEngine {
    Direct(BinauralRenderer),
    Partitioned(Box<PartitionedBinauralRenderer>),
}

/// Real-JOC speaker virtualization followed by static SOFA HRIR binaural
/// rendering. Exact directions are preferred and safely covered directions
/// are prepared by `openjoc-sofa` interpolation. This is an OpenJOC renderer
/// path, not a vendor-fidelity or direct-object binaural implementation.
pub struct JocBinauralRenderer {
    speaker: JocSpeakerRenderer,
    layout: String,
    bank: HrirBank,
    sofa_sample_rate: u32,
    backend: BinauralBackend,
    lfe_policy: Option<BinauralLfePolicy>,
    lfe_index: Option<usize>,
    mappings: Vec<BinauralSpeakerMapping>,
    interpolated_hrir_count: usize,
    max_taps: usize,
    engine: Option<BinauralEngine>,
    sample_rate: Option<u32>,
    pending_sources: Vec<Vec<f64>>,
    pending_lfe: Vec<f64>,
    pending_len: usize,
    finished: bool,
    stage_timings: RenderStageTiming,
    stage_timing_enabled: bool,
}

impl JocBinauralRenderer {
    /// Creates a preflighted JOC binaural renderer from a strict SOFA bank.
    ///
    /// Every non-LFE public speaker channel is resolved before any input PCM
    /// is rendered. Exact-direction lookup and the bank's sample-rate contract
    /// are otherwise delegated unchanged to the existing renderer APIs.
    pub fn new(
        layout: &str,
        bank: HrirBank,
        backend: BinauralBackend,
        lfe_policy: Option<BinauralLfePolicy>,
        control: Option<RenderControl>,
    ) -> Result<Self, JocRenderError> {
        Self::new_with_contribution(
            layout,
            bank,
            backend,
            lfe_policy,
            control,
            SpatialContributionMode::Full,
        )
    }

    /// Creates the same virtualized-speaker path with diagnostic PCM selection.
    pub fn new_with_contribution(
        layout: &str,
        bank: HrirBank,
        backend: BinauralBackend,
        lfe_policy: Option<BinauralLfePolicy>,
        control: Option<RenderControl>,
        contribution_mode: SpatialContributionMode,
    ) -> Result<Self, JocRenderError> {
        let preset = SpeakerLayoutPreset::for_name(layout)?;
        validate_binaural_layout_preset(layout, &preset)?;
        if preset.lfe_index().is_some() && lfe_policy.is_none() {
            return Err(JocRenderError::BinauralLfePolicyRequired {
                layout: layout.to_owned(),
            });
        }
        if let BinauralBackend::Partitioned { partition_size } = backend {
            UniformPartitionedConfig::new(partition_size)?;
        }
        let source_bank = bank;
        let mut prepared_entries = source_bank.entries().to_vec();
        let mut next_interpolated_id = u64::MAX;
        let mut mappings = Vec::with_capacity(preset.channel_count().saturating_sub(1));
        let mut missing = Vec::new();
        let mut interpolated_hrir_count = 0;
        let mut max_taps = 0;
        for (channel_index, label) in preset.labels.iter().enumerate() {
            if preset.lfe_index() == Some(channel_index) {
                continue;
            }
            let Some(direction) = virtual_speaker_direction(label) else {
                return Err(JocRenderError::InvalidControl(format!(
                    "no binaural direction is defined for public speaker {label}"
                )));
            };
            let resolved = match resolve_hrir(&source_bank, direction) {
                Ok(resolved) => resolved,
                Err(error) => {
                    missing.push(format!("{label}={direction:?}: {error}"));
                    continue;
                }
            };
            let (hrir_entry, interpolated) = if let Some(exact_entry) = resolved.exact_entry {
                (exact_entry, false)
            } else {
                while prepared_entries
                    .iter()
                    .any(|entry| entry.id() == HrirEntryId::new(next_interpolated_id))
                {
                    next_interpolated_id = next_interpolated_id
                        .checked_sub(1)
                        .ok_or(JocRenderError::FrameSampleCount)?;
                }
                let entry_id = HrirEntryId::new(next_interpolated_id);
                next_interpolated_id = next_interpolated_id
                    .checked_sub(1)
                    .ok_or(JocRenderError::FrameSampleCount)?;
                let entry = HrirEntry::new(entry_id, direction, resolved.pair)?;
                max_taps = max_taps.max(entry.pair().tap_count());
                prepared_entries.push(entry);
                interpolated_hrir_count += 1;
                (entry_id, true)
            };
            let entry = prepared_entries
                .iter()
                .find(|entry| entry.id() == hrir_entry)
                .ok_or(JocRenderError::FrameSampleCount)?;
            max_taps = max_taps.max(entry.pair().tap_count());
            let source_id = SourceId::new(
                u64::try_from(channel_index)
                    .map_err(|_| JocRenderError::FrameSampleCount)?
                    .checked_add(1)
                    .ok_or(JocRenderError::FrameSampleCount)?,
            );
            mappings.push(BinauralSpeakerMapping {
                label,
                channel_index,
                direction,
                source_id,
                hrir_entry,
                interpolated,
            });
        }
        if !missing.is_empty() {
            return Err(JocRenderError::BinauralHrirCoverage {
                layout: layout.to_owned(),
                missing,
            });
        }
        let sample_rate_hz = source_bank.sample_rate_hz();
        drop(source_bank);
        let bank = HrirBank::new(sample_rate_hz, prepared_entries)?;
        let speaker = match control {
            Some(control) => JocSpeakerRenderer::new_with_contribution_and_linked_gain(
                layout,
                control,
                contribution_mode,
                false,
            )?,
            None => JocSpeakerRenderer::new_automatic_with_contribution_and_linked_gain(
                layout,
                contribution_mode,
                false,
            )?,
        };
        Ok(Self {
            speaker,
            layout: layout.to_owned(),
            sofa_sample_rate: bank.sample_rate_hz(),
            bank,
            backend,
            lfe_policy,
            lfe_index: preset.lfe_index(),
            mappings,
            interpolated_hrir_count,
            max_taps,
            engine: None,
            sample_rate: None,
            pending_sources: Vec::new(),
            pending_lfe: Vec::new(),
            pending_len: 0,
            finished: false,
            stage_timings: RenderStageTiming::default(),
            stage_timing_enabled: false,
        })
    }

    /// Returns the maximum causal HRIR tail in samples.
    #[must_use]
    pub const fn tail_samples(&self) -> usize {
        self.max_taps.saturating_sub(1)
    }

    /// Renders one decoded JOC frame and returns zero or more stereo blocks.
    /// Partitioned rendering may return several fixed-size blocks because the
    /// decoder frame size need not equal the selected convolution partition.
    /// Compatibility entry point for already aligned/synthetic bridge frames.
    #[allow(dead_code)]
    pub fn render_frame(
        &mut self,
        frame_index: usize,
        frame: &DecodedPayloadFrame,
        base: &DecodedAccessUnitPcm,
    ) -> Result<Vec<BinauralRenderedBlock>, JocRenderError> {
        if self.finished {
            return Err(JocRenderError::BinauralOutput(
                "renderer has already been finalized".to_owned(),
            ));
        }
        self.ensure_engine(frame.sample_rate)?;
        let rendered = self.speaker.render_frame(frame_index, frame, base)?;
        let start = self.stage_timing_enabled.then(Instant::now);
        let result = match self.backend {
            BinauralBackend::Direct => self.render_direct_block(&rendered),
            BinauralBackend::Partitioned { .. } => self.render_partitioned_block(&rendered),
        }?;
        if let Some(start) = start {
            self.stage_timings.binaural_render += start.elapsed();
        }
        Ok(result)
    }

    /// Queues raw reconstruction output and renders all complete aligned
    /// intervals available after the declared QMF delay.
    pub fn render_frame_aligned(
        &mut self,
        frame_index: usize,
        frame: &DecodedPayloadFrame,
        base: &DecodedAccessUnitPcm,
    ) -> Result<Vec<BinauralRenderedBlock>, JocRenderError> {
        if self.finished {
            return Err(JocRenderError::BinauralOutput(
                "renderer has already been finalized".to_owned(),
            ));
        }
        self.ensure_engine(frame.sample_rate)?;
        let rendered = self
            .speaker
            .render_frame_aligned(frame_index, frame, base)?;
        let speaker_timings = self.speaker.take_stage_timings();
        self.stage_timings.bridge_control_assembly += speaker_timings.bridge_control_assembly;
        self.stage_timings.spatial_bridge_render += speaker_timings.spatial_bridge_render;
        let start = self.stage_timing_enabled.then(Instant::now);
        let mut result = Vec::new();
        for block in &rendered {
            let mut blocks = match self.backend {
                BinauralBackend::Direct => self.render_direct_block(block)?,
                BinauralBackend::Partitioned { .. } => self.render_partitioned_block(block)?,
            };
            result.append(&mut blocks);
        }
        if let Some(start) = start {
            self.stage_timings.binaural_render += start.elapsed();
        }
        Ok(result)
    }

    /// Records the selected validation profile without changing JOC policy.
    pub fn record_profile(&mut self, metadata: &JocMetadataFrame) -> Result<(), JocRenderError> {
        self.speaker.record_profile(metadata)
    }

    /// Resets the persistent JOC and binaural states for stream reuse.
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.speaker.reset();
        if let Some(engine) = self.engine.as_mut() {
            match engine {
                BinauralEngine::Direct(renderer) => renderer.reset(),
                BinauralEngine::Partitioned(renderer) => renderer.reset(),
            }
        }
        for source in &mut self.pending_sources {
            source.fill(0.0);
        }
        self.pending_lfe.fill(0.0);
        self.pending_len = 0;
        self.finished = false;
        self.stage_timings = RenderStageTiming::default();
    }

    pub(crate) fn take_stage_timings(&mut self) -> RenderStageTiming {
        std::mem::take(&mut self.stage_timings)
    }

    pub(crate) fn enable_stage_timing(&mut self) {
        self.stage_timing_enabled = true;
        self.speaker.enable_stage_timing();
    }

    /// Finishes bridge validation and drains the complete binaural FIR tail.
    #[allow(dead_code)]
    pub fn finish(&mut self) -> Result<Vec<BinauralRenderedBlock>, JocRenderError> {
        if self.finished {
            return Err(JocRenderError::BinauralOutput(
                "renderer has already been finalized".to_owned(),
            ));
        }
        self.speaker.finish()?;
        self.finish_after_speaker_blocks(&[])
    }

    /// Flushes the reconstruction timeline before draining the binaural FIR
    /// tail, preserving the final aligned ReconstructionBasis samples.
    pub fn finish_with_reconstruction_tail(
        &mut self,
        reconstruction_tail: &ReconstructionBasis,
    ) -> Result<Vec<BinauralRenderedBlock>, JocRenderError> {
        if self.finished {
            return Err(JocRenderError::BinauralOutput(
                "renderer has already been finalized".to_owned(),
            ));
        }
        let speaker_blocks = self
            .speaker
            .finish_with_reconstruction_tail(reconstruction_tail)?;
        self.finish_after_speaker_blocks(&speaker_blocks)
    }

    fn finish_after_speaker_blocks(
        &mut self,
        speaker_blocks: &[RenderedBlock],
    ) -> Result<Vec<BinauralRenderedBlock>, JocRenderError> {
        let start = self.stage_timing_enabled.then(Instant::now);
        let mut output = Vec::new();
        for block in speaker_blocks {
            let mut blocks = match self.backend {
                BinauralBackend::Direct => self.render_direct_block(block)?,
                BinauralBackend::Partitioned { .. } => self.render_partitioned_block(block)?,
            };
            output.append(&mut blocks);
        }
        if let Some(start) = start {
            self.stage_timings.binaural_render += start.elapsed();
        }
        let sample_rate = self.sample_rate.ok_or(JocRenderError::NoRenderedFrames)?;
        match self
            .engine
            .as_mut()
            .ok_or(JocRenderError::NoRenderedFrames)?
        {
            BinauralEngine::Direct(renderer) => {
                drain_direct_tail(renderer, sample_rate, self.max_taps, &mut output)?;
            }
            BinauralEngine::Partitioned(renderer) => {
                let partition_size = renderer.partition_size();
                if self.pending_len > 0 {
                    let blocks = self
                        .pending_sources
                        .iter()
                        .zip(&self.mappings)
                        .map(|(source, mapping)| {
                            BinauralSourceBlock::new(mapping.source_id, &source[..self.pending_len])
                        })
                        .collect::<Vec<_>>();
                    let mut left = vec![0.0; self.pending_len];
                    let mut right = vec![0.0; self.pending_len];
                    renderer.finish_input(&blocks, self.pending_len, &mut left, &mut right)?;
                    add_lfe(
                        self.lfe_policy,
                        &self.pending_lfe[..self.pending_len],
                        &mut left,
                        &mut right,
                    )?;
                    output.push(BinauralRenderedBlock {
                        sample_rate,
                        left,
                        right,
                    });
                    self.pending_len = 0;
                } else {
                    let blocks = self
                        .mappings
                        .iter()
                        .map(|mapping| BinauralSourceBlock::new(mapping.source_id, &[]))
                        .collect::<Vec<_>>();
                    let mut left = Vec::new();
                    let mut right = Vec::new();
                    renderer.finish_input(&blocks, 0, &mut left, &mut right)?;
                }
                drain_partitioned_tail(renderer, sample_rate, partition_size, &mut output)?;
            }
        }
        self.finished = true;
        Ok(output)
    }

    /// Returns the user-facing diagnostic summary for this mode.
    pub fn diagnostics(
        &self,
        sofa_file: &Path,
        requested_profile: crate::eac3_decode::ValidationProfileRequest,
        selected_profile: JocValidationProfile,
        summary: &openjoc_scene::StreamingSceneSummary,
        output: &Path,
        output_format: SampleFormat,
    ) -> String {
        let contribution_diagnostic =
            if self.speaker.contribution_mode == SpatialContributionMode::Full {
                String::new()
            } else {
                format!(
                    "\ndiagnostic contribution: {} (experimental fidelity isolation)",
                    self.speaker.contribution_mode.as_str()
                )
            };
        let output_container =
            output_container_for_path(output).map_or("unknown", |container| container.name());
        format!(
            "feature: JocSpatialBridge\nimplementation maturity: Experimental\nsemantic binding: Unresolved\nvalidation profile: requested={} selected={}\noutput layout: Binaural stereo\noutput mode: binaural stereo (L/R ears)\nphysical output channel count: 2\nvirtual speaker layout: {}\nvirtual speaker count: {}\nSOFA: {}\nHRIR coverage: {} exact, {} interpolated\nbinaural backend: {}\nalgorithmic latency: {} samples\nLFE policy: {}\nsample rate: {} Hz\ninput samples: {}\nconvolution tail: {} samples\noutput samples: {}\noutput channel order: Left Ear, Right Ear\noutput format: {} {}\noutput: {}\nraw3: preserved and excluded from projection arithmetic\nautomatic bridge-control: enabled unless --topology is supplied\nCONTROL.json requirement: none\nvendor binaural fidelity: not claimed{}",
            requested_profile.as_str(),
            selected_profile.as_str(),
            self.layout,
            self.mappings.len(),
            sofa_file
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("<unnamed>"),
            self.mappings
                .len()
                .saturating_sub(self.interpolated_hrir_count),
            self.interpolated_hrir_count,
            self.backend.name(),
            match self.backend {
                BinauralBackend::Direct => 0,
                BinauralBackend::Partitioned { partition_size } => partition_size,
            },
            self.lfe_policy
                .map_or("not-applicable", BinauralLfePolicy::name),
            summary.sample_rate,
            summary.duration_samples,
            self.tail_samples(),
            summary
                .duration_samples
                .saturating_add(self.tail_samples() as u64),
            match output_format {
                SampleFormat::F32 => "IEEE float32 stereo",
                SampleFormat::F64 => "IEEE float64 stereo",
                SampleFormat::S24 => "signed PCM24 stereo",
                SampleFormat::S16 => "signed PCM16 stereo",
            },
            output_container,
            output.display(),
            contribution_diagnostic,
        )
    }

    fn ensure_engine(&mut self, sample_rate: u32) -> Result<(), JocRenderError> {
        if sample_rate != self.sofa_sample_rate {
            return Err(JocRenderError::BinauralSampleRateMismatch {
                expected: sample_rate,
                actual: self.sofa_sample_rate,
            });
        }
        if let Some(previous) = self.sample_rate {
            if previous != sample_rate {
                return Err(JocRenderError::BinauralSampleRateMismatch {
                    expected: previous,
                    actual: sample_rate,
                });
            }
            return Ok(());
        }
        let definitions = self
            .mappings
            .iter()
            .map(|mapping| {
                StaticBinauralSource::new(
                    mapping.source_id,
                    mapping.direction,
                    1.0,
                    mapping.hrir_entry,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.engine = Some(match self.backend {
            BinauralBackend::Direct => BinauralEngine::Direct(BinauralRenderer::new(
                sample_rate,
                self.bank.clone(),
                definitions,
            )?),
            BinauralBackend::Partitioned { partition_size } => {
                let config = UniformPartitionedConfig::new(partition_size)?;
                BinauralEngine::Partitioned(Box::new(PartitionedBinauralRenderer::new(
                    sample_rate,
                    config,
                    self.bank.clone(),
                    definitions,
                )?))
            }
        });
        if let BinauralBackend::Partitioned { partition_size } = self.backend {
            self.pending_sources = self
                .mappings
                .iter()
                .map(|_| vec![0.0; partition_size])
                .collect();
            self.pending_lfe = vec![0.0; partition_size];
        }
        self.sample_rate = Some(sample_rate);
        Ok(())
    }

    fn render_direct_block(
        &mut self,
        rendered: &RenderedBlock,
    ) -> Result<Vec<BinauralRenderedBlock>, JocRenderError> {
        let blocks = self
            .mappings
            .iter()
            .map(|mapping| {
                BinauralSourceBlock::new(
                    mapping.source_id,
                    &rendered.channels[mapping.channel_index],
                )
            })
            .collect::<Vec<_>>();
        let mut left = vec![0.0; rendered.channels[0].len()];
        let mut right = vec![0.0; rendered.channels[0].len()];
        let BinauralEngine::Direct(binaural) = self
            .engine
            .as_mut()
            .ok_or(JocRenderError::NoRenderedFrames)?
        else {
            return Err(JocRenderError::BinauralOutput(
                "direct engine is unavailable".to_owned(),
            ));
        };
        binaural.render_block(&blocks, &mut left, &mut right)?;
        if let Some(lfe_index) = self.lfe_index {
            add_lfe(
                self.lfe_policy,
                &rendered.channels[lfe_index],
                &mut left,
                &mut right,
            )?;
        }
        Ok(vec![BinauralRenderedBlock {
            sample_rate: rendered.sample_rate,
            left,
            right,
        }])
    }

    fn render_partitioned_block(
        &mut self,
        rendered: &RenderedBlock,
    ) -> Result<Vec<BinauralRenderedBlock>, JocRenderError> {
        let partition_size = match self.backend {
            BinauralBackend::Partitioned { partition_size } => partition_size,
            BinauralBackend::Direct => unreachable!(),
        };
        let lfe = self
            .lfe_index
            .map(|index| rendered.channels[index].as_slice());
        let mut output = Vec::new();
        let mut offset = 0;
        while offset < rendered.channels[0].len() {
            let count =
                (partition_size - self.pending_len).min(rendered.channels[0].len() - offset);
            for (source, mapping) in self.pending_sources.iter_mut().zip(&self.mappings) {
                source[self.pending_len..self.pending_len + count].copy_from_slice(
                    &rendered.channels[mapping.channel_index][offset..offset + count],
                );
            }
            if let Some(lfe) = lfe {
                self.pending_lfe[self.pending_len..self.pending_len + count]
                    .copy_from_slice(&lfe[offset..offset + count]);
            }
            self.pending_len += count;
            offset += count;
            if self.pending_len == partition_size {
                output.push(self.flush_partition(rendered.sample_rate)?);
                self.pending_len = 0;
            }
        }
        Ok(output)
    }

    fn flush_partition(
        &mut self,
        sample_rate: u32,
    ) -> Result<BinauralRenderedBlock, JocRenderError> {
        let valid = self.pending_sources[0].len();
        let blocks = self
            .pending_sources
            .iter()
            .zip(&self.mappings)
            .map(|(source, mapping)| BinauralSourceBlock::new(mapping.source_id, source))
            .collect::<Vec<_>>();
        let mut left = vec![0.0; valid];
        let mut right = vec![0.0; valid];
        let BinauralEngine::Partitioned(renderer) = self
            .engine
            .as_mut()
            .ok_or(JocRenderError::NoRenderedFrames)?
        else {
            return Err(JocRenderError::BinauralOutput(
                "partitioned engine is unavailable".to_owned(),
            ));
        };
        renderer.render_partition(&blocks, &mut left, &mut right)?;
        add_lfe(self.lfe_policy, &self.pending_lfe, &mut left, &mut right)?;
        Ok(BinauralRenderedBlock {
            sample_rate,
            left,
            right,
        })
    }
}

fn virtual_speaker_direction(label: &str) -> Option<CartesianPosition> {
    Some(match label {
        "FL" => CartesianPosition::new(-1.0, 1.0, 0.0),
        "FR" => CartesianPosition::new(1.0, 1.0, 0.0),
        "FC" => CartesianPosition::new(0.0, 1.0, 0.0),
        "Ls" => CartesianPosition::new(-1.0, 0.0, 0.0),
        "Rs" => CartesianPosition::new(1.0, 0.0, 0.0),
        "Lb" => CartesianPosition::new(-1.0, -1.0, 0.0),
        "Rb" => CartesianPosition::new(1.0, -1.0, 0.0),
        "TFL" | "Ltf" => CartesianPosition::new(-1.0, 1.0, 1.0),
        "TFR" | "Rtf" => CartesianPosition::new(1.0, 1.0, 1.0),
        "Ltm" => CartesianPosition::new(-1.0, 0.0, 1.0),
        "Rtm" => CartesianPosition::new(1.0, 0.0, 1.0),
        "TBL" | "Ltr" => CartesianPosition::new(-1.0, -1.0, 1.0),
        "TBR" | "Rtr" => CartesianPosition::new(1.0, -1.0, 1.0),
        // The public 9.1 wide row is slightly forward of the side row.  Keep
        // this renderer direction tied to the existing scene geometry's
        // normalized coordinate convention (+Y front, -Y rear).
        "Lw" => CartesianPosition::new(-1.0, 0.67767333984375, 0.0),
        "Rw" => CartesianPosition::new(1.0, 0.67767333984375, 0.0),
        _ => return None,
    })
}

fn validate_binaural_layout_preset(
    layout: &str,
    preset: &SpeakerLayoutPreset,
) -> Result<(), JocRenderError> {
    let missing = preset
        .labels
        .iter()
        .enumerate()
        .filter(|(index, _)| preset.lfe_index() != Some(*index))
        .filter_map(|(_, label)| {
            virtual_speaker_direction(label)
                .is_none()
                .then_some((*label).to_owned())
        })
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(JocRenderError::BinauralLayoutNotReady {
            layout: layout.to_owned(),
            missing,
        })
    }
}

/// Validates a speaker layout and the selected semantic output backend without
/// creating output files or loading input media.
pub fn validate_speaker_output(
    layout: &str,
    output: &Path,
) -> Result<OutputContainer, JocRenderError> {
    let preset = SpeakerLayoutPreset::for_name(layout)?;
    let semantic = preset.semantic_channel_layout();
    let container = output_container_for_path(output)?;
    match container {
        OutputContainer::Wav if semantic.wav_channel_mask().is_none() => {
            Err(JocRenderError::WavLayoutNotExactlyRepresentable {
                layout: layout.to_owned(),
            })
        }
        OutputContainer::Caf => {
            for label in &semantic.labels {
                caf_description(&semantic, label)?;
            }
            Ok(container)
        }
        OutputContainer::Wav => Ok(container),
    }
}

/// Validates public direction mappings independently of SOFA dataset coverage
/// and the selected physical speaker output container.
pub fn validate_binaural_layout(layout: &str) -> Result<(), JocRenderError> {
    let preset = SpeakerLayoutPreset::for_name(layout)?;
    validate_binaural_layout_preset(layout, &preset)
}

fn add_lfe(
    policy: Option<BinauralLfePolicy>,
    lfe: &[f64],
    left: &mut [f64],
    right: &mut [f64],
) -> Result<(), JocRenderError> {
    if lfe.len() != left.len() || lfe.len() != right.len() {
        return Err(JocRenderError::BinauralOutput(
            "LFE and binaural block lengths differ".to_owned(),
        ));
    }
    if matches!(policy, Some(BinauralLfePolicy::EqualPowerDualMono)) {
        let gain = std::f64::consts::FRAC_1_SQRT_2;
        for index in 0..lfe.len() {
            left[index] += lfe[index] * gain;
            right[index] += lfe[index] * gain;
            if !left[index].is_finite() || !right[index].is_finite() {
                return Err(JocRenderError::BinauralOutput(
                    "non-finite LFE accumulation".to_owned(),
                ));
            }
        }
    }
    Ok(())
}

fn drain_direct_tail(
    renderer: &mut BinauralRenderer,
    sample_rate: u32,
    max_taps: usize,
    output: &mut Vec<BinauralRenderedBlock>,
) -> Result<(), JocRenderError> {
    let mut remaining = renderer.remaining_tail_samples();
    while remaining > 0 {
        let count = remaining.min(max_taps.max(1));
        let mut left = vec![0.0; count];
        let mut right = vec![0.0; count];
        renderer.drain_tail_block(&mut left, &mut right)?;
        output.push(BinauralRenderedBlock {
            sample_rate,
            left,
            right,
        });
        remaining -= count;
    }
    Ok(())
}

fn drain_partitioned_tail(
    renderer: &mut PartitionedBinauralRenderer,
    sample_rate: u32,
    partition_size: usize,
    output: &mut Vec<BinauralRenderedBlock>,
) -> Result<(), JocRenderError> {
    let mut remaining = renderer.remaining_tail_samples();
    while remaining > 0 {
        let count = remaining.min(partition_size);
        let mut left = vec![0.0; count];
        let mut right = vec![0.0; count];
        renderer.drain_tail_block(&mut left, &mut right)?;
        output.push(BinauralRenderedBlock {
            sample_rate,
            left,
            right,
        });
        remaining -= count;
    }
    Ok(())
}

fn speaker_output_presentation(preset: &SpeakerLayoutPreset) -> (String, String, String) {
    let identities = preset.channel_labels().join(", ");
    if preset.name == "2.0" {
        (
            "Stereo speakers (2.0)".to_owned(),
            "Left, Right".to_owned(),
            identities,
        )
    } else {
        (
            format!("Speaker layout ({})", preset.name),
            identities.clone(),
            identities,
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

#[cfg(test)]
fn selected_stereo_policy(
    requested: StereoDownmixPolicy,
    metadata: DownmixMetadata,
) -> StereoDownmixPolicy {
    stereo_downmix_matrix(stereo_downmix_mode(requested), metadata, &[])
        .expect("an empty stereo matrix has no unsupported channel")
        .selected_mode()
        .into()
}

impl From<StereoDownmixMode> for StereoDownmixPolicy {
    fn from(mode: StereoDownmixMode) -> Self {
        match mode {
            StereoDownmixMode::Auto => Self::Auto,
            StereoDownmixMode::LoRo => Self::LoRo,
            StereoDownmixMode::LtRt => Self::LtRt,
        }
    }
}

const fn stereo_downmix_mode(policy: StereoDownmixPolicy) -> StereoDownmixMode {
    match policy {
        StereoDownmixPolicy::Auto => StereoDownmixMode::Auto,
        StereoDownmixPolicy::LoRo => StereoDownmixMode::LoRo,
        StereoDownmixPolicy::LtRt => StereoDownmixMode::LtRt,
    }
}

#[cfg(test)]
type StereoDownmixCoefficients = (StereoDownmixPolicy, Vec<(f64, f64)>, Option<f64>);

#[cfg(test)]
fn stereo_downmix_coefficients(
    requested: StereoDownmixPolicy,
    metadata: DownmixMetadata,
    locations: &[ChannelLocation],
) -> Result<StereoDownmixCoefficients, JocRenderError> {
    let matrix = stereo_downmix_matrix(stereo_downmix_mode(requested), metadata, locations)
        .map_err(|error| JocRenderError::InvalidControl(error.to_string()))?;
    Ok((
        matrix.selected_mode().into(),
        matrix
            .rows()
            .iter()
            .map(|row| (row.left(), row.right()))
            .collect(),
        matrix.lfe_coefficient(),
    ))
}

fn add_stereo_base_downmix(
    active: &mut [Vec<f64>],
    base: &DecodedAccessUnitPcm,
    policy: StereoDownmixPolicy,
) -> Result<(), JocRenderError> {
    if active.len() != 2 {
        return Err(JocRenderError::InvalidControl(
            "2.0 downmix requires exactly two active output channels".to_owned(),
        ));
    }
    let matrix = stereo_downmix_matrix(
        stereo_downmix_mode(policy),
        base.downmix,
        &base.channel_locations,
    )
    .map_err(|error| JocRenderError::InvalidControl(error.to_string()))?;
    matrix
        .apply(base, active)
        .map_err(|error| JocRenderError::InvalidControl(error.to_string()))
}

fn aligned_base_pcm(
    pending: &PendingRenderFrame,
    aligned: &AlignedReconstructionOutput,
) -> DecodedAccessUnitPcm {
    let samples = aligned.base_full_band_pcm.first().map_or(0, Vec::len);
    DecodedAccessUnitPcm {
        sample_rate: aligned.timeline.sample_rate,
        samples: u16::try_from(samples).unwrap_or(u16::MAX),
        channel_locations: pending.channel_locations.clone(),
        channels: aligned.base_full_band_pcm.clone(),
        lfe_location: pending.lfe_location,
        lfe: aligned.lfe_pcm.clone(),
        downmix: pending.downmix,
        dialnorm: pending.dialnorm,
    }
}

#[cfg(test)]
fn five_point_one_layout() -> Result<SpatialLayout, JocRenderError> {
    Ok(SpeakerLayoutPreset::for_name("5.1")?.layout)
}

#[cfg(test)]
fn five_point_one_preset() -> Result<SpeakerLayoutPreset, JocRenderError> {
    Ok(SpeakerLayoutPreset::for_name(JOC_RENDER_LAYOUT)?)
}

fn replacement_backup_path(output: &Path) -> Result<PathBuf, io::Error> {
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = output
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "output has no filename"))?
        .to_string_lossy();
    Ok(parent.join(format!(".{name}.openjoc-replace-{}", std::process::id())))
}

pub(crate) fn replace_existing_file(staging: &Path, output: &Path) -> io::Result<()> {
    match fs::rename(staging, output) {
        Ok(()) => Ok(()),
        Err(_rename_error) if output.exists() => {
            let backup = replacement_backup_path(output)?;
            if backup.exists() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!(
                        "temporary replacement path already exists: {}",
                        backup.display()
                    ),
                ));
            }
            fs::rename(output, &backup)?;
            match fs::rename(staging, output) {
                Ok(()) => {
                    let _ = fs::remove_file(backup);
                    Ok(())
                }
                Err(replace_error) => {
                    if let Err(restore_error) = fs::rename(&backup, output) {
                        return Err(io::Error::other(format!(
                            "could not replace {} ({replace_error}); restoring the previous output also failed ({restore_error})",
                            output.display()
                        )));
                    }
                    Err(replace_error)
                }
            }
        }
        Err(error) => Err(error),
    }
}

/// The output container selected by the destination extension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputContainer {
    Wav,
    Caf,
}

impl OutputContainer {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Wav => "WAV",
            Self::Caf => "CAF",
        }
    }
}

fn output_container_for_path(path: &Path) -> Result<OutputContainer, JocRenderError> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("wav") => Ok(OutputContainer::Wav),
        Some(extension) if extension.eq_ignore_ascii_case("caf") => Ok(OutputContainer::Caf),
        _ => Err(JocRenderError::UnsupportedOutputExtension(path.to_owned())),
    }
}

pub fn validate_output_path(path: &Path) -> Result<OutputContainer, JocRenderError> {
    output_container_for_path(path)
}

struct StagedOutput {
    output: PathBuf,
    staging: PathBuf,
    overwrite: bool,
}

impl StagedOutput {
    fn new(output: &Path, overwrite: bool) -> Result<Self, JocRenderError> {
        if output.exists() && !overwrite {
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
            overwrite,
        })
    }

    fn create(&self) -> Result<fs::File, JocRenderError> {
        Ok(fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.staging)?)
    }

    fn finish(&self) -> Result<(), JocRenderError> {
        let replacement = if self.overwrite {
            replace_existing_file(&self.staging, &self.output)
        } else if self.output.exists() {
            Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("refusing to overwrite output {}", self.output.display()),
            ))
        } else {
            fs::rename(&self.staging, &self.output)
        };
        replacement.map_err(Into::into)
    }

    fn abort(&self) {
        let _ = fs::remove_file(&self.staging);
    }
}

pub struct JocWavOutput {
    transaction: StagedOutput,
    format: SampleFormat,
    writer: Option<WaveWriter<fs::File>>,
    sample_rate: Option<u32>,
    channels: Option<usize>,
    speaker_mask: Option<u32>,
}

impl JocWavOutput {
    pub fn new_with_overwrite(
        output: &Path,
        format: SampleFormat,
        overwrite: bool,
    ) -> Result<Self, JocRenderError> {
        Self::new_with_mask(output, format, overwrite, None)
    }

    /// Creates transactional output for one of the admitted speaker layouts.
    /// The WAV header carries the standard speaker mask matching the preset's
    /// explicit public channel identities and order.
    #[allow(dead_code)]
    pub fn new_for_speaker_layout(
        output: &Path,
        format: SampleFormat,
        overwrite: bool,
        layout: &str,
    ) -> Result<Self, JocRenderError> {
        let preset = SpeakerLayoutPreset::for_name(layout)?;
        Self::new_for_semantic_layout(output, format, overwrite, &preset.semantic_channel_layout())
    }

    pub fn new_for_semantic_layout(
        output: &Path,
        format: SampleFormat,
        overwrite: bool,
        layout: &SemanticChannelLayout,
    ) -> Result<Self, JocRenderError> {
        let speaker_mask = layout.wav_channel_mask().ok_or_else(|| {
            JocRenderError::WavLayoutNotExactlyRepresentable {
                layout: layout.name.clone(),
            }
        })?;
        Self::new_with_mask(output, format, overwrite, Some(speaker_mask))
    }

    fn new_with_mask(
        output: &Path,
        format: SampleFormat,
        overwrite: bool,
        speaker_mask: Option<u32>,
    ) -> Result<Self, JocRenderError> {
        Ok(Self {
            transaction: StagedOutput::new(output, overwrite)?,
            format,
            writer: None,
            sample_rate: None,
            channels: None,
            speaker_mask,
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
            let file = self.transaction.create()?;
            let options = WaveEncodeOptions {
                sample_format: self.format,
                clipping: Clipping::Reject,
                dither: Dither::None,
            };
            let writer = match self.speaker_mask {
                Some(mask) => WaveWriter::new_with_speaker_mask(
                    file,
                    block.sample_rate,
                    channels,
                    options,
                    mask,
                )?,
                None => WaveWriter::new(file, block.sample_rate, channels, options)?,
            };
            self.writer = Some(writer);
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

    pub fn finish(&mut self) -> Result<(), JocRenderError> {
        let writer = self.writer.take().ok_or(JocRenderError::NoRenderedFrames)?;
        if let Err(error) = writer.finish() {
            self.transaction.abort();
            return Err(error.into());
        }
        if let Err(error) = self.transaction.finish() {
            self.transaction.abort();
            return Err(error);
        }
        Ok(())
    }

    pub fn abort(&mut self) {
        self.writer.take();
        self.transaction.abort();
    }

    pub fn frames(&self) -> u64 {
        self.writer.as_ref().map_or(0, WaveWriter::frames)
    }
}

pub struct JocCafOutput {
    transaction: StagedOutput,
    format: SampleFormat,
    writer: Option<CafWriter<fs::File>>,
    sample_rate: Option<u32>,
    channels: usize,
    descriptions: Vec<CafChannelDescription>,
}

impl JocCafOutput {
    pub fn new_for_semantic_layout(
        output: &Path,
        format: SampleFormat,
        overwrite: bool,
        layout: &SemanticChannelLayout,
    ) -> Result<Self, JocRenderError> {
        let descriptions = layout
            .labels
            .iter()
            .map(|label| caf_description(layout, label))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            transaction: StagedOutput::new(output, overwrite)?,
            format,
            writer: None,
            sample_rate: None,
            channels: layout.channel_count(),
            descriptions,
        })
    }

    pub fn write_block(&mut self, block: &RenderedBlock) -> Result<(), JocRenderError> {
        if block.channels.is_empty() {
            return Err(JocRenderError::NoRenderedFrames);
        }
        let channels = block.channels.len();
        if channels != self.channels {
            return Err(JocRenderError::InvalidControl(
                "render output channel semantics changed during stream".to_owned(),
            ));
        }
        if let Some(expected) = self.sample_rate {
            if expected != block.sample_rate {
                return Err(JocRenderError::InvalidControl(
                    "render output sample rate changed during stream".to_owned(),
                ));
            }
        } else {
            let file = self.transaction.create()?;
            let options = WaveEncodeOptions {
                sample_format: self.format,
                clipping: Clipping::Reject,
                dither: Dither::None,
            };
            self.writer = Some(CafWriter::new(
                file,
                block.sample_rate,
                channels,
                options,
                &self.descriptions,
            )?);
            self.sample_rate = Some(block.sample_rate);
        }
        let references = block.channels.iter().map(Vec::as_slice).collect::<Vec<_>>();
        self.writer
            .as_mut()
            .ok_or(JocRenderError::NoRenderedFrames)?
            .write_channels(&references)?;
        Ok(())
    }

    pub fn finish(&mut self) -> Result<(), JocRenderError> {
        let writer = self.writer.take().ok_or(JocRenderError::NoRenderedFrames)?;
        if let Err(error) = writer.finish() {
            self.transaction.abort();
            return Err(error.into());
        }
        if let Err(error) = self.transaction.finish() {
            self.transaction.abort();
            return Err(error);
        }
        Ok(())
    }

    pub fn abort(&mut self) {
        self.writer.take();
        self.transaction.abort();
    }

    pub fn frames(&self) -> u64 {
        self.writer.as_ref().map_or(0, CafWriter::frames)
    }
}

/// Container-selected output sink. Renderer blocks are independent of this
/// enum; only the destination extension chooses the serializer.
pub enum JocPcmOutput {
    Wav(JocWavOutput),
    Caf(JocCafOutput),
}

impl JocPcmOutput {
    #[allow(dead_code)]
    pub fn new_for_speaker_layout(
        output: &Path,
        format: SampleFormat,
        overwrite: bool,
        layout: &str,
    ) -> Result<Self, JocRenderError> {
        let preset = SpeakerLayoutPreset::for_name(layout)?;
        let semantic = preset.semantic_channel_layout();
        Self::new_for_semantic_layout(output, format, overwrite, &semantic)
    }

    pub fn new_for_binaural(
        output: &Path,
        format: SampleFormat,
        overwrite: bool,
    ) -> Result<Self, JocRenderError> {
        let semantic =
            SemanticChannelLayout::without_wav_mapping("binaural", ["Left", "Right"], None);
        match output_container_for_path(output)? {
            OutputContainer::Wav => Ok(Self::Wav(JocWavOutput::new_with_overwrite(
                output, format, overwrite,
            )?)),
            OutputContainer::Caf => Ok(Self::Caf(JocCafOutput::new_for_semantic_layout(
                output, format, overwrite, &semantic,
            )?)),
        }
    }

    pub(crate) fn new_for_semantic_layout(
        output: &Path,
        format: SampleFormat,
        overwrite: bool,
        layout: &SemanticChannelLayout,
    ) -> Result<Self, JocRenderError> {
        match output_container_for_path(output)? {
            OutputContainer::Wav => Ok(Self::Wav(JocWavOutput::new_for_semantic_layout(
                output, format, overwrite, layout,
            )?)),
            OutputContainer::Caf => Ok(Self::Caf(JocCafOutput::new_for_semantic_layout(
                output, format, overwrite, layout,
            )?)),
        }
    }

    pub fn write_block(&mut self, block: &RenderedBlock) -> Result<(), JocRenderError> {
        match self {
            Self::Wav(output) => output.write_block(block),
            Self::Caf(output) => output.write_block(block),
        }
    }

    pub fn finish(&mut self) -> Result<(), JocRenderError> {
        match self {
            Self::Wav(output) => output.finish(),
            Self::Caf(output) => output.finish(),
        }
    }

    pub fn abort(&mut self) {
        match self {
            Self::Wav(output) => output.abort(),
            Self::Caf(output) => output.abort(),
        }
    }

    pub fn frames(&self) -> u64 {
        match self {
            Self::Wav(output) => output.frames(),
            Self::Caf(output) => output.frames(),
        }
    }

    pub const fn container(&self) -> OutputContainer {
        match self {
            Self::Wav(_) => OutputContainer::Wav,
            Self::Caf(_) => OutputContainer::Caf,
        }
    }
}

const CAF_LABEL_USE_COORDINATES: u32 = 100;
const CAF_FLAG_RECTANGULAR_COORDINATES: u32 = 1;
const TOP_MIDDLE_X_LEFT: f64 = 7_928.0 / 32_768.0;
const TOP_MIDDLE_X_RIGHT: f64 = 24_840.0 / 32_768.0;
const TOP_MIDDLE_Z: f64 = 32_767.0 / 32_768.0;

fn caf_description(
    layout: &SemanticChannelLayout,
    label: &str,
) -> Result<CafChannelDescription, JocRenderError> {
    let description = match label {
        "FL" | "front-left" | "Left" => caf_label(1),
        "FR" | "front-right" | "Right" => caf_label(2),
        "FC" | "front-center" | "Center" => caf_label(3),
        "LFE" | "low-frequency" => caf_label(4),
        "Lb" | "BL" | "back-left" => caf_label(5),
        "Rb" | "BR" | "back-right" => caf_label(6),
        "Ls" | "SL" | "side-left" => caf_label(10),
        "Rs" | "SR" | "side-right" => caf_label(11),
        "TFL" | "Ltf" | "top-front-left" => caf_label(13),
        "TFR" | "Rtf" | "top-front-right" => caf_label(15),
        "TBL" | "Ltr" | "top-back-left" => caf_label(16),
        "TBR" | "Rtr" | "top-back-right" => caf_label(18),
        "Lw" | "left-wide" => caf_label(35),
        "Rw" | "right-wide" => caf_label(36),
        "Ltm" => caf_coordinate(TOP_MIDDLE_X_LEFT),
        "Rtm" => caf_coordinate(TOP_MIDDLE_X_RIGHT),
        _ => {
            return Err(JocRenderError::UnsupportedCafSpeaker {
                layout: layout.name.clone(),
                label: label.to_owned(),
            });
        }
    };
    Ok(description)
}

const fn caf_label(label: u32) -> CafChannelDescription {
    CafChannelDescription {
        label,
        flags: 0,
        coordinates: [0.0; 3],
    }
}

fn caf_coordinate(openjoc_x: f64) -> CafChannelDescription {
    CafChannelDescription {
        label: CAF_LABEL_USE_COORDINATES,
        flags: CAF_FLAG_RECTANGULAR_COORDINATES,
        coordinates: [(2.0 * openjoc_x - 1.0) as f32, 0.0, TOP_MIDDLE_Z as f32],
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BinauralBackend, BinauralLfePolicy, FrameUpdates, JOC_RENDER_CHANNEL_ORDER,
        JOC_RENDER_SUPPORTED_LAYOUTS, JocBinauralRenderer, JocCafOutput, JocPcmOutput,
        JocRenderError, JocSpeakerRenderer, JocWavOutput, OutputContainer, RenderControl,
        RenderedBlock, SemanticChannelLayout, SpeakerLayoutPreset, StereoDownmixPolicy,
        add_stereo_base_downmix, five_point_one_layout, five_point_one_preset,
        selected_stereo_policy, stereo_downmix_coefficients, virtual_speaker_direction,
    };
    use openjoc_eac3::{ChannelLocation, DecodedAccessUnitPcm, DialnormState, DownmixMetadata};
    use openjoc_emdf::JocValidationProfile;
    use openjoc_joc::{DecodedJocFrame, JocFrame, JocHeader, ReconstructionBasis};
    use openjoc_oamd::{
        ContentDescription, Gain, MetadataBlockTiming, MetadataTiming, OamdContentPrefix,
        OamdElement, OamdElementMetadata, OamdPayload, ObjectBasicInfo, ObjectClass, ObjectElement,
        ObjectRenderInfo, ObjectUpdate,
    };
    use openjoc_render::{
        BinauralRenderer, BinauralSourceBlock, FinalLinkedGain, HrirBank, HrirEntry, HrirEntryId,
        HrirPair, StaticBinauralSource,
    };
    use openjoc_scene::{
        BaseFullBandCoordinate, DecodedPayloadFrame, JocSpatialBridge, ProgrammeLayout,
        SampleRange, SpatialBindingRecord, SpatialContributionMode, SpatialDescriptor,
        SpatialExplicitGroup, SpatialExplicitMember, SpatialRouteVector, SpatialSourceClass,
        SpatialTopologySnapshot, speaker_channel_mask_for_labels,
    };
    use openjoc_wave::{SampleFormat, decode};
    use std::{
        fs,
        path::Path,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn caf_channel_descriptions(bytes: &[u8]) -> Vec<(u32, u32, [f32; 3])> {
        let mut position = 8;
        while position < bytes.len() {
            let chunk_type = &bytes[position..position + 4];
            let size = i64::from_be_bytes(bytes[position + 4..position + 12].try_into().unwrap());
            let size = usize::try_from(size).unwrap();
            let start = position + 12;
            let end = start + size;
            if chunk_type == b"chan" {
                let count = usize::try_from(u32::from_be_bytes(
                    bytes[start + 8..start + 12].try_into().unwrap(),
                ))
                .unwrap();
                return (0..count)
                    .map(|index| {
                        let offset = start + 12 + index * 20;
                        (
                            u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()),
                            u32::from_be_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()),
                            [
                                f32::from_be_bytes(
                                    bytes[offset + 8..offset + 12].try_into().unwrap(),
                                ),
                                f32::from_be_bytes(
                                    bytes[offset + 12..offset + 16].try_into().unwrap(),
                                ),
                                f32::from_be_bytes(
                                    bytes[offset + 16..offset + 20].try_into().unwrap(),
                                ),
                            ],
                        )
                    })
                    .collect();
            }
            position = end;
        }
        panic!("CAF channel layout chunk missing");
    }

    fn caf_f32_samples(bytes: &[u8]) -> Vec<f64> {
        let mut position = 8;
        while position < bytes.len() {
            let chunk_type = &bytes[position..position + 4];
            let size = i64::from_be_bytes(bytes[position + 4..position + 12].try_into().unwrap());
            let size = usize::try_from(size).unwrap();
            let start = position + 12;
            let end = start + size;
            if chunk_type == b"data" {
                assert_eq!(
                    u32::from_be_bytes(bytes[start..start + 4].try_into().unwrap()),
                    0
                );
                return bytes[start + 4..end]
                    .chunks_exact(4)
                    .map(|sample| f32::from_le_bytes(sample.try_into().unwrap()) as f64)
                    .collect();
            }
            position = end;
        }
        panic!("CAF data chunk missing");
    }

    fn expected_caf_label(label: &str) -> u32 {
        match label {
            "FL" => 1,
            "FR" => 2,
            "FC" => 3,
            "LFE" => 4,
            "Lb" => 5,
            "Rb" => 6,
            "Ls" => 10,
            "Rs" => 11,
            "TFL" | "Ltf" => 13,
            "TFR" | "Rtf" => 15,
            "TBL" | "Ltr" => 16,
            "TBR" | "Rtr" => 18,
            "Lw" => 35,
            "Rw" => 36,
            "Ltm" | "Rtm" => 100,
            _ => panic!("unexpected public semantic label {label}"),
        }
    }

    fn record(identity: &str) -> SpatialBindingRecord {
        record_with_class(SpatialSourceClass::ExplicitChannel, identity)
    }

    fn record_with_class(class: SpatialSourceClass, identity: &str) -> SpatialBindingRecord {
        SpatialBindingRecord {
            descriptor: SpatialDescriptor {
                source_class: class,
                identity: identity.to_owned(),
                coordinates: vec![0.5, 0.5, 0.0],
                spread: None,
                paired: None,
                pair_span_q15: None,
                raw3: Some(vec![3]),
                extent: None,
                zones: None,
                channel_lock: false,
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
            route_vectors: Vec::new(),
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

    fn fixed_route_control(with_route: bool) -> RenderControl {
        let base_identities = ["FL", "FR", "FC", "Ls", "Rs"];
        let explicit_groups = base_identities
            .iter()
            .enumerate()
            .map(|(group_order, identity)| SpatialExplicitGroup {
                group_order: group_order as u32,
                members: vec![SpatialExplicitMember {
                    canonical_label: (*identity).to_owned(),
                    record: record(identity),
                }],
            })
            .collect();
        RenderControl {
            topology: SpatialTopologySnapshot {
                explicit_groups,
                fixed_layout: vec![record_with_class(SpatialSourceClass::FixedLayout, "fixed")],
                dynamic_records: Vec::new(),
            },
            route_vectors: with_route
                .then_some(vec![SpatialRouteVector {
                    identity: "fixed".to_owned(),
                    vector: vec![1.0, 0.0, 0.0, 0.0, 0.0],
                }])
                .unwrap_or_default(),
            updates: Vec::new(),
            consumed_updates: Vec::new(),
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

    fn automatic_decoded_frame(position: openjoc_oamd::Position3) -> DecodedPayloadFrame {
        let mut frame = decoded_frame(0, 0, 2);
        frame.oamd.prefix.element_count = 1;
        frame.oamd.elements = vec![OamdElementMetadata {
            id: 1,
            alternate_data_id: None,
            discard_unknown: false,
            element: OamdElement::Objects(ObjectElement {
                timing: MetadataTiming {
                    sample_offset: 0,
                    blocks: vec![MetadataBlockTiming {
                        start_sample: 0,
                        ramp_duration: 0,
                    }],
                },
                objects: vec![vec![ObjectUpdate {
                    active: true,
                    basic: ObjectBasicInfo {
                        gain: Gain::Decibels(0),
                        priority: 0.0,
                    },
                    render: ObjectRenderInfo {
                        position,
                        ..ObjectRenderInfo::DEFAULT
                    },
                    additional_table_data: None,
                }]],
                consumed_bits: 0,
            }),
        }];
        frame
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
            downmix: DownmixMetadata::default(),
            dialnorm: DialnormState::default(),
        }
    }

    fn stereo_base(
        locations: &[ChannelLocation],
        values: &[f64],
        lfe: Option<f64>,
        downmix: DownmixMetadata,
    ) -> DecodedAccessUnitPcm {
        DecodedAccessUnitPcm {
            sample_rate: 48_000,
            samples: 1,
            channel_locations: locations.to_vec(),
            channels: values.iter().copied().map(|value| vec![value]).collect(),
            lfe_location: lfe.map(|_| ChannelLocation::Lfe(0)),
            lfe: lfe.map(|value| vec![value]),
            downmix,
            dialnorm: DialnormState::default(),
        }
    }

    fn peak_metrics(channels: &[Vec<f64>]) -> (f64, f64) {
        let peak = channels
            .iter()
            .flatten()
            .map(|sample| sample.abs())
            .fold(0.0, f64::max);
        let db = if peak == 0.0 {
            f64::NEG_INFINITY
        } else {
            20.0 * peak.log10()
        };
        (peak, db)
    }

    fn binaural_bank(layout: &str, sample_rate: u32) -> HrirBank {
        let preset = SpeakerLayoutPreset::for_name(layout).unwrap();
        let entries = preset
            .labels
            .iter()
            .enumerate()
            .filter_map(|(index, label)| {
                if preset.lfe_index() == Some(index) {
                    return None;
                }
                Some(
                    HrirEntry::new(
                        HrirEntryId::new(index as u64 + 100),
                        virtual_speaker_direction(label).unwrap(),
                        HrirPair::new(
                            sample_rate,
                            vec![1.0, 0.25, 0.125],
                            vec![0.5, 0.125, 0.0625],
                        )
                        .unwrap(),
                    )
                    .unwrap(),
                )
            })
            .collect();
        HrirBank::new(sample_rate, entries).unwrap()
    }

    fn collect_binaural(
        renderer: &mut JocBinauralRenderer,
        frame_index: usize,
        frame: &DecodedPayloadFrame,
        pcm: &DecodedAccessUnitPcm,
    ) -> Vec<(f64, f64)> {
        let mut output = renderer.render_frame(frame_index, frame, pcm).unwrap();
        output.extend(renderer.finish().unwrap());
        output
            .into_iter()
            .flat_map(|block| block.left.into_iter().zip(block.right))
            .collect()
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
    fn stereo_downmix_loro_numeric_fixture_uses_center_and_surround_metadata() {
        let metadata = DownmixMetadata {
            loro_center_mix_level: Some(4),
            loro_surround_mix_level: Some(4),
            ..DownmixMetadata::default()
        };
        let base = stereo_base(
            &[
                ChannelLocation::Left,
                ChannelLocation::Right,
                ChannelLocation::Centre,
                ChannelLocation::LeftSurround,
                ChannelLocation::RightSurround,
            ],
            &[1.0, 2.0, 3.0, 4.0, 5.0],
            None,
            metadata,
        );
        let (_, coefficients, _) = stereo_downmix_coefficients(
            StereoDownmixPolicy::LoRo,
            metadata,
            &base.channel_locations,
        )
        .unwrap();
        let scale = 1.0 / 2.414;
        assert!((coefficients[0].0 - scale).abs() < 1.0e-12);
        assert_eq!(coefficients[0].1, 0.0);
        assert!((coefficients[2].0 - 0.707 * scale).abs() < 1.0e-12);
        assert!((coefficients[2].1 - 0.707 * scale).abs() < 1.0e-12);
        let mut active = vec![vec![0.0], vec![0.0]];
        add_stereo_base_downmix(&mut active, &base, StereoDownmixPolicy::LoRo).unwrap();
        assert_eq!(
            active[0][0],
            coefficients[0].0 + 3.0 * coefficients[2].0 + 4.0 * coefficients[3].0
        );
        assert_eq!(
            active[1][0],
            2.0 * coefficients[1].1 + 3.0 * coefficients[2].1 + 5.0 * coefficients[4].1
        );
        let drc_scaled = stereo_base(
            &base.channel_locations,
            &[2.0, 4.0, 6.0, 8.0, 10.0],
            None,
            metadata,
        );
        let mut scaled_active = vec![vec![0.0], vec![0.0]];
        add_stereo_base_downmix(&mut scaled_active, &drc_scaled, StereoDownmixPolicy::LoRo)
            .unwrap();
        assert_eq!(scaled_active[0][0], 2.0 * active[0][0]);
        assert_eq!(scaled_active[1][0], 2.0 * active[1][0]);
    }

    #[test]
    fn stereo_downmix_ltrt_numeric_fixture_preserves_surround_polarity() {
        let metadata = DownmixMetadata {
            ltrt_center_mix_level: Some(4),
            ltrt_surround_mix_level: Some(4),
            ..DownmixMetadata::default()
        };
        let base = stereo_base(
            &[
                ChannelLocation::Left,
                ChannelLocation::Right,
                ChannelLocation::Centre,
                ChannelLocation::LeftSurround,
                ChannelLocation::RightSurround,
            ],
            &[1.0, 2.0, 3.0, 4.0, 5.0],
            None,
            metadata,
        );
        let mut active = vec![vec![0.0], vec![0.0]];
        add_stereo_base_downmix(&mut active, &base, StereoDownmixPolicy::LtRt).unwrap();
        let scale = 1.0 / 3.121;
        assert!(
            (active[0][0] - (1.0 + 3.0 * 0.707 - 4.0 * 0.707 - 5.0 * 0.707) * scale).abs()
                < 1.0e-12
        );
        assert!(
            (active[1][0] - (2.0 + 3.0 * 0.707 + 4.0 * 0.707 + 5.0 * 0.707) * scale).abs()
                < 1.0e-12
        );
        assert!(active[0][0] < active[1][0]);
    }

    #[test]
    fn stereo_downmix_auto_follows_preference_and_defaults_to_loro() {
        let ltrt = DownmixMetadata {
            dmixmod: Some(1),
            ..DownmixMetadata::default()
        };
        let loro = DownmixMetadata {
            dmixmod: Some(2),
            ..DownmixMetadata::default()
        };
        assert_eq!(
            selected_stereo_policy(StereoDownmixPolicy::Auto, ltrt),
            StereoDownmixPolicy::LtRt
        );
        assert_eq!(
            selected_stereo_policy(StereoDownmixPolicy::Auto, loro),
            StereoDownmixPolicy::LoRo
        );
        assert_eq!(
            selected_stereo_policy(StereoDownmixPolicy::Auto, DownmixMetadata::default()),
            StereoDownmixPolicy::LoRo
        );
    }

    #[test]
    fn stereo_downmix_lfe_is_optional_metadata_not_bass_management() {
        let base = stereo_base(
            &[ChannelLocation::Left, ChannelLocation::Right],
            &[0.0, 0.0],
            Some(1.0),
            DownmixMetadata {
                lfe_mix_level_code: Some(31),
                ..DownmixMetadata::default()
            },
        );
        let mut active = vec![vec![0.0], vec![0.0]];
        add_stereo_base_downmix(&mut active, &base, StereoDownmixPolicy::LoRo).unwrap();
        let (_, _, expected) = stereo_downmix_coefficients(
            StereoDownmixPolicy::LoRo,
            base.downmix,
            &base.channel_locations,
        )
        .unwrap();
        let expected = expected.unwrap();
        assert_eq!(active[0][0], expected);
        assert_eq!(active[1][0], expected);

        let excluded = stereo_base(
            &[ChannelLocation::Left, ChannelLocation::Right],
            &[0.0, 0.0],
            Some(1.0),
            DownmixMetadata::default(),
        );
        let mut excluded_output = vec![vec![0.0], vec![0.0]];
        add_stereo_base_downmix(&mut excluded_output, &excluded, StereoDownmixPolicy::LoRo)
            .unwrap();
        assert_eq!(excluded_output, vec![vec![0.0], vec![0.0]]);
    }

    #[test]
    fn stereo_downmix_rejects_unmapped_back_or_height_base_channels() {
        let base = stereo_base(
            &[ChannelLocation::LeftBack],
            &[1.0],
            None,
            DownmixMetadata::default(),
        );
        let mut active = vec![vec![0.0], vec![0.0]];
        assert!(add_stereo_base_downmix(&mut active, &base, StereoDownmixPolicy::LoRo).is_err());
    }

    #[test]
    fn clean_full_xyz_side_position_uses_the_5_1_side_row() {
        let preset = five_point_one_preset().expect("5.1 preset");
        assert_eq!(preset.layout.coordinate_dimension_count(), 3);
        let descriptor = SpatialDescriptor::new(
            SpatialSourceClass::DynamicPoint,
            "side-point",
            vec![(32_767.0 / 32_768.0) / 2.0, 0.5, 0.0],
        );
        let projected = preset.layout.project(&descriptor).expect("3D point");
        let root = 0.5_f64.sqrt();
        assert!(projected[0].abs() < 1.0e-12);
        assert!(projected[1].abs() < 1.0e-12);
        assert!(projected[2].abs() < 1.0e-12);
        assert!((projected[3] - root).abs() < 1.0e-12);
        assert!((projected[4] - root).abs() < 1.0e-12);
    }

    #[test]
    fn canonical_layout_presets_have_explicit_public_contracts() {
        assert_eq!(
            JOC_RENDER_SUPPORTED_LAYOUTS,
            [
                "2.0", "5.1", "5.1.2", "5.1.4", "7.1", "7.1.2", "7.1.4", "7.1.6", "9.1", "9.1.2",
                "9.1.4", "9.1.6",
            ]
        );
        let expected = [
            ("5.1", vec!["FL", "FR", "FC", "LFE", "Ls", "Rs"]),
            (
                "5.1.2",
                vec!["FL", "FR", "FC", "LFE", "Ls", "Rs", "TFL", "TFR"],
            ),
            (
                "5.1.4",
                vec![
                    "FL", "FR", "FC", "LFE", "Ls", "Rs", "TFL", "TFR", "TBL", "TBR",
                ],
            ),
            ("7.1", vec!["FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs"]),
            (
                "7.1.2",
                vec![
                    "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "TFL", "TFR",
                ],
            ),
            (
                "7.1.4",
                vec![
                    "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "TFL", "TFR", "TBL", "TBR",
                ],
            ),
            (
                "7.1.6",
                vec![
                    "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "Ltf", "Rtf", "Ltm", "Rtm",
                    "Ltr", "Rtr",
                ],
            ),
            (
                "9.1",
                vec!["FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "Lw", "Rw"],
            ),
            (
                "9.1.2",
                vec![
                    "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "Lw", "Rw", "Ltm", "Rtm",
                ],
            ),
            (
                "9.1.4",
                vec![
                    "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "Lw", "Rw", "Ltf", "Rtf",
                    "Ltr", "Rtr",
                ],
            ),
            (
                "9.1.6",
                vec![
                    "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "Lw", "Rw", "Ltf", "Rtf",
                    "Ltm", "Rtm", "Ltr", "Rtr",
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
    fn speaker_mask_order_distinguishes_back_and_side_pairs() {
        for (layout, expected_mask) in [
            ("5.1.4", 0x0002_d60f),
            ("7.1", 0x0000_063f),
            ("7.1.2", 0x0000_563f),
            ("7.1.4", 0x0002_d63f),
        ] {
            let preset = SpeakerLayoutPreset::for_name(layout).unwrap();
            let labels = preset.channel_labels();
            assert_eq!(
                labels,
                preset
                    .layout
                    .channels()
                    .iter()
                    .map(|channel| channel.identity.as_str())
                    .collect::<Vec<_>>()
            );
            assert!(
                labels
                    .windows(2)
                    .all(|window| speaker_channel_mask_for_labels(window).is_ok())
            );
            assert_eq!(
                speaker_channel_mask_for_labels(&labels).unwrap(),
                expected_mask
            );
        }
    }

    #[test]
    fn admitted_fixed_route_uses_explicit_sidecar_registry() {
        let mut renderer = JocSpeakerRenderer::new("5.1", fixed_route_control(true)).unwrap();
        let block = renderer
            .render_frame(0, &decoded_frame(0, 0, 2), &base(2, 1.0))
            .expect("fixed route resolves from the explicit registry");
        assert_eq!(block.channels[0], vec![7.0; 2]);

        let mut renderer = JocSpeakerRenderer::new("5.1", fixed_route_control(false)).unwrap();
        let error = renderer
            .render_frame(0, &decoded_frame(0, 0, 2), &base(2, 1.0))
            .expect_err("missing fixed route is explicit");
        assert!(error.to_string().contains("missing spatial route: fixed"));
    }

    #[test]
    fn aligned_renderer_holds_frames_until_qmf_tail_and_preserves_sample_count() {
        let mut renderer = JocSpeakerRenderer::new("5.1", control(false, 6)).unwrap();
        let first_frame = decoded_frame(0, 0, 640);
        let second_frame = decoded_frame(1, 640, 640);
        let first = renderer
            .render_frame_aligned(0, &first_frame, &base(640, 1.0))
            .unwrap();
        assert!(first.is_empty());
        let second = renderer
            .render_frame_aligned(1, &second_frame, &base(640, 1.0))
            .unwrap();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].channels.len(), 6);
        assert_eq!(second[0].channels[0].len(), 640);
        assert_eq!(second[0].channels[3], vec![99.0; 640]);

        let tail = renderer
            .finish_with_reconstruction_tail(&ReconstructionBasis {
                rows: vec![vec![
                    0.0;
                    openjoc_joc::ReconstructionOutputTimeline::qmf_latency_samples()
                ]],
            })
            .unwrap();
        assert_eq!(tail.len(), 1);
        assert_eq!(tail[0].channels.len(), 6);
        assert_eq!(tail[0].channels[0].len(), 640);
        assert_eq!(tail[0].channels[3], vec![99.0; 640]);
    }

    #[test]
    fn final_linked_gain_is_downstream_of_combined_speaker_accumulation() {
        let mut renderer = JocSpeakerRenderer::new("5.1", control(false, 6)).unwrap();
        let first = renderer
            .render_frame(0, &decoded_frame(0, 0, 32), &base(32, 1.0))
            .unwrap();
        assert!(first.channels.iter().flatten().all(|&sample| sample == 0.0));
        let second = renderer
            .render_frame(1, &decoded_frame(1, 32, 32), &base(32, 1.0))
            .unwrap();
        assert!(
            second
                .channels
                .iter()
                .flatten()
                .any(|&sample| sample != 0.0)
        );
    }

    #[test]
    fn dialnorm_scales_base_and_reconstruction_before_common_projection() {
        let frame = decoded_frame(0, 0, 2);
        let unity_base = base(2, 1.0);
        let mut calibrated_base = base(2, 1.0);
        calibrated_base.dialnorm = DialnormState::new(openjoc_eac3::DialnormMode::Digital, 24);

        let mut unity_renderer =
            JocSpeakerRenderer::new("5.1", fixed_route_control(true)).expect("unity renderer");
        let mut calibrated_renderer =
            JocSpeakerRenderer::new("5.1", fixed_route_control(true)).expect("renderer");
        let unity = unity_renderer
            .render_frame(0, &frame, &unity_base)
            .expect("unity output");
        let calibrated = calibrated_renderer
            .render_frame(0, &frame, &calibrated_base)
            .expect("calibrated output");
        let gain = calibrated_base.dialnorm.linear_gain();
        assert_eq!(unity.channels[0], vec![7.0; 2]);
        for (actual, expected) in calibrated.channels[0]
            .iter()
            .zip(unity.channels[0].iter().map(|sample| sample * gain))
        {
            assert!((actual - expected).abs() <= 1.0e-12);
        }
        assert!(unity_base.dialnorm.linear_gain() > gain);
    }

    #[test]
    fn calibrated_dialnorm_reduces_the_level_seen_by_final_linked_gain() {
        let mut first_frame = decoded_frame(0, 0, 32);
        let mut second_frame = decoded_frame(1, 32, 32);
        for frame in [&mut first_frame, &mut second_frame] {
            for row in &mut frame.decoded.reconstruction_basis.rows {
                row.fill(0.7);
            }
        }
        let mut calibrated_base = base(32, 1.0);
        for channel in &mut calibrated_base.channels {
            channel.fill(0.7);
        }
        calibrated_base.lfe.as_mut().expect("LFE").fill(0.7);
        calibrated_base.dialnorm = DialnormState::new(openjoc_eac3::DialnormMode::Digital, 20);
        let mut unity_renderer =
            JocSpeakerRenderer::new("5.1", control(false, 6)).expect("unity renderer");
        let mut calibrated_renderer =
            JocSpeakerRenderer::new("5.1", control(false, 6)).expect("renderer");
        let mut unity_base = calibrated_base.clone();
        unity_base.dialnorm = DialnormState::default();
        let _ = unity_renderer
            .render_frame(0, &first_frame, &unity_base)
            .expect("unity first block");
        let _ = calibrated_renderer
            .render_frame(0, &first_frame, &calibrated_base)
            .expect("calibrated first block");
        let unity = unity_renderer
            .render_frame(1, &second_frame, &unity_base)
            .expect("unity linked block");
        let calibrated = calibrated_renderer
            .render_frame(1, &second_frame, &calibrated_base)
            .expect("calibrated linked block");
        let unity_peak = peak_metrics(&unity.channels).0;
        let calibrated_peak = peak_metrics(&calibrated.channels).0;
        assert!(calibrated_peak < unity_peak);
    }

    #[test]
    fn low_level_dialnorm_audit_keeps_final_linked_gain_at_unity() {
        let mut linked = FinalLinkedGain::new(48_000, 32, &[true]).expect("linked gain");
        let mut channels = vec![vec![0.375; 32]];
        linked
            .process(&mut channels)
            .expect("low-level linked gain");
        let mut next = vec![vec![0.375; 32]];
        linked
            .process(&mut next)
            .expect("low-level linked gain output");
        assert_eq!(next[0], vec![0.375; 32]);
    }

    #[test]
    fn unknown_layout_reports_supported_values_without_fallback() {
        let error = JocSpeakerRenderer::new("22.2", control(false, 6)).unwrap_err();
        assert!(matches!(error, JocRenderError::UnsupportedLayout(_)));
        assert!(error.to_string().contains("5.1.4"));
        assert!(error.to_string().contains("7.1.2"));
        assert!(error.to_string().contains("7.1.4"));
        assert!(!error.to_string().contains("fallback"));
    }

    #[test]
    fn stereo_downmix_policy_is_admitted_only_for_the_2_0_speaker_layout() {
        let mut stereo = JocSpeakerRenderer::new_automatic("2.0").unwrap();
        stereo
            .set_downmix_policy(StereoDownmixPolicy::LtRt)
            .expect("2.0 accepts stereo policy");
        let mut multichannel = JocSpeakerRenderer::new_automatic("5.1").unwrap();
        assert!(
            multichannel
                .set_downmix_policy(StereoDownmixPolicy::LoRo)
                .is_err()
        );
    }

    #[test]
    fn stereo_diagnostic_explains_position_labels_without_changing_identities() {
        let renderer = JocSpeakerRenderer::new_automatic("2.0").unwrap();
        let summary = openjoc_scene::StreamingSceneSummary {
            sample_rate: 48_000,
            duration_samples: 0,
            frames: 0,
            object_count: 0,
            max_reconstruction_rows: 0,
            max_frame_samples: 0,
            metadata_events: 0,
            trim_events: 0,
        };
        let diagnostic = renderer.diagnostics(
            "2.0",
            crate::eac3_decode::ValidationProfileRequest::EtsiStrict,
            JocValidationProfile::EtsiStrict,
            &summary,
            Path::new("stereo.wav"),
        );
        assert!(diagnostic.contains("output layout: Stereo speakers (2.0)"));
        assert!(diagnostic.contains("output channel order: Left, Right"));
        assert!(diagnostic.contains("speaker identities: FL, FR"));
    }

    #[test]
    fn automatic_20_render_initializes_spatial_topology_before_downmix() {
        let positions = [
            (
                "center",
                openjoc_oamd::Position3 {
                    x: 0.5,
                    y: 0.5,
                    z: 0.0,
                },
            ),
            (
                "left",
                openjoc_oamd::Position3 {
                    x: 0.0,
                    y: 0.5,
                    z: 0.0,
                },
            ),
            (
                "right",
                openjoc_oamd::Position3 {
                    x: 1.0,
                    y: 0.5,
                    z: 0.0,
                },
            ),
            (
                "rear",
                openjoc_oamd::Position3 {
                    x: 0.5,
                    y: 1.0,
                    z: 0.0,
                },
            ),
            (
                "top",
                openjoc_oamd::Position3 {
                    x: 0.5,
                    y: 0.5,
                    z: 1.0,
                },
            ),
        ];
        let frame = automatic_decoded_frame(positions[0].1);
        let base_coordinates = [
            BaseFullBandCoordinate::Left,
            BaseFullBandCoordinate::Right,
            BaseFullBandCoordinate::Centre,
            BaseFullBandCoordinate::LeftSurround,
            BaseFullBandCoordinate::RightSurround,
        ];
        let mut topology_renderer = JocSpeakerRenderer::new_automatic("2.0").unwrap();
        let control = topology_renderer
            .assembler
            .as_mut()
            .unwrap()
            .assemble_frame(&frame, &base_coordinates, None)
            .unwrap();
        let topology = control.initial_topology.as_ref().unwrap();
        for record in topology.flatten().iter().filter(|record| record.active) {
            assert_eq!(record.descriptor.coordinates.len(), 3);
            assert!(
                record
                    .descriptor
                    .coordinates
                    .iter()
                    .all(|value| value.is_finite())
            );
        }

        for policy in [
            StereoDownmixPolicy::Auto,
            StereoDownmixPolicy::LoRo,
            StereoDownmixPolicy::LtRt,
        ] {
            for (label, position) in positions {
                let mut renderer = JocSpeakerRenderer::new_automatic("2.0").unwrap();
                renderer.set_downmix_policy(policy).unwrap();
                let block = renderer
                    .render_frame(0, &automatic_decoded_frame(position), &base(2, 1.0))
                    .unwrap_or_else(|error| panic!("{label} / {policy:?}: {error}"));
                assert_eq!(block.channels.len(), 2);
                assert!(
                    block
                        .channels
                        .iter()
                        .flatten()
                        .all(|sample| sample.is_finite())
                );
            }
        }
    }

    #[test]
    fn two_point_zero_speaker_renderer_combines_projected_objects_with_base_stereo() {
        let mut renderer = JocSpeakerRenderer::new("2.0", control(false, 6)).unwrap();
        renderer
            .set_downmix_policy(StereoDownmixPolicy::LoRo)
            .unwrap();
        let block = renderer
            .render_frame(0, &decoded_frame(0, 0, 2), &base(2, 1.0))
            .unwrap();
        assert_eq!(block.channels.len(), 2);
        assert_eq!(block.channels[0].len(), 2);
        assert_eq!(block.channels[1].len(), 2);
        assert_ne!(block.channels[0], block.channels[1]);
    }

    #[test]
    fn binaural_mapping_uses_public_channel_order_for_every_preset() {
        for layout in JOC_RENDER_SUPPORTED_LAYOUTS
            .into_iter()
            .filter(|layout| !matches!(*layout, "7.1.6" | "9.1" | "9.1.2" | "9.1.4" | "9.1.6"))
        {
            let renderer = JocBinauralRenderer::new(
                layout,
                binaural_bank(layout, 48_000),
                BinauralBackend::Direct,
                Some(BinauralLfePolicy::Exclude),
                Some(control(false, 6)),
            )
            .unwrap();
            let preset = SpeakerLayoutPreset::for_name(layout).unwrap();
            let expected = preset
                .labels
                .iter()
                .enumerate()
                .filter(|(index, _)| preset.lfe_index() != Some(*index))
                .map(|(index, label)| (*label, index))
                .collect::<Vec<_>>();
            assert_eq!(
                renderer
                    .mappings
                    .iter()
                    .map(|mapping| (mapping.label, mapping.channel_index))
                    .collect::<Vec<_>>(),
                expected
            );
            for mapping in &renderer.mappings {
                assert_eq!(
                    mapping.direction,
                    virtual_speaker_direction(mapping.label).unwrap()
                );
            }
        }
    }

    #[test]
    fn binaural_preflight_admits_public_716_direction_mappings() {
        super::validate_binaural_layout("7.1.6").expect("7.1.6 has public directions");
    }

    #[test]
    fn binaural_preflight_admits_the_91_family_direction_mappings() {
        for layout in ["9.1", "9.1.2", "9.1.4", "9.1.6"] {
            super::validate_binaural_layout(layout)
                .expect("9.1-family has public direction mappings");
        }
    }

    #[test]
    fn binaural_interpolates_top_middle_and_wide_directions_from_714_bank() {
        for (layout, expected_interpolated) in [("7.1.6", 2), ("9.1.6", 4)] {
            let renderer = JocBinauralRenderer::new(
                layout,
                binaural_bank("7.1.4", 48_000),
                BinauralBackend::Direct,
                Some(BinauralLfePolicy::Exclude),
                Some(control(false, 6)),
            )
            .expect("covered virtual directions interpolate");
            assert_eq!(renderer.interpolated_hrir_count, expected_interpolated);
            assert_eq!(
                renderer.mappings.len(),
                SpeakerLayoutPreset::for_name(layout)
                    .unwrap()
                    .channel_count()
                    - 1
            );
        }
    }

    #[test]
    fn binaural_preflight_rejects_missing_exact_hrir_direction() {
        let preset = SpeakerLayoutPreset::for_name("5.1").unwrap();
        let bank = HrirBank::new(
            48_000,
            vec![
                HrirEntry::new(
                    HrirEntryId::new(1),
                    virtual_speaker_direction("FL").unwrap(),
                    HrirPair::new(48_000, vec![1.0], vec![1.0]).unwrap(),
                )
                .unwrap(),
            ],
        )
        .unwrap();
        let Err(error) = JocBinauralRenderer::new(
            "5.1",
            bank,
            BinauralBackend::Direct,
            Some(BinauralLfePolicy::Exclude),
            Some(control(false, 6)),
        ) else {
            panic!("missing HRIR coverage was accepted");
        };
        assert!(matches!(error, JocRenderError::BinauralHrirCoverage { .. }));
        assert!(error.to_string().contains("FR"));
        assert_eq!(preset.channel_count(), 6);
    }

    #[test]
    fn binaural_sample_rate_is_checked_before_rendering() {
        let mut renderer = JocBinauralRenderer::new(
            "5.1",
            binaural_bank("5.1", 44_100),
            BinauralBackend::Direct,
            Some(BinauralLfePolicy::Exclude),
            Some(control(false, 6)),
        )
        .unwrap();
        let error = renderer
            .render_frame(0, &decoded_frame(0, 0, 2), &base(2, 1.0))
            .unwrap_err();
        assert!(matches!(
            error,
            JocRenderError::BinauralSampleRateMismatch {
                expected: 48_000,
                actual: 44_100
            }
        ));
    }

    #[test]
    fn integrated_direct_matches_existing_direct_reference_and_drains_tail() {
        let bank = binaural_bank("5.1", 48_000);
        let frame = decoded_frame(0, 0, 5);
        let pcm = base(5, 1.0);
        let mut speaker = JocSpeakerRenderer::new("5.1", control(false, 6)).unwrap();
        let rendered = speaker.render_frame(0, &frame, &pcm).unwrap();
        let mut integrated = JocBinauralRenderer::new(
            "5.1",
            bank.clone(),
            BinauralBackend::Direct,
            Some(BinauralLfePolicy::Exclude),
            Some(control(false, 6)),
        )
        .unwrap();
        let actual = collect_binaural(&mut integrated, 0, &frame, &pcm);
        let definitions = integrated
            .mappings
            .iter()
            .map(|mapping| {
                StaticBinauralSource::new(
                    mapping.source_id,
                    mapping.direction,
                    1.0,
                    mapping.hrir_entry,
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let mut reference = BinauralRenderer::new(48_000, bank, definitions.clone()).unwrap();
        let blocks = integrated
            .mappings
            .iter()
            .map(|mapping| {
                BinauralSourceBlock::new(
                    mapping.source_id,
                    &rendered.channels[mapping.channel_index],
                )
            })
            .collect::<Vec<_>>();
        let mut left = vec![0.0; 5];
        let mut right = vec![0.0; 5];
        reference
            .render_block(&blocks, &mut left, &mut right)
            .unwrap();
        let mut expected = left.into_iter().zip(right).collect::<Vec<_>>();
        let mut tail_left = vec![0.0; 2];
        let mut tail_right = vec![0.0; 2];
        reference
            .drain_tail_block(&mut tail_left, &mut tail_right)
            .unwrap();
        expected.extend(tail_left.into_iter().zip(tail_right));
        assert_eq!(actual, expected);
        assert_eq!(actual.len(), 7);
    }

    #[test]
    fn integrated_partitioned_matches_direct_across_frame_partition_boundary() {
        let frame = decoded_frame(0, 0, 5);
        let pcm = base(5, 1.0);
        let bank = binaural_bank("5.1", 48_000);
        let mut direct = JocBinauralRenderer::new(
            "5.1",
            bank.clone(),
            BinauralBackend::Direct,
            Some(BinauralLfePolicy::Exclude),
            Some(control(false, 6)),
        )
        .unwrap();
        let mut partitioned = JocBinauralRenderer::new(
            "5.1",
            bank,
            BinauralBackend::Partitioned { partition_size: 4 },
            Some(BinauralLfePolicy::Exclude),
            Some(control(false, 6)),
        )
        .unwrap();
        let direct_output = collect_binaural(&mut direct, 0, &frame, &pcm);
        let partitioned_output = collect_binaural(&mut partitioned, 0, &frame, &pcm);
        assert_eq!(direct_output.len(), partitioned_output.len());
        for ((left, right), (other_left, other_right)) in
            direct_output.into_iter().zip(partitioned_output)
        {
            assert!((left - other_left).abs() < 1.0e-10);
            assert!((right - other_right).abs() < 1.0e-10);
        }
    }

    #[test]
    fn integrated_binaural_reset_reuses_bridge_and_convolution_state() {
        let frame = decoded_frame(0, 0, 3);
        let pcm = base(3, 1.0);
        let mut renderer = JocBinauralRenderer::new(
            "5.1",
            binaural_bank("5.1", 48_000),
            BinauralBackend::Direct,
            Some(BinauralLfePolicy::Exclude),
            Some(control(false, 6)),
        )
        .unwrap();
        let first = collect_binaural(&mut renderer, 0, &frame, &pcm);
        renderer.reset();
        let second = collect_binaural(&mut renderer, 0, &frame, &pcm);
        assert_eq!(first, second);
    }

    #[test]
    fn integrated_partitioned_output_is_invariant_to_upstream_joc_frame_boundaries() {
        let bank = binaural_bank("5.1", 48_000);
        let mut whole = JocBinauralRenderer::new(
            "5.1",
            bank.clone(),
            BinauralBackend::Partitioned { partition_size: 4 },
            Some(BinauralLfePolicy::Exclude),
            Some(control(false, 6)),
        )
        .unwrap();
        let whole_frame = decoded_frame(0, 0, 5);
        let whole_pcm = base(5, 1.0);
        let whole_output = collect_binaural(&mut whole, 0, &whole_frame, &whole_pcm);

        let mut split = JocBinauralRenderer::new(
            "5.1",
            bank,
            BinauralBackend::Partitioned { partition_size: 4 },
            Some(BinauralLfePolicy::Exclude),
            Some(control(false, 6)),
        )
        .unwrap();
        let first_frame = decoded_frame(0, 0, 2);
        let second_frame = decoded_frame(1, 2, 3);
        let first_pcm = base(2, 1.0);
        let second_pcm = base(3, 1.0);
        let mut split_output = split.render_frame(0, &first_frame, &first_pcm).unwrap();
        split_output.extend(split.render_frame(1, &second_frame, &second_pcm).unwrap());
        split_output.extend(split.finish().unwrap());
        let split_output = split_output
            .into_iter()
            .flat_map(|block| block.left.into_iter().zip(block.right))
            .collect::<Vec<_>>();
        assert_eq!(whole_output.len(), split_output.len());
        for ((left, right), (other_left, other_right)) in whole_output.into_iter().zip(split_output)
        {
            assert!((left - other_left).abs() < 1.0e-10);
            assert!((right - other_right).abs() < 1.0e-10);
        }
    }

    #[test]
    fn binaural_lfe_requires_policy_and_dual_mono_is_explicit() {
        let Err(missing) = JocBinauralRenderer::new(
            "5.1",
            binaural_bank("5.1", 48_000),
            BinauralBackend::Direct,
            None,
            Some(control(false, 6)),
        ) else {
            panic!("missing LFE policy was accepted");
        };
        assert!(matches!(
            missing,
            JocRenderError::BinauralLfePolicyRequired { .. }
        ));
        let frame = decoded_frame(0, 0, 2);
        let pcm = base(2, 0.0);
        let mut exclude = JocBinauralRenderer::new(
            "5.1",
            binaural_bank("5.1", 48_000),
            BinauralBackend::Direct,
            Some(BinauralLfePolicy::Exclude),
            Some(control(false, 6)),
        )
        .unwrap();
        let mut dual = JocBinauralRenderer::new(
            "5.1",
            binaural_bank("5.1", 48_000),
            BinauralBackend::Direct,
            Some(BinauralLfePolicy::EqualPowerDualMono),
            Some(control(false, 6)),
        )
        .unwrap();
        let excluded = collect_binaural(&mut exclude, 0, &frame, &pcm);
        let dual_mono = collect_binaural(&mut dual, 0, &frame, &pcm);
        assert!((dual_mono[0].0 - excluded[0].0 - 99.0 / 2.0_f64.sqrt()).abs() < 1.0e-12);
        assert!((dual_mono[0].1 - excluded[0].1 - 99.0 / 2.0_f64.sqrt()).abs() < 1.0e-12);
    }

    #[test]
    fn binaural_output_abort_leaves_no_final_wav() {
        let root = std::env::temp_dir().join(format!(
            "openjoc-binaural-transaction-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let output = root.join("transactional-binaural.wav");
        let staging = root.join(".transactional-binaural.wav.openjoc-partial");
        let mut writer =
            JocWavOutput::new_with_overwrite(&output, SampleFormat::F32, false).unwrap();
        writer
            .write_block(&RenderedBlock {
                sample_rate: 48_000,
                channels: vec![vec![0.0, 1.0], vec![0.0, 1.0]],
            })
            .unwrap();
        writer.abort();
        assert!(!output.exists());
        assert!(!staging.exists());
    }

    #[test]
    fn authorized_overwrite_abort_preserves_existing_final_wav() {
        let root = std::env::temp_dir().join(format!(
            "openjoc-overwrite-abort-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let output = root.join("existing.wav");
        fs::write(&output, b"previous-valid-output").unwrap();
        let mut writer =
            JocWavOutput::new_with_overwrite(&output, SampleFormat::F32, true).unwrap();
        writer
            .write_block(&RenderedBlock {
                sample_rate: 48_000,
                channels: vec![vec![0.0, 1.0], vec![1.0, 0.0]],
            })
            .unwrap();
        writer.abort();
        assert_eq!(fs::read(&output).unwrap(), b"previous-valid-output");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn authorized_overwrite_replaces_existing_final_wav_only_on_finish() {
        let root = std::env::temp_dir().join(format!(
            "openjoc-overwrite-finish-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let output = root.join("existing.wav");
        fs::write(&output, b"previous-valid-output").unwrap();
        let mut writer =
            JocWavOutput::new_with_overwrite(&output, SampleFormat::F32, true).unwrap();
        writer
            .write_block(&RenderedBlock {
                sample_rate: 48_000,
                channels: vec![vec![0.25, 0.5], vec![-0.25, -0.5]],
            })
            .unwrap();
        writer.finish().unwrap();
        let pcm = decode(&fs::read(&output).unwrap()).unwrap();
        assert_eq!(pcm.sample_rate, 48_000);
        assert_eq!(pcm.channels, vec![vec![0.25, 0.5], vec![-0.25, -0.5]]);
        fs::remove_dir_all(root).unwrap();
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
            if let Some(lfe_index) = preset.lfe_index() {
                assert_eq!(block.channels[lfe_index], vec![99.0; 2]);
            } else {
                assert_ne!(block.channels, vec![vec![99.0; 2], vec![99.0; 2]]);
            }
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
                    vec![1.0 / 3.0, 0.5, 32_767.0 / 32_768.0],
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
    fn public_716_renderer_uses_the_normal_decoded_pipeline_and_keeps_lfe_base_owned() {
        let mut renderer = JocSpeakerRenderer::new("7.1.6", control(false, 6)).unwrap();
        let block = renderer
            .render_frame(0, &decoded_frame(0, 0, 2), &base(2, 1.0))
            .unwrap();
        assert_eq!(block.channels.len(), 14);
        assert_eq!(block.channels[3], vec![99.0; 2]);
        assert!(block.channels[10].iter().all(|sample| sample.is_finite()));
        assert!(block.channels[11].iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn contribution_modes_decompose_every_output_and_assign_lfe_to_base() {
        let frame = decoded_frame(0, 0, 3);
        let pcm = base(3, 1.0);
        for (layout, channel_count) in [
            ("2.0", 2),
            ("5.1.4", 10),
            ("7.1.2", 10),
            ("7.1.4", 12),
            ("7.1.6", 14),
            ("9.1", 10),
            ("9.1.2", 12),
            ("9.1.4", 14),
            ("9.1.6", 16),
        ] {
            let render = |mode| {
                JocSpeakerRenderer::new_with_contribution(layout, control(false, 6), mode)
                    .unwrap()
                    .render_frame(0, &frame, &pcm)
                    .unwrap()
            };
            let full = render(SpatialContributionMode::Full);
            let base_only = render(SpatialContributionMode::BaseOnly);
            let reconstruction_only = render(SpatialContributionMode::ReconstructionOnly);

            assert_eq!(full.channels.len(), channel_count);
            for channel in 0..full.channels.len() {
                for sample in 0..full.channels[channel].len() {
                    assert!(
                        (full.channels[channel][sample]
                            - base_only.channels[channel][sample]
                            - reconstruction_only.channels[channel][sample])
                            .abs()
                            < 1.0e-12
                    );
                }
            }
            if layout == "2.0" {
                continue;
            }
            assert_eq!(full.channels[3], base_only.channels[3]);
            assert_eq!(full.channels[3], vec![99.0; 3]);
            assert_eq!(reconstruction_only.channels[3], vec![0.0; 3]);
            assert_eq!(base_only.channels[2], vec![3.0; 3]);
            assert_eq!(reconstruction_only.channels[2], vec![6.0; 3]);
            assert_eq!(full.channels[2], vec![9.0; 3]);
        }
    }

    #[test]
    fn stereo_contribution_metrics_show_base_matrix_headroom_and_linearity() {
        let frame = decoded_frame(0, 0, 1);
        let pcm = stereo_base(
            &[
                ChannelLocation::Left,
                ChannelLocation::Right,
                ChannelLocation::Centre,
                ChannelLocation::LeftSurround,
                ChannelLocation::RightSurround,
            ],
            &[1.0; 5],
            None,
            DownmixMetadata::default(),
        );
        let render = |mode| {
            JocSpeakerRenderer::new_with_contribution("2.0", control(false, 6), mode)
                .unwrap()
                .render_frame(0, &frame, &pcm)
                .unwrap()
        };
        let base_only = render(SpatialContributionMode::BaseOnly);
        let reconstruction_only = render(SpatialContributionMode::ReconstructionOnly);
        let full = render(SpatialContributionMode::Full);
        let (base_peak, base_db) = peak_metrics(&base_only.channels);
        let (reconstruction_peak, reconstruction_db) = peak_metrics(&reconstruction_only.channels);
        let (full_peak, full_db) = peak_metrics(&full.channels);
        assert!(
            base_peak <= 1.0 + 1.0e-12,
            "Base peak={base_peak} ({base_db} dBFS)"
        );
        assert!(reconstruction_peak.is_finite());
        assert!(full_peak.is_finite());
        assert!(reconstruction_db.is_finite());
        assert!(full_db.is_finite());
        for channel in 0..2 {
            assert_eq!(
                full.channels[channel][0],
                base_only.channels[channel][0] + reconstruction_only.channels[channel][0]
            );
        }
    }

    #[test]
    fn default_full_is_identical_to_explicit_full() {
        let first_frame = decoded_frame(0, 0, 2);
        let second_frame = decoded_frame(1, 2, 2);
        let pcm = base(2, 0.25);
        let mut default_renderer = JocSpeakerRenderer::new("7.1.4", control(true, 6)).unwrap();
        let mut explicit_full = JocSpeakerRenderer::new_with_contribution(
            "7.1.4",
            control(true, 6),
            SpatialContributionMode::Full,
        )
        .unwrap();

        for (index, frame) in [first_frame, second_frame].iter().enumerate() {
            assert_eq!(
                default_renderer.render_frame(index, frame, &pcm).unwrap(),
                explicit_full.render_frame(index, frame, &pcm).unwrap()
            );
        }
        default_renderer.finish().unwrap();
        explicit_full.finish().unwrap();
    }

    #[test]
    fn default_full_wav_matches_start_head_checksum() {
        const START_HEAD_FNV1A64: u64 = 0x78c8_61a1_e818_8c45;
        let path = std::env::temp_dir().join(format!(
            "openjoc-full-checksum-{}-{}.wav",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut renderer = JocSpeakerRenderer::new("7.1.4", control(true, 6)).unwrap();
        let mut output =
            JocWavOutput::new_for_speaker_layout(&path, SampleFormat::F32, false, "7.1.4").unwrap();
        for (index, frame) in [decoded_frame(0, 0, 2), decoded_frame(1, 2, 2)]
            .iter()
            .enumerate()
        {
            output
                .write_block(&renderer.render_frame(index, frame, &base(2, 0.25)).unwrap())
                .unwrap();
        }
        renderer.finish().unwrap();
        output.finish().unwrap();
        let bytes = fs::read(&path).unwrap();
        let checksum = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
        assert_eq!(bytes.len(), 260);
        assert_eq!(checksum, START_HEAD_FNV1A64);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn contribution_selection_preserves_control_scheduler_and_timeline_state() {
        let renderer = |mode| {
            let mut renderer =
                JocSpeakerRenderer::new_with_contribution("5.1", control(true, 6), mode).unwrap();
            renderer.selected_profile = Some(JocValidationProfile::EtsiStrict);
            renderer
        };
        let mut full = renderer(SpatialContributionMode::Full);
        let mut base_only = renderer(SpatialContributionMode::BaseOnly);
        let mut reconstruction_only = renderer(SpatialContributionMode::ReconstructionOnly);

        for (index, frame) in [decoded_frame(0, 0, 33), decoded_frame(1, 33, 31)]
            .iter()
            .enumerate()
        {
            let pcm = base(frame.sample_range.len() as usize, 0.5);
            full.render_frame(index, frame, &pcm).unwrap();
            base_only.render_frame(index, frame, &pcm).unwrap();
            reconstruction_only
                .render_frame(index, frame, &pcm)
                .unwrap();
        }

        for renderer in [&base_only, &reconstruction_only] {
            assert_eq!(renderer.expected_coordinates, full.expected_coordinates);
            assert_eq!(renderer.expected_frame, full.expected_frame);
            assert_eq!(renderer.expected_sample, full.expected_sample);
            assert_eq!(renderer.base_coordinates, full.base_coordinates);
            assert_eq!(renderer.selected_profile, full.selected_profile);
            assert_eq!(renderer.deviations, full.deviations);
            assert_eq!(renderer.bridge, full.bridge);
            assert_eq!(
                renderer.control.as_ref().unwrap().consumed_updates,
                full.control.as_ref().unwrap().consumed_updates
            );
            assert_eq!(
                renderer.bridge.semantic_binding(),
                full.bridge.semantic_binding()
            );
        }
        assert_eq!(full.expected_frame, 2);
        assert_eq!(full.expected_sample, 64);
        assert_eq!(full.control.as_ref().unwrap().consumed_updates, [true]);
        full.finish().unwrap();
        base_only.finish().unwrap();
        reconstruction_only.finish().unwrap();
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
        let error = JocSpeakerRenderer::new("22.2", control(false, 6)).unwrap_err();
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
        let mut output = JocWavOutput::new_with_overwrite(&path, SampleFormat::F32, false).unwrap();
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
        let mut output =
            JocWavOutput::new_for_speaker_layout(&path, SampleFormat::F32, false, "7.1.4").unwrap();
        output
            .write_block(&RenderedBlock {
                sample_rate: 48_000,
                channels,
            })
            .unwrap();
        output.finish().unwrap();

        let pcm = decode(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(pcm.sample_rate, 48_000);
        assert_eq!(pcm.channel_mask, Some(0x0002_d63f));
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
    fn new_height_presets_emit_exact_extensible_headers_and_channel_payload_order() {
        for (layout, channel_count, mask) in
            [("5.1.4", 10, 0x0002_d60f), ("7.1.2", 10, 0x0000_563f)]
        {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "openjoc-joc-render-{layout}-{}-{nonce}.wav",
                std::process::id()
            ));
            let channels = (0..channel_count)
                .map(|index| vec![index as f64, -(index as f64)])
                .collect::<Vec<_>>();
            let mut output =
                JocWavOutput::new_for_speaker_layout(&path, SampleFormat::F32, false, layout)
                    .unwrap();
            output
                .write_block(&RenderedBlock {
                    sample_rate: 48_000,
                    channels,
                })
                .unwrap();
            output.finish().unwrap();

            let bytes = fs::read(&path).unwrap();
            assert_eq!(
                u16::from_le_bytes(bytes[20..22].try_into().unwrap()),
                0xfffe
            );
            assert_eq!(u16::from_le_bytes(bytes[34..36].try_into().unwrap()), 32);
            assert_eq!(u16::from_le_bytes(bytes[36..38].try_into().unwrap()), 22);
            assert_eq!(u16::from_le_bytes(bytes[38..40].try_into().unwrap()), 32);
            assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), mask);
            let pcm = decode(&bytes).unwrap();
            assert_eq!(pcm.channel_mask, Some(mask));
            assert_eq!(pcm.channels.len(), channel_count);
            for (index, channel) in pcm.channels.iter().enumerate() {
                assert!((channel[0] - index as f64).abs() < 1.0e-6);
                assert!((channel[1] + index as f64).abs() < 1.0e-6);
            }
            fs::remove_file(path).unwrap();
        }
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
        let mut output = JocWavOutput::new_with_overwrite(&path, SampleFormat::F32, false).unwrap();
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

    #[test]
    fn public_layouts_are_representable_in_both_selected_containers() {
        for &layout in JOC_RENDER_SUPPORTED_LAYOUTS.iter().filter(|layout| {
            matches!(
                **layout,
                "2.0" | "5.1" | "5.1.2" | "5.1.4" | "7.1" | "7.1.2" | "7.1.4"
            )
        }) {
            let root = std::env::temp_dir().join(format!(
                "openjoc-output-matrix-{}-{}",
                std::process::id(),
                layout.replace('.', "_")
            ));
            fs::create_dir_all(&root).unwrap();
            let preset = SpeakerLayoutPreset::for_name(layout).unwrap();
            let channels = (0..preset.channel_count())
                .map(|index| vec![index as f64 / 32.0])
                .collect::<Vec<_>>();
            let wav_path = root.join("render.wav");
            let mut wav =
                JocPcmOutput::new_for_speaker_layout(&wav_path, SampleFormat::F32, false, layout)
                    .unwrap();
            assert_eq!(wav.container(), OutputContainer::Wav);
            wav.write_block(&RenderedBlock {
                sample_rate: 48_000,
                channels: channels.clone(),
            })
            .unwrap();
            wav.finish().unwrap();
            let decoded = decode(&fs::read(&wav_path).unwrap()).unwrap();
            assert_eq!(decoded.channels.len(), preset.channel_count());
            assert_eq!(decoded.channel_mask, preset.wav_channel_mask());

            let caf_path = root.join("render.caf");
            let mut caf =
                JocPcmOutput::new_for_speaker_layout(&caf_path, SampleFormat::F32, false, layout)
                    .unwrap();
            assert_eq!(caf.container(), OutputContainer::Caf);
            caf.write_block(&RenderedBlock {
                sample_rate: 48_000,
                channels,
            })
            .unwrap();
            caf.finish().unwrap();
            let descriptions = caf_channel_descriptions(&fs::read(&caf_path).unwrap());
            assert_eq!(descriptions.len(), preset.channel_count());
            for (description, label) in descriptions.iter().zip(&preset.labels) {
                assert_eq!(description.0, expected_caf_label(label));
                assert_eq!(description.1, 0);
            }
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn public_91_family_is_caf_capable_and_wav_fails_closed() {
        for layout in ["9.1", "9.1.2", "9.1.4", "9.1.6"] {
            let root = std::env::temp_dir().join(format!(
                "openjoc-output-matrix-{layout}-{}",
                std::process::id()
            ));
            fs::create_dir_all(&root).unwrap();
            let preset = SpeakerLayoutPreset::for_name(layout).unwrap();
            let semantic = preset.semantic_channel_layout();
            assert_eq!(preset.channel_count(), semantic.labels.len());
            assert_eq!(preset.lfe_index(), Some(3));
            assert_eq!(preset.wav_channel_mask(), None);

            let wav_path = root.join("render.wav");
            let result = JocPcmOutput::new_for_semantic_layout(
                &wav_path,
                SampleFormat::F32,
                false,
                &semantic,
            );
            let Err(error) = result else {
                panic!("9.1-family WAV must fail before output creation");
            };
            assert!(matches!(
                error,
                JocRenderError::WavLayoutNotExactlyRepresentable { layout: ref selected }
                    if selected == layout
            ));
            assert!(!wav_path.exists());

            let caf_path = root.join("render.caf");
            let mut caf = JocPcmOutput::new_for_semantic_layout(
                &caf_path,
                SampleFormat::F32,
                false,
                &semantic,
            )
            .unwrap();
            let channels = (0..preset.channel_count())
                .map(|index| vec![index as f64 / 32.0, -(index as f64) / 32.0])
                .collect::<Vec<_>>();
            caf.write_block(&RenderedBlock {
                sample_rate: 48_000,
                channels: channels.clone(),
            })
            .unwrap();
            caf.finish().unwrap();
            let descriptions = caf_channel_descriptions(&fs::read(&caf_path).unwrap());
            assert_eq!(descriptions.len(), preset.channel_count());
            assert_eq!(descriptions[3].0, expected_caf_label("LFE"));
            assert_eq!(
                descriptions
                    .iter()
                    .map(|description| description.0)
                    .collect::<Vec<_>>(),
                preset
                    .labels
                    .iter()
                    .map(|label| expected_caf_label(label))
                    .collect::<Vec<_>>()
            );
            assert_eq!(caf_f32_samples(&fs::read(&caf_path).unwrap()), {
                (0..2)
                    .flat_map(|frame| channels.iter().map(move |channel| channel[frame]))
                    .collect::<Vec<_>>()
            });
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn public_716_semantics_reject_wav_and_accept_caf_with_top_middle_coordinates() {
        let preset = SpeakerLayoutPreset::for_name("7.1.6").unwrap();
        let semantic = preset.semantic_channel_layout();
        let root = std::env::temp_dir().join(format!("openjoc-716-output-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let Err(wav_error) = JocWavOutput::new_for_semantic_layout(
            &root.join("future.wav"),
            SampleFormat::F32,
            false,
            &semantic,
        ) else {
            panic!("7.1.6 must not be admitted to WAV");
        };
        assert!(matches!(
            wav_error,
            JocRenderError::WavLayoutNotExactlyRepresentable { .. }
        ));
        assert!(!root.join("future.wav").exists());

        let path = root.join("future.caf");
        let mut caf =
            JocCafOutput::new_for_semantic_layout(&path, SampleFormat::F32, false, &semantic)
                .unwrap();
        caf.write_block(&RenderedBlock {
            sample_rate: 48_000,
            channels: (0..semantic.channel_count())
                .map(|index| vec![index as f64 / 32.0])
                .collect(),
        })
        .unwrap();
        caf.finish().unwrap();
        let bytes = fs::read(&path).unwrap();
        let descriptions = caf_channel_descriptions(&bytes);
        assert_eq!(
            descriptions
                .iter()
                .map(|description| description.0)
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4, 5, 6, 10, 11, 13, 15, 100, 100, 16, 18]
        );
        assert_eq!(descriptions[10].1, 1);
        assert_eq!(descriptions[11].1, 1);
        assert!(descriptions[10].2[0] < 0.0);
        assert!(descriptions[11].2[0] > 0.0);
        assert_eq!(descriptions[10].2[1], 0.0);
        assert_eq!(descriptions[11].2[1], 0.0);
        assert!(descriptions[10].2[2] > 0.99);
        assert!(descriptions[11].2[2] > 0.99);
        assert_eq!(
            caf_f32_samples(&bytes),
            (0..semantic.channel_count())
                .map(|index| index as f64 / 32.0)
                .collect::<Vec<_>>()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn caf_transaction_abort_preserves_authorized_existing_output() {
        let root =
            std::env::temp_dir().join(format!("openjoc-caf-transaction-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let output = root.join("existing.caf");
        fs::write(&output, b"previous-valid-output").unwrap();
        let semantic = SemanticChannelLayout::without_wav_mapping(
            "5.1",
            ["FL", "FR", "FC", "LFE", "Ls", "Rs"],
            Some(3),
        );
        let mut caf =
            JocCafOutput::new_for_semantic_layout(&output, SampleFormat::F32, true, &semantic)
                .unwrap();
        caf.write_block(&RenderedBlock {
            sample_rate: 48_000,
            channels: vec![vec![0.0]; semantic.channel_count()],
        })
        .unwrap();
        caf.abort();
        assert_eq!(fs::read(&output).unwrap(), b"previous-valid-output");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn future_wide_semantics_use_public_caf_wide_labels_without_geometry_inference() {
        let semantic = SemanticChannelLayout::without_wav_mapping(
            "9.1",
            ["FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "Lw", "Rw"],
            Some(3),
        );
        let root = std::env::temp_dir().join(format!("openjoc-wide-output-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("wide.caf");
        let mut caf =
            JocCafOutput::new_for_semantic_layout(&path, SampleFormat::F32, false, &semantic)
                .unwrap();
        caf.write_block(&RenderedBlock {
            sample_rate: 48_000,
            channels: vec![vec![0.0]; semantic.channel_count()],
        })
        .unwrap();
        caf.finish().unwrap();
        let descriptions = caf_channel_descriptions(&fs::read(path).unwrap());
        assert_eq!(descriptions[8].0, 35);
        assert_eq!(descriptions[9].0, 36);
        assert_eq!(descriptions[8].1, 0);
        assert_eq!(descriptions[9].1, 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extension_selection_is_explicit_and_withheld_layouts_stay_private() {
        assert_eq!(
            JOC_RENDER_SUPPORTED_LAYOUTS,
            [
                "2.0", "5.1", "5.1.2", "5.1.4", "7.1", "7.1.2", "7.1.4", "7.1.6", "9.1", "9.1.2",
                "9.1.4", "9.1.6",
            ]
        );
        assert!(SpeakerLayoutPreset::for_name("22.2").is_err());
        assert_eq!(
            super::validate_output_path(std::path::Path::new("output.WAV")).unwrap(),
            OutputContainer::Wav
        );
        assert_eq!(
            super::validate_output_path(std::path::Path::new("output.caf")).unwrap(),
            OutputContainer::Caf
        );
        assert!(matches!(
            super::validate_output_path(std::path::Path::new("output.raw")),
            Err(JocRenderError::UnsupportedOutputExtension(_))
        ));
    }

    #[test]
    fn rendered_pcm_is_identical_when_serialized_to_wav_or_caf() {
        let root =
            std::env::temp_dir().join(format!("openjoc-pcm-identity-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let wav_path = root.join("render.wav");
        let caf_path = root.join("render.caf");
        let mut wav =
            JocPcmOutput::new_for_speaker_layout(&wav_path, SampleFormat::F32, false, "7.1.4")
                .unwrap();
        let mut caf =
            JocPcmOutput::new_for_speaker_layout(&caf_path, SampleFormat::F32, false, "7.1.4")
                .unwrap();
        let mut renderer = JocSpeakerRenderer::new("7.1.4", control(true, 6)).unwrap();
        let mut expected_frames = 0_u64;
        for (index, frame) in [decoded_frame(0, 0, 2), decoded_frame(1, 2, 2)]
            .iter()
            .enumerate()
        {
            let block = renderer.render_frame(index, frame, &base(2, 0.25)).unwrap();
            expected_frames += block.channels[0].len() as u64;
            wav.write_block(&block).unwrap();
            caf.write_block(&block).unwrap();
        }
        renderer.finish().unwrap();
        wav.finish().unwrap();
        caf.finish().unwrap();
        let wav_pcm = decode(&fs::read(wav_path).unwrap()).unwrap();
        let caf_samples = caf_f32_samples(&fs::read(caf_path).unwrap());
        let wav_samples = (0..wav_pcm.channels[0].len())
            .flat_map(|frame| wav_pcm.channels.iter().map(move |channel| channel[frame]))
            .collect::<Vec<_>>();
        assert_eq!(wav_samples, caf_samples);
        assert_eq!(wav_pcm.channels.len(), 12);
        assert_eq!(wav_pcm.channels[0].len() as u64, expected_frames);
        assert_eq!(caf_samples.len(), 12 * expected_frames as usize);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wav_sink_reports_unrepresentable_future_semantics_instead_of_losing_identity() {
        let path = std::env::temp_dir().join(format!(
            "openjoc-joc-render-716-red-{}-{}.wav",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let semantic = SemanticChannelLayout::without_wav_mapping(
            "7.1.6",
            [
                "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "Ltf", "Rtf", "Ltm", "Rtm", "Ltr",
                "Rtr",
            ],
            Some(3),
        );
        let Err(error) =
            JocWavOutput::new_for_semantic_layout(&path, SampleFormat::F32, false, &semantic)
        else {
            panic!("the current WAV-only sink must reject future semantic layouts");
        };
        assert!(
            error
                .to_string()
                .contains("no channel identities were substituted")
        );
    }

    #[test]
    #[ignore = "manual release performance harness"]
    fn performance_harness_speaker_wav_and_caf() {
        use std::time::Instant;

        const FRAME_COUNT: usize = 128;
        const SAMPLES: usize = 1_536;
        let repetitions = std::env::var("OPENJOC_PERF_REPETITIONS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(5);
        let frames = (0..FRAME_COUNT)
            .map(|index| decoded_frame(index as u64, (index * SAMPLES) as u64, SAMPLES))
            .collect::<Vec<_>>();
        let bases = (0..FRAME_COUNT)
            .map(|index| base(SAMPLES, index as f64 * 0.001))
            .collect::<Vec<_>>();

        for layout in ["2.0", "5.1", "7.1.4", "7.1.6", "9.1", "9.1.6"] {
            let mut samples = Vec::new();
            for repetition in 0..=repetitions {
                let mut renderer = JocSpeakerRenderer::new(layout, control(false, 6)).unwrap();
                let start = Instant::now();
                for (frame, pcm) in frames.iter().zip(&bases) {
                    let block = renderer
                        .render_frame(frame.frame_index as usize, frame, pcm)
                        .unwrap();
                    std::hint::black_box(block);
                }
                renderer.finish().unwrap();
                if repetition > 0 {
                    samples.push(start.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            let median = samples[samples.len() / 2];
            println!(
                "performance layout={layout} sink=null frames={FRAME_COUNT} samples_per_frame={SAMPLES} median_seconds={:.6} realtime_factor={:.3}",
                median,
                FRAME_COUNT as f64 * SAMPLES as f64 / 48_000.0 / median
            );

            let path = std::env::temp_dir().join(format!(
                "openjoc-performance-{}-{layout}.wav",
                std::process::id()
            ));
            let mut samples = Vec::new();
            for repetition in 0..=repetitions {
                let mut renderer = JocSpeakerRenderer::new(layout, control(false, 6)).unwrap();
                let mut output =
                    JocWavOutput::new_with_overwrite(&path, SampleFormat::F32, false).unwrap();
                let start = Instant::now();
                for (frame, pcm) in frames.iter().zip(&bases) {
                    let block = renderer
                        .render_frame(frame.frame_index as usize, frame, pcm)
                        .unwrap();
                    output.write_block(&block).unwrap();
                }
                renderer.finish().unwrap();
                output.finish().unwrap();
                if repetition > 0 {
                    samples.push(start.elapsed().as_secs_f64());
                }
                fs::remove_file(&path).unwrap();
            }
            samples.sort_by(f64::total_cmp);
            let median = samples[samples.len() / 2];
            println!(
                "performance layout={layout} sink=wav frames={FRAME_COUNT} samples_per_frame={SAMPLES} median_seconds={:.6} realtime_factor={:.3}",
                median,
                FRAME_COUNT as f64 * SAMPLES as f64 / 48_000.0 / median
            );

            let path = std::env::temp_dir().join(format!(
                "openjoc-performance-{}-{layout}.caf",
                std::process::id()
            ));
            let mut samples = Vec::new();
            for repetition in 0..=repetitions {
                let mut renderer = JocSpeakerRenderer::new(layout, control(false, 6)).unwrap();
                let mut output =
                    JocPcmOutput::new_for_speaker_layout(&path, SampleFormat::F32, false, layout)
                        .unwrap();
                let start = Instant::now();
                for (frame, pcm) in frames.iter().zip(&bases) {
                    let block = renderer
                        .render_frame(frame.frame_index as usize, frame, pcm)
                        .unwrap();
                    output.write_block(&block).unwrap();
                }
                renderer.finish().unwrap();
                output.finish().unwrap();
                if repetition > 0 {
                    samples.push(start.elapsed().as_secs_f64());
                }
                fs::remove_file(&path).unwrap();
            }
            samples.sort_by(f64::total_cmp);
            let median = samples[samples.len() / 2];
            println!(
                "performance layout={layout} sink=caf frames={FRAME_COUNT} samples_per_frame={SAMPLES} median_seconds={:.6} realtime_factor={:.3}",
                median,
                FRAME_COUNT as f64 * SAMPLES as f64 / 48_000.0 / median
            );
        }
    }
}
