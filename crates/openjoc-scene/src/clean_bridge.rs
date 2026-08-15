//! Clean, experimental JOC spatial bridge.
//!
//! The types in this module are deliberately separate from the metadata-only
//! scene model. They describe codec-coordinate records and public layout
//! registries, not authored objects. The module is downstream of the existing
//! ETSI/vendor validation profiles; it does not alter parser policy.

use super::JocSpatialReconstructionFrame;
use crate::SemanticBindingState;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt};

/// Public schema label for the clean experimental bridge.
pub const CLEAN_SPATIAL_BRIDGE_SCHEMA: &str = "openjoc.clean.experimental-joc-spatial-bridge.v1";

const Q32: usize = 32;
const Q32_HALF_MINUS_ONE: u64 = 15;
const EPS_ACTIVITY: f64 = 0.000_001;
const EPS_DELTA: f64 = 0.000_1;
const SUM_TOLERANCE: f64 = 1.0e-9;

/// Clean descriptor dispatch classes from the implementation bundle.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanSourceClass {
    Inactive,
    ExplicitChannel,
    DynamicPoint,
    DynamicRegion,
    FixedLayout,
    NamedLayout,
    /// An explicit unsupported class. It is rejected instead of guessed.
    Unsupported(String),
}

/// One finite public spread sample and its normalized weight.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CleanSpreadSample {
    pub position: Vec<f64>,
    pub weight: f64,
}

/// Public finite spread profile for a region descriptor.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CleanSpreadProfile {
    pub samples: Vec<CleanSpreadSample>,
}

/// Public paired geometry used by the equal-power pair operator.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CleanPairedGeometry {
    pub first: Vec<f64>,
    pub second: Vec<f64>,
    pub blend: f64,
}

/// One effective clean descriptor. `raw3` is retained but never used in
/// projection arithmetic.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CleanSpatialDescriptor {
    pub source_class: CleanSourceClass,
    pub identity: String,
    pub coordinates: Vec<f64>,
    pub spread: Option<CleanSpreadProfile>,
    pub paired: Option<CleanPairedGeometry>,
    pub raw3: Option<Vec<u8>>,
}

impl CleanSpatialDescriptor {
    /// Creates a descriptor without optional spread, pair, or opaque data.
    #[must_use]
    pub fn new(
        source_class: CleanSourceClass,
        identity: impl Into<String>,
        coordinates: Vec<f64>,
    ) -> Self {
        Self {
            source_class,
            identity: identity.into(),
            coordinates,
            spread: None,
            paired: None,
            raw3: None,
        }
    }
}

/// One codec-coordinate record. The ordinal is assigned only after topology
/// flattening; it is never authored-object identity.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CleanBindingRecord {
    pub descriptor: CleanSpatialDescriptor,
    pub scalar: f64,
    pub active: bool,
}

/// A labeled member of an explicit topology group.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CleanExplicitMember {
    pub canonical_label: String,
    pub record: CleanBindingRecord,
}

/// An explicit group with deterministic group and label ordering.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CleanExplicitGroup {
    pub group_order: u32,
    pub members: Vec<CleanExplicitMember>,
}

/// A valid topology snapshot in the clean ordinary-path domain.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CleanTopologySnapshot {
    pub explicit_groups: Vec<CleanExplicitGroup>,
    pub fixed_layout: Vec<CleanBindingRecord>,
    pub dynamic_records: Vec<CleanBindingRecord>,
}

impl CleanTopologySnapshot {
    /// Flattens the declared domain using the clean ordering rule.
    #[must_use]
    pub fn flatten(&self) -> Vec<CleanBindingRecord> {
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
pub struct CleanDescriptorPatch {
    pub source_class: Option<CleanSourceClass>,
    pub identity: Option<String>,
    pub coordinates: Option<Vec<f64>>,
    pub spread: Option<Option<CleanSpreadProfile>>,
    pub paired: Option<Option<CleanPairedGeometry>>,
    pub raw3: Option<Option<Vec<u8>>>,
}

/// Selective block update. Absent fields inherit from the current coordinate.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CleanCoordinateUpdate {
    pub ordinal: usize,
    pub descriptor: Option<CleanDescriptorPatch>,
    pub scalar: Option<f64>,
    pub active: Option<bool>,
}

/// Binding state machine transitions from the clean specification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CleanBindingTransition {
    Init,
    Stable,
    Reuse,
    Rebuild,
}

/// Effective binding snapshot keyed by `(topology_epoch, ordinal)`.
#[derive(Clone, Debug, PartialEq)]
pub struct CleanBindingSnapshot {
    pub topology_epoch: u64,
    pub records: Vec<CleanBindingRecord>,
    pub active_count: usize,
}

/// Result of applying a topology/payload event to the binding state machine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CleanBindingResult {
    pub transition: CleanBindingTransition,
    pub event: bool,
}

/// Binding-state failures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CleanBindingError {
    NoTopologyForInitialization,
    EmptyTopology,
    UnsupportedSourceClass(String),
    NonFiniteScalar { ordinal: usize },
    UpdateOrdinalOutOfRange { ordinal: usize, count: usize },
    TopologyEpochOverflow,
}

impl fmt::Display for CleanBindingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoTopologyForInitialization => {
                formatter.write_str("clean binding requires an initial valid topology")
            }
            Self::EmptyTopology => formatter.write_str("clean topology must contain a record"),
            Self::UnsupportedSourceClass(class) => {
                write!(formatter, "unsupported clean source class: {class}")
            }
            Self::NonFiniteScalar { ordinal } => {
                write!(formatter, "non-finite scalar at clean coordinate {ordinal}")
            }
            Self::UpdateOrdinalOutOfRange { ordinal, count } => write!(
                formatter,
                "clean update ordinal {ordinal} is outside record count {count}"
            ),
            Self::TopologyEpochOverflow => formatter.write_str("clean topology epoch overflow"),
        }
    }
}

impl std::error::Error for CleanBindingError {}

/// Stateful clean codec-coordinate binding.
#[derive(Clone, Debug, Default)]
pub struct CleanBindingState {
    snapshot: Option<CleanBindingSnapshot>,
}

impl CleanBindingState {
    /// Creates an empty binding state.
    #[must_use]
    pub const fn new() -> Self {
        Self { snapshot: None }
    }

    /// Applies a full topology snapshot and/or same-coordinate block updates.
    /// `None, None` is the clean no-new-payload reuse event.
    pub fn apply(
        &mut self,
        topology: Option<&CleanTopologySnapshot>,
        updates: Option<&[CleanCoordinateUpdate]>,
        pcm_count: usize,
    ) -> Result<CleanBindingResult, CleanBindingError> {
        let mut result = if let Some(topology) = topology {
            let records = topology.flatten();
            validate_records(&records)?;
            if records.is_empty() {
                return Err(CleanBindingError::EmptyTopology);
            }
            match self.snapshot.as_ref() {
                None => {
                    self.snapshot = Some(CleanBindingSnapshot {
                        topology_epoch: 1,
                        records,
                        active_count: 0,
                    });
                    CleanBindingResult {
                        transition: CleanBindingTransition::Init,
                        event: true,
                    }
                }
                Some(previous) => {
                    let transition =
                        if binding_signature(&previous.records) == binding_signature(&records) {
                            CleanBindingTransition::Stable
                        } else {
                            CleanBindingTransition::Rebuild
                        };
                    let epoch = if transition == CleanBindingTransition::Rebuild {
                        previous
                            .topology_epoch
                            .checked_add(1)
                            .ok_or(CleanBindingError::TopologyEpochOverflow)?
                    } else {
                        previous.topology_epoch
                    };
                    self.snapshot = Some(CleanBindingSnapshot {
                        topology_epoch: epoch,
                        records,
                        active_count: 0,
                    });
                    CleanBindingResult {
                        transition,
                        event: true,
                    }
                }
            }
        } else if self.snapshot.is_some() {
            CleanBindingResult {
                transition: if updates.is_some() {
                    CleanBindingTransition::Stable
                } else {
                    CleanBindingTransition::Reuse
                },
                event: updates.is_some(),
            }
        } else {
            return Err(CleanBindingError::NoTopologyForInitialization);
        };

        if let Some(updates) = updates {
            let Some(snapshot) = self.snapshot.as_mut() else {
                return Err(CleanBindingError::NoTopologyForInitialization);
            };
            let prior_signature = binding_signature(&snapshot.records);
            for update in updates {
                apply_update(snapshot, update)?;
            }
            let next_signature = binding_signature(&snapshot.records);
            if prior_signature != next_signature {
                if result.transition != CleanBindingTransition::Init {
                    snapshot.topology_epoch = snapshot
                        .topology_epoch
                        .checked_add(1)
                        .ok_or(CleanBindingError::TopologyEpochOverflow)?;
                }
                result.transition = CleanBindingTransition::Rebuild;
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
    pub const fn snapshot(&self) -> Option<&CleanBindingSnapshot> {
        self.snapshot.as_ref()
    }
}

fn validate_records(records: &[CleanBindingRecord]) -> Result<(), CleanBindingError> {
    for (ordinal, record) in records.iter().enumerate() {
        if !record.scalar.is_finite() {
            return Err(CleanBindingError::NonFiniteScalar { ordinal });
        }
        if let CleanSourceClass::Unsupported(class) = &record.descriptor.source_class {
            return Err(CleanBindingError::UnsupportedSourceClass(class.clone()));
        }
    }
    Ok(())
}

fn apply_update(
    snapshot: &mut CleanBindingSnapshot,
    update: &CleanCoordinateUpdate,
) -> Result<(), CleanBindingError> {
    let count = snapshot.records.len();
    let record = snapshot.records.get_mut(update.ordinal).ok_or(
        CleanBindingError::UpdateOrdinalOutOfRange {
            ordinal: update.ordinal,
            count,
        },
    )?;
    if let Some(patch) = &update.descriptor {
        if let Some(source_class) = &patch.source_class {
            if let CleanSourceClass::Unsupported(class) = source_class {
                return Err(CleanBindingError::UnsupportedSourceClass(class.clone()));
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
        if let Some(raw3) = &patch.raw3 {
            record.descriptor.raw3.clone_from(raw3);
        }
    }
    if let Some(scalar) = update.scalar {
        if !scalar.is_finite() {
            return Err(CleanBindingError::NonFiniteScalar {
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

fn binding_signature(records: &[CleanBindingRecord]) -> Vec<(CleanSourceClass, String)> {
    records
        .iter()
        .map(|record| {
            (
                record.descriptor.source_class.clone(),
                record.descriptor.identity.clone(),
            )
        })
        .collect()
}

/// One public output channel in a clean layout registry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CleanLayoutChannel {
    pub identity: String,
    pub enabled: bool,
    pub lfe: bool,
}

/// One public knot/node vector in active non-LFE layout order.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CleanLayoutNode {
    pub knot_indices: Vec<usize>,
    pub vector: Vec<f64>,
}

/// One public fixed/named route vector in active non-LFE layout order.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CleanRouteVector {
    pub identity: String,
    pub vector: Vec<f64>,
}

/// Projection failures for malformed or unsupported clean layout input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CleanProjectionError {
    InvalidLayout(&'static str),
    DuplicateChannel(String),
    InvalidKnotAxis { axis: usize },
    NonFiniteLayoutValue,
    VectorDimension { expected: usize, actual: usize },
    NodeDimension { expected: usize, actual: usize },
    NodeIndexOutOfRange { axis: usize, index: usize },
    DuplicateNode(Vec<usize>),
    DuplicateRoute(String),
    MissingRoute(String),
    UnsupportedSourceClass(String),
    CoordinateDimension { expected: usize, actual: usize },
    NonFiniteCoordinate { axis: usize },
    MissingNode(Vec<usize>),
    InvalidSpread,
    InvalidPair,
}

impl fmt::Display for CleanProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLayout(reason) => write!(formatter, "invalid clean layout: {reason}"),
            Self::DuplicateChannel(identity) => {
                write!(formatter, "duplicate layout channel: {identity}")
            }
            Self::InvalidKnotAxis { axis } => write!(formatter, "invalid knot axis {axis}"),
            Self::NonFiniteLayoutValue => formatter.write_str("non-finite clean layout value"),
            Self::VectorDimension { expected, actual } => write!(
                formatter,
                "clean vector has {actual} components; expected {expected}"
            ),
            Self::NodeDimension { expected, actual } => write!(
                formatter,
                "clean node has {actual} axes; expected {expected}"
            ),
            Self::NodeIndexOutOfRange { axis, index } => {
                write!(
                    formatter,
                    "clean node index {index} is out of range on axis {axis}"
                )
            }
            Self::DuplicateNode(indices) => write!(formatter, "duplicate clean node {indices:?}"),
            Self::DuplicateRoute(identity) => {
                write!(formatter, "duplicate clean route: {identity}")
            }
            Self::MissingRoute(identity) => write!(formatter, "missing clean route: {identity}"),
            Self::UnsupportedSourceClass(class) => {
                write!(formatter, "unsupported clean projection class: {class}")
            }
            Self::CoordinateDimension { expected, actual } => write!(
                formatter,
                "clean descriptor has {actual} coordinates; expected {expected}"
            ),
            Self::NonFiniteCoordinate { axis } => {
                write!(
                    formatter,
                    "non-finite clean descriptor coordinate on axis {axis}"
                )
            }
            Self::MissingNode(indices) => write!(formatter, "missing clean node {indices:?}"),
            Self::InvalidSpread => formatter.write_str("invalid clean spread profile"),
            Self::InvalidPair => formatter.write_str("invalid clean paired geometry"),
        }
    }
}

impl std::error::Error for CleanProjectionError {}

/// Public layout registry used by the clean projection equations.
#[derive(Clone, Debug, PartialEq)]
pub struct CleanSpatialLayout {
    channels: Vec<CleanLayoutChannel>,
    active_indices: Vec<usize>,
    knot_axes: Vec<Vec<f64>>,
    node_vectors: Vec<CleanLayoutNode>,
    route_vectors: Vec<CleanRouteVector>,
}

impl CleanSpatialLayout {
    /// Validates and constructs an ordered public layout registry.
    pub fn new(
        channels: Vec<CleanLayoutChannel>,
        knot_axes: Vec<Vec<f64>>,
        node_vectors: Vec<CleanLayoutNode>,
        route_vectors: Vec<CleanRouteVector>,
    ) -> Result<Self, CleanProjectionError> {
        if channels.is_empty() {
            return Err(CleanProjectionError::InvalidLayout("no channels"));
        }
        let mut identities = HashSet::with_capacity(channels.len());
        for channel in &channels {
            if !identities.insert(channel.identity.as_str()) {
                return Err(CleanProjectionError::DuplicateChannel(
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
            return Err(CleanProjectionError::InvalidLayout(
                "no active non-LFE channels",
            ));
        }
        for (axis, knots) in knot_axes.iter().enumerate() {
            if knots.is_empty()
                || knots.iter().any(|value| !value.is_finite())
                || knots.windows(2).any(|window| window[0] >= window[1])
            {
                return Err(CleanProjectionError::InvalidKnotAxis { axis });
            }
        }
        let component_count = active_indices.len();
        let mut seen_nodes = Vec::with_capacity(node_vectors.len());
        for node in &node_vectors {
            if node.knot_indices.len() != knot_axes.len() {
                return Err(CleanProjectionError::NodeDimension {
                    expected: knot_axes.len(),
                    actual: node.knot_indices.len(),
                });
            }
            for (axis, &index) in node.knot_indices.iter().enumerate() {
                if index >= knot_axes[axis].len() {
                    return Err(CleanProjectionError::NodeIndexOutOfRange { axis, index });
                }
            }
            if seen_nodes
                .iter()
                .any(|indices: &Vec<usize>| indices == &node.knot_indices)
            {
                return Err(CleanProjectionError::DuplicateNode(
                    node.knot_indices.clone(),
                ));
            }
            seen_nodes.push(node.knot_indices.clone());
            validate_vector(&node.vector, component_count)?;
        }
        let mut routes = HashSet::with_capacity(route_vectors.len());
        for route in &route_vectors {
            if !routes.insert(route.identity.as_str()) {
                return Err(CleanProjectionError::DuplicateRoute(route.identity.clone()));
            }
            validate_vector(&route.vector, component_count)?;
        }
        Ok(Self {
            channels,
            active_indices,
            knot_axes,
            node_vectors,
            route_vectors,
        })
    }

    /// Returns the number of active non-LFE output components.
    #[must_use]
    pub fn active_channel_count(&self) -> usize {
        self.active_indices.len()
    }

    /// Returns the original configured channel order, including excluded channels.
    #[must_use]
    pub fn channels(&self) -> &[CleanLayoutChannel] {
        &self.channels
    }

    /// Computes normalized `P(d,L)` in active non-LFE channel order.
    pub fn project(
        &self,
        descriptor: &CleanSpatialDescriptor,
    ) -> Result<Vec<f64>, CleanProjectionError> {
        let mut vector = if let Some(spread) = &descriptor.spread {
            self.project_spread(descriptor, spread)?
        } else {
            self.project_without_spread(descriptor)?
        };
        normalize(&mut vector);
        Ok(vector)
    }

    fn project_spread(
        &self,
        descriptor: &CleanSpatialDescriptor,
        spread: &CleanSpreadProfile,
    ) -> Result<Vec<f64>, CleanProjectionError> {
        if spread.samples.is_empty() {
            return Err(CleanProjectionError::InvalidSpread);
        }
        let mut total_weight = 0.0;
        let mut vector = vec![0.0; self.active_channel_count()];
        for sample in &spread.samples {
            if !sample.weight.is_finite() || sample.weight < 0.0 {
                return Err(CleanProjectionError::InvalidSpread);
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
            return Err(CleanProjectionError::InvalidSpread);
        }
        Ok(vector)
    }

    fn project_without_spread(
        &self,
        descriptor: &CleanSpatialDescriptor,
    ) -> Result<Vec<f64>, CleanProjectionError> {
        let is_inactive = matches!(descriptor.source_class, CleanSourceClass::Inactive);
        let mut vector = match &descriptor.source_class {
            CleanSourceClass::Inactive => vec![0.0; self.active_channel_count()],
            CleanSourceClass::ExplicitChannel => self
                .active_channel_index(&descriptor.identity)
                .map_or_else(
                    || self.point_vector(&descriptor.coordinates),
                    |index| {
                        let mut vector = vec![0.0; self.active_channel_count()];
                        vector[index] = 1.0;
                        Ok(vector)
                    },
                )?,
            CleanSourceClass::DynamicPoint | CleanSourceClass::DynamicRegion => {
                self.point_vector(&descriptor.coordinates)?
            }
            CleanSourceClass::FixedLayout | CleanSourceClass::NamedLayout => self
                .route_vectors
                .iter()
                .find(|route| route.identity == descriptor.identity)
                .map(|route| route.vector.clone())
                .ok_or_else(|| CleanProjectionError::MissingRoute(descriptor.identity.clone()))?,
            CleanSourceClass::Unsupported(class) => {
                return Err(CleanProjectionError::UnsupportedSourceClass(class.clone()));
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

    fn point_vector(&self, coordinates: &[f64]) -> Result<Vec<f64>, CleanProjectionError> {
        if coordinates.len() != self.knot_axes.len() {
            return Err(CleanProjectionError::CoordinateDimension {
                expected: self.knot_axes.len(),
                actual: coordinates.len(),
            });
        }
        let mut choices = Vec::with_capacity(coordinates.len());
        for (axis, (&coordinate, knots)) in coordinates.iter().zip(&self.knot_axes).enumerate() {
            if !coordinate.is_finite() {
                return Err(CleanProjectionError::NonFiniteCoordinate { axis });
            }
            if knots.len() == 1 || coordinate <= knots[0] {
                choices.push((0, 0, 1.0, 0.0));
                continue;
            }
            if coordinate >= knots[knots.len() - 1] {
                let last = knots.len() - 1;
                choices.push((last, last, 1.0, 0.0));
                continue;
            }
            let upper = knots.partition_point(|knot| *knot < coordinate);
            let lower = upper - 1;
            let t = (coordinate - knots[lower]) / (knots[upper] - knots[lower]);
            let lower_weight = (std::f64::consts::PI * t / 2.0).cos();
            let upper_weight = (std::f64::consts::PI * t / 2.0).sin();
            choices.push((lower, upper, lower_weight, upper_weight));
        }
        let mut vector = vec![0.0; self.active_channel_count()];
        let mut indices = Vec::with_capacity(choices.len());
        self.accumulate_corners(0, 1.0, &choices, &mut indices, &mut vector)?;
        Ok(vector)
    }

    fn accumulate_corners(
        &self,
        axis: usize,
        coefficient: f64,
        choices: &[(usize, usize, f64, f64)],
        indices: &mut Vec<usize>,
        vector: &mut [f64],
    ) -> Result<(), CleanProjectionError> {
        if axis == choices.len() {
            let node = self
                .node_vectors
                .iter()
                .find(|node| node.knot_indices.as_slice() == indices.as_slice())
                .ok_or_else(|| CleanProjectionError::MissingNode(indices.clone()))?;
            for (out, value) in vector.iter_mut().zip(&node.vector) {
                *out += coefficient * value;
            }
            return Ok(());
        }
        let (lower, upper, lower_weight, upper_weight) = choices[axis];
        indices.push(lower);
        self.accumulate_corners(
            axis + 1,
            coefficient * lower_weight,
            choices,
            indices,
            vector,
        )?;
        indices.pop();
        if upper != lower {
            indices.push(upper);
            self.accumulate_corners(
                axis + 1,
                coefficient * upper_weight,
                choices,
                indices,
                vector,
            )?;
            indices.pop();
        }
        Ok(())
    }

    fn paired_vector(&self, pair: &CleanPairedGeometry) -> Result<Vec<f64>, CleanProjectionError> {
        if !pair.blend.is_finite()
            || !(0.0..=1.0).contains(&pair.blend)
            || pair.first.len() != self.active_channel_count()
            || pair.second.len() != self.active_channel_count()
            || pair.first.iter().any(|value| !value.is_finite())
            || pair.second.iter().any(|value| !value.is_finite())
        {
            return Err(CleanProjectionError::InvalidPair);
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

fn validate_vector(vector: &[f64], expected: usize) -> Result<(), CleanProjectionError> {
    if vector.len() != expected {
        return Err(CleanProjectionError::VectorDimension {
            expected,
            actual: vector.len(),
        });
    }
    if vector.iter().any(|value| !value.is_finite()) {
        return Err(CleanProjectionError::NonFiniteLayoutValue);
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
pub enum CleanSchedulerError {
    InvalidSampleRate,
    NonFiniteTarget,
    DurationOverflow,
}

impl fmt::Display for CleanSchedulerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSampleRate => formatter.write_str("clean scheduler sample rate is zero"),
            Self::NonFiniteTarget => formatter.write_str("clean scheduler target is non-finite"),
            Self::DurationOverflow => formatter.write_str("clean scheduler duration overflow"),
        }
    }
}

impl std::error::Error for CleanSchedulerError {}

/// Stateful Q32 experimental route scheduler.
#[derive(Clone, Debug, Default)]
pub struct CleanRouteScheduler {
    current: f64,
    target: f64,
    delta: f64,
    remaining_quanta: u64,
    phase: usize,
}

impl CleanRouteScheduler {
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

    /// Applies a target event under the explicitly experimental Q32 profile.
    pub fn set_target(
        &mut self,
        target: f64,
        event: bool,
        duration_samples: u64,
        sample_rate: u32,
    ) -> Result<(), CleanSchedulerError> {
        if sample_rate == 0 {
            return Err(CleanSchedulerError::InvalidSampleRate);
        }
        if !target.is_finite() {
            return Err(CleanSchedulerError::NonFiniteTarget);
        }
        self.target = target;
        if !event {
            return Ok(());
        }
        let rho = u64::from(sample_rate > 48_000) + 1;
        let scaled = duration_samples
            .checked_mul(rho)
            .and_then(|value| value.checked_add(Q32_HALF_MINUS_ONE))
            .ok_or(CleanSchedulerError::DurationOverflow)?;
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

    /// Returns whether this route is above the clean activity floor.
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

/// Top-level errors from the experimental clean accumulation bridge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CleanSpatialBridgeError {
    Binding(CleanBindingError),
    Projection(CleanProjectionError),
    Scheduler(CleanSchedulerError),
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

impl fmt::Display for CleanSpatialBridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Binding(error) => write!(formatter, "clean binding error: {error}"),
            Self::Projection(error) => write!(formatter, "clean projection error: {error}"),
            Self::Scheduler(error) => write!(formatter, "clean scheduler error: {error}"),
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

impl std::error::Error for CleanSpatialBridgeError {}

impl From<CleanBindingError> for CleanSpatialBridgeError {
    fn from(value: CleanBindingError) -> Self {
        Self::Binding(value)
    }
}

impl From<CleanProjectionError> for CleanSpatialBridgeError {
    fn from(value: CleanProjectionError) -> Self {
        Self::Projection(value)
    }
}

impl From<CleanSchedulerError> for CleanSpatialBridgeError {
    fn from(value: CleanSchedulerError) -> Self {
        Self::Scheduler(value)
    }
}

/// Explicitly activated clean experimental bridge with persistent streaming state.
#[derive(Clone, Debug, Default)]
pub struct ExperimentalCleanSpatialBridge {
    binding: CleanBindingState,
    schedulers: Vec<CleanRouteScheduler>,
    targets: Vec<Vec<f64>>,
    last_layout: Option<CleanSpatialLayout>,
}

impl ExperimentalCleanSpatialBridge {
    /// Creates an empty experimental bridge. It has no production-resolved state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            binding: CleanBindingState::new(),
            schedulers: Vec::new(),
            targets: Vec::new(),
            last_layout: None,
        }
    }

    /// Returns the current clean binding state.
    #[must_use]
    pub const fn binding_state(&self) -> &CleanBindingState {
        &self.binding
    }

    /// Returns the truthful project-level semantic binding state.
    #[must_use]
    pub const fn semantic_binding(&self) -> SemanticBindingState {
        SemanticBindingState::Unresolved
    }

    /// This candidate is executable only through its explicit experimental type.
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
        topology: Option<&CleanTopologySnapshot>,
        updates: Option<&[CleanCoordinateUpdate]>,
        layout: &CleanSpatialLayout,
        duration_samples: u64,
        sample_rate: u32,
        outputs: &mut [&mut [f64]],
    ) -> Result<(), CleanSpatialBridgeError> {
        validate_block_shapes(coordinates, layout, outputs)?;
        let block_length = outputs.first().map_or(0, |output| output.len());
        let layout_changed = self
            .last_layout
            .as_ref()
            .is_none_or(|previous| previous != layout);
        let result = self.binding.apply(topology, updates, coordinates.len())?;
        let Some(snapshot) = self.binding.snapshot() else {
            return Err(CleanSpatialBridgeError::Binding(
                CleanBindingError::NoTopologyForInitialization,
            ));
        };
        let active_count = snapshot.active_count;
        let route_shape_changed = layout_changed
            || self.targets.len() != active_count
            || self.schedulers.len() != active_count * layout.active_channel_count();
        let reset_routes = route_shape_changed
            || matches!(
                result.transition,
                CleanBindingTransition::Init | CleanBindingTransition::Rebuild
            );
        if reset_routes {
            self.schedulers = (0..active_count * layout.active_channel_count())
                .map(|_| CleanRouteScheduler::new())
                .collect();
            self.targets = vec![vec![0.0; layout.active_channel_count()]; active_count];
            self.last_layout = Some(layout.clone());
        }
        let refresh_targets = reset_routes || result.event;
        if refresh_targets {
            for (index, target) in self.targets.iter_mut().enumerate() {
                let record = &snapshot.records[index];
                if record.active {
                    let vector = layout.project(&record.descriptor)?;
                    for (target_value, projection) in target.iter_mut().zip(vector) {
                        *target_value = record.scalar * projection;
                    }
                } else {
                    target.fill(0.0);
                }
            }
            for (index, target) in self.targets.iter().enumerate() {
                for (channel, &value) in target.iter().enumerate() {
                    self.schedulers[index * layout.active_channel_count() + channel].set_target(
                        value,
                        true,
                        duration_samples,
                        sample_rate,
                    )?;
                }
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
                            return Err(CleanSpatialBridgeError::NonFiniteOutput {
                                channel,
                                sample,
                            });
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Renders the existing decoded Base/RB frame boundary through the clean
    /// bridge. Base full-band coordinates precede ReconstructionBasis rows;
    /// `RcLfe` remains separate and is not accumulated here.
    pub fn render_codec_basis_frame(
        &mut self,
        frame: &JocSpatialReconstructionFrame<'_>,
        topology: Option<&CleanTopologySnapshot>,
        updates: Option<&[CleanCoordinateUpdate]>,
        layout: &CleanSpatialLayout,
        duration_samples: u64,
        outputs: &mut [&mut [f64]],
    ) -> Result<(), CleanSpatialBridgeError> {
        let mut coordinates = Vec::with_capacity(
            frame.basis.base_full_band_pcm.len() + frame.basis.reconstruction_basis.rows.len(),
        );
        coordinates.extend(frame.basis.base_full_band_pcm.iter().map(Vec::as_slice));
        coordinates.extend(
            frame
                .basis
                .reconstruction_basis
                .rows
                .iter()
                .map(Vec::as_slice),
        );
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

fn validate_block_shapes(
    coordinates: &[&[f64]],
    layout: &CleanSpatialLayout,
    outputs: &[&mut [f64]],
) -> Result<(), CleanSpatialBridgeError> {
    let expected_channels = layout.active_channel_count();
    if outputs.len() != expected_channels {
        return Err(CleanSpatialBridgeError::OutputChannelCount {
            expected: expected_channels,
            actual: outputs.len(),
        });
    }
    let block_length = outputs.first().map_or(0, |output| output.len());
    for (channel, output) in outputs.iter().enumerate() {
        if output.len() != block_length {
            return Err(CleanSpatialBridgeError::OutputLengthMismatch {
                channel,
                expected: block_length,
                actual: output.len(),
            });
        }
    }
    for (coordinate, input) in coordinates.iter().enumerate() {
        if input.len() != block_length {
            return Err(CleanSpatialBridgeError::InputLengthMismatch {
                coordinate,
                expected: block_length,
                actual: input.len(),
            });
        }
        if let Some(sample) = input.iter().position(|value| !value.is_finite()) {
            return Err(CleanSpatialBridgeError::NonFiniteInput { coordinate, sample });
        }
    }
    Ok(())
}
