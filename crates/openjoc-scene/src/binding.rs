//! Evidence and admission contract for semantic object-to-audio binding.
//!
//! The decoder can expose metadata objects and reconstruction rows without
//! claiming that either side identifies the other.  This module keeps the
//! evidence taxonomy separate from [`SemanticBindingState`], so a structural
//! or empirical observation cannot silently become an audio binding.

use openjoc_joc::{ReconstructionBasis, ReconstructionBasisRowIndex};
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

/// Codec profiles admitted by the clean-room decoded-object contract.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BindingCodecProfile {
    EAc3JocObservedOrdinary,
    EAc3JocObservedOrdinaryCompatWarp3,
    Unsupported,
}

/// Structural class used only while validating the OAMD total-object domain.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OamdBindingObjectClass {
    BaseLfe,
    Bed,
    Isf,
    Dynamic,
}

/// JOC decoded-object ordinal. This is not an authored ADM object identity.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct JocDecodedObjectOrdinal(pub usize);

/// OAMD ordinal within the dynamic-object subdomain.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct OamdDynamicObjectOrdinal(pub usize);

/// OAMD index within the complete total-object list, including Base LFE.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct OamdTotalObjectIndex(pub usize);

impl JocDecodedObjectOrdinal {
    /// Promotes a public reconstruction-basis coordinate to the decoded JOC
    /// output-object domain at the same JOC output boundary.
    #[must_use]
    pub const fn from_reconstruction_row(row: ReconstructionBasisRowIndex) -> Self {
        Self(row.0)
    }

    /// Returns the reconstruction-basis coordinate represented by this local
    /// decoded JOC output-object ordinal.
    #[must_use]
    pub const fn reconstruction_row(self) -> ReconstructionBasisRowIndex {
        ReconstructionBasisRowIndex(self.0)
    }

    /// Returns the corresponding OAMD dynamic-domain ordinal for the admitted
    /// carrier-local profile.
    #[must_use]
    pub const fn oamd_dynamic_ordinal(self) -> OamdDynamicObjectOrdinal {
        OamdDynamicObjectOrdinal(self.0)
    }

    /// Returns the corresponding OAMD total-domain index. The leading Base
    /// LFE offset is centralized here and is never an authored-object ID.
    pub const fn oamd_total_index(
        self,
    ) -> Result<OamdTotalObjectIndex, DecodedJocBindingUnavailable> {
        match self.0.checked_add(1) {
            Some(index) => Ok(OamdTotalObjectIndex(index)),
            None => Err(DecodedJocBindingUnavailable::OrdinalOverflow),
        }
    }
}

/// Structural facts supplied by the decoded JOC/OAMD boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedJocBindingFacts {
    pub codec_profile: BindingCodecProfile,
    pub joc_object_count: usize,
    pub reconstruction_row_count: usize,
    pub oamd_total_classes: Vec<OamdBindingObjectClass>,
}

impl DecodedJocBindingFacts {
    #[must_use]
    pub fn new(
        codec_profile: BindingCodecProfile,
        joc_object_count: usize,
        reconstruction_row_count: usize,
        oamd_total_classes: Vec<OamdBindingObjectClass>,
    ) -> Self {
        Self {
            codec_profile,
            joc_object_count,
            reconstruction_row_count,
            oamd_total_classes,
        }
    }

    pub fn replace_total_class(&mut self, index: usize, class: OamdBindingObjectClass) {
        if let Some(existing) = self.oamd_total_classes.get_mut(index) {
            *existing = class;
        }
    }

    pub const fn set_joc_object_count(&mut self, count: usize) {
        self.joc_object_count = count;
    }

    #[must_use]
    pub fn from_programme_layout(
        reconstruction_row_count: usize,
        layout: &crate::ProgrammeLayout,
    ) -> Self {
        Self::from_programme_layout_with_joc_count(
            reconstruction_row_count,
            reconstruction_row_count,
            layout,
        )
    }

    #[must_use]
    pub fn from_programme_layout_with_joc_count(
        joc_object_count: usize,
        reconstruction_row_count: usize,
        layout: &crate::ProgrammeLayout,
    ) -> Self {
        Self::from_programme_layout_with_profile(
            BindingCodecProfile::EAc3JocObservedOrdinary,
            joc_object_count,
            reconstruction_row_count,
            layout,
        )
    }

    #[must_use]
    pub fn from_programme_layout_with_profile(
        codec_profile: BindingCodecProfile,
        joc_object_count: usize,
        reconstruction_row_count: usize,
        layout: &crate::ProgrammeLayout,
    ) -> Self {
        let oamd_total_classes = layout
            .bindings
            .iter()
            .map(|binding| match binding.class {
                crate::ProgrammeObjectClass::Lfe => OamdBindingObjectClass::BaseLfe,
                crate::ProgrammeObjectClass::Bed => OamdBindingObjectClass::Bed,
                crate::ProgrammeObjectClass::Isf => OamdBindingObjectClass::Isf,
                crate::ProgrammeObjectClass::Dynamic => OamdBindingObjectClass::Dynamic,
            })
            .collect();
        Self::new(
            codec_profile,
            joc_object_count,
            reconstruction_row_count,
            oamd_total_classes,
        )
    }

    #[must_use]
    pub fn from_scene_classes(
        reconstruction_row_count: usize,
        classes: &[crate::ObjectClass],
    ) -> Self {
        let oamd_total_classes = classes
            .iter()
            .map(|class| match class {
                crate::ObjectClass::Lfe => OamdBindingObjectClass::BaseLfe,
                crate::ObjectClass::Dynamic => OamdBindingObjectClass::Dynamic,
                crate::ObjectClass::BedOrIsf => OamdBindingObjectClass::Bed,
            })
            .collect();
        Self::new(
            BindingCodecProfile::EAc3JocObservedOrdinary,
            reconstruction_row_count,
            reconstruction_row_count,
            oamd_total_classes,
        )
    }
}

/// One carrier-local association between decoded JOC PCM and OAMD metadata.
///
/// The reconstruction row remains a decoder coordinate. The OAMD indices are
/// local carrier ordinals and do not imply original authored identity.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BoundDecodedJocObject {
    pub joc_ordinal: JocDecodedObjectOrdinal,
    pub reconstruction_row: ReconstructionBasisRowIndex,
    pub oamd_dynamic_ordinal: OamdDynamicObjectOrdinal,
    pub oamd_total_index: OamdTotalObjectIndex,
}

/// Borrowed decoded-scene view for one admitted carrier-local object.
///
/// The view pairs one reconstruction row with only the matching OAMD event
/// references. It does not copy programme-duration PCM and carries no
/// authored identity.
#[derive(Debug)]
pub struct BoundDecodedJocObjectView<'a> {
    pub binding: BoundDecodedJocObject,
    pub pcm: &'a [f64],
    pub metadata: Vec<&'a crate::MetadataUpdate>,
    pub binding_profile: BindingCodecProfile,
}

/// A fully admitted, scoped decoded-object binding profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedJocBindingProfile {
    codec_profile: BindingCodecProfile,
    joc_object_count: usize,
}

impl DecodedJocBindingProfile {
    #[must_use]
    pub const fn codec_profile(&self) -> BindingCodecProfile {
        self.codec_profile
    }

    #[must_use]
    pub const fn profile_name(&self) -> &'static str {
        match self.codec_profile {
            BindingCodecProfile::EAc3JocObservedOrdinary => "E_AC_3_JOC_OBSERVED_ORDINARY_PROFILE",
            BindingCodecProfile::EAc3JocObservedOrdinaryCompatWarp3 => {
                "E_AC_3_JOC_OBSERVED_ORDINARY_COMPAT_WARP3_PROFILE"
            }
            BindingCodecProfile::Unsupported => "UNSUPPORTED",
        }
    }

    #[must_use]
    pub const fn joc_object_count(&self) -> usize {
        self.joc_object_count
    }

    /// Creates the canonical typed mapping for every admitted output slot.
    ///
    /// The only total-domain offset is created here, after the full profile
    /// gate has passed.
    pub fn bind_decoded_objects(
        &self,
    ) -> Result<Vec<BoundDecodedJocObject>, DecodedJocBindingUnavailable> {
        let mut objects = Vec::with_capacity(self.joc_object_count);
        for ordinal in 0..self.joc_object_count {
            let joc_ordinal = JocDecodedObjectOrdinal(ordinal);
            objects.push(BoundDecodedJocObject {
                joc_ordinal,
                reconstruction_row: joc_ordinal.reconstruction_row(),
                oamd_dynamic_ordinal: joc_ordinal.oamd_dynamic_ordinal(),
                oamd_total_index: joc_ordinal.oamd_total_index()?,
            });
        }
        Ok(objects)
    }

    /// Binds the admitted typed coordinates to borrowed scene data.
    ///
    /// This is the canonical complete decoded-object scene view:
    /// `ReconstructionBasis.rows[j] + metadata(object_id = j + 1)`. The
    /// metadata vector contains references into the caller-owned timeline and
    /// therefore does not duplicate PCM or expand storage with programme
    /// duration.
    pub fn bind_scene_objects<'a>(
        &self,
        basis: &'a ReconstructionBasis,
        metadata_timeline: &'a [crate::MetadataUpdate],
    ) -> Result<Vec<BoundDecodedJocObjectView<'a>>, DecodedJocBindingUnavailable> {
        if basis.rows.len() != self.joc_object_count {
            return Err(
                DecodedJocBindingUnavailable::DecodedJocObjectPopulationMismatch {
                    joc: self.joc_object_count,
                    reconstruction_rows: basis.rows.len(),
                    oamd_dynamic: self.joc_object_count,
                },
            );
        }

        self.bind_decoded_objects()?
            .into_iter()
            .map(|binding| {
                let pcm = basis.rows.get(binding.reconstruction_row.0).ok_or(
                    DecodedJocBindingUnavailable::ReconstructionRowUnavailable {
                        row_index: binding.reconstruction_row.0,
                    },
                )?;
                let metadata = metadata_timeline
                    .iter()
                    .filter(|update| {
                        update.object_id
                            == u32::try_from(binding.oamd_total_index.0).unwrap_or(u32::MAX)
                    })
                    .collect::<Vec<_>>();
                if metadata.is_empty() {
                    return Err(DecodedJocBindingUnavailable::MissingOamdMetadata {
                        total_index: binding.oamd_total_index.0,
                    });
                }
                Ok(BoundDecodedJocObjectView {
                    binding,
                    pcm,
                    metadata,
                    binding_profile: self.codec_profile,
                })
            })
            .collect()
    }
}

/// Why a decoded-object/OAMD binding was not produced.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DecodedJocBindingUnavailable {
    UnsupportedCodecProfile {
        actual: BindingCodecProfile,
    },
    UnsupportedBed {
        total_index: usize,
    },
    UnsupportedIsf {
        total_index: usize,
    },
    BaseLfeBindingPrecondition,
    DecodedJocObjectPopulationMismatch {
        joc: usize,
        reconstruction_rows: usize,
        oamd_dynamic: usize,
    },
    OamdTotalPopulationMismatch {
        expected: usize,
        actual: usize,
    },
    OamdObjectOrdering {
        total_index: usize,
    },
    ReconstructionRowUnavailable {
        row_index: usize,
    },
    MissingOamdMetadata {
        total_index: usize,
    },
    OrdinalOverflow,
}

impl fmt::Display for DecodedJocBindingUnavailable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedCodecProfile { actual } => {
                write!(
                    formatter,
                    "unsupported JOC binding codec profile {actual:?}"
                )
            }
            Self::UnsupportedBed { total_index } => {
                write!(
                    formatter,
                    "unsupported bed-bearing JOC/OAMD binding profile at OAMD total index {total_index}"
                )
            }
            Self::UnsupportedIsf { total_index } => {
                write!(
                    formatter,
                    "unsupported ISF-bearing JOC/OAMD binding profile at OAMD total index {total_index}"
                )
            }
            Self::BaseLfeBindingPrecondition => {
                formatter.write_str("Base LFE binding precondition not satisfied")
            }
            Self::DecodedJocObjectPopulationMismatch {
                joc,
                reconstruction_rows,
                oamd_dynamic,
            } => write!(
                formatter,
                "decoded JOC/OAMD object population mismatch: joc={joc}, reconstruction_rows={reconstruction_rows}, oamd_dynamic={oamd_dynamic}"
            ),
            Self::OamdTotalPopulationMismatch { expected, actual } => write!(
                formatter,
                "OAMD total object population mismatch: expected {expected}, actual {actual}"
            ),
            Self::OamdObjectOrdering { total_index } => {
                write!(
                    formatter,
                    "OAMD total object ordering is unsupported at index {total_index}"
                )
            }
            Self::ReconstructionRowUnavailable { row_index } => {
                write!(
                    formatter,
                    "decoded JOC reconstruction row {row_index} is unavailable"
                )
            }
            Self::MissingOamdMetadata { total_index } => {
                write!(
                    formatter,
                    "no OAMD metadata updates for admitted OAMD total index {total_index}"
                )
            }
            Self::OrdinalOverflow => formatter.write_str("decoded object ordinal overflow"),
        }
    }
}

impl Error for DecodedJocBindingUnavailable {}

/// Applies the exact clean-room admission predicate and returns an admission token.
pub fn admit_decoded_joc_binding(
    facts: &DecodedJocBindingFacts,
) -> Result<DecodedJocBindingProfile, DecodedJocBindingUnavailable> {
    if !matches!(
        facts.codec_profile,
        BindingCodecProfile::EAc3JocObservedOrdinary
            | BindingCodecProfile::EAc3JocObservedOrdinaryCompatWarp3
    ) {
        return Err(DecodedJocBindingUnavailable::UnsupportedCodecProfile {
            actual: facts.codec_profile,
        });
    }
    if facts.oamd_total_classes.len() != 16 {
        return Err(DecodedJocBindingUnavailable::OamdTotalPopulationMismatch {
            expected: 16,
            actual: facts.oamd_total_classes.len(),
        });
    }
    let Some(OamdBindingObjectClass::BaseLfe) = facts.oamd_total_classes.first().copied() else {
        return Err(DecodedJocBindingUnavailable::BaseLfeBindingPrecondition);
    };
    let mut dynamic_count = 0;
    for (total_index, class) in facts.oamd_total_classes.iter().copied().enumerate().skip(1) {
        match class {
            OamdBindingObjectClass::Dynamic => dynamic_count += 1,
            OamdBindingObjectClass::Bed => {
                return Err(DecodedJocBindingUnavailable::UnsupportedBed { total_index });
            }
            OamdBindingObjectClass::Isf => {
                return Err(DecodedJocBindingUnavailable::UnsupportedIsf { total_index });
            }
            OamdBindingObjectClass::BaseLfe => {
                return Err(DecodedJocBindingUnavailable::OamdObjectOrdering { total_index });
            }
        }
    }
    if facts.joc_object_count != 15 || facts.reconstruction_row_count != 15 || dynamic_count != 15 {
        return Err(
            DecodedJocBindingUnavailable::DecodedJocObjectPopulationMismatch {
                joc: facts.joc_object_count,
                reconstruction_rows: facts.reconstruction_row_count,
                oamd_dynamic: dynamic_count,
            },
        );
    }
    Ok(DecodedJocBindingProfile {
        codec_profile: facts.codec_profile,
        joc_object_count: facts.joc_object_count,
    })
}

/// The relation a contributor is proposing to support.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingRelationKind {
    AuthoredObjectToRow,
    OamdSlotToRow,
    SpatialStateToBasis,
    DistributedSpatialBasis,
    ContextDependentBasis,
}

/// How strong the observations are, independently of production admission.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingEvidenceClass {
    Structural,
    Empirical,
    Verified,
}

/// Allowed provenance classes for a binding evidence package.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BindingProvenance {
    NormativePublic,
    PublicReference,
    #[serde(alias = "CONTROLLED_CLEANROOM_EMPIRICAL")]
    ControlledEmpirical,
    StructuralImplementation,
    OtherAllowed,
}

/// Report-level admission status for an evidence package. It is separate from
/// the production decoded-JOC carrier binding state and does not recover
/// authored-object identity.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingAdmissionStatus {
    #[default]
    NotAdmitted,
    Admitted,
}

/// Evidence dimensions required to distinguish identity from coincidence.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BindingEvidenceDimensions {
    pub who: bool,
    #[serde(rename = "where")]
    pub where_: bool,
    pub slot: bool,
    pub row_or_basis: bool,
    pub audio_identity: bool,
    pub context: bool,
    pub time: bool,
    pub repeatability: bool,
    pub negative_control: bool,
    pub cross_state: bool,
}

impl BindingEvidenceDimensions {
    fn missing_from(self, requirements: Self) -> Vec<&'static str> {
        let fields = [
            (requirements.who && !self.who, "who"),
            (requirements.where_ && !self.where_, "where"),
            (requirements.slot && !self.slot, "slot"),
            (
                requirements.row_or_basis && !self.row_or_basis,
                "row_or_basis",
            ),
            (
                requirements.audio_identity && !self.audio_identity,
                "audio_identity",
            ),
            (requirements.context && !self.context, "context"),
            (requirements.time && !self.time, "time"),
            (
                requirements.repeatability && !self.repeatability,
                "repeatability",
            ),
            (
                requirements.negative_control && !self.negative_control,
                "negative_control",
            ),
            (requirements.cross_state && !self.cross_state, "cross_state"),
        ];
        fields
            .into_iter()
            .filter_map(|(missing, name)| missing.then_some(name))
            .collect()
    }
}

/// A machine-readable, falsifiable evidence package for one proposed
/// semantic relation.  Strings are intentionally evidence references rather
/// than semantic guesses; contributors must point to reports, fixtures, or
/// public text that another person can inspect.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SemanticBindingEvidence {
    pub relation: BindingRelationKind,
    pub scope: String,
    pub evidence_class: BindingEvidenceClass,
    pub provenance: BindingProvenance,
    pub supporting_observations: Vec<String>,
    pub contradictions: Vec<String>,
    pub negative_controls: Vec<String>,
    pub producer_constraints: Vec<String>,
    pub dimensions: BindingEvidenceDimensions,
    pub falsifier: String,
    #[serde(default)]
    admission_status: BindingAdmissionStatus,
}

impl SemanticBindingEvidence {
    /// Creates a not-yet-admitted evidence record.
    pub fn new(
        relation: BindingRelationKind,
        scope: impl Into<String>,
        evidence_class: BindingEvidenceClass,
        provenance: BindingProvenance,
    ) -> Self {
        Self {
            relation,
            scope: scope.into(),
            evidence_class,
            provenance,
            supporting_observations: Vec::new(),
            contradictions: Vec::new(),
            negative_controls: Vec::new(),
            producer_constraints: Vec::new(),
            dimensions: BindingEvidenceDimensions::default(),
            falsifier: String::new(),
            admission_status: BindingAdmissionStatus::NotAdmitted,
        }
    }

    pub fn admission_status(&self) -> BindingAdmissionStatus {
        self.admission_status
    }

    /// Checks whether the package is strong enough to mint a future verified
    /// admission token.  The token is deliberately separate from the current
    /// production scene state; J1R13 does not admit any real binding.
    pub fn try_admit(
        &self,
        requirements: &BindingAdmissionRequirements,
    ) -> Result<VerifiedBindingAdmission, BindingAdmissionError> {
        if self.evidence_class != BindingEvidenceClass::Verified {
            return Err(BindingAdmissionError::EvidenceClassNotVerified {
                actual: self.evidence_class,
            });
        }
        let missing = self.dimensions.missing_from(requirements.dimensions);
        if !missing.is_empty() {
            return Err(BindingAdmissionError::MissingDimensions { fields: missing });
        }
        if self.supporting_observations.is_empty() {
            return Err(BindingAdmissionError::MissingSupportingObservations);
        }
        if self.negative_controls.is_empty() {
            return Err(BindingAdmissionError::MissingNegativeControls);
        }
        if self.producer_constraints.is_empty() {
            return Err(BindingAdmissionError::MissingProducerConstraints);
        }
        if self.falsifier.trim().is_empty() {
            return Err(BindingAdmissionError::MissingFalsifier);
        }
        if !self.contradictions.is_empty() {
            return Err(BindingAdmissionError::ContradictionsRemain);
        }
        Ok(VerifiedBindingAdmission {
            relation: self.relation,
            scope: self.scope.clone(),
        })
    }
}

/// Minimum dimensions for a future verified semantic binding.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BindingAdmissionRequirements {
    pub dimensions: BindingEvidenceDimensions,
}

impl Default for BindingAdmissionRequirements {
    fn default() -> Self {
        Self {
            dimensions: BindingEvidenceDimensions {
                who: true,
                where_: true,
                slot: true,
                row_or_basis: true,
                audio_identity: true,
                context: true,
                time: true,
                repeatability: true,
                negative_control: true,
                cross_state: true,
            },
        }
    }
}

/// A private-field capability proving that the admission checks passed.
/// There is intentionally no conversion from this token to the current scene
/// state: no real J1R13 evidence is verified.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedBindingAdmission {
    relation: BindingRelationKind,
    scope: String,
}

impl VerifiedBindingAdmission {
    pub fn relation(&self) -> BindingRelationKind {
        self.relation
    }

    pub fn scope(&self) -> &str {
        &self.scope
    }
}

/// Explicit rejection reasons for unsupported semantic admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BindingAdmissionError {
    EvidenceClassNotVerified { actual: BindingEvidenceClass },
    MissingDimensions { fields: Vec<&'static str> },
    MissingSupportingObservations,
    MissingNegativeControls,
    MissingProducerConstraints,
    MissingFalsifier,
    ContradictionsRemain,
}

impl fmt::Display for BindingAdmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EvidenceClassNotVerified { actual } => {
                write!(formatter, "evidence class {actual:?} is not verified")
            }
            Self::MissingDimensions { fields } => {
                write!(
                    formatter,
                    "missing required evidence dimensions: {fields:?}"
                )
            }
            Self::MissingSupportingObservations => {
                formatter.write_str("supporting observations are required")
            }
            Self::MissingNegativeControls => formatter.write_str("negative controls are required"),
            Self::MissingProducerConstraints => {
                formatter.write_str("producer/carrier constraints are required")
            }
            Self::MissingFalsifier => formatter.write_str("a falsifier is required"),
            Self::ContradictionsRemain => formatter.write_str("contradictions remain"),
        }
    }
}

impl Error for BindingAdmissionError {}
