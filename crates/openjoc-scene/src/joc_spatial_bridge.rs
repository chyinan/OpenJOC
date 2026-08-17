//! JOC spatial bridge implementation.
//!
//! The types in this module are deliberately separate from the metadata-only
//! scene model. They describe codec-coordinate records and public layout
//! registries, not authored objects. The module is downstream of the existing
//! ETSI/vendor validation profiles; it does not alter parser policy.

use super::{JocSpatialReconstructionFrame, SpatialContributionMode};
use crate::{RegionSemanticState, RegionTopologySelector, SemanticBindingState};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt};

/// Public schema label for the JOC spatial bridge.
pub const JOC_SPATIAL_BRIDGE_SCHEMA: &str = "openjoc.joc-spatial-bridge.v1";

const Q32: usize = 32;
const Q32_HALF_MINUS_ONE: u64 = 15;
const Q: f64 = 32_768.0;
const QMAX_Q15: f64 = 32_767.0;
const QMAX: f64 = QMAX_Q15 / Q;
const EPS_ACTIVITY: f64 = 0.000_001;
const EPS_DELTA: f64 = 0.000_1;
const SUM_TOLERANCE: f64 = 1.0e-9;
const CHANNEL_LOCK_THRESHOLD_SQUARED: f64 = 0.04;

/// Spatial descriptor dispatch classes from the implementation bundle.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpatialSourceClass {
    Inactive,
    ExplicitChannel,
    DynamicPoint,
    DynamicRegion,
    FixedLayout,
    NamedLayout,
    /// An explicit unsupported class. It is rejected instead of guessed.
    Unsupported(String),
}

/// Validated neutral selector for the Fixed source family.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct FixedFamilyId(u8);

impl FixedFamilyId {
    /// Creates a Fixed family selector in the closed `5..=10` domain.
    #[must_use]
    pub const fn new(value: u8) -> Option<Self> {
        if value >= 5 && value <= 10 {
            Some(Self(value))
        } else {
            None
        }
    }

    /// Returns the neutral family selector.
    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }

    const fn member_range(self) -> (u8, u8) {
        match self.0 {
            5 => (1, 4),
            6 => (5, 12),
            7 => (13, 22),
            8 => (23, 36),
            9 => (37, 51),
            10 => (52, 81),
            _ => (0, 0),
        }
    }
}

/// Neutral global member selector used by a [`FixedRouteKey`].
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct FixedMemberId(u8);

impl FixedMemberId {
    /// Creates a member selector in the closed global `1..=81` domain.
    #[must_use]
    pub const fn new(value: u8) -> Option<Self> {
        if value >= 1 && value <= 81 {
            Some(Self(value))
        } else {
            None
        }
    }

    /// Returns the neutral global member selector.
    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }
}

/// Validated Fixed family/member identity.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct FixedRouteKey {
    family: FixedFamilyId,
    member: FixedMemberId,
}

impl FixedRouteKey {
    /// Creates a key after validating family and global member domains.
    #[must_use]
    pub const fn new(family: u8, member: u8) -> Option<Self> {
        let Some(family) = FixedFamilyId::new(family) else {
            return None;
        };
        let Some(member) = FixedMemberId::new(member) else {
            return None;
        };
        let (first, last) = family.member_range();
        if member.0 < first || member.0 > last {
            return None;
        }
        Some(Self { family, member })
    }

    /// Returns the validated family selector.
    #[must_use]
    pub const fn family(self) -> FixedFamilyId {
        self.family
    }

    /// Returns the validated member selector.
    #[must_use]
    pub const fn member(self) -> FixedMemberId {
        self.member
    }

    /// Returns the neutral route-table identity for this key.
    #[must_use]
    pub fn identity(self) -> String {
        format!("fixed/{}/{}", self.family.value(), self.member.value())
    }
}

/// Validated opaque Named semantic identity.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct NamedTargetId(u8);

impl NamedTargetId {
    /// Creates an admitted opaque Named identity in the `0..=15` domain.
    #[must_use]
    pub const fn new(value: u8) -> Option<Self> {
        if value < 16 { Some(Self(value)) } else { None }
    }

    /// Returns the neutral ID value.
    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }

    /// Returns the canonical routing-domain slot for this ID.
    #[must_use]
    pub const fn canonical_route_slot(self) -> u8 {
        const SLOTS: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 13, 14, 17, 18, 21, 22, 11, 12];
        SLOTS[self.0 as usize]
    }

    /// Returns the neutral route-table identity for this ID.
    #[must_use]
    pub fn identity(self) -> String {
        format!("named/{}", self.value())
    }
}

/// Classification of a discrete route lookup.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SpatialRouteStatus {
    DirectReady,
    FallbackReady,
    FallbackWithheld,
    Unsupported,
    Unresolved,
}

impl SpatialRouteStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DirectReady => "direct_ready",
            Self::FallbackReady => "fallback_ready",
            Self::FallbackWithheld => "fallback_withheld",
            Self::Unsupported => "unsupported",
            Self::Unresolved => "unresolved",
        }
    }
}

/// One finite public spread sample and its normalized weight.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpatialSpreadSample {
    pub position: Vec<f64>,
    pub weight: f64,
}

/// Public finite spread profile for a region descriptor.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SpatialSpreadProfile {
    pub samples: Vec<SpatialSpreadSample>,
}

/// Public paired geometry used by the equal-power pair operator.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpatialPairedGeometry {
    pub first: Vec<f64>,
    pub second: Vec<f64>,
    pub blend: f64,
}

/// One effective spatial descriptor. `raw3` is retained but never used in
/// projection arithmetic.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpatialDescriptor {
    pub source_class: SpatialSourceClass,
    pub identity: String,
    pub coordinates: Vec<f64>,
    pub spread: Option<SpatialSpreadProfile>,
    pub paired: Option<SpatialPairedGeometry>,
    /// Effective semantic Q15 half-span for the standalone Dynamic Pair mode.
    /// This is intentionally separate from the caller-defined vector-pair API.
    #[serde(default)]
    pub pair_span_q15: Option<u16>,
    pub raw3: Option<Vec<u8>>,
    /// Quantized decoded extent retained at the bridge boundary. The current
    /// projection contract does not use this field for P(d,L).
    #[serde(default)]
    pub extent: Option<[f64; 3]>,
    /// Decoded horizontal/elevation zone enables. The current projection
    /// contract validates these values before selecting a region topology.
    #[serde(default)]
    pub zones: Option<[bool; 6]>,
    /// Dynamic-point ChannelLock is applied at the target-generation boundary
    /// after effective Region topology selection; the ordinary projector
    /// itself does not consume this field.
    #[serde(default)]
    pub channel_lock: bool,
}

impl SpatialDescriptor {
    /// Creates a descriptor without optional spread, pair, or opaque data.
    #[must_use]
    pub fn new(
        source_class: SpatialSourceClass,
        identity: impl Into<String>,
        coordinates: Vec<f64>,
    ) -> Self {
        Self {
            source_class,
            identity: identity.into(),
            coordinates,
            spread: None,
            paired: None,
            pair_span_q15: None,
            raw3: None,
            extent: None,
            zones: None,
            channel_lock: false,
        }
    }

    /// Creates a Fixed descriptor from a validated neutral route key.
    #[must_use]
    pub fn fixed(key: FixedRouteKey, coordinates: Vec<f64>) -> Self {
        Self::new(SpatialSourceClass::FixedLayout, key.identity(), coordinates)
    }

    /// Creates a Named descriptor from a validated opaque target ID.
    #[must_use]
    pub fn named(target: NamedTargetId, coordinates: Vec<f64>) -> Self {
        Self::new(
            SpatialSourceClass::NamedLayout,
            target.identity(),
            coordinates,
        )
    }
}

/// One codec-coordinate record. The ordinal is assigned only after topology
/// flattening; it is never authored-object identity.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpatialBindingRecord {
    pub descriptor: SpatialDescriptor,
    pub scalar: f64,
    pub active: bool,
}

/// A labeled member of an explicit topology group.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpatialExplicitMember {
    pub canonical_label: String,
    pub record: SpatialBindingRecord,
}

/// An explicit group with deterministic group and label ordering.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpatialExplicitGroup {
    pub group_order: u32,
    pub members: Vec<SpatialExplicitMember>,
}

/// A valid topology snapshot in the ordinary spatial domain.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SpatialTopologySnapshot {
    pub explicit_groups: Vec<SpatialExplicitGroup>,
    pub fixed_layout: Vec<SpatialBindingRecord>,
    pub dynamic_records: Vec<SpatialBindingRecord>,
}

impl SpatialTopologySnapshot {
    /// Flattens the declared domain using the spatial ordering rule.
    #[must_use]
    pub fn flatten(&self) -> Vec<SpatialBindingRecord> {
        let mut groups = self.explicit_groups.clone();
        groups.sort_by_key(|group| group.group_order);
        let mut records = Vec::new();
        for mut group in groups {
            group
                .members
                .sort_by(|left, right| left.canonical_label.cmp(&right.canonical_label));
            records.extend(group.members.into_iter().map(|member| member.record));
        }
        records.extend(self.fixed_layout.iter().cloned());
        records.extend(self.dynamic_records.iter().cloned());
        records
    }
}

/// Present-field patch for one same-coordinate block update.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SpatialDescriptorPatch {
    pub source_class: Option<SpatialSourceClass>,
    pub identity: Option<String>,
    pub coordinates: Option<Vec<f64>>,
    pub spread: Option<Option<SpatialSpreadProfile>>,
    pub paired: Option<Option<SpatialPairedGeometry>>,
    #[serde(default)]
    pub pair_span_q15: Option<Option<u16>>,
    pub raw3: Option<Option<Vec<u8>>>,
    pub extent: Option<Option<[f64; 3]>>,
    pub zones: Option<Option<[bool; 6]>>,
    pub channel_lock: Option<Option<bool>>,
}

/// Selective block update. Absent fields inherit from the current coordinate.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpatialCoordinateUpdate {
    pub ordinal: usize,
    pub descriptor: Option<SpatialDescriptorPatch>,
    pub scalar: Option<f64>,
    pub active: Option<bool>,
}

/// Binding state machine transitions for the spatial topology.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpatialBindingTransition {
    Init,
    Stable,
    Reuse,
    Rebuild,
}

/// Effective binding snapshot keyed by `(topology_epoch, ordinal)`.
#[derive(Clone, Debug, PartialEq)]
pub struct SpatialBindingSnapshot {
    pub topology_epoch: u64,
    pub records: Vec<SpatialBindingRecord>,
    pub active_count: usize,
}

/// Result of applying a topology/payload event to the binding state machine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpatialBindingResult {
    pub transition: SpatialBindingTransition,
    pub event: bool,
}

/// Binding-state failures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpatialBindingError {
    NoTopologyForInitialization,
    EmptyTopology,
    UnsupportedSourceClass(String),
    InvalidRegionState([bool; 6]),
    InvalidExtent { ordinal: usize },
    NonFiniteScalar { ordinal: usize },
    UpdateOrdinalOutOfRange { ordinal: usize, count: usize },
    TopologyEpochOverflow,
}

impl fmt::Display for SpatialBindingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoTopologyForInitialization => {
                formatter.write_str("spatial binding requires an initial valid topology")
            }
            Self::EmptyTopology => formatter.write_str("spatial topology must contain a record"),
            Self::UnsupportedSourceClass(class) => {
                write!(formatter, "unsupported spatial source class: {class}")
            }
            Self::InvalidRegionState(zones) => {
                write!(formatter, "invalid semantic region state {zones:?}")
            }
            Self::InvalidExtent { ordinal } => {
                write!(
                    formatter,
                    "invalid semantic extent at spatial coordinate {ordinal}"
                )
            }
            Self::NonFiniteScalar { ordinal } => {
                write!(
                    formatter,
                    "non-finite scalar at spatial coordinate {ordinal}"
                )
            }
            Self::UpdateOrdinalOutOfRange { ordinal, count } => write!(
                formatter,
                "spatial update ordinal {ordinal} is outside record count {count}"
            ),
            Self::TopologyEpochOverflow => formatter.write_str("spatial topology epoch overflow"),
        }
    }
}

impl std::error::Error for SpatialBindingError {}

/// Stateful spatial codec-coordinate binding.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpatialBindingState {
    snapshot: Option<SpatialBindingSnapshot>,
}

impl SpatialBindingState {
    /// Creates an empty binding state.
    #[must_use]
    pub const fn new() -> Self {
        Self { snapshot: None }
    }

    /// Applies a full topology snapshot and/or same-coordinate block updates.
    /// `None, None` is the spatial no-new-payload reuse event.
    pub fn apply(
        &mut self,
        topology: Option<&SpatialTopologySnapshot>,
        updates: Option<&[SpatialCoordinateUpdate]>,
        pcm_count: usize,
    ) -> Result<SpatialBindingResult, SpatialBindingError> {
        let mut candidate = self.clone();
        let result = candidate.apply_inner(topology, updates, pcm_count)?;
        *self = candidate;
        Ok(result)
    }

    fn apply_inner(
        &mut self,
        topology: Option<&SpatialTopologySnapshot>,
        updates: Option<&[SpatialCoordinateUpdate]>,
        pcm_count: usize,
    ) -> Result<SpatialBindingResult, SpatialBindingError> {
        let mut result = if let Some(topology) = topology {
            let records = topology.flatten();
            validate_records(&records)?;
            if records.is_empty() {
                return Err(SpatialBindingError::EmptyTopology);
            }
            match self.snapshot.as_ref() {
                None => {
                    self.snapshot = Some(SpatialBindingSnapshot {
                        topology_epoch: 1,
                        records,
                        active_count: 0,
                    });
                    SpatialBindingResult {
                        transition: SpatialBindingTransition::Init,
                        event: true,
                    }
                }
                Some(previous) => {
                    let transition =
                        if binding_signature(&previous.records) == binding_signature(&records) {
                            SpatialBindingTransition::Stable
                        } else {
                            SpatialBindingTransition::Rebuild
                        };
                    let epoch = if transition == SpatialBindingTransition::Rebuild {
                        previous
                            .topology_epoch
                            .checked_add(1)
                            .ok_or(SpatialBindingError::TopologyEpochOverflow)?
                    } else {
                        previous.topology_epoch
                    };
                    self.snapshot = Some(SpatialBindingSnapshot {
                        topology_epoch: epoch,
                        records,
                        active_count: 0,
                    });
                    SpatialBindingResult {
                        transition,
                        event: true,
                    }
                }
            }
        } else if self.snapshot.is_some() {
            SpatialBindingResult {
                transition: if updates.is_some() {
                    SpatialBindingTransition::Stable
                } else {
                    SpatialBindingTransition::Reuse
                },
                event: updates.is_some(),
            }
        } else {
            return Err(SpatialBindingError::NoTopologyForInitialization);
        };

        if let Some(updates) = updates {
            let Some(snapshot) = self.snapshot.as_mut() else {
                return Err(SpatialBindingError::NoTopologyForInitialization);
            };
            let prior_signature = binding_signature(&snapshot.records);
            for update in updates {
                apply_update(snapshot, update)?;
            }
            let next_signature = binding_signature(&snapshot.records);
            if prior_signature != next_signature {
                if result.transition != SpatialBindingTransition::Init {
                    snapshot.topology_epoch = snapshot
                        .topology_epoch
                        .checked_add(1)
                        .ok_or(SpatialBindingError::TopologyEpochOverflow)?;
                }
                result.transition = SpatialBindingTransition::Rebuild;
            }
            result.event = true;
        }

        if let Some(snapshot) = self.snapshot.as_mut() {
            snapshot.active_count = pcm_count.min(snapshot.records.len());
        }
        Ok(result)
    }

    /// Clears the current epoch, records, and keys.
    pub fn reset(&mut self) {
        self.snapshot = None;
    }

    /// Returns the current effective snapshot.
    #[must_use]
    pub const fn snapshot(&self) -> Option<&SpatialBindingSnapshot> {
        self.snapshot.as_ref()
    }
}

fn validate_records(records: &[SpatialBindingRecord]) -> Result<(), SpatialBindingError> {
    for (ordinal, record) in records.iter().enumerate() {
        if !record.scalar.is_finite() {
            return Err(SpatialBindingError::NonFiniteScalar { ordinal });
        }
        if let SpatialSourceClass::Unsupported(class) = &record.descriptor.source_class {
            return Err(SpatialBindingError::UnsupportedSourceClass(class.clone()));
        }
        if let Some(zones) = record.descriptor.zones {
            RegionSemanticState::from_decoded_zones(zones)
                .map_err(|_| SpatialBindingError::InvalidRegionState(zones))?;
        }
        if let Some(extent) = record.descriptor.extent {
            crate::extent::extent_scalar(extent)
                .map_err(|_| SpatialBindingError::InvalidExtent { ordinal })?;
        }
    }
    Ok(())
}

fn apply_update(
    snapshot: &mut SpatialBindingSnapshot,
    update: &SpatialCoordinateUpdate,
) -> Result<(), SpatialBindingError> {
    let count = snapshot.records.len();
    let record = snapshot.records.get_mut(update.ordinal).ok_or(
        SpatialBindingError::UpdateOrdinalOutOfRange {
            ordinal: update.ordinal,
            count,
        },
    )?;
    if let Some(patch) = &update.descriptor {
        if let Some(source_class) = &patch.source_class {
            if let SpatialSourceClass::Unsupported(class) = source_class {
                return Err(SpatialBindingError::UnsupportedSourceClass(class.clone()));
            }
            record.descriptor.source_class = source_class.clone();
        }
        if let Some(identity) = &patch.identity {
            record.descriptor.identity.clone_from(identity);
        }
        if let Some(coordinates) = &patch.coordinates {
            record.descriptor.coordinates.clone_from(coordinates);
        }
        if let Some(spread) = &patch.spread {
            record.descriptor.spread.clone_from(spread);
        }
        if let Some(paired) = &patch.paired {
            record.descriptor.paired.clone_from(paired);
        }
        if let Some(pair_span_q15) = patch.pair_span_q15 {
            record.descriptor.pair_span_q15 = pair_span_q15;
        }
        if let Some(raw3) = &patch.raw3 {
            record.descriptor.raw3.clone_from(raw3);
        }
        if let Some(extent) = &patch.extent {
            if let Some(extent) = extent {
                crate::extent::extent_scalar(*extent).map_err(|_| {
                    SpatialBindingError::InvalidExtent {
                        ordinal: update.ordinal,
                    }
                })?;
            }
            record.descriptor.extent = *extent;
        }
        if let Some(zones) = &patch.zones {
            if let Some(zones) = zones {
                RegionSemanticState::from_decoded_zones(*zones)
                    .map_err(|_| SpatialBindingError::InvalidRegionState(*zones))?;
            }
            record.descriptor.zones = *zones;
        }
        if let Some(channel_lock) = &patch.channel_lock {
            record.descriptor.channel_lock = channel_lock.unwrap_or(false);
        }
    }
    if let Some(scalar) = update.scalar {
        if !scalar.is_finite() {
            return Err(SpatialBindingError::NonFiniteScalar {
                ordinal: update.ordinal,
            });
        }
        record.scalar = scalar;
    }
    if let Some(active) = update.active {
        record.active = active;
    }
    Ok(())
}

fn binding_signature(records: &[SpatialBindingRecord]) -> Vec<(&'static str, String)> {
    records
        .iter()
        .map(|record| {
            (
                source_family(&record.descriptor.source_class),
                record.descriptor.identity.clone(),
            )
        })
        .collect()
}

fn source_family(class: &SpatialSourceClass) -> &'static str {
    match class {
        SpatialSourceClass::ExplicitChannel => "explicit_channel",
        SpatialSourceClass::FixedLayout => "fixed",
        SpatialSourceClass::NamedLayout => "named",
        SpatialSourceClass::DynamicPoint | SpatialSourceClass::DynamicRegion => "dynamic",
        SpatialSourceClass::Inactive => "inactive",
        SpatialSourceClass::Unsupported(_) => "unsupported",
    }
}

/// One public output channel in a spatial layout registry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SpatialLayoutChannel {
    pub identity: String,
    pub enabled: bool,
    pub lfe: bool,
}

/// Legacy rectangular node data accepted by [`SpatialLayout::new`].
///
/// New layout code should use [`SpatialLayoutTopology`] and
/// [`SpatialLayout::from_topology`]. The compatibility constructor translates
/// one-hot nodes into the same generic layer/row/anchor representation.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpatialLayoutNode {
    pub knot_indices: Vec<usize>,
    pub vector: Vec<f64>,
}

/// One speaker anchor in a topology row.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpatialLayoutAnchor {
    pub identity: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// One ordered depth row of a layout layer.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpatialLayoutRow {
    pub y: f64,
    pub anchors: Vec<SpatialLayoutAnchor>,
}

/// One ordered height layer of a layout topology.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpatialLayoutLayer {
    pub z: f64,
    pub rows: Vec<SpatialLayoutRow>,
}

/// An explicitly supplied fallback alias.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpatialLayoutAlias {
    pub identity: String,
    pub target_identity: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Data-only speaker topology consumed by the generic point projector.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SpatialLayoutTopology {
    pub layers: Vec<SpatialLayoutLayer>,
    #[serde(default)]
    pub aliases: Vec<SpatialLayoutAlias>,
}

/// One public fixed/named route vector in active non-LFE layout order.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpatialRouteVector {
    pub identity: String,
    pub vector: Vec<f64>,
}

impl SpatialRouteVector {
    /// Creates a Fixed route row keyed by a validated neutral identity.
    #[must_use]
    pub fn fixed(key: FixedRouteKey, vector: Vec<f64>) -> Self {
        Self {
            identity: key.identity(),
            vector,
        }
    }

    /// Creates a Named direct route row keyed by an opaque target ID.
    #[must_use]
    pub fn named(target: NamedTargetId, vector: Vec<f64>) -> Self {
        Self {
            identity: target.identity(),
            vector,
        }
    }
}

/// Local result of one spatial target-generation evaluation.
///
/// The effective position and locked output are sidecar information for the
/// same evaluation that produced `target`; they are not retained by metadata,
/// topology, or the scheduler.
#[derive(Clone, Debug, PartialEq)]
pub struct SpatialProjectionOutcome {
    pub target: Vec<f64>,
    pub effective_position: Option<[f64; 3]>,
    pub locked_output: Option<usize>,
}

/// Projection failures for malformed or unsupported spatial layout input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpatialProjectionError {
    InvalidLayout(&'static str),
    DuplicateChannel(String),
    InvalidKnotAxis {
        axis: usize,
    },
    NonFiniteLayoutValue,
    VectorDimension {
        expected: usize,
        actual: usize,
    },
    NodeDimension {
        expected: usize,
        actual: usize,
    },
    NodeIndexOutOfRange {
        axis: usize,
        index: usize,
    },
    DuplicateNode(Vec<usize>),
    DuplicateRoute(String),
    MissingRoute(String),
    InvalidFixedIdentity(String),
    InvalidNamedIdentity(String),
    UnsupportedRoute {
        source_class: &'static str,
        identity: String,
        status: SpatialRouteStatus,
    },
    UnsupportedDiscreteCombination {
        source_class: &'static str,
    },
    UnsupportedSourceClass(String),
    CoordinateDimension {
        expected: usize,
        actual: usize,
    },
    NonFiniteCoordinate {
        axis: usize,
    },
    MissingNode(Vec<usize>),
    InvalidSpread,
    InvalidPair,
    DuplicateAnchor(String),
    MissingAnchor(String),
    UnadmittedLayerPolicy,
    InvalidRegionState([bool; 6]),
    InvalidExtent,
    UnsupportedChannelLock,
    UnsupportedExtent,
    UnsupportedRegionLayout(&'static str),
}

impl fmt::Display for SpatialProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLayout(reason) => write!(formatter, "invalid spatial layout: {reason}"),
            Self::DuplicateChannel(identity) => {
                write!(formatter, "duplicate layout channel: {identity}")
            }
            Self::InvalidKnotAxis { axis } => write!(formatter, "invalid knot axis {axis}"),
            Self::NonFiniteLayoutValue => formatter.write_str("non-finite spatial layout value"),
            Self::VectorDimension { expected, actual } => write!(
                formatter,
                "spatial vector has {actual} components; expected {expected}"
            ),
            Self::NodeDimension { expected, actual } => write!(
                formatter,
                "spatial node has {actual} axes; expected {expected}"
            ),
            Self::NodeIndexOutOfRange { axis, index } => {
                write!(
                    formatter,
                    "spatial node index {index} is out of range on axis {axis}"
                )
            }
            Self::DuplicateNode(indices) => write!(formatter, "duplicate spatial node {indices:?}"),
            Self::DuplicateRoute(identity) => {
                write!(formatter, "duplicate spatial route: {identity}")
            }
            Self::MissingRoute(identity) => write!(formatter, "missing spatial route: {identity}"),
            Self::InvalidFixedIdentity(identity) => {
                write!(formatter, "invalid Fixed route identity: {identity}")
            }
            Self::InvalidNamedIdentity(identity) => {
                write!(formatter, "invalid Named route identity: {identity}")
            }
            Self::UnsupportedRoute {
                source_class,
                identity,
                status,
            } => write!(
                formatter,
                "unsupported {source_class} route {identity} ({})",
                status.as_str()
            ),
            Self::UnsupportedDiscreteCombination { source_class } => write!(
                formatter,
                "unsupported Dynamic-control combination for {source_class} source"
            ),
            Self::UnsupportedSourceClass(class) => {
                write!(formatter, "unsupported spatial projection class: {class}")
            }
            Self::CoordinateDimension { expected, actual } => write!(
                formatter,
                "spatial descriptor has {actual} coordinates; expected {expected}"
            ),
            Self::NonFiniteCoordinate { axis } => {
                write!(
                    formatter,
                    "non-finite spatial descriptor coordinate on axis {axis}"
                )
            }
            Self::MissingNode(indices) => write!(formatter, "missing spatial node {indices:?}"),
            Self::InvalidSpread => formatter.write_str("invalid spatial spread profile"),
            Self::InvalidPair => formatter.write_str("invalid spatial paired geometry"),
            Self::DuplicateAnchor(identity) => {
                write!(formatter, "duplicate spatial anchor: {identity}")
            }
            Self::MissingAnchor(identity) => {
                write!(formatter, "missing spatial anchor: {identity}")
            }
            Self::UnadmittedLayerPolicy => {
                formatter.write_str("spatial topology has no admitted multi-layer policy")
            }
            Self::InvalidRegionState(zones) => {
                write!(formatter, "invalid semantic region state {zones:?}")
            }
            Self::InvalidExtent => formatter.write_str("invalid semantic extent"),
            Self::UnsupportedChannelLock => {
                formatter.write_str("channel lock is unsupported for this spatial combination")
            }
            Self::UnsupportedExtent => {
                formatter.write_str("nonzero extent is unsupported by the point-region operator")
            }
            Self::UnsupportedRegionLayout(reason) => {
                write!(formatter, "unsupported constrained region layout: {reason}")
            }
        }
    }
}

impl std::error::Error for SpatialProjectionError {}

/// Public layout registry used by the spatial projection equations.
#[derive(Clone, Debug, PartialEq)]
pub struct SpatialLayout {
    channels: Vec<SpatialLayoutChannel>,
    active_indices: Vec<usize>,
    topology: SpatialLayoutTopology,
    coordinate_dimension: usize,
    route_vectors: Vec<SpatialRouteVector>,
    allow_legacy_duplicate_anchors: bool,
}

impl SpatialLayout {
    /// Validates the generic data-driven layout topology.
    pub fn from_topology(
        channels: Vec<SpatialLayoutChannel>,
        topology: SpatialLayoutTopology,
        route_vectors: Vec<SpatialRouteVector>,
    ) -> Result<Self, SpatialProjectionError> {
        Self::build(channels, topology, route_vectors, 3, false, true)
    }

    /// Validates and translates the pre-topology rectangular layout API.
    ///
    /// This compatibility path has no separate projection law: its one-hot
    /// node vectors are converted to ordinary topology anchors before any
    /// projection occurs.
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(
        channels: Vec<SpatialLayoutChannel>,
        knot_axes: Vec<Vec<f64>>,
        node_vectors: Vec<SpatialLayoutNode>,
        route_vectors: Vec<SpatialRouteVector>,
    ) -> Result<Self, SpatialProjectionError> {
        if !(1..=3).contains(&knot_axes.len()) {
            return Err(SpatialProjectionError::InvalidLayout(
                "legacy layout must have one, two, or three axes",
            ));
        }
        let topology = legacy_topology(&channels, &knot_axes, &node_vectors)?;
        Self::build(
            channels,
            topology,
            route_vectors,
            knot_axes.len(),
            true,
            true,
        )
    }

    fn build(
        channels: Vec<SpatialLayoutChannel>,
        topology: SpatialLayoutTopology,
        route_vectors: Vec<SpatialRouteVector>,
        coordinate_dimension: usize,
        allow_legacy_duplicate_anchors: bool,
        require_all_active_anchors: bool,
    ) -> Result<Self, SpatialProjectionError> {
        if channels.is_empty() {
            return Err(SpatialProjectionError::InvalidLayout("no channels"));
        }
        let mut identities = HashSet::with_capacity(channels.len());
        for channel in &channels {
            if !identities.insert(channel.identity.as_str()) {
                return Err(SpatialProjectionError::DuplicateChannel(
                    channel.identity.clone(),
                ));
            }
        }
        let active_indices: Vec<_> = channels
            .iter()
            .enumerate()
            .filter_map(|(index, channel)| (channel.enabled && !channel.lfe).then_some(index))
            .collect();
        if active_indices.is_empty() {
            return Err(SpatialProjectionError::InvalidLayout(
                "no active non-LFE channels",
            ));
        }
        validate_topology(
            &channels,
            &active_indices,
            &topology,
            allow_legacy_duplicate_anchors,
            require_all_active_anchors,
        )?;
        let component_count = active_indices.len();
        let mut routes = HashSet::with_capacity(route_vectors.len());
        for route in &route_vectors {
            if route.identity.starts_with("fixed/") {
                let Some(key) = parse_fixed_identity(&route.identity)? else {
                    unreachable!("fixed identity parser returns Some for fixed-prefixed input")
                };
                if key.identity() != route.identity {
                    return Err(SpatialProjectionError::InvalidFixedIdentity(
                        route.identity.clone(),
                    ));
                }
            } else if route.identity.starts_with("named/") {
                let Some(target) = parse_named_identity(&route.identity)? else {
                    unreachable!("named identity parser returns Some for named-prefixed input")
                };
                if target.identity() != route.identity {
                    return Err(SpatialProjectionError::InvalidNamedIdentity(
                        route.identity.clone(),
                    ));
                }
            }
            if !routes.insert(route.identity.as_str()) {
                return Err(SpatialProjectionError::DuplicateRoute(
                    route.identity.clone(),
                ));
            }
            validate_vector(&route.vector, component_count)?;
        }
        Ok(Self {
            channels,
            active_indices,
            topology,
            coordinate_dimension,
            route_vectors,
            allow_legacy_duplicate_anchors,
        })
    }

    /// Returns this validated layout with an explicitly supplied fixed/named
    /// route registry. Route vectors are data owned by the current layout; no
    /// vector is derived from a route identity or numeric position.
    pub fn with_route_vectors(
        &self,
        route_vectors: Vec<SpatialRouteVector>,
    ) -> Result<Self, SpatialProjectionError> {
        Self::build(
            self.channels.clone(),
            self.topology.clone(),
            route_vectors,
            self.coordinate_dimension,
            self.allow_legacy_duplicate_anchors,
            true,
        )
    }

    /// Returns a validated layout view whose topology may omit canonical
    /// anchors while retaining the canonical output channel vector.
    pub(crate) fn with_constrained_topology(
        &self,
        topology: SpatialLayoutTopology,
    ) -> Result<Self, SpatialProjectionError> {
        Self::build(
            self.channels.clone(),
            topology,
            self.route_vectors.clone(),
            self.coordinate_dimension,
            self.allow_legacy_duplicate_anchors,
            false,
        )
    }

    /// Returns the number of active non-LFE output components.
    #[must_use]
    pub fn active_channel_count(&self) -> usize {
        self.active_indices.len()
    }

    /// Returns the original configured channel order, including excluded channels.
    #[must_use]
    pub fn channels(&self) -> &[SpatialLayoutChannel] {
        &self.channels
    }

    /// Returns the validated data-only topology consumed by the projector.
    #[must_use]
    pub fn topology(&self) -> &SpatialLayoutTopology {
        &self.topology
    }

    /// Returns the number of normalized coordinate axes consumed by P(d,L).
    #[must_use]
    pub fn coordinate_dimension_count(&self) -> usize {
        self.coordinate_dimension
    }

    /// Computes the point target in active non-LFE channel order.
    pub fn project(
        &self,
        descriptor: &SpatialDescriptor,
    ) -> Result<Vec<f64>, SpatialProjectionError> {
        Ok(self.project_with_outcome(descriptor)?.target)
    }

    /// Classifies the current layout mapping for a neutral Named identity.
    /// Legacy opaque route strings remain available to existing callers, but
    /// the admitted neutral ID path reports its explicit direct/fallback
    /// boundary.
    #[must_use]
    pub fn named_route_status(&self, target: NamedTargetId) -> SpatialRouteStatus {
        if !self.named_direct_layout_admitted() {
            return SpatialRouteStatus::Unsupported;
        }
        if self
            .route_vectors
            .iter()
            .any(|route| route.identity == target.identity())
        {
            SpatialRouteStatus::DirectReady
        } else {
            SpatialRouteStatus::FallbackWithheld
        }
    }

    /// Classifies the discrete mapping represented by a descriptor without
    /// consuming authored coordinates or invoking the Dynamic projector.
    #[must_use]
    pub fn route_status(&self, descriptor: &SpatialDescriptor) -> SpatialRouteStatus {
        match descriptor.source_class {
            SpatialSourceClass::FixedLayout => {
                let Ok(key) = parse_fixed_identity(&descriptor.identity) else {
                    return SpatialRouteStatus::Unsupported;
                };
                let identity =
                    key.map_or_else(|| descriptor.identity.clone(), FixedRouteKey::identity);
                if self
                    .route_vectors
                    .iter()
                    .any(|route| route.identity == identity)
                {
                    SpatialRouteStatus::DirectReady
                } else {
                    SpatialRouteStatus::Unsupported
                }
            }
            SpatialSourceClass::NamedLayout => {
                let Ok(target) = parse_named_identity(&descriptor.identity) else {
                    return SpatialRouteStatus::Unsupported;
                };
                target.map_or_else(
                    || SpatialRouteStatus::Unsupported,
                    |target| self.named_route_status(target),
                )
            }
            _ => SpatialRouteStatus::Unsupported,
        }
    }

    /// Computes a target and the local effective-position outcome for one
    /// descriptor snapshot.
    pub fn project_with_outcome(
        &self,
        descriptor: &SpatialDescriptor,
    ) -> Result<SpatialProjectionOutcome, SpatialProjectionError> {
        if descriptor.channel_lock
            && !matches!(descriptor.source_class, SpatialSourceClass::DynamicPoint)
        {
            return Err(SpatialProjectionError::UnsupportedChannelLock);
        }
        if matches!(
            descriptor.source_class,
            SpatialSourceClass::DynamicPoint | SpatialSourceClass::DynamicRegion
        ) {
            return crate::RegionTopologySelector::new().project_outcome(self, descriptor);
        }
        Ok(SpatialProjectionOutcome {
            target: self.project_unconstrained(descriptor)?,
            effective_position: None,
            locked_output: None,
        })
    }

    pub(crate) fn project_unconstrained(
        &self,
        descriptor: &SpatialDescriptor,
    ) -> Result<Vec<f64>, SpatialProjectionError> {
        match descriptor.source_class {
            SpatialSourceClass::FixedLayout => return self.project_fixed(descriptor),
            SpatialSourceClass::NamedLayout => return self.project_named(descriptor),
            SpatialSourceClass::Unsupported(ref class) => {
                return Err(SpatialProjectionError::UnsupportedSourceClass(
                    class.clone(),
                ));
            }
            SpatialSourceClass::Inactive
            | SpatialSourceClass::ExplicitChannel
            | SpatialSourceClass::DynamicPoint
            | SpatialSourceClass::DynamicRegion => {}
        }
        if descriptor.pair_span_q15.is_some() {
            return self.project_semantic_pair(descriptor);
        }
        let mut vector = if let Some(spread) = &descriptor.spread {
            self.project_spread(descriptor, spread)?
        } else {
            self.project_without_spread(descriptor)?
        };
        normalize(&mut vector);
        Ok(vector)
    }

    fn project_semantic_pair(
        &self,
        descriptor: &SpatialDescriptor,
    ) -> Result<Vec<f64>, SpatialProjectionError> {
        let Some(pair_span_q15) = descriptor.pair_span_q15 else {
            unreachable!("semantic Pair projection requires a resolved span")
        };
        if pair_span_q15 > 32_767 || descriptor.spread.is_some() || descriptor.paired.is_some() {
            return Err(SpatialProjectionError::InvalidPair);
        }

        let center = self.point_position(&descriptor.coordinates)?;
        if !(0.0..=1.0).contains(&center[0]) {
            return Err(SpatialProjectionError::InvalidPair);
        }
        if pair_span_q15 == 0 {
            return self.normalized_point_target(center);
        }
        if !matches!(descriptor.source_class, SpatialSourceClass::DynamicPoint)
            || self.topology.layers.is_empty()
            || self.topology.layers.len() > 2
        {
            return Err(SpatialProjectionError::InvalidPair);
        }

        let requested_span = f64::from(pair_span_q15) / Q;
        let effective_span = requested_span.min(center[0]).min(1.0 - center[0]);
        if effective_span == 0.0 {
            return self.normalized_point_target(center);
        }

        let endpoint_a = [center[0] - effective_span, center[1], center[2]];
        let endpoint_b = [center[0] + effective_span, center[1], center[2]];
        let endpoint_a_target = self.normalized_point_target(endpoint_a)?;
        let endpoint_b_target = self.normalized_point_target(endpoint_b)?;
        let mut target = endpoint_a_target
            .into_iter()
            .zip(endpoint_b_target)
            .map(|(a, b)| a + b)
            .collect::<Vec<_>>();
        normalize(&mut target);
        Ok(target)
    }

    fn normalized_point_target(
        &self,
        position: [f64; 3],
    ) -> Result<Vec<f64>, SpatialProjectionError> {
        let mut target = generic_point_projector(
            &self.topology,
            &self.channels,
            &self.active_indices,
            position,
        )?;
        normalize(&mut target);
        Ok(target)
    }

    fn project_spread(
        &self,
        descriptor: &SpatialDescriptor,
        spread: &SpatialSpreadProfile,
    ) -> Result<Vec<f64>, SpatialProjectionError> {
        if spread.samples.is_empty() {
            return Err(SpatialProjectionError::InvalidSpread);
        }
        let mut total_weight = 0.0;
        let mut vector = vec![0.0; self.active_channel_count()];
        for sample in &spread.samples {
            if !sample.weight.is_finite() || sample.weight < 0.0 {
                return Err(SpatialProjectionError::InvalidSpread);
            }
            total_weight += sample.weight;
            let mut point_descriptor = descriptor.clone();
            point_descriptor.spread = None;
            point_descriptor.coordinates.clone_from(&sample.position);
            let point = self.project_without_spread(&point_descriptor)?;
            for (out, value) in vector.iter_mut().zip(point) {
                *out += sample.weight * value;
            }
        }
        if !total_weight.is_finite() || (total_weight - 1.0).abs() > SUM_TOLERANCE {
            return Err(SpatialProjectionError::InvalidSpread);
        }
        Ok(vector)
    }

    fn project_without_spread(
        &self,
        descriptor: &SpatialDescriptor,
    ) -> Result<Vec<f64>, SpatialProjectionError> {
        let is_inactive = matches!(descriptor.source_class, SpatialSourceClass::Inactive);
        let mut vector = match &descriptor.source_class {
            SpatialSourceClass::Inactive => vec![0.0; self.active_channel_count()],
            SpatialSourceClass::ExplicitChannel => self
                .active_channel_index(&descriptor.identity)
                .map_or_else(
                    || self.point_vector(&descriptor.coordinates),
                    |index| {
                        let mut vector = vec![0.0; self.active_channel_count()];
                        vector[index] = 1.0;
                        Ok(vector)
                    },
                )?,
            SpatialSourceClass::DynamicPoint | SpatialSourceClass::DynamicRegion => {
                self.point_vector(&descriptor.coordinates)?
            }
            SpatialSourceClass::FixedLayout | SpatialSourceClass::NamedLayout => self
                .route_vectors
                .iter()
                .find(|route| route.identity == descriptor.identity)
                .map(|route| route.vector.clone())
                .ok_or_else(|| SpatialProjectionError::MissingRoute(descriptor.identity.clone()))?,
            SpatialSourceClass::Unsupported(class) => {
                return Err(SpatialProjectionError::UnsupportedSourceClass(
                    class.clone(),
                ));
            }
        };
        if !is_inactive {
            if let Some(pair) = &descriptor.paired {
                vector = self.paired_vector(pair)?;
            }
        }
        Ok(vector)
    }

    fn active_channel_index(&self, identity: &str) -> Option<usize> {
        self.active_indices
            .iter()
            .position(|&index| self.channels[index].identity == identity)
    }

    fn point_vector(&self, coordinates: &[f64]) -> Result<Vec<f64>, SpatialProjectionError> {
        let position = self.point_position(coordinates)?;
        generic_point_projector(
            &self.topology,
            &self.channels,
            &self.active_indices,
            position,
        )
    }

    pub(crate) fn point_position(
        &self,
        coordinates: &[f64],
    ) -> Result<[f64; 3], SpatialProjectionError> {
        if coordinates.len() != self.coordinate_dimension {
            return Err(SpatialProjectionError::CoordinateDimension {
                expected: self.coordinate_dimension,
                actual: coordinates.len(),
            });
        }
        let position = match self.coordinate_dimension {
            1 => [coordinates[0], 0.0, 0.0],
            2 => [coordinates[0], 0.0, coordinates[1] * 2.0 - 1.0],
            3 => [coordinates[0], coordinates[1], coordinates[2]],
            _ => unreachable!("validated coordinate dimension"),
        };
        for (axis, coordinate) in position.iter().enumerate() {
            if !coordinate.is_finite() {
                return Err(SpatialProjectionError::NonFiniteCoordinate { axis });
            }
        }
        Ok(position)
    }

    pub(crate) fn channel_lock_outcome(
        &self,
        descriptor: &SpatialDescriptor,
        ordinary: Vec<f64>,
    ) -> Result<SpatialProjectionOutcome, SpatialProjectionError> {
        let position = self.point_position(&descriptor.coordinates)?;
        let mut candidate = None;
        let mut maximum = f64::NEG_INFINITY;
        for (index, gain) in ordinary.iter().enumerate() {
            if gain.is_finite() && *gain > maximum {
                candidate = Some(index);
                maximum = *gain;
            }
        }
        let candidate = candidate.ok_or(SpatialProjectionError::InvalidLayout(
            "ordinary point target has no active dominant output",
        ))?;
        let identity = self
            .active_indices
            .get(candidate)
            .map(|index| self.channels[*index].identity.as_str())
            .ok_or(SpatialProjectionError::InvalidLayout(
                "ordinary point target has an invalid output index",
            ))?;
        let anchor = self.anchor_for_identity(identity)?;
        let dx = position[0] - anchor.x;
        let dy = position[1] - anchor.y;
        let dz = position[2] - anchor.z;
        let distance_squared = dx * dx + dy * dy + dz * dz;
        if descriptor.channel_lock && distance_squared < CHANNEL_LOCK_THRESHOLD_SQUARED {
            let mut target = vec![0.0; self.active_channel_count()];
            target[candidate] = 1.0;
            Ok(SpatialProjectionOutcome {
                target,
                effective_position: Some([anchor.x, anchor.y, anchor.z]),
                locked_output: Some(candidate),
            })
        } else {
            Ok(SpatialProjectionOutcome {
                target: ordinary,
                effective_position: Some(position),
                locked_output: None,
            })
        }
    }

    fn project_fixed(
        &self,
        descriptor: &SpatialDescriptor,
    ) -> Result<Vec<f64>, SpatialProjectionError> {
        validate_discrete_descriptor(descriptor, "fixed")?;
        let identity = if let Some(key) = parse_fixed_identity(&descriptor.identity)? {
            key.identity()
        } else {
            descriptor.identity.clone()
        };
        self.route_vectors
            .iter()
            .find(|route| route.identity == identity)
            .map(|route| route.vector.clone())
            .ok_or(SpatialProjectionError::MissingRoute(identity))
    }

    fn project_named(
        &self,
        descriptor: &SpatialDescriptor,
    ) -> Result<Vec<f64>, SpatialProjectionError> {
        validate_discrete_descriptor(descriptor, "named")?;
        let Some(target) = parse_named_identity(&descriptor.identity)? else {
            return self
                .route_vectors
                .iter()
                .find(|route| route.identity == descriptor.identity)
                .map(|route| route.vector.clone())
                .ok_or_else(|| SpatialProjectionError::MissingRoute(descriptor.identity.clone()));
        };
        let status = self.named_route_status(target);
        if status != SpatialRouteStatus::DirectReady {
            return Err(SpatialProjectionError::UnsupportedRoute {
                source_class: "named",
                identity: target.identity(),
                status,
            });
        }
        self.route_vectors
            .iter()
            .find(|route| route.identity == target.identity())
            .map(|route| route.vector.clone())
            .ok_or_else(|| SpatialProjectionError::UnsupportedRoute {
                source_class: "named",
                identity: target.identity(),
                status: SpatialRouteStatus::Unresolved,
            })
    }

    fn named_direct_layout_admitted(&self) -> bool {
        matches!(self.active_channel_count(), 5 | 7 | 11)
    }

    fn anchor_for_identity(
        &self,
        identity: &str,
    ) -> Result<SpatialLayoutAnchor, SpatialProjectionError> {
        if let Some(anchor) = self
            .topology
            .layers
            .iter()
            .flat_map(|layer| &layer.rows)
            .flat_map(|row| &row.anchors)
            .find(|anchor| anchor.identity == identity)
        {
            return Ok(anchor.clone());
        }
        self.topology
            .aliases
            .iter()
            .find(|alias| alias.target_identity == identity)
            .map(|alias| SpatialLayoutAnchor {
                identity: identity.to_owned(),
                x: alias.x,
                y: alias.y,
                z: alias.z,
            })
            .ok_or_else(|| SpatialProjectionError::MissingAnchor(identity.to_owned()))
    }

    fn paired_vector(
        &self,
        pair: &SpatialPairedGeometry,
    ) -> Result<Vec<f64>, SpatialProjectionError> {
        if !pair.blend.is_finite()
            || !(0.0..=1.0).contains(&pair.blend)
            || pair.first.len() != self.active_channel_count()
            || pair.second.len() != self.active_channel_count()
            || pair.first.iter().any(|value| !value.is_finite())
            || pair.second.iter().any(|value| !value.is_finite())
        {
            return Err(SpatialProjectionError::InvalidPair);
        }
        let lower = (std::f64::consts::PI * pair.blend / 2.0).cos();
        let upper = (std::f64::consts::PI * pair.blend / 2.0).sin();
        Ok(pair
            .first
            .iter()
            .zip(&pair.second)
            .map(|(first, second)| lower * first + upper * second)
            .collect())
    }
}

fn legacy_topology(
    channels: &[SpatialLayoutChannel],
    knot_axes: &[Vec<f64>],
    nodes: &[SpatialLayoutNode],
) -> Result<SpatialLayoutTopology, SpatialProjectionError> {
    for (axis, knots) in knot_axes.iter().enumerate() {
        if knots.is_empty()
            || knots.iter().any(|value| !value.is_finite())
            || knots.windows(2).any(|window| window[0] >= window[1])
        {
            return Err(SpatialProjectionError::InvalidKnotAxis { axis });
        }
    }
    let active_indices: Vec<_> = channels
        .iter()
        .enumerate()
        .filter_map(|(index, channel)| (channel.enabled && !channel.lfe).then_some(index))
        .collect();
    if active_indices.is_empty() {
        return Err(SpatialProjectionError::InvalidLayout(
            "no active non-LFE channels",
        ));
    }
    let expected_dimension = knot_axes.len();
    let mut seen_nodes = HashSet::with_capacity(nodes.len());
    for node in nodes {
        if node.knot_indices.len() != expected_dimension {
            return Err(SpatialProjectionError::NodeDimension {
                expected: expected_dimension,
                actual: node.knot_indices.len(),
            });
        }
        for (axis, &index) in node.knot_indices.iter().enumerate() {
            if index >= knot_axes[axis].len() {
                return Err(SpatialProjectionError::NodeIndexOutOfRange { axis, index });
            }
        }
        if !seen_nodes.insert(node.knot_indices.clone()) {
            return Err(SpatialProjectionError::DuplicateNode(
                node.knot_indices.clone(),
            ));
        }
        validate_vector(&node.vector, active_indices.len())?;
    }
    let anchor_for = |indices: &[usize]| -> Result<SpatialLayoutAnchor, SpatialProjectionError> {
        let node = nodes
            .iter()
            .find(|node| node.knot_indices == indices)
            .ok_or_else(|| SpatialProjectionError::MissingNode(indices.to_vec()))?;
        let Some(active) = node
            .vector
            .iter()
            .enumerate()
            .find(|(_, value)| **value > 0.0)
        else {
            return Err(SpatialProjectionError::InvalidLayout(
                "legacy nodes must be nonzero one-hot vectors",
            ));
        };
        if node
            .vector
            .iter()
            .enumerate()
            .any(|(index, value)| index != active.0 && *value != 0.0)
        {
            return Err(SpatialProjectionError::InvalidLayout(
                "legacy nodes must be one-hot vectors",
            ));
        }
        let channel_index = active_indices[active.0];
        Ok(SpatialLayoutAnchor {
            identity: channels[channel_index].identity.clone(),
            x: knot_axes[0][indices[0]],
            y: if expected_dimension >= 3 {
                knot_axes[1][indices[1]]
            } else {
                0.0
            },
            z: if expected_dimension == 2 {
                knot_axes[1][indices[1]]
            } else if expected_dimension == 3 {
                knot_axes[2][indices[2]]
            } else {
                0.0
            },
        })
    };

    let layers = match expected_dimension {
        1 => vec![SpatialLayoutLayer {
            z: 0.0,
            rows: vec![SpatialLayoutRow {
                y: 0.0,
                anchors: (0..knot_axes[0].len())
                    .map(|x| anchor_for(&[x]))
                    .collect::<Result<_, _>>()?,
            }],
        }],
        2 => (0..knot_axes[1].len())
            .map(|z| {
                Ok(SpatialLayoutLayer {
                    z: knot_axes[1][z],
                    rows: vec![SpatialLayoutRow {
                        y: 0.0,
                        anchors: (0..knot_axes[0].len())
                            .map(|x| anchor_for(&[x, z]))
                            .collect::<Result<_, _>>()?,
                    }],
                })
            })
            .collect::<Result<_, SpatialProjectionError>>()?,
        3 => (0..knot_axes[2].len())
            .map(|z| {
                Ok(SpatialLayoutLayer {
                    z: knot_axes[2][z],
                    rows: (0..knot_axes[1].len())
                        .map(|y| {
                            Ok(SpatialLayoutRow {
                                y: knot_axes[1][y],
                                anchors: (0..knot_axes[0].len())
                                    .map(|x| anchor_for(&[x, y, z]))
                                    .collect::<Result<_, _>>()?,
                            })
                        })
                        .collect::<Result<_, SpatialProjectionError>>()?,
                })
            })
            .collect::<Result<_, SpatialProjectionError>>()?,
        _ => unreachable!("validated legacy dimension"),
    };
    Ok(SpatialLayoutTopology {
        layers,
        aliases: Vec::new(),
    })
}

fn validate_topology(
    channels: &[SpatialLayoutChannel],
    active_indices: &[usize],
    topology: &SpatialLayoutTopology,
    allow_duplicate_anchors: bool,
    require_all_active_anchors: bool,
) -> Result<(), SpatialProjectionError> {
    if topology.layers.is_empty() {
        return Err(SpatialProjectionError::InvalidLayout("no topology layers"));
    }
    if topology.layers.iter().any(|layer| !layer.z.is_finite())
        || topology
            .layers
            .windows(2)
            .any(|window| window[0].z >= window[1].z)
    {
        return Err(SpatialProjectionError::InvalidLayout(
            "layer Z values must be finite and strictly ordered",
        ));
    }
    let active_identities: HashSet<_> = active_indices
        .iter()
        .map(|index| channels[*index].identity.as_str())
        .collect();
    let mut seen_anchors = HashSet::new();
    for layer in &topology.layers {
        if layer.rows.is_empty() {
            return Err(SpatialProjectionError::InvalidLayout(
                "empty topology layer",
            ));
        }
        if layer.rows.iter().any(|row| !row.y.is_finite())
            || layer
                .rows
                .windows(2)
                .any(|window| window[0].y >= window[1].y)
        {
            return Err(SpatialProjectionError::InvalidLayout(
                "row Y values must be finite and strictly ordered",
            ));
        }
        for row in &layer.rows {
            if row.anchors.is_empty() {
                return Err(SpatialProjectionError::InvalidLayout("empty topology row"));
            }
            if row.anchors.iter().any(|anchor| {
                !anchor.x.is_finite() || !anchor.y.is_finite() || !anchor.z.is_finite()
            }) || row
                .anchors
                .windows(2)
                .any(|window| window[0].x >= window[1].x)
            {
                return Err(SpatialProjectionError::InvalidLayout(
                    "anchor X values must be finite and strictly ordered",
                ));
            }
            for anchor in &row.anchors {
                if !active_identities.contains(anchor.identity.as_str()) {
                    return Err(SpatialProjectionError::MissingAnchor(
                        anchor.identity.clone(),
                    ));
                }
                if (anchor.y - row.y).abs() > f64::EPSILON
                    || (anchor.z - layer.z).abs() > f64::EPSILON
                {
                    return Err(SpatialProjectionError::InvalidLayout(
                        "anchor position does not match its row and layer",
                    ));
                }
                if !allow_duplicate_anchors && !seen_anchors.insert(anchor.identity.as_str()) {
                    return Err(SpatialProjectionError::DuplicateAnchor(
                        anchor.identity.clone(),
                    ));
                }
            }
        }
    }
    if require_all_active_anchors && !allow_duplicate_anchors {
        for identity in &active_identities {
            if !seen_anchors.contains(identity) {
                return Err(SpatialProjectionError::MissingAnchor(
                    (*identity).to_owned(),
                ));
            }
        }
    }
    let mut aliases = HashSet::new();
    for alias in &topology.aliases {
        if !aliases.insert(alias.identity.as_str()) {
            return Err(SpatialProjectionError::DuplicateAnchor(
                alias.identity.clone(),
            ));
        }
        if !active_identities.contains(alias.target_identity.as_str())
            || !alias.x.is_finite()
            || !alias.y.is_finite()
            || !alias.z.is_finite()
        {
            return Err(SpatialProjectionError::InvalidLayout(
                "invalid topology alias",
            ));
        }
    }
    Ok(())
}

fn generic_point_projector(
    topology: &SpatialLayoutTopology,
    channels: &[SpatialLayoutChannel],
    active_indices: &[usize],
    position: [f64; 3],
) -> Result<Vec<f64>, SpatialProjectionError> {
    match topology.layers.as_slice() {
        [] => Err(SpatialProjectionError::InvalidLayout("no topology layers")),
        [layer] => plane_vector(layer, channels, active_indices, position[0], position[1]),
        [lower, upper] => {
            let bed = plane_vector(lower, channels, active_indices, position[0], position[1])?;
            let top = plane_vector(upper, channels, active_indices, position[0], position[1])?;
            let z = position[2].clamp(-QMAX, QMAX);
            if z <= 0.0 {
                Ok(bed)
            } else if (Q * z).floor() >= QMAX_Q15 {
                Ok(top)
            } else {
                let lower_weight = (std::f64::consts::PI * z / 2.0).cos();
                let upper_weight = (std::f64::consts::PI * z / 2.0).sin();
                Ok(bed
                    .iter()
                    .zip(top)
                    .map(|(bed, top)| lower_weight * bed + upper_weight * top)
                    .collect())
            }
        }
        _ => Err(SpatialProjectionError::UnadmittedLayerPolicy),
    }
}

fn plane_vector(
    layer: &SpatialLayoutLayer,
    channels: &[SpatialLayoutChannel],
    active_indices: &[usize],
    x: f64,
    y: f64,
) -> Result<Vec<f64>, SpatialProjectionError> {
    let mut rows = layer.rows.as_slice();
    let mut weights = [1.0, 0.0];
    if y <= rows[0].y {
        rows = &rows[..1];
    } else if y >= rows[rows.len() - 1].y {
        rows = &rows[rows.len() - 1..];
    } else {
        let upper = rows.partition_point(|row| row.y < y);
        let lower = upper - 1;
        let t = (y - rows[lower].y) / (rows[upper].y - rows[lower].y);
        weights = [
            (std::f64::consts::PI * t / 2.0).cos(),
            (std::f64::consts::PI * t / 2.0).sin(),
        ];
        rows = &rows[lower..=upper];
    }
    let first = row_vector(&rows[0], channels, active_indices, x)?;
    if rows.len() == 1 {
        return Ok(first);
    }
    let second = row_vector(&rows[1], channels, active_indices, x)?;
    Ok(first
        .iter()
        .zip(second)
        .map(|(first, second)| weights[0] * first + weights[1] * second)
        .collect())
}

fn row_vector(
    row: &SpatialLayoutRow,
    channels: &[SpatialLayoutChannel],
    active_indices: &[usize],
    x: f64,
) -> Result<Vec<f64>, SpatialProjectionError> {
    let anchors = &row.anchors;
    let mut vector = vec![0.0; active_indices.len()];
    let (first, second, lower_weight, upper_weight) = if x <= anchors[0].x {
        (0, 0, 1.0, 0.0)
    } else if x >= anchors[anchors.len() - 1].x {
        let last = anchors.len() - 1;
        (last, last, 1.0, 0.0)
    } else {
        let upper = anchors.partition_point(|anchor| anchor.x < x);
        let lower = upper - 1;
        let t = (x - anchors[lower].x) / (anchors[upper].x - anchors[lower].x);
        (
            lower,
            upper,
            (std::f64::consts::PI * t / 2.0).cos(),
            (std::f64::consts::PI * t / 2.0).sin(),
        )
    };
    let first_output = active_indices
        .iter()
        .position(|index| channels[*index].identity == anchors[first].identity)
        .ok_or_else(|| SpatialProjectionError::MissingAnchor(anchors[first].identity.clone()))?;
    vector[first_output] += lower_weight;
    if second != first {
        let second_output = active_indices
            .iter()
            .position(|index| channels[*index].identity == anchors[second].identity)
            .ok_or_else(|| {
                SpatialProjectionError::MissingAnchor(anchors[second].identity.clone())
            })?;
        vector[second_output] += upper_weight;
    }
    Ok(vector)
}

fn validate_vector(vector: &[f64], expected: usize) -> Result<(), SpatialProjectionError> {
    if vector.len() != expected {
        return Err(SpatialProjectionError::VectorDimension {
            expected,
            actual: vector.len(),
        });
    }
    if vector.iter().any(|value| !value.is_finite()) {
        return Err(SpatialProjectionError::NonFiniteLayoutValue);
    }
    Ok(())
}

fn parse_fixed_identity(identity: &str) -> Result<Option<FixedRouteKey>, SpatialProjectionError> {
    let Some(rest) = identity.strip_prefix("fixed/") else {
        return Ok(None);
    };
    let mut parts = rest.split('/');
    let family = parts.next().and_then(|value| value.parse::<u8>().ok());
    let member = parts.next().and_then(|value| value.parse::<u8>().ok());
    if parts.next().is_some() {
        return Err(SpatialProjectionError::InvalidFixedIdentity(
            identity.to_owned(),
        ));
    }
    let Some(key) = family
        .zip(member)
        .and_then(|(family, member)| FixedRouteKey::new(family, member))
    else {
        return Err(SpatialProjectionError::InvalidFixedIdentity(
            identity.to_owned(),
        ));
    };
    if key.identity() != identity {
        return Err(SpatialProjectionError::InvalidFixedIdentity(
            identity.to_owned(),
        ));
    }
    Ok(Some(key))
}

fn parse_named_identity(identity: &str) -> Result<Option<NamedTargetId>, SpatialProjectionError> {
    let Some(rest) = identity.strip_prefix("named/") else {
        return Ok(None);
    };
    let Ok(value) = rest.parse::<u8>() else {
        return Err(SpatialProjectionError::InvalidNamedIdentity(
            identity.to_owned(),
        ));
    };
    let Some(target) = NamedTargetId::new(value) else {
        return Err(SpatialProjectionError::InvalidNamedIdentity(
            identity.to_owned(),
        ));
    };
    if target.identity() != identity {
        return Err(SpatialProjectionError::InvalidNamedIdentity(
            identity.to_owned(),
        ));
    }
    Ok(Some(target))
}

fn validate_discrete_descriptor(
    descriptor: &SpatialDescriptor,
    source_class: &'static str,
) -> Result<(), SpatialProjectionError> {
    let extent_active = descriptor
        .extent
        .map(crate::extent::extent_scalar)
        .transpose()?
        .is_some_and(|(mean_q, _)| mean_q != 0);
    let region_active = match descriptor.zones {
        None => false,
        Some(zones) => {
            let state = RegionSemanticState::from_decoded_zones(zones)
                .map_err(|_| SpatialProjectionError::InvalidRegionState(zones))?;
            !state.is_default()
        }
    };
    if descriptor.spread.is_some()
        || descriptor.paired.is_some()
        || descriptor.pair_span_q15.is_some()
        || extent_active
        || region_active
        || descriptor.channel_lock
    {
        return Err(SpatialProjectionError::UnsupportedDiscreteCombination { source_class });
    }
    Ok(())
}

fn normalize(vector: &mut [f64]) {
    let norm = vector
        .iter()
        .fold(0.0_f64, |sum, value| value.mul_add(*value, sum))
        .sqrt();
    if norm.is_finite() && norm > 0.0 {
        for value in vector {
            *value /= norm;
        }
    } else {
        vector.fill(0.0);
    }
}

/// Q32 scheduler failures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GainSchedulerError {
    InvalidSampleRate,
    NonFiniteTarget,
    DurationOverflow,
}

impl fmt::Display for GainSchedulerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSampleRate => formatter.write_str("gain scheduler sample rate is zero"),
            Self::NonFiniteTarget => formatter.write_str("gain scheduler target is non-finite"),
            Self::DurationOverflow => formatter.write_str("gain scheduler duration overflow"),
        }
    }
}

impl std::error::Error for GainSchedulerError {}

/// Stateful Q32 gain scheduler.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GainScheduler {
    current: f64,
    target: f64,
    delta: f64,
    remaining_quanta: u64,
    phase: usize,
}

impl GainScheduler {
    /// Creates the zeroed scheduler state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            current: 0.0,
            target: 0.0,
            delta: 0.0,
            remaining_quanta: 0,
            phase: 0,
        }
    }

    /// Applies a target event under the Q32 scheduling contract.
    pub fn set_target(
        &mut self,
        target: f64,
        event: bool,
        duration_samples: u64,
        sample_rate: u32,
    ) -> Result<(), GainSchedulerError> {
        if sample_rate == 0 {
            return Err(GainSchedulerError::InvalidSampleRate);
        }
        if !target.is_finite() {
            return Err(GainSchedulerError::NonFiniteTarget);
        }
        self.target = target;
        if !event {
            return Ok(());
        }
        let rho = u64::from(sample_rate > 48_000) + 1;
        let scaled = duration_samples
            .checked_mul(rho)
            .and_then(|value| value.checked_add(Q32_HALF_MINUS_ONE))
            .ok_or(GainSchedulerError::DurationOverflow)?;
        let quanta = scaled / Q32 as u64;
        if (target - self.current).abs() >= EPS_DELTA && quanta > 0 {
            self.remaining_quanta = quanta;
            self.delta = (target - self.current) / quanta as f64;
            self.phase = 0;
        } else {
            self.current = target;
            self.delta = 0.0;
            self.remaining_quanta = 0;
            self.phase = 0;
        }
        Ok(())
    }

    /// Emits one sample and advances only the persistent Q32 state.
    #[must_use]
    pub fn next_sample(&mut self) -> f64 {
        if self.remaining_quanta == 0 {
            self.current = self.target;
            return self.target;
        }
        let value = self.current + self.delta * self.phase as f64 / Q32 as f64;
        self.phase += 1;
        if self.phase == Q32 {
            self.current += self.delta;
            self.remaining_quanta -= 1;
            self.phase = 0;
        }
        value
    }

    /// Emits a block without allocating or resetting scheduler phase.
    pub fn process(&mut self, output: &mut [f64]) {
        for sample in output {
            *sample = self.next_sample();
        }
    }

    /// Clears current, target, ramp, and phase state.
    pub const fn reset(&mut self) {
        self.current = 0.0;
        self.target = 0.0;
        self.delta = 0.0;
        self.remaining_quanta = 0;
        self.phase = 0;
    }

    /// Returns whether this route is above the spatial activity floor.
    #[must_use]
    pub fn active(&self) -> bool {
        self.target.abs() >= EPS_ACTIVITY || self.current.abs() >= EPS_ACTIVITY
    }

    /// Returns the latest target.
    #[must_use]
    pub const fn target(&self) -> f64 {
        self.target
    }

    /// Returns the current ramp base.
    #[must_use]
    pub const fn current(&self) -> f64 {
        self.current
    }
}

/// Top-level errors from the spatial accumulation bridge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpatialBridgeError {
    Binding(SpatialBindingError),
    Projection(SpatialProjectionError),
    Scheduler(GainSchedulerError),
    OutputChannelCount {
        expected: usize,
        actual: usize,
    },
    OutputLengthMismatch {
        channel: usize,
        expected: usize,
        actual: usize,
    },
    InputLengthMismatch {
        coordinate: usize,
        expected: usize,
        actual: usize,
    },
    NonFiniteInput {
        coordinate: usize,
        sample: usize,
    },
    NonFiniteOutput {
        channel: usize,
        sample: usize,
    },
}

impl fmt::Display for SpatialBridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Binding(error) => write!(formatter, "spatial binding error: {error}"),
            Self::Projection(error) => write!(formatter, "spatial projection error: {error}"),
            Self::Scheduler(error) => write!(formatter, "gain scheduler error: {error}"),
            Self::OutputChannelCount { expected, actual } => {
                write!(
                    formatter,
                    "expected {expected} active output channels, got {actual}"
                )
            }
            Self::OutputLengthMismatch {
                channel,
                expected,
                actual,
            } => write!(
                formatter,
                "output channel {channel} has {actual} samples; expected {expected}"
            ),
            Self::InputLengthMismatch {
                coordinate,
                expected,
                actual,
            } => write!(
                formatter,
                "input coordinate {coordinate} has {actual} samples; expected {expected}"
            ),
            Self::NonFiniteInput { coordinate, sample } => {
                write!(
                    formatter,
                    "input coordinate {coordinate} has non-finite sample {sample}"
                )
            }
            Self::NonFiniteOutput { channel, sample } => {
                write!(
                    formatter,
                    "output channel {channel} became non-finite at sample {sample}"
                )
            }
        }
    }
}

impl std::error::Error for SpatialBridgeError {}

impl From<SpatialBindingError> for SpatialBridgeError {
    fn from(value: SpatialBindingError) -> Self {
        Self::Binding(value)
    }
}

impl From<SpatialProjectionError> for SpatialBridgeError {
    fn from(value: SpatialProjectionError) -> Self {
        Self::Projection(value)
    }
}

impl From<GainSchedulerError> for SpatialBridgeError {
    fn from(value: GainSchedulerError) -> Self {
        Self::Scheduler(value)
    }
}

/// JOC spatial bridge with persistent streaming state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JocSpatialBridge {
    binding: SpatialBindingState,
    schedulers: Vec<GainScheduler>,
    targets: Vec<Vec<f64>>,
    last_layout: Option<SpatialLayout>,
    region_selector: RegionTopologySelector,
}

impl JocSpatialBridge {
    /// Creates an empty bridge. Its semantic operator state remains unresolved.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            binding: SpatialBindingState::new(),
            schedulers: Vec::new(),
            targets: Vec::new(),
            last_layout: None,
            region_selector: RegionTopologySelector::new(),
        }
    }

    /// Returns the current spatial binding state.
    #[must_use]
    pub const fn binding_state(&self) -> &SpatialBindingState {
        &self.binding
    }

    /// Returns the truthful project-level semantic binding state.
    #[must_use]
    pub const fn semantic_binding(&self) -> SemanticBindingState {
        SemanticBindingState::Unresolved
    }

    /// The current implementation is executable only through this explicit bridge.
    #[must_use]
    pub const fn is_production_resolved(&self) -> bool {
        false
    }

    /// Resets topology, target cache, route ramps, and layout cache.
    pub fn reset(&mut self) {
        self.binding.reset();
        self.schedulers.clear();
        self.targets.clear();
        self.last_layout = None;
        self.region_selector.clear();
    }

    /// Renders borrowed codec-coordinate PCM into active non-LFE output planes.
    ///
    /// The optional topology is a full valid snapshot. Optional updates carry
    /// present-field overrides/inheritance. Passing neither reuses the current
    /// binding snapshot. LFE PCM is intentionally outside this output contract.
    #[allow(clippy::too_many_arguments)]
    pub fn render_coordinates(
        &mut self,
        coordinates: &[&[f64]],
        topology: Option<&SpatialTopologySnapshot>,
        updates: Option<&[SpatialCoordinateUpdate]>,
        layout: &SpatialLayout,
        duration_samples: u64,
        sample_rate: u32,
        outputs: &mut [&mut [f64]],
    ) -> Result<(), SpatialBridgeError> {
        validate_block_shapes(coordinates, layout, outputs)?;
        // Fail-closed target resolution must not leave caller-owned output
        // planes carrying a previous successful route.
        for output in &mut *outputs {
            output.fill(0.0);
        }
        let block_length = outputs.first().map_or(0, |output| output.len());
        let layout_changed = self
            .last_layout
            .as_ref()
            .is_none_or(|previous| previous != layout);
        if layout_changed {
            self.region_selector.clear();
        }
        let result = self.binding.apply(topology, updates, coordinates.len())?;
        if result.transition == SpatialBindingTransition::Rebuild {
            self.region_selector.clear();
        }
        let Some(snapshot) = self.binding.snapshot() else {
            return Err(SpatialBridgeError::Binding(
                SpatialBindingError::NoTopologyForInitialization,
            ));
        };
        let active_count = snapshot.active_count;
        let route_shape_changed = layout_changed
            || self.targets.len() != active_count
            || self.schedulers.len() != active_count * layout.active_channel_count();
        let reset_routes =
            route_shape_changed || matches!(result.transition, SpatialBindingTransition::Init);
        let mut next_schedulers = if reset_routes {
            (0..active_count * layout.active_channel_count())
                .map(|_| GainScheduler::new())
                .collect::<Vec<_>>()
        } else {
            self.schedulers.clone()
        };
        let mut next_targets = if reset_routes {
            vec![vec![0.0; layout.active_channel_count()]; active_count]
        } else {
            self.targets.clone()
        };
        let refresh_targets = reset_routes || result.event;
        if refresh_targets {
            for (index, target) in next_targets.iter_mut().enumerate() {
                let record = &snapshot.records[index];
                if record.active {
                    let outcome = match self.region_selector.project_outcome_with_epoch(
                        layout,
                        &record.descriptor,
                        snapshot.topology_epoch,
                    ) {
                        Ok(outcome) => outcome,
                        Err(error) => {
                            self.targets.clear();
                            self.schedulers.clear();
                            return Err(error.into());
                        }
                    };
                    for (target_value, projection) in target.iter_mut().zip(outcome.target) {
                        *target_value = record.scalar * projection;
                    }
                } else {
                    target.fill(0.0);
                }
            }
            for (index, target) in next_targets.iter().enumerate() {
                for (channel, &value) in target.iter().enumerate() {
                    if let Err(error) = next_schedulers
                        [index * layout.active_channel_count() + channel]
                        .set_target(value, true, duration_samples, sample_rate)
                    {
                        self.targets.clear();
                        self.schedulers.clear();
                        return Err(error.into());
                    }
                }
            }
            self.targets = next_targets;
            self.schedulers = next_schedulers;
            if reset_routes {
                self.last_layout = Some(layout.clone());
            }
        }

        for output in &mut *outputs {
            output.fill(0.0);
        }
        for sample in 0..block_length {
            for (coordinate, input_plane) in coordinates.iter().take(active_count).enumerate() {
                let input = input_plane[sample];
                for (channel, output) in outputs
                    .iter_mut()
                    .take(layout.active_channel_count())
                    .enumerate()
                {
                    let scheduler =
                        &mut self.schedulers[coordinate * layout.active_channel_count() + channel];
                    let gain = scheduler.next_sample();
                    if gain != 0.0 {
                        let value = output[sample] + gain * input;
                        output[sample] = value;
                        if !value.is_finite() {
                            output.fill(0.0);
                            return Err(SpatialBridgeError::NonFiniteOutput { channel, sample });
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Renders the existing decoded Base/RB frame boundary through the spatial
    /// bridge. Base full-band coordinates precede ReconstructionBasis rows;
    /// `RcLfe` remains separate and is not accumulated here.
    pub fn render_codec_basis_frame(
        &mut self,
        frame: &JocSpatialReconstructionFrame<'_>,
        topology: Option<&SpatialTopologySnapshot>,
        updates: Option<&[SpatialCoordinateUpdate]>,
        layout: &SpatialLayout,
        duration_samples: u64,
        outputs: &mut [&mut [f64]],
    ) -> Result<(), SpatialBridgeError> {
        self.render_codec_basis_frame_with_contribution(
            frame,
            SpatialContributionMode::Full,
            topology,
            updates,
            layout,
            duration_samples,
            outputs,
        )
    }

    /// Renders the same codec-basis topology while zeroing only the selected
    /// diagnostic PCM contribution. Coordinate count and order are unchanged.
    #[allow(clippy::too_many_arguments)]
    pub fn render_codec_basis_frame_with_contribution(
        &mut self,
        frame: &JocSpatialReconstructionFrame<'_>,
        contribution_mode: SpatialContributionMode,
        topology: Option<&SpatialTopologySnapshot>,
        updates: Option<&[SpatialCoordinateUpdate]>,
        layout: &SpatialLayout,
        duration_samples: u64,
        outputs: &mut [&mut [f64]],
    ) -> Result<(), SpatialBridgeError> {
        const STACK_COORDINATE_CAPACITY: usize = 64;
        let coordinate_count =
            frame.basis.base_full_band_pcm.len() + frame.basis.reconstruction_basis.rows.len();
        let zero_pcm = (!matches!(contribution_mode, SpatialContributionMode::Full))
            .then(|| vec![0.0; outputs.first().map_or(0, |output| output.len())]);
        let zero_pcm = zero_pcm.as_deref().unwrap_or(&[]);
        if coordinate_count <= STACK_COORDINATE_CAPACITY {
            let mut coordinates = [&[][..]; STACK_COORDINATE_CAPACITY];
            let mut index = 0;
            for pcm in frame.basis.base_full_band_pcm {
                coordinates[index] = if contribution_mode.includes_base() {
                    pcm
                } else {
                    zero_pcm
                };
                index += 1;
            }
            for pcm in &frame.basis.reconstruction_basis.rows {
                coordinates[index] = if contribution_mode.includes_reconstruction() {
                    pcm
                } else {
                    zero_pcm
                };
                index += 1;
            }
            self.render_coordinates(
                &coordinates[..coordinate_count],
                topology,
                updates,
                layout,
                duration_samples,
                frame.sample_rate,
                outputs,
            )
        } else {
            let mut coordinates = Vec::with_capacity(coordinate_count);
            coordinates.extend(frame.basis.base_full_band_pcm.iter().map(|pcm| {
                if contribution_mode.includes_base() {
                    pcm.as_slice()
                } else {
                    zero_pcm
                }
            }));
            coordinates.extend(frame.basis.reconstruction_basis.rows.iter().map(|pcm| {
                if contribution_mode.includes_reconstruction() {
                    pcm.as_slice()
                } else {
                    zero_pcm
                }
            }));
            self.render_coordinates(
                &coordinates,
                topology,
                updates,
                layout,
                duration_samples,
                frame.sample_rate,
                outputs,
            )
        }
    }
}

fn validate_block_shapes(
    coordinates: &[&[f64]],
    layout: &SpatialLayout,
    outputs: &[&mut [f64]],
) -> Result<(), SpatialBridgeError> {
    let expected_channels = layout.active_channel_count();
    if outputs.len() != expected_channels {
        return Err(SpatialBridgeError::OutputChannelCount {
            expected: expected_channels,
            actual: outputs.len(),
        });
    }
    let block_length = outputs.first().map_or(0, |output| output.len());
    for (channel, output) in outputs.iter().enumerate() {
        if output.len() != block_length {
            return Err(SpatialBridgeError::OutputLengthMismatch {
                channel,
                expected: block_length,
                actual: output.len(),
            });
        }
    }
    for (coordinate, input) in coordinates.iter().enumerate() {
        if input.len() != block_length {
            return Err(SpatialBridgeError::InputLengthMismatch {
                coordinate,
                expected: block_length,
                actual: input.len(),
            });
        }
        if let Some(sample) = input.iter().position(|value| !value.is_finite()) {
            return Err(SpatialBridgeError::NonFiniteInput { coordinate, sample });
        }
    }
    Ok(())
}
