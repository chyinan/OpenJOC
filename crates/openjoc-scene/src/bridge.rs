// pattern: Functional Core

//! Codec-domain input boundary for a future JOC spatial reconstruction operator.
//!
//! This module intentionally stops before authored-object binding.  A
//! [`JocSpatialReconstructionFrame`] carries the decoder coordinates, timing,
//! and metadata needed by a future operator, while [`JocSpatialOperatorState`]
//! makes the unresolved semantic boundary explicit.

use crate::{DecodedPayloadFrame, ProgrammeLayout, SemanticBindingState};
use openjoc_joc::ReconstructionBasis;
use openjoc_oamd::OamdPayload;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt};

#[path = "joc_spatial_bridge.rs"]
mod joc_spatial_bridge;
pub use joc_spatial_bridge::{
    GainScheduler, GainSchedulerError, JOC_SPATIAL_BRIDGE_SCHEMA, JocSpatialBridge,
    SpatialBindingError, SpatialBindingRecord, SpatialBindingResult, SpatialBindingSnapshot,
    SpatialBindingState, SpatialBindingTransition, SpatialBridgeError, SpatialCoordinateUpdate,
    SpatialDescriptor, SpatialDescriptorPatch, SpatialExplicitGroup, SpatialExplicitMember,
    SpatialLayout, SpatialLayoutAlias, SpatialLayoutAnchor, SpatialLayoutChannel,
    SpatialLayoutLayer, SpatialLayoutNode, SpatialLayoutRow, SpatialLayoutTopology,
    SpatialPairedGeometry, SpatialProjectionError, SpatialProjectionOutcome, SpatialRouteVector,
    SpatialSourceClass, SpatialSpreadProfile, SpatialSpreadSample, SpatialTopologySnapshot,
};

/// Versioned schema name for the borrowed codec-domain bridge contract.
pub const JOC_SPATIAL_RECONSTRUCTION_SCHEMA: &str = "openjoc.joc-spatial-reconstruction.v1";

/// An absolute half-open sample interval.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SampleRange {
    pub start_sample: u64,
    pub end_sample: u64,
}

impl SampleRange {
    /// Creates a range, rejecting reversed endpoints.
    pub fn new(start_sample: u64, end_sample: u64) -> Result<Self, BridgeError> {
        if end_sample < start_sample {
            return Err(BridgeError::InvalidSampleRange {
                start_sample,
                end_sample,
            });
        }
        Ok(Self {
            start_sample,
            end_sample,
        })
    }

    /// Number of samples in the range.
    #[must_use]
    pub const fn len(self) -> u64 {
        self.end_sample - self.start_sample
    }

    /// Whether the interval contains no samples.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start_sample == self.end_sample
    }
}

/// A stable label for one decoded Base full-band coordinate.
///
/// These labels identify codec coordinates only.  They do not identify an
/// authored object or a renderer output channel.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BaseFullBandCoordinate {
    Left,
    Right,
    Centre,
    LeftSurround,
    RightSurround,
    LeftBack,
    RightBack,
    TopFrontLeft,
    TopFrontRight,
    Other(u8),
}

/// Expert-only PCM contribution selection for spatial-fidelity diagnostics.
///
/// This mode never changes codec-coordinate topology, binding, metadata, or
/// scheduler state. It only replaces the selected coordinate family's PCM
/// planes with exact zeroes at the spatial accumulation boundary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SpatialContributionMode {
    /// Preserve Base and ReconstructionBasis coordinate PCM.
    #[default]
    Full,
    /// Preserve Base PCM and zero ReconstructionBasis PCM.
    BaseOnly,
    /// Zero Base PCM and preserve ReconstructionBasis PCM.
    ReconstructionOnly,
}

impl SpatialContributionMode {
    /// Stable diagnostic CLI/report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::BaseOnly => "base-only",
            Self::ReconstructionOnly => "reconstruction-only",
        }
    }

    #[must_use]
    pub const fn includes_base(self) -> bool {
        !matches!(self, Self::ReconstructionOnly)
    }

    #[must_use]
    pub const fn includes_reconstruction(self) -> bool {
        !matches!(self, Self::BaseOnly)
    }
}

/// Typed reasons why a JOC spatial operator is not ready for rendering.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JocSpatialOperatorUnresolvedReason {
    MissingNormativeSyntaxInput,
    ParserInputNotImplemented,
    ReconstructionEquationNotEstablished,
    ReservedOrUnsupportedSyntax,
    #[serde(alias = "experimental_semantic_ambiguity")]
    SemanticAmbiguity,
}

/// Readiness state for the codec-to-spatial reconstruction boundary.
///
/// There is deliberately no constructible resolved variant in this release:
/// the public API cannot accidentally manufacture a semantic operator.  A
/// future milestone may add a resolved state only after the operator contract
/// and its independent evidence are admitted.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum JocSpatialOperatorState {
    Unresolved {
        reason: JocSpatialOperatorUnresolvedReason,
    },
}

impl JocSpatialOperatorState {
    /// The current production state: decoded rows are codec coordinates and
    /// no authored-object reconstruction operator has been admitted.
    #[must_use]
    pub const fn current() -> Self {
        Self::Unresolved {
            reason: JocSpatialOperatorUnresolvedReason::ReconstructionEquationNotEstablished,
        }
    }

    #[must_use]
    pub const fn is_unresolved(self) -> bool {
        matches!(self, Self::Unresolved { .. })
    }
}

/// Borrowed Base/RB/RcLfe coordinates for one decoder frame.
#[derive(Clone, Copy, Debug)]
pub struct CodecBasisBlock<'a> {
    pub base_full_band_coordinates: &'a [BaseFullBandCoordinate],
    pub base_full_band_pcm: &'a [Vec<f64>],
    pub reconstruction_basis: &'a ReconstructionBasis,
    /// Base-carried LFE/RcLfe, kept outside the dynamic RB coordinate set.
    pub rclfe_pcm: Option<&'a [f64]>,
}

/// Metadata references aligned to the same decoder frame.
#[derive(Clone, Copy, Debug)]
pub struct JocSpatialMetadataFrame<'a> {
    pub oamd: &'a OamdPayload,
    /// Structural programme layout only; it is not an audio binding.
    pub programme_layout: &'a ProgrammeLayout,
}

/// One time-aligned codec-domain frame supplied to a future operator.
#[derive(Clone, Copy, Debug)]
pub struct JocSpatialReconstructionFrame<'a> {
    pub frame_index: u64,
    pub sample_rate: u32,
    pub sample_range: SampleRange,
    pub basis: CodecBasisBlock<'a>,
    pub metadata: JocSpatialMetadataFrame<'a>,
    pub operator_state: JocSpatialOperatorState,
    pub semantic_binding: SemanticBindingState,
}

impl<'a> JocSpatialReconstructionFrame<'a> {
    /// Builds a bridge frame from one committed decoder result and borrowed
    /// current-frame Base input. No PCM is copied and no object scene is made.
    pub fn from_decoded(
        decoded: &'a DecodedPayloadFrame,
        base_full_band_coordinates: &'a [BaseFullBandCoordinate],
        base_full_band_pcm: &'a [Vec<f64>],
        rclfe_pcm: Option<&'a [f64]>,
    ) -> Result<Self, BridgeError> {
        let basis = CodecBasisBlock {
            base_full_band_coordinates,
            base_full_band_pcm,
            reconstruction_basis: &decoded.decoded.reconstruction_basis,
            rclfe_pcm,
        };
        validate_basis(&basis, decoded.sample_range)?;
        Ok(Self {
            frame_index: decoded.frame_index,
            sample_rate: decoded.sample_rate,
            sample_range: decoded.sample_range,
            basis,
            metadata: JocSpatialMetadataFrame {
                oamd: &decoded.oamd,
                programme_layout: &decoded.programme_layout,
            },
            operator_state: JocSpatialOperatorState::current(),
            semantic_binding: SemanticBindingState::Unresolved,
        })
    }

    /// Dynamic OAMD cardinality is a dimensional observation only.
    #[must_use]
    pub const fn dynamic_metadata_count(&self) -> usize {
        self.metadata.programme_layout.dynamic_slot_count
    }

    /// Current RB row count, without assigning rows to metadata objects.
    #[must_use]
    pub fn reconstruction_basis_count(&self) -> usize {
        self.basis.reconstruction_basis.rows.len()
    }

    /// Explicit hard gate for future renderer composition.
    pub fn require_resolved_operator(&self) -> Result<(), BridgeError> {
        Err(BridgeError::OperatorUnresolved {
            reason: match self.operator_state {
                JocSpatialOperatorState::Unresolved { reason } => reason,
            },
        })
    }
}

/// Narrow bridge facade used by streaming consumers.
#[derive(Clone, Copy, Debug, Default)]
pub struct JocSpatialFrameBridge;

impl JocSpatialFrameBridge {
    /// Creates the codec-domain frame without retaining duration-proportional
    /// PCM. The caller may immediately consume the returned borrowed frame.
    pub fn frame<'a>(
        &self,
        decoded: &'a DecodedPayloadFrame,
        base_full_band_coordinates: &'a [BaseFullBandCoordinate],
        base_full_band_pcm: &'a [Vec<f64>],
        rclfe_pcm: Option<&'a [f64]>,
    ) -> Result<JocSpatialReconstructionFrame<'a>, BridgeError> {
        JocSpatialReconstructionFrame::from_decoded(
            decoded,
            base_full_band_coordinates,
            base_full_band_pcm,
            rclfe_pcm,
        )
    }
}

/// Bridge validation failures. These fail before any semantic output exists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BridgeError {
    InvalidSampleRange {
        start_sample: u64,
        end_sample: u64,
    },
    EmptyBaseCoordinates,
    BaseCoordinateCountMismatch {
        coordinates: usize,
        channels: usize,
    },
    DuplicateBaseCoordinate {
        coordinate: BaseFullBandCoordinate,
    },
    BaseFrameLengthMismatch {
        expected: usize,
        actual: usize,
    },
    ReconstructionFrameLengthMismatch {
        row_index: usize,
        expected: usize,
        actual: usize,
    },
    RclfeFrameLengthMismatch {
        expected: usize,
        actual: usize,
    },
    NonFiniteBase {
        channel: usize,
        sample: usize,
    },
    NonFiniteReconstruction {
        row: usize,
        sample: usize,
    },
    NonFiniteRclfe {
        sample: usize,
    },
    OperatorUnresolved {
        reason: JocSpatialOperatorUnresolvedReason,
    },
}

impl fmt::Display for BridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSampleRange {
                start_sample,
                end_sample,
            } => write!(
                formatter,
                "invalid sample range [{start_sample},{end_sample})"
            ),
            Self::EmptyBaseCoordinates => formatter.write_str("Base coordinate list is empty"),
            Self::BaseCoordinateCountMismatch {
                coordinates,
                channels,
            } => write!(
                formatter,
                "{coordinates} Base coordinates for {channels} channels"
            ),
            Self::DuplicateBaseCoordinate { coordinate } => {
                write!(formatter, "duplicate Base coordinate {coordinate:?}")
            }
            Self::BaseFrameLengthMismatch { expected, actual } => {
                write!(
                    formatter,
                    "Base frame has {actual} samples; expected {expected}"
                )
            }
            Self::ReconstructionFrameLengthMismatch {
                row_index,
                expected,
                actual,
            } => write!(
                formatter,
                "ReconstructionBasis row {row_index} has {actual} samples; expected {expected}"
            ),
            Self::RclfeFrameLengthMismatch { expected, actual } => {
                write!(
                    formatter,
                    "RcLfe frame has {actual} samples; expected {expected}"
                )
            }
            Self::NonFiniteBase { channel, sample } => {
                write!(
                    formatter,
                    "Base channel {channel} has non-finite sample {sample}"
                )
            }
            Self::NonFiniteReconstruction { row, sample } => {
                write!(
                    formatter,
                    "ReconstructionBasis row {row} has non-finite sample {sample}"
                )
            }
            Self::NonFiniteRclfe { sample } => {
                write!(formatter, "RcLfe has non-finite sample {sample}")
            }
            Self::OperatorUnresolved { reason } => {
                write!(formatter, "JOC spatial operator is unresolved: {reason:?}")
            }
        }
    }
}

impl std::error::Error for BridgeError {}

fn validate_basis(
    basis: &CodecBasisBlock<'_>,
    sample_range: SampleRange,
) -> Result<(), BridgeError> {
    if basis.base_full_band_coordinates.is_empty() {
        return Err(BridgeError::EmptyBaseCoordinates);
    }
    if basis.base_full_band_coordinates.len() != basis.base_full_band_pcm.len() {
        return Err(BridgeError::BaseCoordinateCountMismatch {
            coordinates: basis.base_full_band_coordinates.len(),
            channels: basis.base_full_band_pcm.len(),
        });
    }
    let mut seen = HashSet::with_capacity(basis.base_full_band_coordinates.len());
    for &coordinate in basis.base_full_band_coordinates {
        if !seen.insert(coordinate) {
            return Err(BridgeError::DuplicateBaseCoordinate { coordinate });
        }
    }
    let expected = usize::try_from(sample_range.len()).unwrap_or(usize::MAX);
    for (channel, pcm) in basis.base_full_band_pcm.iter().enumerate() {
        if pcm.len() != expected {
            return Err(BridgeError::BaseFrameLengthMismatch {
                expected,
                actual: pcm.len(),
            });
        }
        if let Some(sample) = pcm.iter().position(|value| !value.is_finite()) {
            return Err(BridgeError::NonFiniteBase { channel, sample });
        }
    }
    for (row, pcm) in basis.reconstruction_basis.rows.iter().enumerate() {
        if pcm.len() != expected {
            return Err(BridgeError::ReconstructionFrameLengthMismatch {
                row_index: row,
                expected,
                actual: pcm.len(),
            });
        }
        if let Some(sample) = pcm.iter().position(|value| !value.is_finite()) {
            return Err(BridgeError::NonFiniteReconstruction { row, sample });
        }
    }
    if let Some(pcm) = basis.rclfe_pcm {
        if pcm.len() != expected {
            return Err(BridgeError::RclfeFrameLengthMismatch {
                expected,
                actual: pcm.len(),
            });
        }
        if let Some(sample) = pcm.iter().position(|value| !value.is_finite()) {
            return Err(BridgeError::NonFiniteRclfe { sample });
        }
    }
    Ok(())
}
