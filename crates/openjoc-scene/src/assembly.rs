// pattern: Functional Core

use crate::{
    Extent3, IsfLabel, IsfRing, MetadataUpdate, ObjectClass, ObjectScene, ObjectTrack, Position,
    Position3, ProgrammeLayout, ProgrammeLayoutError, SceneError, SpeakerLabel, TrimUpdate,
    ZoneConstraint, metadata_is_finite, trim_is_finite,
};
use openjoc_oamd::{
    ExtendedObjectElement, Gain, IsfRing as OamdIsfRing, OamdContentPrefix, OamdElement, OamdError,
    OamdPayload, ObjectAnchor, ObjectElement, ReferenceScreen, RoomPosition,
    SpeakerLabel as OamdSpeakerLabel, TrimElement, ZoneConstraint as OamdZoneConstraint,
};
use std::fmt;

/// Failures while joining reconstructed PCM with decoded OAMD frames.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SceneBuildError {
    Oamd(OamdError),
    ProgrammeLayout(ProgrammeLayoutError),
    ProgrammeLayoutMismatch,
    Scene(SceneError),
    ContentDescriptionChanged,
    ObjectCount { expected: usize, actual: usize },
    FrameLengthMismatch,
    MetadataShapeMismatch,
    MissingReferenceScreen,
    DurationOverflow,
}

impl fmt::Display for SceneBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Oamd(error) => write!(formatter, "invalid OAMD scene data: {error}"),
            Self::ProgrammeLayout(error) => {
                write!(formatter, "invalid OAMD/JOC programme layout: {error}")
            }
            Self::ProgrammeLayoutMismatch => formatter
                .write_str("programme layout does not match the OAMD content-description ordering"),
            Self::Scene(error) => write!(formatter, "invalid assembled scene: {error}"),
            Self::ContentDescriptionChanged => {
                formatter.write_str("OAMD content description changed without a reset")
            }
            Self::ObjectCount { expected, actual } => write!(
                formatter,
                "scene requires {expected} objects but frame contains {actual}"
            ),
            Self::FrameLengthMismatch => {
                formatter.write_str("reconstructed object frame lengths differ")
            }
            Self::MetadataShapeMismatch => {
                formatter.write_str("OAMD object/block metadata dimensions disagree")
            }
            Self::MissingReferenceScreen => {
                formatter.write_str("screen-anchored OAMD requires reference-screen geometry")
            }
            Self::DurationOverflow => formatter.write_str("scene duration overflow"),
        }
    }
}

impl std::error::Error for SceneBuildError {}

impl From<OamdError> for SceneBuildError {
    fn from(value: OamdError) -> Self {
        Self::Oamd(value)
    }
}

impl From<ProgrammeLayoutError> for SceneBuildError {
    fn from(value: ProgrammeLayoutError) -> Self {
        Self::ProgrammeLayout(value)
    }
}

impl From<SceneError> for SceneBuildError {
    fn from(value: SceneError) -> Self {
        Self::Scene(value)
    }
}

/// Atomic cross-frame assembler for object PCM and OAMD metadata.
#[derive(Clone, Debug)]
pub struct SceneBuilder {
    scene: ObjectScene,
    anchors: Vec<ObjectAnchor>,
}

impl SceneBuilder {
    /// Creates an empty scene from one normative OAMD content description.
    ///
    /// # Errors
    /// Returns [`SceneBuildError`] for an invalid sample rate or content model.
    pub fn new(sample_rate: u32, prefix: &OamdContentPrefix) -> Result<Self, SceneBuildError> {
        if sample_rate == 0 {
            return Err(SceneError::InvalidSampleRate.into());
        }
        let anchors = prefix.object_anchors()?;
        let objects = anchors
            .iter()
            .enumerate()
            .map(|(object_id, anchor)| {
                Ok(ObjectTrack {
                    object_id: u32::try_from(object_id)
                        .map_err(|_| SceneBuildError::DurationOverflow)?,
                    class: match anchor {
                        ObjectAnchor::Dynamic => ObjectClass::Dynamic,
                        ObjectAnchor::Speaker(
                            OamdSpeakerLabel::RcLfe | OamdSpeakerLabel::RcLfe2,
                        ) => ObjectClass::Lfe,
                        ObjectAnchor::Speaker(_) | ObjectAnchor::IntermediateSpatial(_) => {
                            ObjectClass::BedOrIsf
                        }
                    },
                    pcm: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>, SceneBuildError>>()?;
        Ok(Self {
            scene: ObjectScene {
                sample_rate,
                duration_samples: 0,
                objects,
                metadata_timeline: Vec::new(),
                trim_timeline: Vec::new(),
            },
            anchors,
        })
    }

    /// Atomically appends one aligned reconstructed-audio/OAMD frame.
    ///
    /// # Errors
    /// Returns [`SceneBuildError`] for changed configuration, inconsistent
    /// dimensions/timing, invalid positions, or scene arithmetic failures.
    pub fn append_frame(
        &mut self,
        object_pcm: &[Vec<f64>],
        oamd: &OamdPayload,
        reference_screen: Option<ReferenceScreen>,
    ) -> Result<(), SceneBuildError> {
        let layout = ProgrammeLayout::from_prefix(&oamd.prefix)?;
        self.append_frame_with_layout(object_pcm, None, oamd, reference_screen, &layout)
    }

    /// Atomically appends one frame after explicitly binding JOC rows and a
    /// separately carried base LFE to the OAMD programme layout.
    pub fn append_frame_with_layout(
        &mut self,
        joc_pcm: &[Vec<f64>],
        base_lfe_pcm: Option<&[f64]>,
        oamd: &OamdPayload,
        reference_screen: Option<ReferenceScreen>,
        layout: &ProgrammeLayout,
    ) -> Result<(), SceneBuildError> {
        if oamd.prefix.object_anchors()? != self.anchors {
            return Err(SceneBuildError::ContentDescriptionChanged);
        }
        if ProgrammeLayout::from_prefix(&oamd.prefix)? != *layout {
            return Err(SceneBuildError::ProgrammeLayoutMismatch);
        }
        let object_pcm = layout.bind_audio(joc_pcm, base_lfe_pcm)?;
        if object_pcm.len() != self.scene.objects.len() {
            return Err(SceneBuildError::ObjectCount {
                expected: self.scene.objects.len(),
                actual: object_pcm.len(),
            });
        }
        let frame_samples = object_pcm.first().map_or(0, Vec::len);
        if object_pcm.iter().any(|pcm| pcm.len() != frame_samples) {
            return Err(SceneBuildError::FrameLengthMismatch);
        }
        let frame_offset = self.scene.duration_samples;
        let frame_samples_u64 =
            u64::try_from(frame_samples).map_err(|_| SceneBuildError::DurationOverflow)?;
        let next_duration = frame_offset
            .checked_add(frame_samples_u64)
            .ok_or(SceneBuildError::DurationOverflow)?;
        for (object_id, pcm) in object_pcm.iter().enumerate() {
            if let Some(sample) = pcm.iter().position(|value| !value.is_finite()) {
                return Err(SceneBuildError::Scene(SceneError::NonFiniteAudio {
                    object_id: u32::try_from(object_id)
                        .map_err(|_| SceneBuildError::DurationOverflow)?,
                    sample,
                }));
            }
        }

        let trim = oamd.elements.iter().rev().find_map(|metadata| {
            if let OamdElement::Trim(trim) = &metadata.element {
                Some(trim)
            } else {
                None
            }
        });
        let mut frame_metadata = Vec::new();
        for (element_index, metadata) in oamd.elements.iter().enumerate() {
            let OamdElement::Objects(objects) = &metadata.element else {
                continue;
            };
            let extension = extension_after(&oamd.elements, element_index);
            let updates = append_object_updates(
                &self.anchors,
                objects,
                extension,
                trim,
                frame_offset,
                reference_screen,
            )?;
            for update in &updates {
                if update.start_sample >= next_duration {
                    return Err(SceneBuildError::Scene(SceneError::MetadataOutsideScene {
                        object_id: update.object_id,
                        start_sample: update.start_sample,
                    }));
                }
                if !metadata_is_finite(update) {
                    return Err(SceneBuildError::Scene(SceneError::NonFiniteMetadata {
                        object_id: update.object_id,
                    }));
                }
            }
            frame_metadata.extend(updates);
        }
        let mut frame_trims = Vec::new();
        for metadata in &oamd.elements {
            if let OamdElement::Trim(trim) = &metadata.element {
                if frame_offset >= next_duration {
                    return Err(SceneBuildError::Scene(SceneError::TrimOutsideScene {
                        start_sample: frame_offset,
                    }));
                }
                if trim.disable_trim_per_object.len() != self.scene.objects.len() {
                    return Err(SceneBuildError::Scene(
                        SceneError::TrimObjectCountMismatch {
                            expected: self.scene.objects.len(),
                            actual: trim.disable_trim_per_object.len(),
                        },
                    ));
                }
                if !trim_is_finite(trim) {
                    return Err(SceneBuildError::Scene(SceneError::NonFiniteTrim));
                }
                frame_trims.push(TrimUpdate {
                    start_sample: frame_offset,
                    trim: trim.clone(),
                });
            }
        }
        self.scene.duration_samples = next_duration;
        for (track, pcm) in self.scene.objects.iter_mut().zip(object_pcm) {
            track.pcm.extend_from_slice(&pcm);
        }
        self.scene.metadata_timeline.extend(frame_metadata);
        self.scene.trim_timeline.extend(frame_trims);
        debug_assert!(self.scene.validate().is_ok());
        Ok(())
    }

    /// Finalizes and validates the assembled scene.
    ///
    /// # Errors
    /// Returns [`SceneBuildError`] if final scene invariants are invalid.
    pub fn finish(self) -> Result<ObjectScene, SceneBuildError> {
        self.scene.validate()?;
        Ok(self.scene)
    }
}

fn extension_after(
    elements: &[openjoc_oamd::OamdElementMetadata],
    object_index: usize,
) -> Option<&ExtendedObjectElement> {
    elements
        .iter()
        .skip(object_index + 1)
        .take_while(|metadata| !matches!(metadata.element, OamdElement::Objects(_)))
        .find_map(|metadata| {
            if let OamdElement::Extended(extension) = &metadata.element {
                Some(extension)
            } else {
                None
            }
        })
}

#[allow(clippy::too_many_arguments)]
fn append_object_updates(
    anchors: &[ObjectAnchor],
    objects: &ObjectElement,
    extension: Option<&ExtendedObjectElement>,
    trim: Option<&TrimElement>,
    frame_offset: u64,
    reference_screen: Option<ReferenceScreen>,
) -> Result<Vec<MetadataUpdate>, SceneBuildError> {
    let mut resolved_objects = objects.clone();
    if let Some(extension) = extension {
        extension.apply_positions(&mut resolved_objects)?;
    }
    let objects = &resolved_objects;
    if objects.objects.len() != anchors.len()
        || objects
            .objects
            .iter()
            .any(|updates| updates.len() != objects.timing.blocks.len())
    {
        return Err(SceneBuildError::MetadataShapeMismatch);
    }
    if let Some(divergence) = extension.and_then(|value| value.divergence.as_ref())
        && (divergence.len() != anchors.len()
            || divergence
                .iter()
                .any(|updates| updates.len() != objects.timing.blocks.len()))
    {
        return Err(SceneBuildError::MetadataShapeMismatch);
    }
    if let Some(trim) = trim
        && trim.disable_trim_per_object.len() != anchors.len()
    {
        return Err(SceneBuildError::MetadataShapeMismatch);
    }

    let mut output = Vec::new();
    for (object_index, (anchor, updates)) in anchors.iter().zip(&objects.objects).enumerate() {
        for (block_index, (timing, update)) in objects.timing.blocks.iter().zip(updates).enumerate()
        {
            let object_id =
                u32::try_from(object_index).map_err(|_| SceneBuildError::DurationOverflow)?;
            output.push(MetadataUpdate {
                object_id,
                start_sample: frame_offset
                    .checked_add(u64::from(timing.start_sample))
                    .ok_or(SceneBuildError::DurationOverflow)?,
                ramp_duration: timing.ramp_duration,
                active: update.active,
                position: convert_position(*anchor, update.render, reference_screen)?,
                size: Extent3 {
                    width: update.render.size.width,
                    depth: update.render.size.depth,
                    height: update.render.size.height,
                },
                priority: update.basic.priority,
                gain_db: match update.basic.gain {
                    Gain::Decibels(value) => Some(f64::from(value)),
                    Gain::NegativeInfinity => None,
                },
                channel_lock: update.render.channel_lock,
                zones: update.render.zones.map(|zone| match zone {
                    OamdZoneConstraint::Include => ZoneConstraint::Include,
                    OamdZoneConstraint::Exclude => ZoneConstraint::Exclude,
                }),
                divergence: extension
                    .and_then(|value| value.divergence.as_ref())
                    .map_or(0.0, |values| values[object_index][block_index]),
                trim_disabled: trim
                    .is_some_and(|value| value.disable_trim_per_object[object_index]),
            });
        }
    }
    Ok(output)
}

fn convert_position(
    anchor: ObjectAnchor,
    render: openjoc_oamd::ObjectRenderInfo,
    reference_screen: Option<ReferenceScreen>,
) -> Result<Position, SceneBuildError> {
    Ok(match anchor {
        ObjectAnchor::Speaker(label) => Position::Speaker(convert_speaker(label)),
        ObjectAnchor::IntermediateSpatial(label) => Position::IntermediateSpatial(IsfLabel {
            ring: match label.ring {
                OamdIsfRing::Middle => IsfRing::Middle,
                OamdIsfRing::Upper => IsfRing::Upper,
                OamdIsfRing::Lower => IsfRing::Lower,
                OamdIsfRing::Zenith => IsfRing::Zenith,
            },
            index: label.index,
        }),
        ObjectAnchor::Dynamic if render.screen_anchor => Position::Screen {
            coded: convert_coordinate(render.position),
            interpolated_room: convert_coordinate(
                render
                    .screen_position(
                        reference_screen.ok_or(SceneBuildError::MissingReferenceScreen)?,
                    )?
                    .ok_or(SceneBuildError::MetadataShapeMismatch)?,
            ),
        },
        ObjectAnchor::Dynamic => match render.room_position()? {
            RoomPosition::Finite(position) => Position::Room(convert_coordinate(position)),
            RoomPosition::AtInfinity {
                boundary_intersection,
            } => Position::RoomAtInfinity {
                boundary_intersection: convert_coordinate(boundary_intersection),
            },
        },
    })
}

fn convert_coordinate(value: openjoc_oamd::Position3) -> Position3 {
    Position3 {
        x: value.x,
        y: value.y,
        z: value.z,
    }
}

fn convert_speaker(value: OamdSpeakerLabel) -> SpeakerLabel {
    match value {
        OamdSpeakerLabel::RcL => SpeakerLabel::RcL,
        OamdSpeakerLabel::RcR => SpeakerLabel::RcR,
        OamdSpeakerLabel::RcC => SpeakerLabel::RcC,
        OamdSpeakerLabel::RcLfe => SpeakerLabel::RcLfe,
        OamdSpeakerLabel::RcLs => SpeakerLabel::RcLs,
        OamdSpeakerLabel::RcRs => SpeakerLabel::RcRs,
        OamdSpeakerLabel::RcLb => SpeakerLabel::RcLb,
        OamdSpeakerLabel::RcRb => SpeakerLabel::RcRb,
        OamdSpeakerLabel::RcTfl => SpeakerLabel::RcTfl,
        OamdSpeakerLabel::RcTfr => SpeakerLabel::RcTfr,
        OamdSpeakerLabel::RcTsl => SpeakerLabel::RcTsl,
        OamdSpeakerLabel::RcTsr => SpeakerLabel::RcTsr,
        OamdSpeakerLabel::RcTbl => SpeakerLabel::RcTbl,
        OamdSpeakerLabel::RcTbr => SpeakerLabel::RcTbr,
        OamdSpeakerLabel::RcLw => SpeakerLabel::RcLw,
        OamdSpeakerLabel::RcRw => SpeakerLabel::RcRw,
        OamdSpeakerLabel::RcLfe2 => SpeakerLabel::RcLfe2,
    }
}
