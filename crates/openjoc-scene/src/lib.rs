// pattern: Functional Core

//! Renderer-independent object-scene model for the TS 103 420 decoder interface.

use openjoc_oamd::TrimElement;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt};

mod assembly;
pub use assembly::{SceneBuildError, SceneBuilder};
mod layout;
pub use layout::{
    ObjectAudioSource, ProgrammeAnchor, ProgrammeLayout, ProgrammeLayoutError,
    ProgrammeObjectBinding, ProgrammeObjectClass,
};
mod payload_decoder;
pub use payload_decoder::{
    DecodedPayloadFrame, JocFrameInput, PayloadDecodeError, PayloadDecoder, PayloadDecoderConfig,
};

/// Cartesian decoder-interface coordinate.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Position3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Speaker-coordinate label from TS 103 420 Tables 12 and 13.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeakerLabel {
    RcL,
    RcR,
    RcC,
    RcLfe,
    RcLs,
    RcRs,
    RcLb,
    RcRb,
    RcTfl,
    RcTfr,
    RcTsl,
    RcTsr,
    RcTbl,
    RcTbr,
    RcLw,
    RcRw,
    RcLfe2,
}

/// Ring class for a Table 11b intermediate-spatial-format coordinate.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IsfRing {
    Middle,
    Upper,
    Lower,
    Zenith,
}

/// Typed MULZ intermediate-spatial-format label.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IsfLabel {
    pub ring: IsfRing,
    pub index: u8,
}

/// Decoder-interface position and its normative anchor semantics.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "anchor", content = "value", rename_all = "snake_case")]
pub enum Position {
    Room(Position3),
    RoomAtInfinity {
        boundary_intersection: Position3,
    },
    Screen {
        coded: Position3,
        interpolated_room: Position3,
    },
    Speaker(SpeakerLabel),
    IntermediateSpatial(IsfLabel),
}

/// Three-dimensional object extent.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Extent3 {
    pub width: f64,
    pub depth: f64,
    pub height: f64,
}

/// Inclusion/exclusion constraint for one normative room zone.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneConstraint {
    Include,
    Exclude,
}

/// Decoder-interface object class.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectClass {
    BedOrIsf,
    Lfe,
    Dynamic,
}

/// One reconstructed object essence and its stable identity.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ObjectTrack {
    pub object_id: u32,
    pub class: ObjectClass,
    /// Mono f64 PCM samples in decoder time.
    pub pcm: Vec<f64>,
}

/// One timed, fully resolved metadata update.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MetadataUpdate {
    pub object_id: u32,
    pub start_sample: u64,
    pub ramp_duration: u16,
    pub active: bool,
    pub position: Position,
    pub size: Extent3,
    pub priority: f64,
    /// `None` represents negative-infinity gain.
    pub gain_db: Option<f64>,
    pub channel_lock: bool,
    pub zones: [ZoneConstraint; 6],
    pub divergence: f64,
    pub trim_disabled: bool,
}

/// One timed, renderer-independent snapshot of decoded OAMD trim state.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TrimUpdate {
    pub start_sample: u64,
    pub trim: TrimElement,
}

/// Renderer-independent scene produced by the `OpenJOC` codec core.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ObjectScene {
    pub sample_rate: u32,
    pub duration_samples: u64,
    pub objects: Vec<ObjectTrack>,
    pub metadata_timeline: Vec<MetadataUpdate>,
    pub trim_timeline: Vec<TrimUpdate>,
}

#[derive(Serialize)]
struct SceneManifest {
    sample_rate: u32,
    duration_samples: u64,
    objects: Vec<ObjectManifest>,
    metadata_timeline: &'static str,
    trim_timeline: &'static str,
}

#[derive(Serialize)]
struct ObjectManifest {
    object_id: u32,
    class: ObjectClass,
    wav: String,
}

/// Scene-model and JSON validation failures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SceneError {
    InvalidSampleRate,
    DuplicateObjectId {
        object_id: u32,
    },
    TrackDurationMismatch {
        object_id: u32,
        expected: u64,
        actual: u64,
    },
    UnknownMetadataObject {
        object_id: u32,
    },
    MetadataOutsideScene {
        object_id: u32,
        start_sample: u64,
    },
    NonFiniteAudio {
        object_id: u32,
        sample: usize,
    },
    NonFiniteMetadata {
        object_id: u32,
    },
    TrimOutsideScene {
        start_sample: u64,
    },
    TrimObjectCountMismatch {
        expected: usize,
        actual: usize,
    },
    NonFiniteTrim,
    Json(String),
}

impl fmt::Display for SceneError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSampleRate => formatter.write_str("invalid scene sample rate"),
            Self::DuplicateObjectId { object_id } => {
                write!(formatter, "duplicate scene object ID {object_id}")
            }
            Self::TrackDurationMismatch {
                object_id,
                expected,
                actual,
            } => write!(
                formatter,
                "object {object_id} has {actual} samples, expected {expected}"
            ),
            Self::UnknownMetadataObject { object_id } => {
                write!(formatter, "metadata references unknown object {object_id}")
            }
            Self::MetadataOutsideScene {
                object_id,
                start_sample,
            } => write!(
                formatter,
                "object {object_id} metadata starts outside scene at sample {start_sample}"
            ),
            Self::NonFiniteAudio { object_id, sample } => write!(
                formatter,
                "object {object_id} contains non-finite PCM at sample {sample}"
            ),
            Self::NonFiniteMetadata { object_id } => {
                write!(formatter, "object {object_id} contains non-finite metadata")
            }
            Self::TrimOutsideScene { start_sample } => write!(
                formatter,
                "trim metadata starts outside scene at sample {start_sample}"
            ),
            Self::TrimObjectCountMismatch { expected, actual } => write!(
                formatter,
                "trim metadata contains {actual} object flags, expected {expected}"
            ),
            Self::NonFiniteTrim => {
                formatter.write_str("trim metadata contains non-finite controls")
            }
            Self::Json(message) => write!(formatter, "invalid scene JSON: {message}"),
        }
    }
}

impl std::error::Error for SceneError {}

impl ObjectScene {
    /// Validates cross-field invariants required for scene export.
    ///
    /// # Errors
    /// Returns [`SceneError`] for invalid rates, identities, durations, time
    /// bounds, or non-finite numeric data.
    pub fn validate(&self) -> Result<(), SceneError> {
        if self.sample_rate == 0 {
            return Err(SceneError::InvalidSampleRate);
        }
        let mut object_ids = HashSet::with_capacity(self.objects.len());
        for object in &self.objects {
            if !object_ids.insert(object.object_id) {
                return Err(SceneError::DuplicateObjectId {
                    object_id: object.object_id,
                });
            }
            let actual = u64::try_from(object.pcm.len()).unwrap_or(u64::MAX);
            if actual != self.duration_samples {
                return Err(SceneError::TrackDurationMismatch {
                    object_id: object.object_id,
                    expected: self.duration_samples,
                    actual,
                });
            }
            if let Some(sample) = object.pcm.iter().position(|value| !value.is_finite()) {
                return Err(SceneError::NonFiniteAudio {
                    object_id: object.object_id,
                    sample,
                });
            }
        }
        for update in &self.metadata_timeline {
            if !object_ids.contains(&update.object_id) {
                return Err(SceneError::UnknownMetadataObject {
                    object_id: update.object_id,
                });
            }
            if update.start_sample >= self.duration_samples {
                return Err(SceneError::MetadataOutsideScene {
                    object_id: update.object_id,
                    start_sample: update.start_sample,
                });
            }
            if !position_is_finite(&update.position)
                || !update.size.width.is_finite()
                || !update.size.depth.is_finite()
                || !update.size.height.is_finite()
                || !update.priority.is_finite()
                || update.gain_db.is_some_and(|gain| !gain.is_finite())
                || !update.divergence.is_finite()
            {
                return Err(SceneError::NonFiniteMetadata {
                    object_id: update.object_id,
                });
            }
        }
        for update in &self.trim_timeline {
            if update.start_sample >= self.duration_samples {
                return Err(SceneError::TrimOutsideScene {
                    start_sample: update.start_sample,
                });
            }
            if update.trim.disable_trim_per_object.len() != self.objects.len() {
                return Err(SceneError::TrimObjectCountMismatch {
                    expected: self.objects.len(),
                    actual: update.trim.disable_trim_per_object.len(),
                });
            }
            if !trim_is_finite(&update.trim) {
                return Err(SceneError::NonFiniteTrim);
            }
        }
        Ok(())
    }

    /// Serializes a validated scene as readable JSON.
    ///
    /// # Errors
    /// Returns [`SceneError`] when validation or JSON serialization fails.
    pub fn to_json_pretty(&self) -> Result<String, SceneError> {
        self.validate()?;
        serde_json::to_string_pretty(self).map_err(|error| SceneError::Json(error.to_string()))
    }

    /// Serializes the output-directory manifest without embedding PCM arrays.
    ///
    /// # Errors
    /// Returns [`SceneError`] when validation or JSON serialization fails.
    pub fn to_manifest_json_pretty(&self) -> Result<String, SceneError> {
        self.validate()?;
        let manifest = SceneManifest {
            sample_rate: self.sample_rate,
            duration_samples: self.duration_samples,
            objects: self
                .objects
                .iter()
                .map(|object| ObjectManifest {
                    object_id: object.object_id,
                    class: object.class,
                    wav: format!("objects/object_{:03}.wav", object.object_id),
                })
                .collect(),
            metadata_timeline: "metadata/timeline.json",
            trim_timeline: "metadata/trim_timeline.json",
        };
        serde_json::to_string_pretty(&manifest).map_err(|error| SceneError::Json(error.to_string()))
    }

    /// Serializes the complete timed metadata update list for artifact export.
    ///
    /// # Errors
    /// Returns [`SceneError`] when validation or JSON serialization fails.
    pub fn to_timeline_json_pretty(&self) -> Result<String, SceneError> {
        self.validate()?;
        serde_json::to_string_pretty(&self.metadata_timeline)
            .map_err(|error| SceneError::Json(error.to_string()))
    }

    /// Serializes the decoded trim state timeline separately from object updates.
    pub fn to_trim_timeline_json_pretty(&self) -> Result<String, SceneError> {
        self.validate()?;
        serde_json::to_string_pretty(&self.trim_timeline)
            .map_err(|error| SceneError::Json(error.to_string()))
    }

    /// Parses and validates a scene JSON document.
    ///
    /// # Errors
    /// Returns [`SceneError`] for malformed JSON or invalid scene invariants.
    pub fn from_json(json: &str) -> Result<Self, SceneError> {
        let scene: Self =
            serde_json::from_str(json).map_err(|error| SceneError::Json(error.to_string()))?;
        scene.validate()?;
        Ok(scene)
    }
}

pub(crate) fn position_is_finite(position: &Position) -> bool {
    let finite =
        |value: &Position3| value.x.is_finite() && value.y.is_finite() && value.z.is_finite();
    match position {
        Position::Room(value)
        | Position::RoomAtInfinity {
            boundary_intersection: value,
        } => finite(value),
        Position::Screen {
            coded,
            interpolated_room,
        } => finite(coded) && finite(interpolated_room),
        Position::Speaker(_) | Position::IntermediateSpatial(_) => true,
    }
}

pub(crate) fn metadata_is_finite(update: &MetadataUpdate) -> bool {
    position_is_finite(&update.position)
        && update.size.width.is_finite()
        && update.size.depth.is_finite()
        && update.size.height.is_finite()
        && update.priority.is_finite()
        && update.gain_db.is_none_or(f64::is_finite)
        && update.divergence.is_finite()
}

pub(crate) fn trim_is_finite(trim: &TrimElement) -> bool {
    let controls_are_finite = |controls: &openjoc_oamd::TrimControls| {
        [
            controls.centre_db,
            controls.surround_db,
            controls.height_db,
            controls.top_bottom_y_balance,
            controls.listener_y_balance,
        ]
        .into_iter()
        .flatten()
        .all(f64::is_finite)
    };
    match &trim.global_trim {
        openjoc_oamd::GlobalTrim::Custom(configurations) => {
            configurations.iter().all(|config| match config {
                openjoc_oamd::TrimConfiguration::Custom(controls) => controls_are_finite(controls),
                openjoc_oamd::TrimConfiguration::Default
                | openjoc_oamd::TrimConfiguration::Disabled => true,
            })
        }
        openjoc_oamd::GlobalTrim::Default | openjoc_oamd::GlobalTrim::Disabled => true,
    }
}
