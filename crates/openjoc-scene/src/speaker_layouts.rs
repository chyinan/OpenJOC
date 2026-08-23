//! Canonical public speaker-layout presets.
//!
//! Presets are data instances for [`SpatialLayout`].  They intentionally do
//! not contain a renderer or a layout-specific projection law.

use crate::{
    SpatialDescriptor, SpatialLayout, SpatialLayoutAnchor, SpatialLayoutChannel,
    SpatialLayoutLayer, SpatialLayoutRow, SpatialLayoutTopology, SpatialProjectionError,
    SpatialRouteVector, SpatialSourceClass,
};
use serde::{Deserialize, Serialize};
use std::{fmt, fs, path::Path};

/// Version of the hand-editable custom speaker-layout JSON format.
pub const SPEAKER_LAYOUT_JSON_VERSION: u32 = 1;
/// Maximum number of custom output speakers admitted by the canonical layout.
/// This is a safety bound for allocations, not a DSP-specific channel limit.
pub const MAX_CUSTOM_SPEAKERS: usize = 64;
const MIN_CUSTOM_SPEAKERS: usize = 2;
const MIN_GEOMETRY_SEPARATION_RADIANS: f64 = 1.0e-6;

/// Public speaker presets exposed by the JOC speaker-rendering workflow.
pub const SPEAKER_LAYOUT_PRESET_NAMES: [&str; 13] = [
    "2.0", "5.1", "5.1.2", "5.1.4", "7.1", "7.1.2", "7.1.4", "7.1.6", "9.1", "9.1.2", "9.1.4",
    "9.1.6", "22.2",
];

/// Canonical channel order for the admitted 2.0 speaker pair.
pub const SPEAKER_LAYOUT_2_0_CHANNELS: [&str; 2] = ["FL", "FR"];

/// Canonical channel order for the 5.1 family.
pub const SPEAKER_LAYOUT_5_1_CHANNELS: [&str; 6] = ["FL", "FR", "FC", "LFE", "Ls", "Rs"];

/// Canonical channel order for 5.1.2.
pub const SPEAKER_LAYOUT_5_1_2_CHANNELS: [&str; 8] =
    ["FL", "FR", "FC", "LFE", "Ls", "Rs", "TFL", "TFR"];

/// Canonical channel order for 5.1.4.
pub const SPEAKER_LAYOUT_5_1_4_CHANNELS: [&str; 10] = [
    "FL", "FR", "FC", "LFE", "Ls", "Rs", "TFL", "TFR", "TBL", "TBR",
];

/// Canonical channel order for the 7.1 bed.
pub const SPEAKER_LAYOUT_7_1_CHANNELS: [&str; 8] =
    ["FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs"];

/// Canonical channel order for 7.1.2.
pub const SPEAKER_LAYOUT_7_1_2_CHANNELS: [&str; 10] = [
    "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "TFL", "TFR",
];

/// Canonical channel order for 7.1.4.
pub const SPEAKER_LAYOUT_7_1_4_CHANNELS: [&str; 12] = [
    "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "TFL", "TFR", "TBL", "TBR",
];

/// Canonical channel order for 7.1.6.
pub const SPEAKER_LAYOUT_7_1_6_CHANNELS: [&str; 14] = [
    "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "Ltf", "Rtf", "Ltm", "Rtm", "Ltr", "Rtr",
];

/// Canonical channel order for the 9.1 bed.
pub const SPEAKER_LAYOUT_9_1_CHANNELS: [&str; 10] =
    ["FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "Lw", "Rw"];

/// Canonical channel order for 9.1.2.
pub const SPEAKER_LAYOUT_9_1_2_CHANNELS: [&str; 12] = [
    "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "Lw", "Rw", "Ltm", "Rtm",
];

/// Canonical channel order for 9.1.4.
pub const SPEAKER_LAYOUT_9_1_4_CHANNELS: [&str; 14] = [
    "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "Lw", "Rw", "Ltf", "Rtf", "Ltr", "Rtr",
];

/// Canonical channel order for 9.1.6.
pub const SPEAKER_LAYOUT_9_1_6_CHANNELS: [&str; 16] = [
    "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "Lw", "Rw", "Ltf", "Rtf", "Ltm", "Rtm", "Ltr",
    "Rtr",
];

/// Canonical OpenJOC semantic order for ITU-R BS.2051-3 Sound System H.
///
/// The order is deliberately renderer-owned. It follows the public H layout
/// identities (including both LFEs) and is not inferred from a WAVE mask or a
/// host API's memory order.
pub const SPEAKER_LAYOUT_22_2_CHANNELS: [&str; 24] = [
    "FL", "FR", "FC", "LFE1", "BL", "BR", "FLc", "FRc", "BC", "LFE2", "SiL", "SiR", "TpFL", "TpFR",
    "TpFC", "TpC", "TpBL", "TpBR", "TpSiL", "TpSiR", "TpBC", "BtFC", "BtFL", "BtFR",
];

const QMAX: f64 = 32_767.0 / 32_768.0;
const TOP_INNER_LEFT: f64 = 0.241_943_359_375;
const TOP_INNER_RIGHT: f64 = 0.758_056_640_625;
const TOP_FRONT_Y: f64 = 0.241_943_359_375;
const TOP_REAR_Y: f64 = 0.758_056_640_625;
const WIDE_ROW_Y_Q15: u32 = 5_285;
const WIDE_LEFT_X_Q15: u32 = 0;
const WIDE_RIGHT_X_Q15: u32 = 32_767;
const WIDE_ROW_Y: f64 = WIDE_ROW_Y_Q15 as f64 / 32_768.0;
const WIDE_LEFT_X: f64 = WIDE_LEFT_X_Q15 as f64 / 32_768.0;
const WIDE_RIGHT_X: f64 = WIDE_RIGHT_X_Q15 as f64 / 32_768.0;

/// Failure while deriving public WAVEFORMATEXTENSIBLE speaker-mask metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpeakerChannelMaskError {
    UnknownIdentity(String),
    NonAscendingOrder,
}

impl fmt::Display for SpeakerChannelMaskError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownIdentity(identity) => write!(
                formatter,
                "speaker identity {identity} has no standard WAV channel-mask bit"
            ),
            Self::NonAscendingOrder => formatter
                .write_str("speaker identities must be ordered by ascending WAV channel-mask bit"),
        }
    }
}

impl std::error::Error for SpeakerChannelMaskError {}

/// Failure while constructing a canonical public speaker preset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpeakerLayoutPresetError {
    UnsupportedLayout(String),
    Projection(SpatialProjectionError),
    ChannelMask(SpeakerChannelMaskError),
}

impl fmt::Display for SpeakerLayoutPresetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedLayout(layout) => {
                write!(formatter, "unsupported speaker layout {layout}")
            }
            Self::Projection(error) => error.fmt(formatter),
            Self::ChannelMask(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for SpeakerLayoutPresetError {}

impl From<SpatialProjectionError> for SpeakerLayoutPresetError {
    fn from(value: SpatialProjectionError) -> Self {
        Self::Projection(value)
    }
}

impl From<SpeakerChannelMaskError> for SpeakerLayoutPresetError {
    fn from(value: SpeakerChannelMaskError) -> Self {
        Self::ChannelMask(value)
    }
}

/// Container-independent semantic identity and order for rendered channels.
///
/// The labels are owned by the canonical scene/layout registry. Output
/// containers consume this record and decide independently whether, and how,
/// each identity can be serialized.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticChannelLayout {
    pub name: String,
    pub labels: Vec<String>,
    pub lfe_index: Option<usize>,
    wav_channel_mask: Option<u32>,
}

impl SemanticChannelLayout {
    /// Creates an internal or caller-defined semantic layout without WAV
    /// representation metadata.
    #[must_use]
    pub fn without_wav_mapping(
        name: impl Into<String>,
        labels: impl IntoIterator<Item = impl Into<String>>,
        lfe_index: Option<usize>,
    ) -> Self {
        Self {
            name: name.into(),
            labels: labels.into_iter().map(Into::into).collect(),
            lfe_index,
            wav_channel_mask: None,
        }
    }

    fn with_wav_mapping(
        name: impl Into<String>,
        labels: impl IntoIterator<Item = impl Into<String>>,
        lfe_index: Option<usize>,
        wav_channel_mask: u32,
    ) -> Self {
        Self {
            name: name.into(),
            labels: labels.into_iter().map(Into::into).collect(),
            lfe_index,
            wav_channel_mask: Some(wav_channel_mask),
        }
    }

    /// Returns the exact WAVEFORMATEXTENSIBLE mask, if one exists.
    #[must_use]
    pub const fn wav_channel_mask(&self) -> Option<u32> {
        self.wav_channel_mask
    }

    /// Returns the number of rendered channels, including LFE.
    #[must_use]
    pub fn channel_count(&self) -> usize {
        self.labels.len()
    }

    /// Returns the number of LFE identities represented by this semantic
    /// layout. This remains independent of any container channel order.
    #[must_use]
    pub fn lfe_count(&self) -> usize {
        self.labels
            .iter()
            .filter(|label| matches!(label.as_str(), "LFE" | "LFE1" | "LFE2" | "low-frequency"))
            .count()
    }
}

/// Logical role of a custom speaker. LFE is kept outside the spatial panner.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeakerRole {
    #[default]
    FullRange,
    Lfe,
}

/// Public spherical geometry for one ordered output speaker.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SpeakerGeometry {
    pub name: String,
    pub azimuth: f64,
    pub elevation: f64,
    #[serde(default)]
    pub role: SpeakerRole,
}

impl SpeakerGeometry {
    /// Creates a full-range speaker using the canonical spherical convention.
    #[must_use]
    pub fn full_range(name: impl Into<String>, azimuth: f64, elevation: f64) -> Self {
        Self {
            name: name.into(),
            azimuth,
            elevation,
            role: SpeakerRole::FullRange,
        }
    }

    /// Creates a logical LFE output. Its coordinates are retained for metadata
    /// but it is never fed to the full-range spatial projector.
    #[must_use]
    pub fn lfe(name: impl Into<String>, azimuth: f64, elevation: f64) -> Self {
        Self {
            name: name.into(),
            azimuth,
            elevation,
            role: SpeakerRole::Lfe,
        }
    }
}

/// Failure while parsing or validating a custom speaker layout.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpeakerLayoutError {
    Json(String),
    UnsupportedLayout(String),
    UnsupportedVersion {
        expected: u32,
        actual: u32,
    },
    InvalidName,
    EmptyLayout,
    TooManySpeakers {
        maximum: usize,
        actual: usize,
    },
    TooFewFullRangeSpeakers,
    InvalidSpeakerName(String),
    DuplicateSpeakerName(String),
    NonFiniteCoordinate {
        speaker: String,
        field: &'static str,
    },
    CoordinateOutOfRange {
        speaker: String,
        field: &'static str,
        value: String,
    },
    DuplicateGeometry {
        first: String,
        second: String,
    },
    NearDegenerateGeometry {
        first: String,
        second: String,
    },
    Projection(SpatialProjectionError),
    Io(String),
}

impl fmt::Display for SpeakerLayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "invalid speaker layout JSON: {error}"),
            Self::UnsupportedLayout(layout) => {
                write!(formatter, "unsupported speaker layout {layout}")
            }
            Self::UnsupportedVersion { expected, actual } => write!(
                formatter,
                "unsupported speaker layout JSON version {actual}; expected {expected}"
            ),
            Self::InvalidName => formatter.write_str("speaker layout name must not be empty"),
            Self::EmptyLayout => formatter.write_str("speaker layout must contain speakers"),
            Self::TooManySpeakers { maximum, actual } => write!(
                formatter,
                "speaker layout contains {actual} speakers; maximum supported is {maximum}"
            ),
            Self::TooFewFullRangeSpeakers => {
                formatter.write_str("speaker layout needs at least two full-range speakers")
            }
            Self::InvalidSpeakerName(name) => {
                write!(formatter, "speaker name {name:?} must not be empty")
            }
            Self::DuplicateSpeakerName(name) => {
                write!(formatter, "duplicate speaker name {name:?}")
            }
            Self::NonFiniteCoordinate { speaker, field } => {
                write!(formatter, "speaker {speaker:?} has non-finite {field}")
            }
            Self::CoordinateOutOfRange {
                speaker,
                field,
                value,
            } => write!(
                formatter,
                "speaker {speaker:?} has {field}={value}, outside the supported range"
            ),
            Self::DuplicateGeometry { first, second } => write!(
                formatter,
                "speakers {first:?} and {second:?} have duplicate geometry"
            ),
            Self::NearDegenerateGeometry { first, second } => write!(
                formatter,
                "speakers {first:?} and {second:?} are too close for a stable panner basis"
            ),
            Self::Projection(error) => error.fmt(formatter),
            Self::Io(error) => write!(formatter, "could not read speaker layout file: {error}"),
        }
    }
}

impl std::error::Error for SpeakerLayoutError {}

impl From<SpatialProjectionError> for SpeakerLayoutError {
    fn from(value: SpatialProjectionError) -> Self {
        Self::Projection(value)
    }
}

/// Canonical renderer layout shared by built-in presets, JSON, Rust callers,
/// and the C ABI. `spatial` is the only representation consumed by DSP.
#[derive(Clone, Debug, PartialEq)]
pub struct SpeakerLayout {
    name: String,
    labels: Vec<String>,
    lfe_indices: Vec<usize>,
    spatial: SpatialLayout,
    semantic: SemanticChannelLayout,
    channel_coordinates: Vec<[f32; 3]>,
    unmasked_wav_allowed: bool,
}

impl SpeakerLayout {
    /// Converts an existing preset into the canonical dynamic layout object.
    pub fn preset(name: &str) -> Result<Self, SpeakerLayoutError> {
        let preset = SpeakerLayoutPreset::for_name(name).map_err(|error| match error {
            SpeakerLayoutPresetError::UnsupportedLayout(layout) => {
                SpeakerLayoutError::UnsupportedLayout(layout)
            }
            other => SpeakerLayoutError::Json(other.to_string()),
        })?;
        Ok(Self::from_preset(preset))
    }

    /// Wraps an already validated preset without changing its topology.
    #[must_use]
    pub fn from_preset(preset: SpeakerLayoutPreset) -> Self {
        let labels = preset
            .labels
            .iter()
            .map(|label| (*label).to_owned())
            .collect();
        let lfe_indices = preset.lfe_indices();
        let semantic = preset.semantic_channel_layout();
        Self {
            name: preset.name.to_owned(),
            labels,
            lfe_indices,
            spatial: preset.layout,
            semantic,
            channel_coordinates: Vec::new(),
            unmasked_wav_allowed: false,
        }
    }

    /// Creates and validates a custom layout in canonical channel order.
    #[allow(clippy::needless_pass_by_value)]
    pub fn custom(
        name: impl Into<String>,
        speakers: Vec<SpeakerGeometry>,
    ) -> Result<Self, SpeakerLayoutError> {
        let name = name.into();
        validate_custom_name(&name)?;
        if speakers.is_empty() {
            return Err(SpeakerLayoutError::EmptyLayout);
        }
        if speakers.len() > MAX_CUSTOM_SPEAKERS {
            return Err(SpeakerLayoutError::TooManySpeakers {
                maximum: MAX_CUSTOM_SPEAKERS,
                actual: speakers.len(),
            });
        }
        let mut names = std::collections::HashSet::with_capacity(speakers.len());
        let mut full_range = Vec::new();
        let mut positions = Vec::with_capacity(speakers.len());
        for speaker in &speakers {
            if speaker.name.trim().is_empty() {
                return Err(SpeakerLayoutError::InvalidSpeakerName(speaker.name.clone()));
            }
            if speaker.name.len() > 128 || speaker.name.chars().any(char::is_control) {
                return Err(SpeakerLayoutError::InvalidSpeakerName(speaker.name.clone()));
            }
            if !names.insert(speaker.name.as_str()) {
                return Err(SpeakerLayoutError::DuplicateSpeakerName(
                    speaker.name.clone(),
                ));
            }
            validate_spherical_coordinate(speaker)?;
            let position = spherical_position(speaker.azimuth, speaker.elevation);
            if matches!(speaker.role, SpeakerRole::FullRange) {
                full_range.push((
                    speaker.name.as_str(),
                    spherical_direction(speaker.azimuth, speaker.elevation),
                ));
            }
            positions.push(position);
        }
        if full_range.len() < MIN_CUSTOM_SPEAKERS {
            return Err(SpeakerLayoutError::TooFewFullRangeSpeakers);
        }
        for (index, (first, first_position)) in full_range.iter().enumerate() {
            for (second, second_position) in full_range.iter().skip(index + 1) {
                let dot = first_position
                    .iter()
                    .zip(second_position)
                    .map(|(a, b)| a * b)
                    .sum::<f64>()
                    .clamp(-1.0, 1.0);
                let angle = dot.acos();
                if angle == 0.0 {
                    return Err(SpeakerLayoutError::DuplicateGeometry {
                        first: (*first).to_owned(),
                        second: (*second).to_owned(),
                    });
                }
                if angle < MIN_GEOMETRY_SEPARATION_RADIANS {
                    return Err(SpeakerLayoutError::NearDegenerateGeometry {
                        first: (*first).to_owned(),
                        second: (*second).to_owned(),
                    });
                }
            }
        }

        let channels = speakers
            .iter()
            .map(|speaker| SpatialLayoutChannel {
                identity: speaker.name.clone(),
                enabled: true,
                lfe: speaker.role == SpeakerRole::Lfe,
            })
            .collect::<Vec<_>>();
        let topology = topology_from_positions(&speakers, &positions);
        let spatial = SpatialLayout::from_topology(channels, topology, Vec::new())?;
        let explicit_routes = custom_base_routes(&spatial)?;
        let spatial = spatial.with_route_vectors(explicit_routes)?;
        let labels = speakers
            .iter()
            .map(|speaker| speaker.name.clone())
            .collect::<Vec<_>>();
        let lfe_indices = speakers
            .iter()
            .enumerate()
            .filter_map(|(index, speaker)| (speaker.role == SpeakerRole::Lfe).then_some(index))
            .collect::<Vec<_>>();
        let coordinates = positions
            .iter()
            .map(|position| [position[0] as f32, position[1] as f32, position[2] as f32])
            .collect::<Vec<_>>();
        let semantic = SemanticChannelLayout::without_wav_mapping(
            name.clone(),
            labels.clone(),
            lfe_indices.first().copied(),
        );
        Ok(Self {
            name,
            labels,
            lfe_indices,
            spatial,
            semantic,
            channel_coordinates: coordinates,
            unmasked_wav_allowed: true,
        })
    }

    /// Parses the versioned custom JSON format.
    pub fn from_json_str(json: &str) -> Result<Self, SpeakerLayoutError> {
        let document: SpeakerLayoutDocument = serde_json::from_str(json)
            .map_err(|error| SpeakerLayoutError::Json(error.to_string()))?;
        if document.version != SPEAKER_LAYOUT_JSON_VERSION {
            return Err(SpeakerLayoutError::UnsupportedVersion {
                expected: SPEAKER_LAYOUT_JSON_VERSION,
                actual: document.version,
            });
        }
        Self::custom(document.name, document.speakers)
    }

    /// Reads and parses a custom layout file without retaining the path.
    pub fn from_json_file(path: impl AsRef<Path>) -> Result<Self, SpeakerLayoutError> {
        let json =
            fs::read_to_string(path).map_err(|error| SpeakerLayoutError::Io(error.to_string()))?;
        Self::from_json_str(&json)
    }

    /// Returns the stable display/name identity.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns output labels in exactly the interleaved PCM order.
    #[must_use]
    pub fn channel_labels(&self) -> &[String] {
        &self.labels
    }

    /// Returns the canonical spatial projector input.
    #[must_use]
    pub fn spatial(&self) -> &SpatialLayout {
        &self.spatial
    }

    /// Returns the same canonical layout with caller-supplied fixed/named
    /// routes installed in the shared spatial projector.
    pub fn with_route_vectors(
        &self,
        route_vectors: Vec<crate::SpatialRouteVector>,
    ) -> Result<Self, SpeakerLayoutError> {
        let mut layout = self.clone();
        layout.spatial = self.spatial.with_route_vectors(route_vectors)?;
        Ok(layout)
    }

    /// Returns the container-independent output semantic record.
    #[must_use]
    pub fn semantic_channel_layout(&self) -> SemanticChannelLayout {
        self.semantic.clone()
    }

    /// Returns normalized Cartesian channel coordinates for custom layouts.
    /// Built-in presets return an empty slice because their established CAF
    /// label mappings remain the authoritative metadata.
    #[must_use]
    pub fn channel_coordinates(&self) -> &[[f32; 3]] {
        &self.channel_coordinates
    }

    /// Returns whether an unmasked WAV is a truthful representation.
    #[must_use]
    pub const fn unmasked_wav_allowed(&self) -> bool {
        self.unmasked_wav_allowed
    }

    /// Returns every logical LFE output index.
    #[must_use]
    pub fn lfe_indices(&self) -> &[usize] {
        &self.lfe_indices
    }

    /// Returns the first logical LFE output index, if present.
    #[must_use]
    pub fn lfe_index(&self) -> Option<usize> {
        self.lfe_indices.first().copied()
    }

    /// Returns the number of output channels, including LFE.
    #[must_use]
    pub fn channel_count(&self) -> usize {
        self.labels.len()
    }

    /// Returns whether this layout is the ordinary physical stereo preset.
    #[must_use]
    pub fn is_stereo(&self) -> bool {
        self.name == "2.0"
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpeakerLayoutDocument {
    version: u32,
    name: String,
    speakers: Vec<SpeakerGeometry>,
}

fn validate_custom_name(name: &str) -> Result<(), SpeakerLayoutError> {
    if name.trim().is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
        Err(SpeakerLayoutError::InvalidName)
    } else {
        Ok(())
    }
}

fn validate_spherical_coordinate(speaker: &SpeakerGeometry) -> Result<(), SpeakerLayoutError> {
    for (field, value) in [
        ("azimuth", speaker.azimuth),
        ("elevation", speaker.elevation),
    ] {
        if !value.is_finite() {
            return Err(SpeakerLayoutError::NonFiniteCoordinate {
                speaker: speaker.name.clone(),
                field,
            });
        }
    }
    if !(-180.0..=180.0).contains(&speaker.azimuth) {
        return Err(SpeakerLayoutError::CoordinateOutOfRange {
            speaker: speaker.name.clone(),
            field: "azimuth",
            value: speaker.azimuth.to_string(),
        });
    }
    if !(-90.0..=90.0).contains(&speaker.elevation) {
        return Err(SpeakerLayoutError::CoordinateOutOfRange {
            speaker: speaker.name.clone(),
            field: "elevation",
            value: speaker.elevation.to_string(),
        });
    }
    Ok(())
}

fn spherical_position(azimuth_degrees: f64, elevation_degrees: f64) -> [f64; 3] {
    let azimuth = azimuth_degrees.to_radians();
    let elevation = elevation_degrees.to_radians();
    let horizontal = elevation.cos();
    [
        0.5 - 0.5 * azimuth.sin() * horizontal,
        0.5 * (1.0 - azimuth.cos() * horizontal),
        elevation.sin() * QMAX,
    ]
}

fn spherical_direction(azimuth_degrees: f64, elevation_degrees: f64) -> [f64; 3] {
    let azimuth = azimuth_degrees.to_radians();
    let elevation = elevation_degrees.to_radians();
    let horizontal = elevation.cos();
    [
        azimuth.sin() * horizontal,
        azimuth.cos() * horizontal,
        elevation.sin(),
    ]
}

fn custom_base_routes(
    spatial: &SpatialLayout,
) -> Result<Vec<SpatialRouteVector>, SpeakerLayoutError> {
    let routes = [
        ("FL", [0.0, 0.0, 0.0]),
        ("FR", [QMAX, 0.0, 0.0]),
        ("FC", [0.5, 0.0, 0.0]),
        ("Ls", [0.0, 0.5, 0.0]),
        ("Rs", [QMAX, 0.5, 0.0]),
        ("Lb", [0.0, QMAX, 0.0]),
        ("Rb", [QMAX, QMAX, 0.0]),
        ("TFL", [TOP_INNER_LEFT, 0.5, QMAX]),
        ("TFR", [TOP_INNER_RIGHT, 0.5, QMAX]),
    ];
    routes
        .into_iter()
        .map(|(identity, coordinates)| {
            let descriptor = SpatialDescriptor::new(
                SpatialSourceClass::DynamicPoint,
                "custom-base-route",
                coordinates.into_iter().collect(),
            );
            let vector = spatial.project(&descriptor)?;
            Ok(SpatialRouteVector {
                identity: format!("explicit/{identity}"),
                vector,
            })
        })
        .collect()
}

fn topology_from_positions(
    speakers: &[SpeakerGeometry],
    positions: &[[f64; 3]],
) -> SpatialLayoutTopology {
    let mut layer_indices = (0..speakers.len())
        .filter(|index| speakers[*index].role == SpeakerRole::FullRange)
        .collect::<Vec<_>>();
    layer_indices.sort_by(|left, right| positions[*left][2].total_cmp(&positions[*right][2]));
    let mut layers = Vec::new();
    for index in layer_indices {
        let z = positions[index][2];
        if layers
            .last()
            .is_none_or(|layer: &SpatialLayoutLayer| layer.z != z)
        {
            layers.push(SpatialLayoutLayer {
                z,
                rows: Vec::new(),
            });
        }
        let layer = layers.last_mut().expect("layer was just created");
        let mut row_index = layer
            .rows
            .iter()
            .position(|row| row.y == positions[index][1]);
        if row_index.is_none() {
            layer.rows.push(SpatialLayoutRow {
                y: positions[index][1],
                anchors: Vec::new(),
            });
            layer.rows.sort_by(|left, right| left.y.total_cmp(&right.y));
            row_index = layer
                .rows
                .iter()
                .position(|row| row.y == positions[index][1]);
        }
        layer.rows[row_index.expect("row was just created")]
            .anchors
            .push(crate::SpatialLayoutAnchor {
                identity: speakers[index].name.clone(),
                x: positions[index][0],
                y: positions[index][1],
                z,
            });
    }
    for layer in &mut layers {
        for row in &mut layer.rows {
            row.anchors
                .sort_by(|left, right| left.x.total_cmp(&right.x));
        }
    }
    SpatialLayoutTopology {
        layers,
        aliases: Vec::new(),
    }
}

/// A validated public speaker preset and its output metadata contract.
#[derive(Clone, Debug, PartialEq)]
pub struct SpeakerLayoutPreset {
    /// Stable public preset name.
    pub name: &'static str,
    /// Ordered output labels, including the independently owned LFE channel.
    pub labels: Vec<&'static str>,
    /// LFE index in [`Self::labels`], when present.
    pub lfe_index: Option<usize>,
    /// Data-only topology consumed by the generic point projector.
    pub layout: SpatialLayout,
    wav_channel_mask: Option<u32>,
}

impl SpeakerLayoutPreset {
    /// Returns one of the thirteen public speaker presets.
    pub fn for_name(name: &str) -> Result<Self, SpeakerLayoutPresetError> {
        match name {
            "2.0" => speaker_layout_2_0_preset(),
            "5.1" => speaker_layout_5_1_preset(),
            "5.1.2" => speaker_layout_5_1_2_preset(),
            "5.1.4" => speaker_layout_5_1_4_preset(),
            "7.1" => speaker_layout_7_1_preset(),
            "7.1.2" => speaker_layout_7_1_2_preset(),
            "7.1.4" => speaker_layout_7_1_4_preset(),
            "7.1.6" => speaker_layout_7_1_6_preset(),
            "9.1" => speaker_layout_9_1_preset(),
            "9.1.2" => speaker_layout_9_1_2_preset(),
            "9.1.4" => speaker_layout_9_1_4_preset(),
            "9.1.6" => speaker_layout_9_1_6_preset(),
            "22.2" => speaker_layout_22_2_preset(),
            other => Err(SpeakerLayoutPresetError::UnsupportedLayout(
                other.to_owned(),
            )),
        }
    }

    /// Returns the ordered public channel labels.
    #[must_use]
    pub fn channel_labels(&self) -> Vec<&'static str> {
        self.labels.clone()
    }

    /// Returns the total output channel count, including LFE.
    #[must_use]
    pub fn channel_count(&self) -> usize {
        self.labels.len()
    }

    /// Returns the LFE output index.
    #[must_use]
    pub const fn lfe_index(&self) -> Option<usize> {
        self.lfe_index
    }

    /// Returns every LFE index in canonical output order.
    #[must_use]
    pub fn lfe_indices(&self) -> Vec<usize> {
        self.layout
            .channels()
            .iter()
            .enumerate()
            .filter_map(|(index, channel)| channel.lfe.then_some(index))
            .collect()
    }

    /// Returns the number of semantic LFE output channels.
    #[must_use]
    pub fn lfe_count(&self) -> usize {
        self.lfe_indices().len()
    }

    /// Returns the standard WAVEFORMATEXTENSIBLE speaker mask, when the
    /// semantic layout has an exact standard speaker representation.
    #[must_use]
    pub const fn wav_channel_mask(&self) -> Option<u32> {
        self.wav_channel_mask
    }

    /// Returns this preset as a container-independent semantic layout.
    #[must_use]
    pub fn semantic_channel_layout(&self) -> SemanticChannelLayout {
        match self.wav_channel_mask {
            Some(mask) => SemanticChannelLayout::with_wav_mapping(
                self.name,
                self.labels.iter().copied(),
                self.lfe_index,
                mask,
            ),
            None => SemanticChannelLayout::without_wav_mapping(
                self.name,
                self.labels.iter().copied(),
                self.lfe_index,
            ),
        }
    }
}

/// Returns the standard WAVEFORMATEXTENSIBLE mask for an ordered label list.
pub fn speaker_channel_mask_for_labels(labels: &[&str]) -> Result<u32, SpeakerChannelMaskError> {
    let mut bits = Vec::with_capacity(labels.len());
    for label in labels {
        let bit = match *label {
            "FL" | "front-left" => 0x0000_0001,
            "FR" | "front-right" => 0x0000_0002,
            "FC" | "front-center" => 0x0000_0004,
            "LFE" | "low-frequency" => 0x0000_0008,
            "Lb" | "BL" | "back-left" => 0x0000_0010,
            "Rb" | "BR" | "back-right" => 0x0000_0020,
            "Ls" | "SL" | "side-left" => 0x0000_0200,
            "Rs" | "SR" | "side-right" => 0x0000_0400,
            "TFL" | "top-front-left" => 0x0000_1000,
            "TFR" | "top-front-right" => 0x0000_4000,
            "TBL" | "top-back-left" => 0x0000_8000,
            "TBR" | "top-back-right" => 0x0002_0000,
            other => return Err(SpeakerChannelMaskError::UnknownIdentity(other.to_owned())),
        };
        bits.push(bit);
    }
    if bits.windows(2).any(|window| window[0] >= window[1]) {
        return Err(SpeakerChannelMaskError::NonAscendingOrder);
    }
    Ok(bits.into_iter().fold(0, |mask, bit| mask | bit))
}

/// Returns the 2.0 [`SpatialLayout`] topology.
pub fn speaker_layout_2_0() -> Result<SpatialLayout, SpatialProjectionError> {
    Ok(speaker_layout_2_0_preset()
        .map_err(|error| match error {
            SpeakerLayoutPresetError::Projection(error) => error,
            other => unreachable!("validated 2.0 preset failed outside projection: {other}"),
        })?
        .layout)
}

/// Returns the canonical ITU-R BS.2051-3 Sound System H (22.2) topology.
pub fn speaker_layout_22_2() -> Result<SpatialLayout, SpatialProjectionError> {
    Ok(speaker_layout_22_2_preset()
        .map_err(|error| match error {
            SpeakerLayoutPresetError::Projection(error) => error,
            other => unreachable!("validated 22.2 preset failed outside projection: {other}"),
        })?
        .layout)
}

fn speaker_layout_2_0_preset() -> Result<SpeakerLayoutPreset, SpeakerLayoutPresetError> {
    generic_preset(
        "2.0",
        &SPEAKER_LAYOUT_2_0_CHANNELS,
        None,
        vec![bed_layer(vec![row(
            0.0,
            vec![anchor("FL", 0.0, 0.0, 0.0), anchor("FR", QMAX, 0.0, 0.0)],
        )])],
    )
}

fn speaker_layout_22_2_preset() -> Result<SpeakerLayoutPreset, SpeakerLayoutPresetError> {
    // ITU-R BS.2051-3 Table 10 specifies ranges for most H positions. The
    // renderer needs one deterministic point per semantic speaker, so the
    // midpoint of each public range is used. Coordinates are a normalized
    // spherical projection: x is left/right, y is front/rear, and z is the
    // signed elevation. LFE channels remain semantic outputs, not vertices.
    let middle = bed_layer(vec![
        spherical_row(0.0, [("FC", 0.0, 0.0)]),
        spherical_row(26.25, [("FLc", 26.25, 0.0), ("FRc", -26.25, 0.0)]),
        spherical_row(52.5, [("FL", 52.5, 0.0), ("FR", -52.5, 0.0)]),
        spherical_row(90.0, [("SiL", 90.0, 0.0), ("SiR", -90.0, 0.0)]),
        spherical_row(122.5, [("BL", 122.5, 0.0), ("BR", -122.5, 0.0)]),
        spherical_row(180.0, [("BC", 180.0, 0.0)]),
    ]);
    let bottom = SpatialLayoutLayer {
        z: spherical_z(-22.5),
        rows: vec![
            spherical_row(-0.0, [("BtFC", 0.0, -22.5)]),
            spherical_row(52.5, [("BtFL", 52.5, -22.5), ("BtFR", -52.5, -22.5)]),
        ],
    };
    let upper = SpatialLayoutLayer {
        z: spherical_z(37.5),
        rows: vec![
            spherical_row(0.0, [("TpFC", 0.0, 37.5)]),
            spherical_row(52.5, [("TpFL", 52.5, 37.5), ("TpFR", -52.5, 37.5)]),
            spherical_row(90.0, [("TpSiL", 90.0, 37.5), ("TpSiR", -90.0, 37.5)]),
            spherical_row(122.5, [("TpBL", 122.5, 37.5), ("TpBR", -122.5, 37.5)]),
            spherical_row(180.0, [("TpBC", 180.0, 37.5)]),
        ],
    };
    let top = SpatialLayoutLayer {
        z: QMAX,
        rows: vec![row(0.5, vec![anchor("TpC", 0.5, 0.5, QMAX)])],
    };
    generic_preset(
        "22.2",
        &SPEAKER_LAYOUT_22_2_CHANNELS,
        Some(3),
        vec![bottom, middle, upper, top],
    )
}

/// Returns the 5.1.4 [`SpatialLayout`] topology.
pub fn speaker_layout_5_1_4() -> Result<SpatialLayout, SpatialProjectionError> {
    Ok(speaker_layout_5_1_4_preset()
        .map_err(|error| match error {
            SpeakerLayoutPresetError::Projection(error) => error,
            other => unreachable!("validated 5.1.4 preset failed outside projection: {other}"),
        })?
        .layout)
}

/// Returns the 7.1.2 [`SpatialLayout`] topology.
pub fn speaker_layout_7_1_2() -> Result<SpatialLayout, SpatialProjectionError> {
    Ok(speaker_layout_7_1_2_preset()
        .map_err(|error| match error {
            SpeakerLayoutPresetError::Projection(error) => error,
            other => unreachable!("validated 7.1.2 preset failed outside projection: {other}"),
        })?
        .layout)
}

/// Returns the 7.1.6 [`SpatialLayout`] topology.
pub fn speaker_layout_7_1_6() -> Result<SpatialLayout, SpatialProjectionError> {
    Ok(speaker_layout_7_1_6_preset()
        .map_err(|error| match error {
            SpeakerLayoutPresetError::Projection(error) => error,
            other => unreachable!("validated 7.1.6 preset failed outside projection: {other}"),
        })?
        .layout)
}

/// Returns the canonical 9.1 [`SpatialLayout`] topology.
pub fn speaker_layout_9_1() -> Result<SpatialLayout, SpatialProjectionError> {
    Ok(speaker_layout_9_1_preset()
        .map_err(|error| match error {
            SpeakerLayoutPresetError::Projection(error) => error,
            other => unreachable!("validated 9.1 preset failed outside projection: {other}"),
        })?
        .layout)
}

/// Returns the canonical 9.1.2 [`SpatialLayout`] topology.
pub fn speaker_layout_9_1_2() -> Result<SpatialLayout, SpatialProjectionError> {
    Ok(speaker_layout_9_1_2_preset()
        .map_err(|error| match error {
            SpeakerLayoutPresetError::Projection(error) => error,
            other => unreachable!("validated 9.1.2 preset failed outside projection: {other}"),
        })?
        .layout)
}

/// Returns the canonical 9.1.4 [`SpatialLayout`] topology.
pub fn speaker_layout_9_1_4() -> Result<SpatialLayout, SpatialProjectionError> {
    Ok(speaker_layout_9_1_4_preset()
        .map_err(|error| match error {
            SpeakerLayoutPresetError::Projection(error) => error,
            other => unreachable!("validated 9.1.4 preset failed outside projection: {other}"),
        })?
        .layout)
}

/// Returns the canonical 9.1.6 [`SpatialLayout`] topology.
pub fn speaker_layout_9_1_6() -> Result<SpatialLayout, SpatialProjectionError> {
    Ok(speaker_layout_9_1_6_preset()
        .map_err(|error| match error {
            SpeakerLayoutPresetError::Projection(error) => error,
            other => unreachable!("validated 9.1.6 preset failed outside projection: {other}"),
        })?
        .layout)
}

/// Returns a canonical full preset for name-based integrations.
pub fn speaker_layout_preset(name: &str) -> Result<SpeakerLayoutPreset, SpeakerLayoutPresetError> {
    SpeakerLayoutPreset::for_name(name)
}

fn speaker_layout_5_1_preset() -> Result<SpeakerLayoutPreset, SpeakerLayoutPresetError> {
    generic_preset(
        "5.1",
        &SPEAKER_LAYOUT_5_1_CHANNELS,
        Some(3),
        vec![bed_layer(vec![
            row(
                0.0,
                vec![
                    anchor("FL", 0.0, 0.0, 0.0),
                    anchor("FC", 0.5, 0.0, 0.0),
                    anchor("FR", QMAX, 0.0, 0.0),
                ],
            ),
            row(
                0.5,
                vec![anchor("Ls", 0.0, 0.5, 0.0), anchor("Rs", QMAX, 0.5, 0.0)],
            ),
        ])],
    )
}

fn speaker_layout_5_1_2_preset() -> Result<SpeakerLayoutPreset, SpeakerLayoutPresetError> {
    generic_preset(
        "5.1.2",
        &SPEAKER_LAYOUT_5_1_2_CHANNELS,
        Some(3),
        vec![
            five_one_bed(),
            upper_layer(vec![row(
                0.5,
                vec![
                    anchor("TFL", TOP_INNER_LEFT, 0.5, QMAX),
                    anchor("TFR", TOP_INNER_RIGHT, 0.5, QMAX),
                ],
            )]),
        ],
    )
}

fn speaker_layout_5_1_4_preset() -> Result<SpeakerLayoutPreset, SpeakerLayoutPresetError> {
    generic_preset(
        "5.1.4",
        &SPEAKER_LAYOUT_5_1_4_CHANNELS,
        Some(3),
        vec![
            five_one_bed(),
            upper_layer(vec![
                row(
                    TOP_FRONT_Y,
                    vec![
                        anchor("TFL", TOP_INNER_LEFT, TOP_FRONT_Y, QMAX),
                        anchor("TFR", TOP_INNER_RIGHT, TOP_FRONT_Y, QMAX),
                    ],
                ),
                row(
                    TOP_REAR_Y,
                    vec![
                        anchor("TBL", TOP_INNER_LEFT, TOP_REAR_Y, QMAX),
                        anchor("TBR", TOP_INNER_RIGHT, TOP_REAR_Y, QMAX),
                    ],
                ),
            ]),
        ],
    )
}

fn speaker_layout_7_1_preset() -> Result<SpeakerLayoutPreset, SpeakerLayoutPresetError> {
    generic_preset(
        "7.1",
        &SPEAKER_LAYOUT_7_1_CHANNELS,
        Some(3),
        vec![seven_one_bed()],
    )
}

fn speaker_layout_7_1_2_preset() -> Result<SpeakerLayoutPreset, SpeakerLayoutPresetError> {
    generic_preset(
        "7.1.2",
        &SPEAKER_LAYOUT_7_1_2_CHANNELS,
        Some(3),
        vec![
            seven_one_bed(),
            upper_layer(vec![row(
                0.5,
                vec![
                    anchor("TFL", TOP_INNER_LEFT, 0.5, QMAX),
                    anchor("TFR", TOP_INNER_RIGHT, 0.5, QMAX),
                ],
            )]),
        ],
    )
}

fn speaker_layout_7_1_4_preset() -> Result<SpeakerLayoutPreset, SpeakerLayoutPresetError> {
    generic_preset(
        "7.1.4",
        &SPEAKER_LAYOUT_7_1_4_CHANNELS,
        Some(3),
        vec![
            seven_one_bed(),
            upper_layer(vec![
                row(
                    TOP_FRONT_Y,
                    vec![
                        anchor("TFL", TOP_INNER_LEFT, TOP_FRONT_Y, QMAX),
                        anchor("TFR", TOP_INNER_RIGHT, TOP_FRONT_Y, QMAX),
                    ],
                ),
                row(
                    TOP_REAR_Y,
                    vec![
                        anchor("TBL", TOP_INNER_LEFT, TOP_REAR_Y, QMAX),
                        anchor("TBR", TOP_INNER_RIGHT, TOP_REAR_Y, QMAX),
                    ],
                ),
            ]),
        ],
    )
}

fn speaker_layout_7_1_6_preset() -> Result<SpeakerLayoutPreset, SpeakerLayoutPresetError> {
    generic_preset(
        "7.1.6",
        &SPEAKER_LAYOUT_7_1_6_CHANNELS,
        Some(3),
        vec![
            seven_one_bed(),
            upper_layer(vec![
                row(
                    TOP_FRONT_Y,
                    vec![
                        anchor("Ltf", TOP_INNER_LEFT, TOP_FRONT_Y, QMAX),
                        anchor("Rtf", TOP_INNER_RIGHT, TOP_FRONT_Y, QMAX),
                    ],
                ),
                row(
                    0.5,
                    vec![
                        anchor("Ltm", TOP_INNER_LEFT, 0.5, QMAX),
                        anchor("Rtm", TOP_INNER_RIGHT, 0.5, QMAX),
                    ],
                ),
                row(
                    TOP_REAR_Y,
                    vec![
                        anchor("Ltr", TOP_INNER_LEFT, TOP_REAR_Y, QMAX),
                        anchor("Rtr", TOP_INNER_RIGHT, TOP_REAR_Y, QMAX),
                    ],
                ),
            ]),
        ],
    )
}

fn speaker_layout_9_1_preset() -> Result<SpeakerLayoutPreset, SpeakerLayoutPresetError> {
    generic_preset(
        "9.1",
        &SPEAKER_LAYOUT_9_1_CHANNELS,
        Some(3),
        vec![wide_bed()],
    )
}

fn speaker_layout_9_1_2_preset() -> Result<SpeakerLayoutPreset, SpeakerLayoutPresetError> {
    generic_preset(
        "9.1.2",
        &SPEAKER_LAYOUT_9_1_2_CHANNELS,
        Some(3),
        vec![
            wide_bed(),
            upper_layer(vec![row(
                0.5,
                vec![
                    anchor("Ltm", TOP_INNER_LEFT, 0.5, QMAX),
                    anchor("Rtm", TOP_INNER_RIGHT, 0.5, QMAX),
                ],
            )]),
        ],
    )
}

fn speaker_layout_9_1_4_preset() -> Result<SpeakerLayoutPreset, SpeakerLayoutPresetError> {
    generic_preset(
        "9.1.4",
        &SPEAKER_LAYOUT_9_1_4_CHANNELS,
        Some(3),
        vec![
            wide_bed(),
            upper_layer(vec![
                row(
                    TOP_FRONT_Y,
                    vec![
                        anchor("Ltf", TOP_INNER_LEFT, TOP_FRONT_Y, QMAX),
                        anchor("Rtf", TOP_INNER_RIGHT, TOP_FRONT_Y, QMAX),
                    ],
                ),
                row(
                    TOP_REAR_Y,
                    vec![
                        anchor("Ltr", TOP_INNER_LEFT, TOP_REAR_Y, QMAX),
                        anchor("Rtr", TOP_INNER_RIGHT, TOP_REAR_Y, QMAX),
                    ],
                ),
            ]),
        ],
    )
}

fn speaker_layout_9_1_6_preset() -> Result<SpeakerLayoutPreset, SpeakerLayoutPresetError> {
    generic_preset(
        "9.1.6",
        &SPEAKER_LAYOUT_9_1_6_CHANNELS,
        Some(3),
        vec![
            wide_bed(),
            upper_layer(vec![
                row(
                    TOP_FRONT_Y,
                    vec![
                        anchor("Ltf", TOP_INNER_LEFT, TOP_FRONT_Y, QMAX),
                        anchor("Rtf", TOP_INNER_RIGHT, TOP_FRONT_Y, QMAX),
                    ],
                ),
                row(
                    0.5,
                    vec![
                        anchor("Ltm", TOP_INNER_LEFT, 0.5, QMAX),
                        anchor("Rtm", TOP_INNER_RIGHT, 0.5, QMAX),
                    ],
                ),
                row(
                    TOP_REAR_Y,
                    vec![
                        anchor("Ltr", TOP_INNER_LEFT, TOP_REAR_Y, QMAX),
                        anchor("Rtr", TOP_INNER_RIGHT, TOP_REAR_Y, QMAX),
                    ],
                ),
            ]),
        ],
    )
}

fn generic_preset(
    name: &'static str,
    labels: &[&'static str],
    lfe_index: Option<usize>,
    layers: Vec<SpatialLayoutLayer>,
) -> Result<SpeakerLayoutPreset, SpeakerLayoutPresetError> {
    let labels = labels.to_vec();
    let channels = labels
        .iter()
        .enumerate()
        .map(|(index, identity)| SpatialLayoutChannel {
            identity: (*identity).to_owned(),
            enabled: true,
            lfe: lfe_index == Some(index) || matches!(*identity, "LFE1" | "LFE2"),
        })
        .collect();
    let layout = SpatialLayout::from_topology(
        channels,
        SpatialLayoutTopology {
            layers,
            aliases: Vec::new(),
        },
        Vec::new(),
    )?;
    let wav_channel_mask = match name {
        "7.1.6" | "9.1" | "9.1.2" | "9.1.4" | "9.1.6" | "22.2" => None,
        _ => Some(speaker_channel_mask_for_labels(&labels)?),
    };
    Ok(SpeakerLayoutPreset {
        name,
        labels,
        lfe_index,
        layout,
        wav_channel_mask,
    })
}

fn five_one_bed() -> SpatialLayoutLayer {
    bed_layer(vec![
        row(
            0.0,
            vec![
                anchor("FL", 0.0, 0.0, 0.0),
                anchor("FC", 0.5, 0.0, 0.0),
                anchor("FR", QMAX, 0.0, 0.0),
            ],
        ),
        row(
            0.5,
            vec![anchor("Ls", 0.0, 0.5, 0.0), anchor("Rs", QMAX, 0.5, 0.0)],
        ),
    ])
}

fn seven_one_bed() -> SpatialLayoutLayer {
    bed_layer(vec![
        five_one_bed().rows[0].clone(),
        five_one_bed().rows[1].clone(),
        row(
            QMAX,
            vec![anchor("Lb", 0.0, QMAX, 0.0), anchor("Rb", QMAX, QMAX, 0.0)],
        ),
    ])
}

fn wide_bed() -> SpatialLayoutLayer {
    bed_layer(vec![
        five_one_bed().rows[0].clone(),
        row(
            WIDE_ROW_Y,
            vec![
                anchor("Lw", WIDE_LEFT_X, WIDE_ROW_Y, 0.0),
                anchor("Rw", WIDE_RIGHT_X, WIDE_ROW_Y, 0.0),
            ],
        ),
        five_one_bed().rows[1].clone(),
        row(
            QMAX,
            vec![anchor("Lb", 0.0, QMAX, 0.0), anchor("Rb", QMAX, QMAX, 0.0)],
        ),
    ])
}

fn bed_layer(rows: Vec<SpatialLayoutRow>) -> SpatialLayoutLayer {
    SpatialLayoutLayer { z: 0.0, rows }
}

fn upper_layer(rows: Vec<SpatialLayoutRow>) -> SpatialLayoutLayer {
    SpatialLayoutLayer { z: QMAX, rows }
}

fn spherical_row<const N: usize>(
    _sort_azimuth: f64,
    speakers: [(&'static str, f64, f64); N],
) -> SpatialLayoutRow {
    let mut anchors = speakers
        .into_iter()
        .map(|(identity, azimuth, elevation)| spherical_anchor(identity, azimuth, elevation))
        .collect::<Vec<_>>();
    anchors.sort_by(|left, right| left.x.total_cmp(&right.x));
    let y = anchors[0].y;
    debug_assert!(anchors.iter().all(|anchor| (anchor.y - y).abs() < 1.0e-12));
    SpatialLayoutRow { y, anchors }
}

fn spherical_anchor(
    identity: &'static str,
    azimuth_degrees: f64,
    elevation_degrees: f64,
) -> SpatialLayoutAnchor {
    let azimuth = azimuth_degrees.to_radians();
    let elevation = elevation_degrees.to_radians();
    let horizontal = elevation.cos();
    anchor(
        identity,
        0.5 - 0.5 * azimuth.sin() * horizontal,
        0.5 * (1.0 - azimuth.cos() * horizontal),
        spherical_z(elevation_degrees),
    )
}

fn spherical_z(elevation_degrees: f64) -> f64 {
    elevation_degrees.to_radians().sin() * QMAX
}

fn row(y: f64, anchors: Vec<SpatialLayoutAnchor>) -> SpatialLayoutRow {
    SpatialLayoutRow { y, anchors }
}

fn anchor(identity: &'static str, x: f64, y: f64, z: f64) -> SpatialLayoutAnchor {
    SpatialLayoutAnchor {
        identity: identity.to_owned(),
        x,
        y,
        z,
    }
}
