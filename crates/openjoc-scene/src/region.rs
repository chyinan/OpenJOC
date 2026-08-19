//! Semantic dynamic-region selection before point projection.
//!
//! Region selection is deliberately data-driven: a canonical layout supplies
//! its speaker identities and coordinates, while this module filters and
//! rebuilds the ordinary layer/row/anchor topology consumed by the existing
//! point projector.

use crate::extent::ExtentFieldCache;
use crate::{
    SpatialDescriptor, SpatialLayout, SpatialLayoutLayer, SpatialLayoutRow, SpatialLayoutTopology,
    SpatialProjectionError, SpatialProjectionOutcome, SpatialSourceClass,
};
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

const REGION_CACHE_CAPACITY: usize = 24;
const EXTENT_CACHE_CAPACITY: usize = 8;

/// Admitted horizontal semantic region states.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionHorizontalState {
    NoConstraints,
    BackExcluded,
    SideExcluded,
    CentreAndBack,
    ScreenOnly,
    SurroundOnly,
}

/// Independent ordinary Top-Bottom inclusion control.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionTopBottomState {
    Include,
    Exclude,
}

/// Validated semantic region snapshot retained by the bridge.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct RegionSemanticState {
    pub horizontal: RegionHorizontalState,
    pub top_bottom: RegionTopBottomState,
}

impl Default for RegionSemanticState {
    fn default() -> Self {
        Self {
            horizontal: RegionHorizontalState::NoConstraints,
            top_bottom: RegionTopBottomState::Include,
        }
    }
}

/// Invalid already-decoded semantic region state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegionSemanticError {
    InvalidDecodedZones([bool; 6]),
}

impl fmt::Display for RegionSemanticError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDecodedZones(zones) => {
                write!(formatter, "invalid decoded semantic region state {zones:?}")
            }
        }
    }
}

impl Error for RegionSemanticError {}

impl RegionSemanticState {
    /// Converts the existing decoded six-value semantic representation.
    ///
    /// Raw metadata packing is intentionally not handled here. The adapter
    /// accepts only the six exact decoded horizontal states in the region
    /// contract and retains Top-Bottom independently.
    pub fn from_decoded_zones(zones: [bool; 6]) -> Result<Self, RegionSemanticError> {
        let horizontal = match zones[..5] {
            [true, true, true, true, true] => RegionHorizontalState::NoConstraints,
            [true, true, true, false, true] => RegionHorizontalState::BackExcluded,
            [true, false, true, true, true] => RegionHorizontalState::SideExcluded,
            [false, false, false, false, true] => RegionHorizontalState::CentreAndBack,
            [true, false, false, false, false] => RegionHorizontalState::ScreenOnly,
            [false, false, true, false, false] => RegionHorizontalState::SurroundOnly,
            _ => return Err(RegionSemanticError::InvalidDecodedZones(zones)),
        };
        Ok(Self {
            horizontal,
            top_bottom: if zones[5] {
                RegionTopBottomState::Include
            } else {
                RegionTopBottomState::Exclude
            },
        })
    }

    /// Returns the decoded six-value representation used by the bridge.
    #[must_use]
    pub const fn to_decoded_zones(self) -> [bool; 6] {
        let mut zones = match self.horizontal {
            RegionHorizontalState::NoConstraints => [true, true, true, true, true, false],
            RegionHorizontalState::BackExcluded => [true, true, true, false, true, false],
            RegionHorizontalState::SideExcluded => [true, false, true, true, true, false],
            RegionHorizontalState::CentreAndBack => [false, false, false, false, true, false],
            RegionHorizontalState::ScreenOnly => [true, false, false, false, false, false],
            RegionHorizontalState::SurroundOnly => [false, false, true, false, false, false],
        };
        zones[5] = matches!(self.top_bottom, RegionTopBottomState::Include);
        zones
    }

    #[must_use]
    pub const fn is_default(self) -> bool {
        matches!(
            self,
            Self {
                horizontal: RegionHorizontalState::NoConstraints,
                top_bottom: RegionTopBottomState::Include,
            }
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
struct RegionTopologyCacheEntry {
    canonical: SpatialLayout,
    state: RegionSemanticState,
    selected: SpatialLayout,
}

#[derive(Clone, Debug, PartialEq)]
struct ExtentCacheEntry {
    effective: SpatialLayout,
    topology_epoch: u64,
    field: ExtentFieldCache,
}

/// Bounded region-topology preparation and cache.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RegionTopologySelector {
    cache: Vec<RegionTopologyCacheEntry>,
    extent_cache: Vec<ExtentCacheEntry>,
}

impl RegionTopologySelector {
    /// Creates an empty selector cache.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            cache: Vec::new(),
            extent_cache: Vec::new(),
        }
    }

    /// Discards all prepared topologies, including those from old layout epochs.
    pub fn clear(&mut self) {
        self.cache.clear();
        self.extent_cache.clear();
    }

    /// Returns the number of prepared constrained topologies.
    #[must_use]
    pub fn cached_topology_count(&self) -> usize {
        self.cache.len()
    }

    /// Projects one descriptor and retains effective-position information only
    /// as the local outcome of this target-generation evaluation.
    pub(crate) fn project_outcome(
        &mut self,
        canonical: &SpatialLayout,
        descriptor: &SpatialDescriptor,
    ) -> Result<SpatialProjectionOutcome, SpatialProjectionError> {
        self.project_outcome_with_epoch(canonical, descriptor, 0)
    }

    pub(crate) fn project_outcome_with_epoch(
        &mut self,
        canonical: &SpatialLayout,
        descriptor: &SpatialDescriptor,
        topology_epoch: u64,
    ) -> Result<SpatialProjectionOutcome, SpatialProjectionError> {
        if descriptor.channel_lock
            && !matches!(descriptor.source_class, SpatialSourceClass::DynamicPoint)
        {
            return Err(SpatialProjectionError::UnsupportedChannelLock);
        }
        if !matches!(
            descriptor.source_class,
            SpatialSourceClass::DynamicPoint | SpatialSourceClass::DynamicRegion
        ) {
            return Ok(SpatialProjectionOutcome {
                target: canonical.project_unconstrained(descriptor)?,
                effective_position: None,
                locked_output: None,
            });
        }
        let extent_active = descriptor
            .extent
            .map(crate::extent::extent_scalar)
            .transpose()?
            .is_some_and(|(mean_q, _)| mean_q != 0);
        let state = match descriptor.zones {
            Some(zones) => RegionSemanticState::from_decoded_zones(zones)
                .map_err(|_| SpatialProjectionError::InvalidRegionState(zones))?,
            None => RegionSemanticState::default(),
        };
        let pair_active = descriptor
            .pair_span_q15
            .is_some_and(|pair_span_q15| pair_span_q15 > 0);
        if pair_active && !state.is_default() {
            return Err(SpatialProjectionError::InvalidPair);
        }
        if descriptor.channel_lock
            && (descriptor.spread.is_some() || descriptor.paired.is_some() || pair_active)
        {
            return Err(SpatialProjectionError::UnsupportedChannelLock);
        }
        if extent_active
            && (descriptor.spread.is_some() || descriptor.paired.is_some() || pair_active)
        {
            return Err(SpatialProjectionError::UnsupportedExtent);
        }

        let effective = if state.is_default() {
            canonical.clone()
        } else {
            self.cached_selected(canonical, state)?.clone()
        };
        let ordinary = effective.project_unconstrained(descriptor)?;
        if descriptor.channel_lock {
            return effective.channel_lock_outcome(descriptor, ordinary);
        }
        if extent_active {
            return self
                .cached_extent(&effective, topology_epoch)?
                .project(&effective, descriptor)
                .map(|target| SpatialProjectionOutcome {
                    target,
                    effective_position: None,
                    locked_output: None,
                });
        }
        let effective_position = if matches!(
            descriptor.source_class,
            SpatialSourceClass::DynamicPoint | SpatialSourceClass::DynamicRegion
        ) {
            Some(effective.point_position(&descriptor.coordinates)?)
        } else {
            None
        };
        Ok(SpatialProjectionOutcome {
            target: ordinary,
            effective_position,
            locked_output: None,
        })
    }

    fn cached_extent(
        &mut self,
        effective: &SpatialLayout,
        topology_epoch: u64,
    ) -> Result<&ExtentFieldCache, SpatialProjectionError> {
        if let Some(index) = self.extent_cache.iter().position(|entry| {
            entry.effective == *effective && entry.topology_epoch == topology_epoch
        }) {
            return Ok(&self.extent_cache[index].field);
        }
        let field = ExtentFieldCache::build(effective)?;
        if self.extent_cache.len() == EXTENT_CACHE_CAPACITY {
            self.extent_cache.remove(0);
        }
        self.extent_cache.push(ExtentCacheEntry {
            effective: effective.clone(),
            topology_epoch,
            field,
        });
        Ok(&self
            .extent_cache
            .last()
            .expect("extent cache entry was just inserted")
            .field)
    }

    /// Selects or prepares one derived topology for a canonical layout/state.
    pub fn select(
        &mut self,
        canonical: &SpatialLayout,
        state: RegionSemanticState,
    ) -> Result<SpatialLayout, SpatialProjectionError> {
        if state.is_default() {
            return Ok(canonical.clone());
        }
        Ok(self.cached_selected(canonical, state)?.clone())
    }

    fn cached_selected(
        &mut self,
        canonical: &SpatialLayout,
        state: RegionSemanticState,
    ) -> Result<&SpatialLayout, SpatialProjectionError> {
        if let Some(index) = self
            .cache
            .iter()
            .position(|entry| entry.canonical == *canonical && entry.state == state)
        {
            return Ok(&self.cache[index].selected);
        }
        let selected = derive_selected_layout(canonical, state)?;
        if self.cache.len() == REGION_CACHE_CAPACITY {
            self.cache.remove(0);
        }
        self.cache.push(RegionTopologyCacheEntry {
            canonical: canonical.clone(),
            state,
            selected: selected.clone(),
        });
        Ok(&self
            .cache
            .last()
            .expect("cache entry was just inserted")
            .selected)
    }
}

fn derive_selected_layout(
    canonical: &SpatialLayout,
    state: RegionSemanticState,
) -> Result<SpatialLayout, SpatialProjectionError> {
    let five_x = !canonical
        .channels()
        .iter()
        .any(|channel| matches!(channel.identity.as_str(), "Lb" | "Rb" | "Lw" | "Rw"));
    let is_22_2 = canonical
        .channels()
        .iter()
        .any(|channel| channel.identity == "FLc");
    let mut layers = Vec::with_capacity(canonical.topology().layers.len());
    for (layer_index, layer) in canonical.topology().layers.iter().enumerate() {
        let is_upper = if is_22_2 {
            layer.z > 0.0
        } else {
            canonical.topology().layers.len() == 2 && layer_index == 1
        };
        if is_upper && matches!(state.top_bottom, RegionTopBottomState::Exclude) {
            continue;
        }
        let mut rows = Vec::with_capacity(layer.rows.len());
        for row in &layer.rows {
            let anchors = row
                .anchors
                .iter()
                .filter(|anchor| {
                    is_upper
                        || (is_22_2 && layer.z < 0.0)
                        || matches_bed_state(state.horizontal, &anchor.identity, five_x)
                            .unwrap_or(false)
                })
                .cloned()
                .collect::<Vec<_>>();
            if !anchors.is_empty() {
                rows.push(SpatialLayoutRow { y: row.y, anchors });
            }
        }
        if !rows.is_empty() {
            layers.push(SpatialLayoutLayer { z: layer.z, rows });
        }
    }
    if layers.is_empty() || layers.iter().any(|layer| layer.rows.is_empty()) {
        return Err(SpatialProjectionError::UnsupportedRegionLayout(
            "region selection produced no populated topology",
        ));
    }
    if state.horizontal != RegionHorizontalState::NoConstraints {
        for layer in &canonical.topology().layers {
            for row in &layer.rows {
                for anchor in &row.anchors {
                    if layer.z == 0.0
                        && matches_bed_state(state.horizontal, &anchor.identity, five_x).is_err()
                    {
                        return Err(SpatialProjectionError::UnsupportedRegionLayout(
                            "canonical bed anchor has no admitted speaker class",
                        ));
                    }
                }
            }
        }
    }
    canonical.with_constrained_topology(SpatialLayoutTopology {
        layers,
        aliases: canonical.topology().aliases.clone(),
    })
}

fn matches_bed_state(
    state: RegionHorizontalState,
    identity: &str,
    five_x: bool,
) -> Result<bool, ()> {
    let classes = match identity {
        "FL" | "FR" | "FLc" | "FRc" => (true, false, false, false),
        "FC" => (true, true, false, false),
        "Ls" | "Rs" if five_x => (false, false, true, true),
        "Ls" | "Rs" | "SiL" | "SiR" => (false, false, true, false),
        "Lb" | "Rb" | "BL" | "BR" | "BC" => (false, false, false, true),
        "Lw" | "Rw" => (false, false, false, false),
        _ => return Err(()),
    };
    let (screen, centre, surround, back) = classes;
    let side = matches!(identity, "Ls" | "Rs" | "Lw" | "Rw") && !five_x;
    Ok(match state {
        RegionHorizontalState::NoConstraints => true,
        RegionHorizontalState::BackExcluded => !back,
        RegionHorizontalState::SideExcluded => !side,
        RegionHorizontalState::CentreAndBack => centre || back,
        RegionHorizontalState::ScreenOnly => screen,
        RegionHorizontalState::SurroundOnly => surround,
    })
}
