//! Assembly from decoded JOC/OAMD state to codec-coordinate bridge control.
//!
//! This module is deliberately upstream of [`JocSpatialBridge`]. It owns
//! topology ordering, decoded metadata conversion, and event timing; the
//! bridge continues to own projection, Q32 scheduling, and accumulation.

use crate::{
    BaseFullBandCoordinate, DecodedPayloadFrame, SpatialBindingRecord, SpatialCoordinateUpdate,
    SpatialDescriptor, SpatialDescriptorPatch, SpatialExplicitGroup, SpatialExplicitMember,
    SpatialSourceClass, SpatialTopologySnapshot,
};
use openjoc_oamd::{
    Gain, OamdElement, OamdError, ObjectAnchor, ObjectUpdate, Position3 as OamdPosition3,
    ReferenceScreen, RoomPosition, SpeakerLabel, ZoneConstraint,
};
use std::{collections::HashSet, fmt};

const RHO_HIGH_RATE: u64 = 2;
const MAX_SUPPORTED_DIMENSIONS: usize = 3;

/// One timed selective bridge update. `quantum` is relative to the current
/// decoded frame and is installed at `quantum * 32` output samples.
#[derive(Clone, Debug, PartialEq)]
pub struct BridgeControlEvent {
    pub quantum: u64,
    /// Decoded/base-rate ramp duration. The existing scheduler applies its
    /// rate factor at the unchanged downstream boundary.
    pub ramp_duration: u16,
    pub updates: Vec<SpatialCoordinateUpdate>,
}

/// Assembled control for one decoded frame.
#[derive(Clone, Debug, PartialEq)]
pub struct BridgeControlFrame {
    pub topology_epoch: u64,
    /// Present only for the first valid topology or a topology rebuild.
    pub initial_topology: Option<SpatialTopologySnapshot>,
    pub events: Vec<BridgeControlEvent>,
}

/// Errors at the decoded-state to bridge-control boundary.
#[derive(Clone, Debug, PartialEq)]
pub enum BridgeControlAssemblyError {
    InvalidCapacity,
    CapacityExceeded { capacity: usize, actual: usize },
    InvalidCoordinateDimensions { dimensions: usize },
    InvalidSampleRate,
    UnsupportedSampleRate { sample_rate: u32 },
    EmptyBaseCoordinates,
    DuplicateBaseCoordinate { coordinate: BaseFullBandCoordinate },
    CoordinateCount { expected: usize, actual: usize },
    UnsupportedCoordinateOrdinal { ordinal: usize, count: usize },
    InvalidOamd(OamdError),
    InvalidObjectCount { expected: usize, actual: usize },
    InvalidObjectBlock { object: usize, block: usize },
    MissingBedCoordinate { label: String },
    DuplicateBedCoordinate { label: String },
    MissingInitialPayload,
    UnsupportedTiming { sample: u64, frame_samples: u64 },
    MissingReferenceScreen,
    NonFinitePosition,
    GainOutOfRange { db: i16 },
    NonFiniteScalar,
    TopologyEpochOverflow,
}

impl fmt::Display for BridgeControlAssemblyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCapacity => formatter.write_str("bridge-control capacity must be nonzero"),
            Self::CapacityExceeded { capacity, actual } => write!(
                formatter,
                "bridge-control coordinate count {actual} exceeds capacity {capacity}"
            ),
            Self::InvalidCoordinateDimensions { dimensions } => write!(
                formatter,
                "bridge-control coordinate dimension {dimensions} is outside 1..=3"
            ),
            Self::InvalidSampleRate => formatter.write_str("bridge-control sample rate is zero"),
            Self::UnsupportedSampleRate { sample_rate } => write!(
                formatter,
                "bridge-control sample rate {sample_rate} Hz has no admitted rate factor"
            ),
            Self::EmptyBaseCoordinates => {
                formatter.write_str("bridge-control Base coordinate set is empty")
            }
            Self::DuplicateBaseCoordinate { coordinate } => {
                write!(formatter, "duplicate Base coordinate {coordinate:?}")
            }
            Self::CoordinateCount { expected, actual } => write!(
                formatter,
                "decoded non-LFE coordinate count {actual} does not match bridge-control count {expected}"
            ),
            Self::UnsupportedCoordinateOrdinal { ordinal, count } => write!(
                formatter,
                "bridge-control coordinate ordinal {ordinal} is outside count {count}"
            ),
            Self::InvalidOamd(error) => write!(formatter, "invalid OAMD control state: {error}"),
            Self::InvalidObjectCount { expected, actual } => write!(
                formatter,
                "OAMD object update count {actual} does not match anchor count {expected}"
            ),
            Self::InvalidObjectBlock { object, block } => write!(
                formatter,
                "OAMD object {object} has no update for timing block {block}"
            ),
            Self::MissingBedCoordinate { label } => {
                write!(
                    formatter,
                    "OAMD bed coordinate {label} is absent from decoded Base"
                )
            }
            Self::DuplicateBedCoordinate { label } => {
                write!(
                    formatter,
                    "OAMD bed coordinate {label} occurs more than once"
                )
            }
            Self::MissingInitialPayload => formatter
                .write_str("automatic bridge-control assembly needs an initial object payload"),
            Self::UnsupportedTiming {
                sample,
                frame_samples,
            } => write!(
                formatter,
                "bridge-control event at sample {sample} is outside decoded frame length {frame_samples}"
            ),
            Self::MissingReferenceScreen => formatter.write_str(
                "screen-anchored bridge control requires complete public screen geometry",
            ),
            Self::NonFinitePosition => {
                formatter.write_str("decoded bridge-control position is non-finite")
            }
            Self::GainOutOfRange { db } => {
                write!(
                    formatter,
                    "decoded gain {db} dB is outside the admitted -49..15 dB range"
                )
            }
            Self::NonFiniteScalar => {
                formatter.write_str("decoded bridge-control scalar is non-finite")
            }
            Self::TopologyEpochOverflow => {
                formatter.write_str("bridge-control topology epoch overflow")
            }
        }
    }
}

impl std::error::Error for BridgeControlAssemblyError {}

impl From<OamdError> for BridgeControlAssemblyError {
    fn from(value: OamdError) -> Self {
        Self::InvalidOamd(value)
    }
}

/// Maps one normalized decoded position to the bridge's quantized coordinate
/// domain. One-dimensional layouts consume x; two-dimensional height layouts
/// consume x and normalized ceiling height; three-dimensional layouts retain
/// x/y/z.
pub fn bridge_position(
    position: OamdPosition3,
    dimensions: usize,
) -> Result<Vec<f64>, BridgeControlAssemblyError> {
    if dimensions == 0 || dimensions > MAX_SUPPORTED_DIMENSIONS {
        return Err(BridgeControlAssemblyError::InvalidCoordinateDimensions { dimensions });
    }
    if [position.x, position.y, position.z]
        .iter()
        .any(|value| !value.is_finite())
    {
        return Err(BridgeControlAssemblyError::NonFinitePosition);
    }
    Ok(match dimensions {
        1 => vec![bridge_quantize(position.x, false)],
        2 => vec![
            bridge_quantize(position.x, false),
            bridge_quantize(position.z.midpoint(1.0), false),
        ],
        3 => vec![
            bridge_quantize(position.x, false),
            bridge_quantize(position.y, false),
            bridge_quantize(position.z, true),
        ],
        _ => unreachable!("dimensions validated above"),
    })
}

/// Applies the bridge-coordinate Q15 quantization contract.
#[must_use]
pub fn bridge_quantize(value: f64, signed: bool) -> f64 {
    let magnitude = if signed {
        value.abs().clamp(0.0, 1.0)
    } else {
        value.clamp(0.0, 1.0)
    };
    let quantized = (32768.0 * magnitude + 0.5).floor().min(32767.0) / 32768.0;
    if signed && value.is_sign_negative() {
        -quantized
    } else {
        quantized
    }
}

/// Applies the bridge extent quantization contract to a normalized extent.
#[must_use]
pub fn bridge_quantize_extent(value: f64) -> f64 {
    bridge_quantize(value, false)
}

/// Converts a decoded OAMD gain to the finite bridge scalar.
pub fn bridge_gain_scalar(gain: Gain) -> Result<f64, BridgeControlAssemblyError> {
    let scalar = match gain {
        Gain::NegativeInfinity => 0.0,
        Gain::Decibels(db) => {
            if !(-49..=15).contains(&db) {
                return Err(BridgeControlAssemblyError::GainOutOfRange { db });
            }
            (4096.0 * 10_f64.powf(f64::from(db) / 20.0)).floor() / 4096.0
        }
    };
    if scalar.is_finite() {
        Ok(scalar)
    } else {
        Err(BridgeControlAssemblyError::NonFiniteScalar)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CoordinateFamily {
    Base,
    Fixed,
    Dynamic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CoordinateBinding {
    family: CoordinateFamily,
    object_index: Option<usize>,
    anchor: Option<ObjectAnchor>,
}

/// Stateful clean bridge-control assembler.
#[derive(Clone, Debug)]
pub struct BridgeControlAssembler {
    capacity: usize,
    coordinate_dimensions: usize,
    topology_epoch: u64,
    signature: Option<Vec<(SpatialSourceClass, String)>>,
    bindings: Vec<CoordinateBinding>,
    records: Option<Vec<SpatialBindingRecord>>,
}

impl BridgeControlAssembler {
    /// Creates a streaming assembler for one public layout dimension.
    #[must_use]
    pub fn new(capacity: usize, coordinate_dimensions: usize) -> Self {
        Self {
            capacity,
            coordinate_dimensions,
            topology_epoch: 0,
            signature: None,
            bindings: Vec::new(),
            records: None,
        }
    }

    /// Clears topology, retained metadata state, and epoch history.
    pub fn reset(&mut self) {
        self.topology_epoch = 0;
        self.signature = None;
        self.bindings.clear();
        self.records = None;
    }

    /// Returns the current epoch, or zero before the first valid topology.
    #[must_use]
    pub const fn topology_epoch(&self) -> u64 {
        self.topology_epoch
    }

    /// Assembles one decoded frame without materializing file-duration state.
    ///
    /// # Errors
    ///
    /// Returns a boundary error when decoded coordinates, metadata, timing,
    /// or the selected public geometry cannot be represented faithfully.
    pub fn assemble_frame(
        &mut self,
        frame: &DecodedPayloadFrame,
        base_coordinates: &[BaseFullBandCoordinate],
        reference_screen: Option<ReferenceScreen>,
    ) -> Result<BridgeControlFrame, BridgeControlAssemblyError> {
        self.validate_inputs(frame, base_coordinates)?;
        let has_object_payload = frame
            .oamd
            .elements
            .iter()
            .any(|element| matches!(element.element, OamdElement::Objects(_)));
        if self.records.is_none() && !has_object_payload {
            return Err(BridgeControlAssemblyError::MissingInitialPayload);
        }

        let (bindings, families, signature) = build_bindings(
            &frame.oamd.prefix,
            base_coordinates,
            frame.decoded.reconstruction_basis.rows.len(),
        )?;
        if bindings.len() > self.capacity {
            return Err(BridgeControlAssemblyError::CapacityExceeded {
                capacity: self.capacity,
                actual: bindings.len(),
            });
        }
        let topology_changed = self.signature.as_ref() != Some(&signature);
        let first_topology = self.records.is_none();
        if topology_changed {
            self.topology_epoch = self
                .topology_epoch
                .checked_add(1)
                .ok_or(BridgeControlAssemblyError::TopologyEpochOverflow)?;
            self.signature = Some(signature);
            self.bindings = bindings;
            self.records = Some(default_records(
                base_coordinates,
                &families,
                &self.bindings,
                self.coordinate_dimensions,
            )?);
        } else {
            self.bindings = bindings;
        }

        let frame_samples = frame.sample_range.len();
        let mut events = Vec::new();
        let mut raw_events = Vec::new();
        for (element_index, element) in frame.oamd.elements.iter().enumerate() {
            let OamdElement::Objects(objects) = &element.element else {
                continue;
            };
            if objects.objects.len() != frame.oamd.prefix.object_anchors()?.len() {
                return Err(BridgeControlAssemblyError::InvalidObjectCount {
                    expected: frame.oamd.prefix.object_anchors()?.len(),
                    actual: objects.objects.len(),
                });
            }
            for (block_index, block) in objects.timing.blocks.iter().enumerate() {
                let relative_sample = u64::from(block.start_sample);
                let quantum = quantum_for_event(relative_sample, frame.sample_rate)?;
                let output_sample = quantum
                    .checked_mul(32)
                    .ok_or(BridgeControlAssemblyError::TopologyEpochOverflow)?;
                if output_sample >= frame_samples {
                    return Err(BridgeControlAssemblyError::UnsupportedTiming {
                        sample: output_sample,
                        frame_samples,
                    });
                }
                raw_events.push((
                    quantum,
                    element_index,
                    block_index,
                    block.ramp_duration,
                    objects,
                ));
            }
        }
        raw_events.sort_by_key(|(quantum, element, block, _, _)| (*quantum, *element, *block));
        let mut event_index = 0;
        while event_index < raw_events.len() {
            let quantum = raw_events[event_index].0;
            let before = self
                .records
                .as_ref()
                .ok_or(BridgeControlAssemblyError::MissingInitialPayload)?
                .clone();
            let mut ramp_duration = 0;
            while event_index < raw_events.len() && raw_events[event_index].0 == quantum {
                let (_, _, block_index, block_ramp, objects) = &raw_events[event_index];
                self.apply_block(&objects.objects, *block_index, reference_screen)?;
                ramp_duration = *block_ramp;
                event_index += 1;
            }
            let after = self
                .records
                .as_ref()
                .ok_or(BridgeControlAssemblyError::MissingInitialPayload)?;
            let updates = selective_diff(&before, after);
            if !updates.is_empty() {
                events.push(BridgeControlEvent {
                    quantum,
                    ramp_duration,
                    updates,
                });
            }
        }

        let initial_topology = if first_topology || topology_changed {
            Some(
                self.topology_snapshot()
                    .ok_or(BridgeControlAssemblyError::MissingInitialPayload)?,
            )
        } else {
            None
        };
        Ok(BridgeControlFrame {
            topology_epoch: self.topology_epoch,
            initial_topology,
            events,
        })
    }

    fn validate_inputs(
        &self,
        frame: &DecodedPayloadFrame,
        base_coordinates: &[BaseFullBandCoordinate],
    ) -> Result<(), BridgeControlAssemblyError> {
        if self.capacity == 0 {
            return Err(BridgeControlAssemblyError::InvalidCapacity);
        }
        if self.coordinate_dimensions == 0 || self.coordinate_dimensions > MAX_SUPPORTED_DIMENSIONS
        {
            return Err(BridgeControlAssemblyError::InvalidCoordinateDimensions {
                dimensions: self.coordinate_dimensions,
            });
        }
        if frame.sample_rate == 0 {
            return Err(BridgeControlAssemblyError::InvalidSampleRate);
        }
        if frame.sample_rate > 96_000 {
            return Err(BridgeControlAssemblyError::UnsupportedSampleRate {
                sample_rate: frame.sample_rate,
            });
        }
        if base_coordinates.is_empty() {
            return Err(BridgeControlAssemblyError::EmptyBaseCoordinates);
        }
        let mut seen = HashSet::with_capacity(base_coordinates.len());
        for &coordinate in base_coordinates {
            if !seen.insert(coordinate) {
                return Err(BridgeControlAssemblyError::DuplicateBaseCoordinate { coordinate });
            }
        }
        Ok(())
    }

    fn apply_block(
        &mut self,
        objects: &[Vec<ObjectUpdate>],
        block_index: usize,
        reference_screen: Option<ReferenceScreen>,
    ) -> Result<(), BridgeControlAssemblyError> {
        for (ordinal, binding) in self.bindings.clone().into_iter().enumerate() {
            let Some(object_index) = binding.object_index else {
                continue;
            };
            let update = objects
                .get(object_index)
                .and_then(|updates| updates.get(block_index))
                .ok_or(BridgeControlAssemblyError::InvalidObjectBlock {
                    object: object_index,
                    block: block_index,
                })?;
            let Some(records) = self.records.as_mut() else {
                return Err(BridgeControlAssemblyError::MissingInitialPayload);
            };
            let count = records.len();
            let Some(record) = records.get_mut(ordinal) else {
                return Err(BridgeControlAssemblyError::UnsupportedCoordinateOrdinal {
                    ordinal,
                    count,
                });
            };
            if update.active {
                let previous = record.clone();
                record.descriptor = descriptor_for(
                    binding.anchor,
                    binding.family,
                    update,
                    &previous.descriptor,
                    self.coordinate_dimensions,
                    reference_screen,
                )?;
                record.scalar = bridge_gain_scalar(update.basic.gain)?;
                record.active = true;
            } else {
                // An inactive record keeps its descriptor and scalar. The
                // downstream bridge independently forces its contribution to
                // zero from the active bit.
                record.active = false;
            }
        }
        Ok(())
    }

    fn topology_snapshot(&self) -> Option<SpatialTopologySnapshot> {
        let records = self.records.as_ref()?;
        let base_count = self
            .bindings
            .iter()
            .take_while(|binding| binding.family == CoordinateFamily::Base)
            .count();
        let fixed_count = self
            .bindings
            .iter()
            .skip(base_count)
            .take_while(|binding| binding.family == CoordinateFamily::Fixed)
            .count();
        let explicit_groups = records[..base_count]
            .iter()
            .enumerate()
            .map(|(index, record)| SpatialExplicitGroup {
                group_order: u32::try_from(index).unwrap_or(u32::MAX),
                members: vec![SpatialExplicitMember {
                    canonical_label: record.descriptor.identity.clone(),
                    record: record.clone(),
                }],
            })
            .collect();
        Some(SpatialTopologySnapshot {
            explicit_groups,
            fixed_layout: records[base_count..base_count + fixed_count].to_vec(),
            dynamic_records: records[base_count + fixed_count..].to_vec(),
        })
    }
}

fn quantum_for_event(
    relative_sample: u64,
    sample_rate: u32,
) -> Result<u64, BridgeControlAssemblyError> {
    if sample_rate == 0 {
        return Err(BridgeControlAssemblyError::InvalidSampleRate);
    }
    if sample_rate > 96_000 {
        return Err(BridgeControlAssemblyError::UnsupportedSampleRate { sample_rate });
    }
    let rho = if sample_rate > 48_000 {
        RHO_HIGH_RATE
    } else {
        1
    };
    let scaled = relative_sample
        .checked_mul(rho)
        .ok_or(BridgeControlAssemblyError::TopologyEpochOverflow)?;
    Ok(scaled.saturating_sub(16).saturating_add(31) / 32)
}

type BindingFamilyRecords = Vec<(CoordinateFamily, Option<ObjectAnchor>, String)>;
type BindingSignature = Vec<(SpatialSourceClass, String)>;

fn build_bindings(
    prefix: &openjoc_oamd::OamdContentPrefix,
    base_coordinates: &[BaseFullBandCoordinate],
    reconstruction_count: usize,
) -> Result<
    (
        Vec<CoordinateBinding>,
        BindingFamilyRecords,
        BindingSignature,
    ),
    BridgeControlAssemblyError,
> {
    let anchors = prefix.object_anchors()?;
    let bed_indices = anchors
        .iter()
        .enumerate()
        .filter_map(|(index, anchor)| {
            matches!(anchor, ObjectAnchor::Speaker(label) if !is_lfe(*label)).then_some(index)
        })
        .collect::<Vec<_>>();
    if !bed_indices.is_empty() && bed_indices.len() != base_coordinates.len() {
        return Err(BridgeControlAssemblyError::CoordinateCount {
            expected: bed_indices.len(),
            actual: base_coordinates.len(),
        });
    }
    let non_base_indices = anchors
        .iter()
        .enumerate()
        .filter_map(|(index, anchor)| match anchor {
            ObjectAnchor::IntermediateSpatial(_) | ObjectAnchor::Dynamic => Some(index),
            ObjectAnchor::Speaker(label) if is_lfe(*label) => None,
            ObjectAnchor::Speaker(_) => None,
        })
        .collect::<Vec<_>>();
    if reconstruction_count != non_base_indices.len() {
        return Err(BridgeControlAssemblyError::CoordinateCount {
            expected: non_base_indices.len(),
            actual: reconstruction_count,
        });
    }

    let mut bindings = Vec::with_capacity(base_coordinates.len() + reconstruction_count);
    let mut families = Vec::with_capacity(bindings.capacity());
    let mut signature = Vec::with_capacity(bindings.capacity());
    for &coordinate in base_coordinates {
        let identity = base_identity(coordinate);
        let object_index = if bed_indices.is_empty() {
            None
        } else {
            let matches = bed_indices
                .iter()
                .copied()
                .filter(|index| match anchors[*index] {
                    ObjectAnchor::Speaker(label) => speaker_identity(label) == identity,
                    _ => false,
                });
            let matching = matches.collect::<Vec<_>>();
            if matching.len() > 1 {
                return Err(BridgeControlAssemblyError::DuplicateBedCoordinate { label: identity });
            }
            Some(*matching.first().ok_or_else(|| {
                BridgeControlAssemblyError::MissingBedCoordinate {
                    label: identity.clone(),
                }
            })?)
        };
        bindings.push(CoordinateBinding {
            family: CoordinateFamily::Base,
            object_index,
            anchor: object_index.map(|index| anchors[index]),
        });
        families.push((
            CoordinateFamily::Base,
            object_index.map(|index| anchors[index]),
            identity.clone(),
        ));
        signature.push((SpatialSourceClass::ExplicitChannel, identity));
    }
    for object_index in non_base_indices {
        let anchor = anchors[object_index];
        let (family, class, identity) = match anchor {
            ObjectAnchor::IntermediateSpatial(label) => (
                CoordinateFamily::Fixed,
                SpatialSourceClass::FixedLayout,
                isf_identity(label),
            ),
            ObjectAnchor::Dynamic => (
                CoordinateFamily::Dynamic,
                SpatialSourceClass::DynamicPoint,
                format!("dynamic-{object_index}"),
            ),
            ObjectAnchor::Speaker(_) => unreachable!("LFE and beds were removed above"),
        };
        bindings.push(CoordinateBinding {
            family,
            object_index: Some(object_index),
            anchor: Some(anchor),
        });
        families.push((family, Some(anchor), identity.clone()));
        signature.push((class, identity));
    }
    Ok((bindings, families, signature))
}

fn default_records(
    base_coordinates: &[BaseFullBandCoordinate],
    families: &[(CoordinateFamily, Option<ObjectAnchor>, String)],
    bindings: &[CoordinateBinding],
    dimensions: usize,
) -> Result<Vec<SpatialBindingRecord>, BridgeControlAssemblyError> {
    let mut records = Vec::with_capacity(families.len());
    for (index, ((family, anchor, identity), binding)) in families.iter().zip(bindings).enumerate()
    {
        let class = match family {
            CoordinateFamily::Base => SpatialSourceClass::ExplicitChannel,
            CoordinateFamily::Fixed => SpatialSourceClass::FixedLayout,
            CoordinateFamily::Dynamic => SpatialSourceClass::DynamicPoint,
        };
        let coordinates = if *family == CoordinateFamily::Dynamic {
            default_coordinates(dimensions)
        } else {
            Vec::new()
        };
        let (active, scalar) = if *family == CoordinateFamily::Base {
            (true, 1.0)
        } else {
            (false, 0.0)
        };
        let base_identity_matches = base_coordinates
            .get(index)
            .is_some_and(|coordinate| base_identity(*coordinate) == *identity);
        if *family == CoordinateFamily::Base && !base_identity_matches {
            return Err(BridgeControlAssemblyError::UnsupportedCoordinateOrdinal {
                ordinal: index,
                count: families.len(),
            });
        }
        let descriptor = SpatialDescriptor {
            source_class: class,
            identity: identity.clone(),
            coordinates,
            spread: None,
            paired: None,
            raw3: None,
            extent: None,
            zones: None,
        };
        let _ = anchor;
        let _ = binding;
        records.push(SpatialBindingRecord {
            descriptor,
            scalar,
            active,
        });
    }
    Ok(records)
}

fn descriptor_for(
    anchor: Option<ObjectAnchor>,
    family: CoordinateFamily,
    update: &ObjectUpdate,
    previous: &SpatialDescriptor,
    dimensions: usize,
    reference_screen: Option<ReferenceScreen>,
) -> Result<SpatialDescriptor, BridgeControlAssemblyError> {
    let (source_class, identity, coordinates) = match (family, anchor) {
        (CoordinateFamily::Base, Some(ObjectAnchor::Speaker(label))) => (
            SpatialSourceClass::ExplicitChannel,
            speaker_identity(label),
            Vec::new(),
        ),
        (CoordinateFamily::Fixed, Some(ObjectAnchor::IntermediateSpatial(label))) => (
            SpatialSourceClass::FixedLayout,
            isf_identity(label),
            Vec::new(),
        ),
        (CoordinateFamily::Dynamic, Some(ObjectAnchor::Dynamic)) => {
            let point = if update.render.screen_anchor {
                let screen =
                    reference_screen.ok_or(BridgeControlAssemblyError::MissingReferenceScreen)?;
                update
                    .render
                    .screen_position(screen)?
                    .ok_or(BridgeControlAssemblyError::MissingReferenceScreen)?
            } else {
                match update.render.room_position()? {
                    RoomPosition::Finite(position)
                    | RoomPosition::AtInfinity {
                        boundary_intersection: position,
                    } => position,
                }
            };
            let raw3 = update
                .additional_table_data
                .clone()
                .or_else(|| previous.raw3.clone());
            let point_size = update.render.size.width == 0.0
                && update.render.size.depth == 0.0
                && update.render.size.height == 0.0;
            let guarded = raw3
                .as_ref()
                .is_some_and(|value| value.iter().any(|byte| *byte != 0));
            let class = if point_size || update.render.channel_lock || guarded {
                SpatialSourceClass::DynamicPoint
            } else {
                SpatialSourceClass::DynamicRegion
            };
            return Ok(SpatialDescriptor {
                source_class: class,
                identity: previous.identity.clone(),
                coordinates: bridge_position(point, dimensions)?,
                spread: previous.spread.clone(),
                paired: previous.paired.clone(),
                raw3,
                extent: Some([
                    bridge_quantize_extent(update.render.size.width),
                    bridge_quantize_extent(update.render.size.depth),
                    bridge_quantize_extent(update.render.size.height),
                ]),
                zones: Some(
                    update
                        .render
                        .zones
                        .map(|zone| zone == ZoneConstraint::Include),
                ),
            });
        }
        _ => return Ok(previous.clone()),
    };
    Ok(SpatialDescriptor {
        source_class,
        identity,
        coordinates,
        spread: previous.spread.clone(),
        paired: previous.paired.clone(),
        raw3: update
            .additional_table_data
            .clone()
            .or_else(|| previous.raw3.clone()),
        extent: Some([
            bridge_quantize_extent(update.render.size.width),
            bridge_quantize_extent(update.render.size.depth),
            bridge_quantize_extent(update.render.size.height),
        ]),
        zones: Some(
            update
                .render
                .zones
                .map(|zone| zone == ZoneConstraint::Include),
        ),
    })
}

fn selective_diff(
    before: &[SpatialBindingRecord],
    after: &[SpatialBindingRecord],
) -> Vec<SpatialCoordinateUpdate> {
    before
        .iter()
        .zip(after)
        .enumerate()
        .filter_map(|(ordinal, (previous, next))| {
            if previous == next {
                return None;
            }
            if previous.descriptor == next.descriptor {
                return Some(SpatialCoordinateUpdate {
                    ordinal,
                    descriptor: None,
                    scalar: (previous.scalar != next.scalar).then_some(next.scalar),
                    active: (previous.active != next.active).then_some(next.active),
                });
            }
            Some(SpatialCoordinateUpdate {
                ordinal,
                descriptor: Some(SpatialDescriptorPatch {
                    source_class: Some(next.descriptor.source_class.clone()),
                    identity: Some(next.descriptor.identity.clone()),
                    coordinates: Some(next.descriptor.coordinates.clone()),
                    spread: Some(next.descriptor.spread.clone()),
                    paired: Some(next.descriptor.paired.clone()),
                    raw3: Some(next.descriptor.raw3.clone()),
                    extent: Some(next.descriptor.extent),
                    zones: Some(next.descriptor.zones),
                }),
                scalar: Some(next.scalar),
                active: Some(next.active),
            })
        })
        .collect()
}

fn default_coordinates(dimensions: usize) -> Vec<f64> {
    match dimensions {
        1 => vec![bridge_quantize(0.5, false)],
        2 => vec![bridge_quantize(0.5, false), bridge_quantize(0.5, false)],
        3 => vec![
            bridge_quantize(0.5, false),
            bridge_quantize(0.5, false),
            bridge_quantize(0.0, true),
        ],
        _ => Vec::new(),
    }
}

fn is_lfe(label: SpeakerLabel) -> bool {
    matches!(label, SpeakerLabel::RcLfe | SpeakerLabel::RcLfe2)
}

fn base_identity(coordinate: BaseFullBandCoordinate) -> String {
    match coordinate {
        BaseFullBandCoordinate::Left => "FL".to_owned(),
        BaseFullBandCoordinate::Right => "FR".to_owned(),
        BaseFullBandCoordinate::Centre => "FC".to_owned(),
        BaseFullBandCoordinate::LeftSurround => "Ls".to_owned(),
        BaseFullBandCoordinate::RightSurround => "Rs".to_owned(),
        BaseFullBandCoordinate::LeftBack => "Lb".to_owned(),
        BaseFullBandCoordinate::RightBack => "Rb".to_owned(),
        BaseFullBandCoordinate::TopFrontLeft => "TFL".to_owned(),
        BaseFullBandCoordinate::TopFrontRight => "TFR".to_owned(),
        BaseFullBandCoordinate::Other(value) => format!("Other-{value}"),
    }
}

fn speaker_identity(label: SpeakerLabel) -> String {
    match label {
        SpeakerLabel::RcL => "FL",
        SpeakerLabel::RcR => "FR",
        SpeakerLabel::RcC => "FC",
        SpeakerLabel::RcLs => "Ls",
        SpeakerLabel::RcRs => "Rs",
        SpeakerLabel::RcLb => "Lb",
        SpeakerLabel::RcRb => "Rb",
        SpeakerLabel::RcTfl => "TFL",
        SpeakerLabel::RcTfr => "TFR",
        SpeakerLabel::RcTsl => "TSL",
        SpeakerLabel::RcTsr => "TSR",
        SpeakerLabel::RcTbl => "TBL",
        SpeakerLabel::RcTbr => "TBR",
        SpeakerLabel::RcLw => "Lw",
        SpeakerLabel::RcRw => "Rw",
        SpeakerLabel::RcLfe => "LFE",
        SpeakerLabel::RcLfe2 => "LFE2",
    }
    .to_owned()
}

fn isf_identity(label: openjoc_oamd::IsfLabel) -> String {
    let prefix = match label.ring {
        openjoc_oamd::IsfRing::Middle => 'M',
        openjoc_oamd::IsfRing::Upper => 'U',
        openjoc_oamd::IsfRing::Lower => 'L',
        openjoc_oamd::IsfRing::Zenith => 'Z',
    };
    format!("{prefix}{}", label.index)
}

#[cfg(test)]
mod tests {
    use super::{
        bridge_gain_scalar, bridge_position, bridge_quantize, bridge_quantize_extent,
        quantum_for_event,
    };
    use crate::SpatialSourceClass;
    use openjoc_oamd::{Gain, Position3};

    #[test]
    fn bc01_position_mapping() {
        assert_eq!(
            bridge_position(
                Position3 {
                    x: 0.5,
                    y: 0.25,
                    z: 0.0
                },
                1
            )
            .unwrap(),
            vec![0.5]
        );
        assert_eq!(
            bridge_position(
                Position3 {
                    x: 0.5,
                    y: 0.25,
                    z: 1.0
                },
                2
            )
            .unwrap(),
            vec![0.5, 1.0 - 1.0 / 32768.0]
        );
    }

    #[test]
    fn bc02_position_boundaries() {
        assert_eq!(bridge_quantize(-1.0, false), 0.0);
        assert_eq!(bridge_quantize(2.0, false), 32767.0 / 32768.0);
        assert_eq!(bridge_quantize(-2.0, true), -32767.0 / 32768.0);
        assert_eq!(bridge_quantize_extent(0.0), 0.0);
        assert_eq!(bridge_quantize_extent(1.0), 32767.0 / 32768.0);
    }

    #[test]
    fn bc03_gain_reference() {
        assert_eq!(bridge_gain_scalar(Gain::Decibels(0)).unwrap(), 1.0);
        assert_eq!(
            bridge_gain_scalar(Gain::Decibels(-6)).unwrap(),
            (4096.0 * 10_f64.powf(-6.0 / 20.0)).floor() / 4096.0
        );
    }

    #[test]
    fn bc04_gain_silence() {
        assert_eq!(bridge_gain_scalar(Gain::NegativeInfinity).unwrap(), 0.0);
    }

    #[test]
    fn bc06_first_timed_update() {
        assert_eq!(quantum_for_event(16, 48_000).unwrap(), 0);
        assert_eq!(quantum_for_event(17, 48_000).unwrap(), 1);
    }

    #[test]
    fn bc07_subframe_update() {
        assert_eq!(quantum_for_event(8, 96_000).unwrap(), 0);
        assert_eq!(quantum_for_event(9, 96_000).unwrap(), 1);
    }

    #[test]
    fn bc05_active_zero_inactive_absent() {
        use crate::{SpatialBindingRecord, SpatialDescriptor, SpatialSourceClass};
        let descriptor = SpatialDescriptor::new(SpatialSourceClass::DynamicPoint, "d", vec![0.5]);
        let active = SpatialBindingRecord {
            descriptor: descriptor.clone(),
            scalar: 0.0,
            active: true,
        };
        let inactive = SpatialBindingRecord {
            descriptor: descriptor.clone(),
            scalar: 0.0,
            active: false,
        };
        assert_eq!(
            super::selective_diff(std::slice::from_ref(&active), std::slice::from_ref(&active)),
            Vec::new()
        );
        assert_eq!(
            super::selective_diff(
                std::slice::from_ref(&inactive),
                std::slice::from_ref(&inactive)
            ),
            Vec::new()
        );
        assert_eq!(
            super::selective_diff(
                &[inactive],
                &[SpatialBindingRecord {
                    descriptor,
                    scalar: 0.0,
                    active: true,
                }]
            )[0]
            .active,
            Some(true)
        );
    }

    #[test]
    fn bc08_no_new_payload_inheritance() {
        use crate::{SpatialBindingRecord, SpatialDescriptor, SpatialSourceClass};
        let record = SpatialBindingRecord {
            descriptor: SpatialDescriptor::new(SpatialSourceClass::DynamicPoint, "d", vec![0.5]),
            scalar: 1.0,
            active: true,
        };
        assert!(
            super::selective_diff(std::slice::from_ref(&record), std::slice::from_ref(&record))
                .is_empty()
        );
    }

    #[test]
    fn bc09_selective_update() {
        use crate::{SpatialBindingRecord, SpatialDescriptor, SpatialSourceClass};
        let before = SpatialBindingRecord {
            descriptor: SpatialDescriptor::new(SpatialSourceClass::DynamicPoint, "d", vec![0.5]),
            scalar: 1.0,
            active: true,
        };
        let mut after = before.clone();
        after.scalar = 0.0;
        let update = super::selective_diff(&[before], &[after]).pop().unwrap();
        assert!(update.descriptor.is_none());
        assert_eq!(update.scalar, Some(0.0));
        assert_eq!(update.active, None);
    }

    #[test]
    fn bc10_topology_rebuild() {
        use openjoc_oamd::{ContentDescription, OamdContentPrefix};
        let first = OamdContentPrefix {
            syntax_version: 0,
            object_count: 1,
            content: ContentDescription::DynamicOnly { lfe_present: false },
            alternate_object_data_present: false,
            element_count: 0,
            consumed_bits: 0,
        };
        let second = OamdContentPrefix {
            object_count: 2,
            ..first.clone()
        };
        let base = [crate::BaseFullBandCoordinate::Left];
        let (_, _, first_signature) = super::build_bindings(&first, &base, 1).unwrap();
        let (_, _, second_signature) = super::build_bindings(&second, &base, 2).unwrap();
        assert_ne!(first_signature, second_signature);
    }

    #[test]
    fn bc11_canonical_coordinate_ordinal() {
        use openjoc_oamd::{ContentDescription, OamdContentPrefix};
        let prefix = OamdContentPrefix {
            syntax_version: 0,
            object_count: 2,
            content: ContentDescription::DynamicOnly { lfe_present: false },
            alternate_object_data_present: false,
            element_count: 0,
            consumed_bits: 0,
        };
        let base = [
            crate::BaseFullBandCoordinate::Left,
            crate::BaseFullBandCoordinate::Right,
        ];
        let (bindings, _, _) = super::build_bindings(&prefix, &base, 2).unwrap();
        assert_eq!(bindings.len(), 4);
        assert_eq!(bindings[0].family, super::CoordinateFamily::Base);
        assert_eq!(bindings[2].object_index, Some(0));
        assert_eq!(bindings[3].object_index, Some(1));
    }

    #[test]
    fn bc12_source_class() {
        use openjoc_oamd::{BedAssignment, ContentDescription, OamdContentPrefix};
        let prefix = OamdContentPrefix {
            syntax_version: 0,
            object_count: 6,
            content: ContentDescription::Mixed {
                bed_channel_distribute: Some(false),
                beds: vec![BedAssignment::Standard(1 << 8)],
                intermediate_spatial_format: Some(0),
                dynamic_objects: Some(1),
            },
            alternate_object_data_present: false,
            element_count: 0,
            consumed_bits: 0,
        };
        let base = [crate::BaseFullBandCoordinate::Centre];
        let (_, families, signature) = super::build_bindings(&prefix, &base, 5).unwrap();
        assert_eq!(signature[0].0, SpatialSourceClass::ExplicitChannel);
        assert_eq!(signature[1].0, SpatialSourceClass::FixedLayout);
        assert_eq!(families.last().unwrap().0, super::CoordinateFamily::Dynamic);
    }

    #[test]
    fn bc13_raw3_opaque_roundtrip() {
        use openjoc_oamd::{ObjectBasicInfo, ObjectRenderInfo, ObjectUpdate};
        let update = ObjectUpdate {
            active: true,
            basic: ObjectBasicInfo {
                gain: Gain::Decibels(0),
                priority: 0.0,
            },
            render: ObjectRenderInfo::DEFAULT,
            additional_table_data: Some(vec![3, 1]),
        };
        let previous =
            crate::SpatialDescriptor::new(SpatialSourceClass::DynamicPoint, "dynamic-0", vec![0.5]);
        let descriptor = super::descriptor_for(
            Some(openjoc_oamd::ObjectAnchor::Dynamic),
            super::CoordinateFamily::Dynamic,
            &update,
            &previous,
            1,
            None,
        )
        .unwrap();
        assert_eq!(descriptor.raw3, Some(vec![3, 1]));
        assert_eq!(descriptor.source_class, SpatialSourceClass::DynamicPoint);
    }

    #[test]
    fn bc14_lfe_exclusion() {
        use openjoc_oamd::{ContentDescription, OamdContentPrefix};
        let prefix = OamdContentPrefix {
            syntax_version: 0,
            object_count: 2,
            content: ContentDescription::DynamicOnly { lfe_present: true },
            alternate_object_data_present: false,
            element_count: 0,
            consumed_bits: 0,
        };
        let base = [crate::BaseFullBandCoordinate::Left];
        let (bindings, _, _) = super::build_bindings(&prefix, &base, 1).unwrap();
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[1].object_index, Some(1));
    }

    fn equivalence_frame() -> crate::DecodedPayloadFrame {
        use crate::{DecodedPayloadFrame, ProgrammeLayout, SampleRange};
        use openjoc_joc::{DecodedJocFrame, JocFrame, JocHeader, ReconstructionBasis};
        use openjoc_oamd::{
            ContentDescription, MetadataBlockTiming, MetadataTiming, OamdContentPrefix,
            OamdElement, OamdElementMetadata, OamdPayload, ObjectBasicInfo, ObjectClass,
            ObjectElement, ObjectRenderInfo, ObjectUpdate,
        };
        let prefix = OamdContentPrefix {
            syntax_version: 0,
            object_count: 1,
            content: ContentDescription::DynamicOnly { lfe_present: false },
            alternate_object_data_present: false,
            element_count: 1,
            consumed_bits: 0,
        };
        DecodedPayloadFrame {
            frame_index: 0,
            sample_rate: 48_000,
            sample_range: SampleRange::new(0, 64).unwrap(),
            joc: JocFrame {
                header: JocHeader {
                    downmix_index: 0,
                    channel_count: 5,
                    object_count_bits: 0,
                    object_count: 1,
                    extension_index: 0,
                },
                clip_gain_x_bits: 0,
                clip_gain_y_bits: 0,
                sequence_count: 0,
                objects: Vec::new(),
            },
            oamd: OamdPayload {
                prefix: prefix.clone(),
                object_classes: vec![ObjectClass::Dynamic],
                elements: vec![OamdElementMetadata {
                    id: 1,
                    alternate_data_id: None,
                    discard_unknown: false,
                    element: OamdElement::Objects(ObjectElement {
                        timing: MetadataTiming {
                            sample_offset: 0,
                            blocks: vec![MetadataBlockTiming {
                                start_sample: 0,
                                ramp_duration: 0,
                            }],
                        },
                        objects: vec![vec![ObjectUpdate {
                            active: true,
                            basic: ObjectBasicInfo {
                                gain: Gain::Decibels(0),
                                priority: 0.0,
                            },
                            render: ObjectRenderInfo::DEFAULT,
                            additional_table_data: None,
                        }]],
                        consumed_bits: 0,
                    }),
                }],
                consumed_bits: 0,
            },
            decoded: DecodedJocFrame {
                reconstruction_qmf: Vec::new(),
                reconstruction_basis: ReconstructionBasis {
                    rows: vec![vec![0.0; 64]],
                },
                stages: Vec::new(),
                state_reset: true,
            },
            programme_layout: ProgrammeLayout::from_prefix(&prefix).unwrap(),
        }
    }

    #[test]
    fn bc15_automatic_assembly_matches_explicit_sidecar() {
        use crate::{
            BaseFullBandCoordinate, JocSpatialBridge, SpatialLayout, SpatialLayoutChannel,
            SpatialLayoutNode,
        };
        let frame = equivalence_frame();
        let mut assembler = super::BridgeControlAssembler::new(8, 1);
        let control = assembler
            .assemble_frame(&frame, &[BaseFullBandCoordinate::Left], None)
            .unwrap();
        let automatic_topology = control.initial_topology.as_ref().unwrap();
        let sidecar_topology: crate::SpatialTopologySnapshot =
            serde_json::from_value(serde_json::to_value(automatic_topology).unwrap()).unwrap();
        assert_eq!(automatic_topology.flatten(), sidecar_topology.flatten());

        let layout = SpatialLayout::new(
            vec![SpatialLayoutChannel {
                identity: "FL".to_owned(),
                enabled: true,
                lfe: false,
            }],
            vec![vec![0.0, 1.0]],
            vec![
                SpatialLayoutNode {
                    knot_indices: vec![0],
                    vector: vec![1.0],
                },
                SpatialLayoutNode {
                    knot_indices: vec![1],
                    vector: vec![1.0],
                },
            ],
            Vec::new(),
        )
        .unwrap();
        let basis = [vec![1.0; 64], vec![2.0; 64]];
        let coordinates = basis.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let mut automatic_output = vec![vec![0.0; 64]];
        let mut automatic_refs = automatic_output
            .iter_mut()
            .map(Vec::as_mut_slice)
            .collect::<Vec<_>>();
        JocSpatialBridge::new()
            .render_coordinates(
                &coordinates,
                Some(automatic_topology),
                Some(&control.events[0].updates),
                &layout,
                0,
                48_000,
                &mut automatic_refs,
            )
            .unwrap();
        let mut sidecar_output = vec![vec![0.0; 64]];
        let mut sidecar_refs = sidecar_output
            .iter_mut()
            .map(Vec::as_mut_slice)
            .collect::<Vec<_>>();
        JocSpatialBridge::new()
            .render_coordinates(
                &coordinates,
                Some(&sidecar_topology),
                Some(&control.events[0].updates),
                &layout,
                0,
                48_000,
                &mut sidecar_refs,
            )
            .unwrap();
        assert_eq!(automatic_output, sidecar_output);
    }

    #[test]
    fn contribution_masking_is_downstream_of_automatic_control_and_scheduler_state() {
        use crate::{
            BaseFullBandCoordinate, JocSpatialBridge, JocSpatialFrameBridge,
            SpatialContributionMode, SpatialLayout, SpatialLayoutChannel, SpatialLayoutNode,
        };
        let mut frame = equivalence_frame();
        frame.decoded.reconstruction_basis.rows[0].fill(2.0);
        let base_coordinates = [BaseFullBandCoordinate::Left];
        let base_pcm = [vec![1.0; 64]];
        let bridge_frame = JocSpatialFrameBridge
            .frame(&frame, &base_coordinates, &base_pcm, None)
            .unwrap();
        let layout = SpatialLayout::new(
            vec![SpatialLayoutChannel {
                identity: "FL".to_owned(),
                enabled: true,
                lfe: false,
            }],
            vec![vec![0.0, 1.0]],
            vec![
                SpatialLayoutNode {
                    knot_indices: vec![0],
                    vector: vec![1.0],
                },
                SpatialLayoutNode {
                    knot_indices: vec![1],
                    vector: vec![1.0],
                },
            ],
            Vec::new(),
        )
        .unwrap();
        let run = |mode| {
            let mut assembler = super::BridgeControlAssembler::new(8, 1);
            let control = assembler
                .assemble_frame(&frame, &base_coordinates, None)
                .unwrap();
            let mut bridge = JocSpatialBridge::new();
            let mut output = vec![vec![0.0; 64]];
            let mut output_refs = output.iter_mut().map(Vec::as_mut_slice).collect::<Vec<_>>();
            bridge
                .render_codec_basis_frame_with_contribution(
                    &bridge_frame,
                    mode,
                    control.initial_topology.as_ref(),
                    Some(&control.events[0].updates),
                    &layout,
                    u64::from(control.events[0].ramp_duration),
                    &mut output_refs,
                )
                .unwrap();
            (control, assembler.topology_epoch(), bridge, output)
        };
        let (full_control, full_epoch, full_bridge, full) = run(SpatialContributionMode::Full);
        let (base_control, base_epoch, base_bridge, base) = run(SpatialContributionMode::BaseOnly);
        let (rb_control, rb_epoch, rb_bridge, rb) =
            run(SpatialContributionMode::ReconstructionOnly);

        assert_eq!(base_control, full_control);
        assert_eq!(rb_control, full_control);
        assert_eq!(base_epoch, full_epoch);
        assert_eq!(rb_epoch, full_epoch);
        assert_eq!(base_bridge, full_bridge);
        assert_eq!(rb_bridge, full_bridge);
        assert_eq!(full[0], vec![3.0; 64]);
        assert_eq!(base[0], vec![1.0; 64]);
        assert_eq!(rb[0], vec![2.0; 64]);
    }

    #[test]
    fn bc16_end_to_end_control_assembly_contract() {
        use crate::{BaseFullBandCoordinate, DecodedPayloadFrame, ProgrammeLayout, SampleRange};
        use openjoc_joc::{DecodedJocFrame, JocFrame, JocHeader, ReconstructionBasis};
        use openjoc_oamd::{
            ContentDescription, MetadataBlockTiming, MetadataTiming, OamdContentPrefix,
            OamdElement, OamdElementMetadata, OamdPayload, ObjectBasicInfo, ObjectClass,
            ObjectElement, ObjectRenderInfo, ObjectUpdate,
        };
        let prefix = OamdContentPrefix {
            syntax_version: 0,
            object_count: 1,
            content: ContentDescription::DynamicOnly { lfe_present: false },
            alternate_object_data_present: false,
            element_count: 1,
            consumed_bits: 0,
        };
        let oamd = OamdPayload {
            prefix: prefix.clone(),
            object_classes: vec![ObjectClass::Dynamic],
            elements: vec![OamdElementMetadata {
                id: 1,
                alternate_data_id: None,
                discard_unknown: false,
                element: OamdElement::Objects(ObjectElement {
                    timing: MetadataTiming {
                        sample_offset: 0,
                        blocks: vec![MetadataBlockTiming {
                            start_sample: 0,
                            ramp_duration: 0,
                        }],
                    },
                    objects: vec![vec![ObjectUpdate {
                        active: true,
                        basic: ObjectBasicInfo {
                            gain: Gain::Decibels(0),
                            priority: 0.0,
                        },
                        render: ObjectRenderInfo::DEFAULT,
                        additional_table_data: None,
                    }]],
                    consumed_bits: 0,
                }),
            }],
            consumed_bits: 0,
        };
        let frame = DecodedPayloadFrame {
            frame_index: 0,
            sample_rate: 48_000,
            sample_range: SampleRange::new(0, 64).unwrap(),
            joc: JocFrame {
                header: JocHeader {
                    downmix_index: 0,
                    channel_count: 5,
                    object_count_bits: 0,
                    object_count: 1,
                    extension_index: 0,
                },
                clip_gain_x_bits: 0,
                clip_gain_y_bits: 0,
                sequence_count: 0,
                objects: Vec::new(),
            },
            oamd,
            decoded: DecodedJocFrame {
                reconstruction_qmf: Vec::new(),
                reconstruction_basis: ReconstructionBasis {
                    rows: vec![vec![0.0; 64]],
                },
                stages: Vec::new(),
                state_reset: true,
            },
            programme_layout: ProgrammeLayout::from_prefix(&prefix).unwrap(),
        };
        let mut assembler = super::BridgeControlAssembler::new(8, 1);
        let control = assembler
            .assemble_frame(&frame, &[BaseFullBandCoordinate::Left], None)
            .unwrap();
        assert_eq!(control.topology_epoch, 1);
        assert!(control.initial_topology.is_some());
        assert_eq!(control.events[0].quantum, 0);
    }
}
