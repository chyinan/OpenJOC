//! Canonical public speaker-layout presets.
//!
//! Presets are data instances for [`SpatialLayout`].  They intentionally do
//! not contain a renderer or a layout-specific projection law.

use crate::{
    SpatialLayout, SpatialLayoutAnchor, SpatialLayoutChannel, SpatialLayoutLayer, SpatialLayoutRow,
    SpatialLayoutTopology, SpatialProjectionError,
};
use std::fmt;

/// Public speaker presets exposed by the JOC speaker-rendering workflow.
pub const SPEAKER_LAYOUT_PRESET_NAMES: [&str; 7] =
    ["5.1", "5.1.2", "5.1.4", "7.1", "7.1.2", "7.1.4", "7.1.6"];

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

const QMAX: f64 = 32_767.0 / 32_768.0;
const TOP_INNER_LEFT: f64 = 0.241_943_359_375;
const TOP_INNER_RIGHT: f64 = 0.758_056_640_625;
const TOP_FRONT_Y: f64 = 0.241_943_359_375;
const TOP_REAR_Y: f64 = 0.758_056_640_625;

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
    /// Returns one of the seven public speaker presets.
    pub fn for_name(name: &str) -> Result<Self, SpeakerLayoutPresetError> {
        match name {
            "5.1" => speaker_layout_5_1_preset(),
            "5.1.2" => speaker_layout_5_1_2_preset(),
            "5.1.4" => speaker_layout_5_1_4_preset(),
            "7.1" => speaker_layout_7_1_preset(),
            "7.1.2" => speaker_layout_7_1_2_preset(),
            "7.1.4" => speaker_layout_7_1_4_preset(),
            "7.1.6" => speaker_layout_7_1_6_preset(),
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
            lfe: lfe_index == Some(index),
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
        "7.1.6" => None,
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

fn bed_layer(rows: Vec<SpatialLayoutRow>) -> SpatialLayoutLayer {
    SpatialLayoutLayer { z: 0.0, rows }
}

fn upper_layer(rows: Vec<SpatialLayoutRow>) -> SpatialLayoutLayer {
    SpatialLayoutLayer { z: QMAX, rows }
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
