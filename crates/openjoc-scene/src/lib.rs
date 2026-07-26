// pattern: Functional Core

//! Renderer-independent object-scene model for the TS 103 420 decoder interface.

use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt};

mod assembly;
pub use assembly::{SceneBuildError, SceneBuilder};

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

/// Renderer-independent scene produced by the `OpenJOC` codec core.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ObjectScene {
    pub sample_rate: u32,
    pub duration_samples: u64,
    pub objects: Vec<ObjectTrack>,
    pub metadata_timeline: Vec<MetadataUpdate>,
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

fn position_is_finite(position: &Position) -> bool {
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
