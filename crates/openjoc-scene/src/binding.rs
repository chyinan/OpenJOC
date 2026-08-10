//! Evidence and admission contract for semantic object-to-audio binding.
//!
//! The decoder can expose metadata objects and reconstruction rows without
//! claiming that either side identifies the other.  This module keeps the
//! evidence taxonomy separate from [`SemanticBindingState`], so a structural
//! or empirical observation cannot silently become an audio binding.

use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

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
    ControlledCleanroomEmpirical,
    StructuralImplementation,
    OtherAllowed,
}

/// Report-level admission status.  It is not a replacement for the
/// production [`super::SemanticBindingState`], which remains unresolved until
/// a future implementation adds an explicit state transition.
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
