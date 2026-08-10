// pattern: Functional Core

use crate::{IsfLabel, SpeakerLabel};
use openjoc_oamd::{OamdContentPrefix, OamdError, ObjectAnchor};
use serde::Serialize;
use std::fmt;

/// Structural source category for one OAMD programme entry. This is not a
/// semantic authored-object binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProgrammeAudioSource {
    ReconstructionRow { row_index: usize },
    BaseLfe { channel_index: usize },
    UnsupportedBed,
    UnsupportedIsf,
}

/// Stable anchor identity retained in a programme binding report.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ProgrammeAnchor {
    Speaker(SpeakerLabel),
    IntermediateSpatial(IsfLabel),
    Dynamic,
}

/// Programme-level class, including the speaker-anchored LFE distinction
/// that the renderer-independent `ObjectClass` intentionally groups as bed.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgrammeObjectClass {
    Bed,
    Lfe,
    Isf,
    Dynamic,
}

/// One typed structural OAMD programme-layout entry. `oamd_index` is never
/// compacted or re-numbered when a speaker-anchored entry precedes dynamic
/// slots. It must not be treated as semantic audio binding evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct ProgrammeLayoutEntry {
    pub oamd_index: usize,
    pub class: ProgrammeObjectClass,
    pub anchor: ProgrammeAnchor,
    pub dynamic_slot_index: Option<usize>,
    pub source: ProgrammeAudioSource,
}

/// Programme-level cardinalities and typed source bindings derived from the
/// OAMD content-description fields. This is not a `count - 1` compatibility
/// rule: every source is derived from its normative anchor and ordering.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProgrammeLayout {
    pub total_oamd_count: usize,
    pub speaker_anchored_count: usize,
    pub bed_count: usize,
    pub lfe_count: usize,
    pub isf_count: usize,
    pub dynamic_slot_count: usize,
    pub bindings: Vec<ProgrammeLayoutEntry>,
}

/// Explicit failures at the OAMD programme/JOC source-mapping boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProgrammeLayoutError {
    Oamd(OamdError),
    DynamicSlotCountMismatch { expected: usize, actual: usize },
    JocDynamicCountMismatch { expected: usize, actual: usize },
    MissingExpectedLfe,
    MultipleLfeObjects { count: usize },
    UnexpectedObjectOrdering { oamd_index: usize },
    UnsupportedBedToJocMapping { oamd_index: usize },
    UnsupportedIsfToJocMapping { oamd_index: usize },
    BaseLfeUnavailable,
    BaseLfeLengthMismatch { expected: usize, actual: usize },
    DynamicRowOutOfRange { row_index: usize, available: usize },
    DuplicateJocRow { row_index: usize },
}

impl fmt::Display for ProgrammeLayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Oamd(error) => write!(formatter, "invalid OAMD programme layout: {error}"),
            Self::DynamicSlotCountMismatch { expected, actual } => write!(
                formatter,
                "OAMD dynamic slot count {expected} does not match JOC output count {actual}"
            ),
            Self::JocDynamicCountMismatch { expected, actual } => write!(
                formatter,
                "JOC dynamic output count {actual} does not match OAMD dynamic slot count {expected}"
            ),
            Self::MissingExpectedLfe => {
                formatter.write_str("OAMD programme requires a base-carried LFE")
            }
            Self::MultipleLfeObjects { count } => {
                write!(
                    formatter,
                    "OAMD programme contains {count} LFE objects; one is supported"
                )
            }
            Self::UnexpectedObjectOrdering { oamd_index } => write!(
                formatter,
                "OAMD LFE/object ordering is unsupported at programme index {oamd_index}"
            ),
            Self::UnsupportedBedToJocMapping { oamd_index } => write!(
                formatter,
                "OAMD bed entry {oamd_index} has no implemented JOC source mapping"
            ),
            Self::UnsupportedIsfToJocMapping { oamd_index } => write!(
                formatter,
                "OAMD ISF entry {oamd_index} has no implemented JOC source mapping"
            ),
            Self::BaseLfeUnavailable => {
                formatter.write_str("OAMD LFE entry has no base PCM source")
            }
            Self::BaseLfeLengthMismatch { expected, actual } => write!(
                formatter,
                "base LFE contains {actual} samples; expected {expected}"
            ),
            Self::DynamicRowOutOfRange {
                row_index,
                available,
            } => write!(
                formatter,
                "OAMD dynamic slot maps to JOC row {row_index}, but only {available} rows exist"
            ),
            Self::DuplicateJocRow { row_index } => {
                write!(formatter, "JOC row {row_index} is mapped more than once")
            }
        }
    }
}

impl std::error::Error for ProgrammeLayoutError {}

impl From<OamdError> for ProgrammeLayoutError {
    fn from(value: OamdError) -> Self {
        Self::Oamd(value)
    }
}

impl ProgrammeLayout {
    /// Expands normative OAMD anchors into typed, stable programme bindings.
    pub fn from_prefix(prefix: &OamdContentPrefix) -> Result<Self, ProgrammeLayoutError> {
        let anchors = prefix.object_anchors()?;
        let total_oamd_count = anchors.len();
        let lfe_count = anchors
            .iter()
            .filter(|anchor| {
                matches!(
                    anchor,
                    ObjectAnchor::Speaker(
                        openjoc_oamd::SpeakerLabel::RcLfe | openjoc_oamd::SpeakerLabel::RcLfe2
                    )
                )
            })
            .count();
        let speaker_anchored_count = anchors
            .iter()
            .filter(|anchor| matches!(anchor, ObjectAnchor::Speaker(_)))
            .count();
        let bed_count = speaker_anchored_count.saturating_sub(lfe_count);
        let isf_count = anchors
            .iter()
            .filter(|anchor| matches!(anchor, ObjectAnchor::IntermediateSpatial(_)))
            .count();
        let dynamic_slot_count = anchors
            .iter()
            .filter(|anchor| matches!(anchor, ObjectAnchor::Dynamic))
            .count();
        let mut dynamic_slot_index = 0;
        let bindings = anchors
            .into_iter()
            .enumerate()
            .map(|(oamd_index, anchor)| {
                let (class, programme_anchor, source, slot) = match anchor {
                    ObjectAnchor::Speaker(
                        label @ (openjoc_oamd::SpeakerLabel::RcLfe
                        | openjoc_oamd::SpeakerLabel::RcLfe2),
                    ) => (
                        ProgrammeObjectClass::Lfe,
                        ProgrammeAnchor::Speaker(map_speaker(label)),
                        ProgrammeAudioSource::BaseLfe {
                            channel_index: usize::from(label == openjoc_oamd::SpeakerLabel::RcLfe2),
                        },
                        None,
                    ),
                    ObjectAnchor::Speaker(label) => (
                        ProgrammeObjectClass::Bed,
                        ProgrammeAnchor::Speaker(map_speaker(label)),
                        ProgrammeAudioSource::UnsupportedBed,
                        None,
                    ),
                    ObjectAnchor::IntermediateSpatial(label) => (
                        ProgrammeObjectClass::Isf,
                        ProgrammeAnchor::IntermediateSpatial(map_isf(label)),
                        ProgrammeAudioSource::UnsupportedIsf,
                        None,
                    ),
                    ObjectAnchor::Dynamic => {
                        let slot = dynamic_slot_index;
                        dynamic_slot_index += 1;
                        (
                            ProgrammeObjectClass::Dynamic,
                            ProgrammeAnchor::Dynamic,
                            ProgrammeAudioSource::ReconstructionRow { row_index: slot },
                            Some(slot),
                        )
                    }
                };
                ProgrammeLayoutEntry {
                    oamd_index,
                    class,
                    anchor: programme_anchor,
                    dynamic_slot_index: slot,
                    source,
                }
            })
            .collect();
        Ok(Self {
            total_oamd_count,
            speaker_anchored_count,
            bed_count,
            lfe_count,
            isf_count,
            dynamic_slot_count,
            bindings,
        })
    }

    /// Validates that the current JOC frame can satisfy every supported
    /// programme entry without compacting or reinterpreting OAMD order.
    pub fn validate_joc_output(&self, joc_output_count: usize) -> Result<(), ProgrammeLayoutError> {
        if self.lfe_count > 1 {
            return Err(ProgrammeLayoutError::MultipleLfeObjects {
                count: self.lfe_count,
            });
        }
        if self.lfe_count == 1
            && !matches!(
                self.bindings.first().map(|binding| binding.source),
                Some(ProgrammeAudioSource::BaseLfe { .. })
            )
        {
            return Err(ProgrammeLayoutError::UnexpectedObjectOrdering {
                oamd_index: self
                    .bindings
                    .iter()
                    .position(|binding| {
                        matches!(binding.source, ProgrammeAudioSource::BaseLfe { .. })
                    })
                    .unwrap_or(0),
            });
        }
        if let Some(binding) = self
            .bindings
            .iter()
            .find(|binding| matches!(binding.source, ProgrammeAudioSource::UnsupportedBed))
        {
            return Err(ProgrammeLayoutError::UnsupportedBedToJocMapping {
                oamd_index: binding.oamd_index,
            });
        }
        if let Some(binding) = self
            .bindings
            .iter()
            .find(|binding| matches!(binding.source, ProgrammeAudioSource::UnsupportedIsf))
        {
            return Err(ProgrammeLayoutError::UnsupportedIsfToJocMapping {
                oamd_index: binding.oamd_index,
            });
        }
        if self.dynamic_slot_count != joc_output_count {
            return Err(ProgrammeLayoutError::JocDynamicCountMismatch {
                expected: self.dynamic_slot_count,
                actual: joc_output_count,
            });
        }
        Ok(())
    }

    /// Validates only the structural reconstruction-basis cardinality. This
    /// is the boundary used by metadata-only scene assembly and deliberately
    /// does not require a semantic OAMD-to-row binding or reject metadata
    /// classes whose audio source is not yet implemented.
    pub fn validate_reconstruction_basis(
        &self,
        reconstruction_row_count: usize,
    ) -> Result<(), ProgrammeLayoutError> {
        if self.dynamic_slot_count != reconstruction_row_count {
            return Err(ProgrammeLayoutError::JocDynamicCountMismatch {
                expected: self.dynamic_slot_count,
                actual: reconstruction_row_count,
            });
        }
        Ok(())
    }
}

fn map_speaker(label: openjoc_oamd::SpeakerLabel) -> SpeakerLabel {
    match label {
        openjoc_oamd::SpeakerLabel::RcL => SpeakerLabel::RcL,
        openjoc_oamd::SpeakerLabel::RcR => SpeakerLabel::RcR,
        openjoc_oamd::SpeakerLabel::RcC => SpeakerLabel::RcC,
        openjoc_oamd::SpeakerLabel::RcLfe => SpeakerLabel::RcLfe,
        openjoc_oamd::SpeakerLabel::RcLs => SpeakerLabel::RcLs,
        openjoc_oamd::SpeakerLabel::RcRs => SpeakerLabel::RcRs,
        openjoc_oamd::SpeakerLabel::RcLb => SpeakerLabel::RcLb,
        openjoc_oamd::SpeakerLabel::RcRb => SpeakerLabel::RcRb,
        openjoc_oamd::SpeakerLabel::RcTfl => SpeakerLabel::RcTfl,
        openjoc_oamd::SpeakerLabel::RcTfr => SpeakerLabel::RcTfr,
        openjoc_oamd::SpeakerLabel::RcTsl => SpeakerLabel::RcTsl,
        openjoc_oamd::SpeakerLabel::RcTsr => SpeakerLabel::RcTsr,
        openjoc_oamd::SpeakerLabel::RcTbl => SpeakerLabel::RcTbl,
        openjoc_oamd::SpeakerLabel::RcTbr => SpeakerLabel::RcTbr,
        openjoc_oamd::SpeakerLabel::RcLw => SpeakerLabel::RcLw,
        openjoc_oamd::SpeakerLabel::RcRw => SpeakerLabel::RcRw,
        openjoc_oamd::SpeakerLabel::RcLfe2 => SpeakerLabel::RcLfe2,
    }
}

fn map_isf(label: openjoc_oamd::IsfLabel) -> IsfLabel {
    IsfLabel {
        ring: match label.ring {
            openjoc_oamd::IsfRing::Middle => crate::IsfRing::Middle,
            openjoc_oamd::IsfRing::Upper => crate::IsfRing::Upper,
            openjoc_oamd::IsfRing::Lower => crate::IsfRing::Lower,
            openjoc_oamd::IsfRing::Zenith => crate::IsfRing::Zenith,
        },
        index: label.index,
    }
}
