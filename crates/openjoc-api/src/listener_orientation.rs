//! Device-independent listener orientation and off-render-path HRIR preparation.

use crate::{BinauralConfig, OpenJocError, SofaLoadLimits, virtual_speaker_direction};
use openjoc_render::{
    BinauralResourceIdentity, CartesianPosition, HrirBank, MAX_DYNAMIC_BINAURAL_HRIR_TAPS,
    PreparedBinauralKernel, PreparedBinauralUpdate, RenderError, SourceId,
};
use openjoc_scene::SpeakerLayoutPreset;
use openjoc_sofa::{
    BUILTIN_GENERIC_HRTF_SAMPLE_RATE_HZ, load_builtin_hrir_f32, load_builtin_hrir_f32_from_asset,
    parse_simple_free_field_hrir, resample_loaded_hrir_bank, resolve_hrir_for_listener_orientation,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// Upper bound on any HRIR pair accepted by orientation preparation.
///
/// Larger valid static SOFA resources remain usable through the static
/// renderer. The orientation preparer rejects an oversized resolved HRIR as a
/// whole update; it never truncates or resamples it to fit this bound.
pub const MAX_DYNAMIC_HRIR_TAPS: usize = MAX_DYNAMIC_BINAURAL_HRIR_TAPS;

/// A normalized active rotation from listener-local coordinates to the scene
/// reference frame, stored in fixed `(x, y, z, w)` order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListenerOrientation {
    xyzw: [f64; 4],
}

impl ListenerOrientation {
    /// Identity orientation `(0, 0, 0, 1)`.
    pub const IDENTITY: Self = Self {
        xyzw: [0.0, 0.0, 0.0, 1.0],
    };

    /// Validates and scale-stably normalizes an `(x, y, z, w)` quaternion.
    /// Equivalent signs are canonicalized so `q` and `-q` produce identical
    /// stored values. The preparer still performs each requested lookup; a
    /// caller may compare these stable values before scheduling redundant work.
    pub fn new(x: f64, y: f64, z: f64, w: f64) -> Result<Self, ListenerOrientationError> {
        let components = [x, y, z, w];
        if let Some(component_index) = components.iter().position(|value| !value.is_finite()) {
            return Err(ListenerOrientationError::NonFiniteComponent {
                component: match component_index {
                    0 => "x",
                    1 => "y",
                    2 => "z",
                    _ => "w",
                },
            });
        }
        let scale = components
            .iter()
            .map(|value| value.abs())
            .fold(0.0_f64, f64::max);
        if scale == 0.0 {
            return Err(ListenerOrientationError::ZeroLength);
        }
        let mut normalized = components.map(|value| value / scale);
        let length = normalized
            .iter()
            .map(|value| value * value)
            .sum::<f64>()
            .sqrt();
        if !length.is_finite() || length == 0.0 {
            return Err(ListenerOrientationError::ZeroLength);
        }
        for component in &mut normalized {
            *component /= length;
        }

        let negate = if normalized[3] < 0.0 {
            true
        } else if normalized[3] > 0.0 {
            false
        } else {
            normalized[..3]
                .iter()
                .find(|component| **component != 0.0)
                .is_some_and(|component| *component < 0.0)
        };
        if negate {
            for component in &mut normalized {
                *component = -*component;
            }
        }
        Ok(Self { xyzw: normalized })
    }

    /// Returns the normalized quaternion in `(x, y, z, w)` order.
    #[must_use]
    pub const fn as_xyzw(self) -> [f64; 4] {
        self.xyzw
    }

    /// Transforms a fixed world-space direction into listener-local axes using
    /// the inverse of the stored active listener rotation.
    #[must_use]
    pub fn world_to_listener(self, direction: [f64; 3]) -> [f64; 3] {
        let [x, y, z, w] = self.xyzw;
        let inverse_vector = [-x, -y, -z];
        let cross = [
            inverse_vector[1] * direction[2] - inverse_vector[2] * direction[1],
            inverse_vector[2] * direction[0] - inverse_vector[0] * direction[2],
            inverse_vector[0] * direction[1] - inverse_vector[1] * direction[0],
        ];
        let twice_cross = cross.map(|value| 2.0 * value);
        let second_cross = [
            inverse_vector[1] * twice_cross[2] - inverse_vector[2] * twice_cross[1],
            inverse_vector[2] * twice_cross[0] - inverse_vector[0] * twice_cross[2],
            inverse_vector[0] * twice_cross[1] - inverse_vector[1] * twice_cross[0],
        ];
        [
            direction[0] + w * twice_cross[0] + second_cross[0],
            direction[1] + w * twice_cross[1] + second_cross[1],
            direction[2] + w * twice_cross[2] + second_cross[2],
        ]
    }
}

/// Invalid listener-orientation input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ListenerOrientationError {
    /// The named quaternion component was NaN or infinite.
    NonFiniteComponent { component: &'static str },
    /// All quaternion components were zero or normalization was degenerate.
    ZeroLength,
}

impl std::fmt::Display for ListenerOrientationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteComponent { component } => {
                write!(
                    formatter,
                    "listener quaternion component {component} is non-finite"
                )
            }
            Self::ZeroLength => formatter.write_str("listener quaternion has zero length"),
        }
    }
}

impl std::error::Error for ListenerOrientationError {}

/// Failure while preparing the complete HRIR set for one listener orientation.
#[derive(Clone, Debug, PartialEq)]
pub enum ListenerOrientationPrepareError {
    /// The HRTF resolver rejected one source direction. This does not claim
    /// that the measurement set itself has a physical data hole: the resolver
    /// may reject a local interpolation search even when a close sample exists.
    HrirResolutionFailure {
        source_id: u64,
        world_direction: [f64; 3],
        listener_direction: [f64; 3],
        reason: String,
    },
    /// A resolved kernel exceeded the explicit dynamic-resource tap limit.
    HrirTooLong {
        source_id: u64,
        tap_count: usize,
        max_tap_count: usize,
    },
    /// An internal validated kernel set could not be formed.
    InvalidPreparedSet(String),
}

impl std::fmt::Display for ListenerOrientationPrepareError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HrirResolutionFailure {
                source_id,
                world_direction,
                listener_direction,
                reason,
            } => write!(
                formatter,
                "source {source_id} HRIR query rejected: world direction {world_direction:?}, listener direction {listener_direction:?}: {reason}"
            ),
            Self::HrirTooLong {
                source_id,
                tap_count,
                max_tap_count,
            } => write!(
                formatter,
                "source {source_id} HRIR has {tap_count} taps, exceeding the dynamic limit {max_tap_count}"
            ),
            Self::InvalidPreparedSet(message) => {
                write!(formatter, "invalid prepared HRIR set: {message}")
            }
        }
    }
}

impl std::error::Error for ListenerOrientationPrepareError {}

#[derive(Clone, Debug)]
enum HrirQueryBank {
    BuiltinF32(Arc<openjoc_sofa::ListenerOrientationHrirBank>),
    Custom(Arc<HrirBank>),
}

impl HrirQueryBank {
    fn resolve(&self, direction: CartesianPosition) -> Result<openjoc_render::HrirPair, String> {
        match self {
            Self::BuiltinF32(bank) => bank
                .resolve(direction)
                .map(|resolved| resolved.pair)
                .map_err(|error| error.to_string()),
            Self::Custom(bank) => resolve_hrir_for_listener_orientation(bank, direction)
                .map(|resolved| resolved.pair)
                .map_err(|error| error.to_string()),
        }
    }

    fn tap_payload_bytes(&self) -> usize {
        match self {
            Self::BuiltinF32(bank) => bank.tap_storage_bytes(),
            Self::Custom(bank) => bank
                .entries()
                .iter()
                .map(|entry| {
                    (entry.pair().left_taps().len() + entry.pair().right_taps().len())
                        * std::mem::size_of::<f64>()
                })
                .sum(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct OrientationSource {
    channel_index: usize,
    source_id: SourceId,
    world_direction: CartesianPosition,
}

#[derive(Clone, Debug)]
struct OrientationResources {
    resource_identity: BinauralResourceIdentity,
    bank: HrirQueryBank,
    sources: Vec<OrientationSource>,
    max_filter_taps: usize,
    lfe_index: Option<usize>,
    lfe_delay_samples: usize,
}

/// Immutable HRTF/layout preparation context that can be cloned to a
/// non-render worker thread. Preparation allocates and performs all direction
/// lookup; it never touches a session, device, or sensor.
#[derive(Clone, Debug)]
pub struct ListenerOrientationPreparer {
    resources: Arc<OrientationResources>,
}

impl ListenerOrientationPreparer {
    /// Loads one built-in or custom SOFA resource for orientation preparation.
    pub fn new(config: &BinauralConfig) -> Result<Self, OpenJocError> {
        Self::new_with_resource_inputs(config, None, None)
    }

    /// Confirms that a shared preparation context was built for the exact
    /// binaural resource/layout configuration supplied to a new session.
    pub fn validate_config(&self, config: &BinauralConfig) -> Result<(), OpenJocError> {
        let expected = orientation_resource_identity(config, self.sample_rate_hz());
        if expected != self.resources.resource_identity {
            return Err(OpenJocError::InvalidConfig(
                "listener-orientation preparer does not match the session HRTF/layout".to_owned(),
            ));
        }
        Ok(())
    }

    pub(crate) fn new_with_resource_inputs(
        config: &BinauralConfig,
        external_builtin_asset: Option<&[u8]>,
        custom_sofa_load_limits: Option<SofaLoadLimits>,
    ) -> Result<Self, OpenJocError> {
        let preset = SpeakerLayoutPreset::for_name(&config.virtual_layout)
            .map_err(|error| OpenJocError::InvalidConfig(error.to_string()))?;
        if preset.labels.len() > openjoc_scene::MAX_CUSTOM_SPEAKERS {
            return Err(OpenJocError::InvalidConfig(
                "virtual layout exceeds the speaker limit".to_owned(),
            ));
        }

        let load_limits = custom_sofa_load_limits.unwrap_or_default();
        let (bank, sample_rate_hz, lfe_delay_samples, max_resolved_taps) =
            if config.sofa_bytes.is_empty() {
                let loaded = if let Some(asset) = external_builtin_asset {
                    load_builtin_hrir_f32_from_asset(config.builtin_hrtf, asset)?
                } else {
                    load_builtin_hrir_f32(config.builtin_hrtf)?
                };
                let max_resolved_taps = loaded.bank.max_resolved_tap_count()?;
                (
                    HrirQueryBank::BuiltinF32(Arc::new(
                        openjoc_sofa::ListenerOrientationHrirBank::new(Arc::new(loaded.bank)),
                    )),
                    loaded.metadata.sample_rate_hz,
                    0,
                    max_resolved_taps,
                )
            } else {
                let loaded = parse_simple_free_field_hrir(&config.sofa_bytes, load_limits)?;
                let source_rate = loaded.bank.sample_rate_hz();
                let lfe_delay_samples = openjoc_sofa::hrir_resampling_delay_samples(
                    source_rate,
                    BUILTIN_GENERIC_HRTF_SAMPLE_RATE_HZ,
                )?;
                let loaded = resample_loaded_hrir_bank(
                    loaded,
                    BUILTIN_GENERIC_HRTF_SAMPLE_RATE_HZ,
                    load_limits,
                )?;
                let max_resolved_taps = max_resolved_custom_tap_count(&loaded.bank)?;
                (
                    HrirQueryBank::Custom(Arc::new(loaded.bank)),
                    BUILTIN_GENERIC_HRTF_SAMPLE_RATE_HZ,
                    lfe_delay_samples,
                    max_resolved_taps,
                )
            };
        if sample_rate_hz != BUILTIN_GENERIC_HRTF_SAMPLE_RATE_HZ {
            return Err(OpenJocError::InvalidConfig(format!(
                "orientation HRTF must resolve to {BUILTIN_GENERIC_HRTF_SAMPLE_RATE_HZ} Hz, got {sample_rate_hz} Hz"
            )));
        }

        let mut sources = Vec::with_capacity(preset.labels.len());
        for (channel_index, label) in preset.labels.iter().enumerate() {
            if preset.layout.channels()[channel_index].lfe {
                continue;
            }
            let world_direction = virtual_speaker_direction(label).ok_or_else(|| {
                OpenJocError::Unsupported(format!("no binaural direction for {label}"))
            })?;
            sources.push(OrientationSource {
                channel_index,
                source_id: SourceId::new(channel_index as u64 + 1),
                world_direction,
            });
        }
        if sources.is_empty() {
            return Err(OpenJocError::InvalidConfig(
                "orientation layout has no non-LFE sources".to_owned(),
            ));
        }

        let resource_identity = orientation_resource_identity(config, sample_rate_hz);
        let max_filter_taps = max_resolved_taps.min(MAX_DYNAMIC_HRIR_TAPS);
        if max_filter_taps == 0 {
            return Err(OpenJocError::InvalidConfig(
                "orientation HRTF has no non-empty resolved HRIR".to_owned(),
            ));
        }
        Ok(Self {
            resources: Arc::new(OrientationResources {
                resource_identity,
                bank,
                sources,
                max_filter_taps,
                lfe_index: preset.lfe_index(),
                lfe_delay_samples,
            }),
        })
    }

    /// Prepares every non-LFE source for one pose. If any source is outside
    /// coverage or exceeds the tap bound, no partial update is returned.
    pub fn prepare(
        &self,
        orientation: ListenerOrientation,
        stream_epoch: u64,
        sequence: u64,
    ) -> Result<PreparedBinauralUpdate, ListenerOrientationPrepareError> {
        let mut kernels = Vec::with_capacity(self.resources.sources.len());
        for source in &self.resources.sources {
            #[cfg(feature = "orientation-profile")]
            let transform_span = openjoc_sofa::orientation_profile::Span::new(0);
            let world = [
                source.world_direction.x,
                source.world_direction.y,
                source.world_direction.z,
            ];
            let listener = orientation.world_to_listener(world);
            let direction = CartesianPosition::new(listener[0], listener[1], listener[2]);
            #[cfg(feature = "orientation-profile")]
            drop(transform_span);
            let pair = self.resources.bank.resolve(direction).map_err(|reason| {
                ListenerOrientationPrepareError::HrirResolutionFailure {
                    source_id: source.source_id.get(),
                    world_direction: world,
                    listener_direction: listener,
                    reason,
                }
            })?;
            if pair.tap_count() > self.resources.max_filter_taps {
                return Err(ListenerOrientationPrepareError::HrirTooLong {
                    source_id: source.source_id.get(),
                    tap_count: pair.tap_count(),
                    max_tap_count: self.resources.max_filter_taps,
                });
            }
            kernels.push(PreparedBinauralKernel::new(source.source_id, pair));
        }
        #[cfg(feature = "orientation-profile")]
        let _update_span = openjoc_sofa::orientation_profile::Span::new(5);
        PreparedBinauralUpdate::new(
            self.resources.resource_identity,
            stream_epoch,
            sequence,
            kernels,
        )
        .map_err(|error| ListenerOrientationPrepareError::InvalidPreparedSet(error.to_string()))
    }

    /// Validates a caller-owned update before an adapter queues it for a
    /// lazily-created renderer. The same checks are repeated at renderer
    /// admission; this method does not consume or allocate kernels.
    pub fn validate_prepared_update(
        &self,
        update: &PreparedBinauralUpdate,
        expected_epoch: u64,
        previous_sequence: u64,
    ) -> Result<(), RenderError> {
        if update.resource_identity() != self.resources.resource_identity {
            return Err(RenderError::BinauralResourceIdentityMismatch);
        }
        if update.stream_epoch() != expected_epoch {
            return Err(RenderError::BinauralUpdateEpochMismatch {
                expected: expected_epoch,
                actual: update.stream_epoch(),
            });
        }
        if update.sequence() <= previous_sequence {
            return Err(RenderError::BinauralUpdateSequenceNotIncreasing {
                previous: previous_sequence,
                actual: update.sequence(),
            });
        }
        if update.sample_rate_hz() != self.sample_rate_hz() {
            return Err(RenderError::HrirSampleRateMismatch {
                expected: self.sample_rate_hz(),
                actual: update.sample_rate_hz(),
            });
        }
        if update.kernels().len() != self.resources.sources.len() {
            return Err(RenderError::BinauralUpdateSourceCountMismatch {
                expected: self.resources.sources.len(),
                actual: update.kernels().len(),
            });
        }
        for (index, (kernel, source)) in update
            .kernels()
            .iter()
            .zip(&self.resources.sources)
            .enumerate()
        {
            if kernel.source_id() != source.source_id {
                return Err(RenderError::BinauralUpdateSourceMismatch {
                    position: index,
                    expected: source.source_id,
                    actual: kernel.source_id(),
                });
            }
            if kernel.pair().tap_count() > self.resources.max_filter_taps {
                return Err(RenderError::BinauralUpdateTapLimitExceeded {
                    id: source.source_id,
                    actual: kernel.pair().tap_count(),
                    maximum: self.resources.max_filter_taps,
                });
            }
        }
        Ok(())
    }

    /// Returns the stable identity bound to this SOFA/built-in resource,
    /// virtual layout, LFE policy, and target sample rate.
    #[must_use]
    pub fn resource_identity(&self) -> BinauralResourceIdentity {
        self.resources.resource_identity
    }

    /// Returns the maximum taps accepted for one prepared HRIR.
    #[must_use]
    pub fn max_filter_taps(&self) -> usize {
        self.resources.max_filter_taps
    }

    /// Returns the number of non-LFE virtual sources in the fixed layout.
    #[must_use]
    pub fn source_count(&self) -> usize {
        self.resources.sources.len()
    }

    pub(crate) fn source_bindings(&self) -> impl Iterator<Item = (usize, SourceId)> + '_ {
        self.resources
            .sources
            .iter()
            .map(|source| (source.channel_index, source.source_id))
    }

    pub(crate) fn lfe_info(&self) -> (Option<usize>, usize) {
        (self.resources.lfe_index, self.resources.lfe_delay_samples)
    }

    /// Returns resident tap payload bytes in the query bank. Built-in f32
    /// resources report their packed tap capacity; custom SOFA reports the
    /// summed post-resample bank tap payload and excludes allocator overhead.
    #[must_use]
    pub fn query_hrir_tap_payload_bytes(&self) -> usize {
        self.resources.bank.tap_payload_bytes()
    }

    /// Returns the target rate of prepared kernels.
    #[must_use]
    pub const fn sample_rate_hz(&self) -> u32 {
        BUILTIN_GENERIC_HRTF_SAMPLE_RATE_HZ
    }

    /// Returns the LFE input-channel index, if the virtual layout has one.
    #[must_use]
    pub fn lfe_channel_index(&self) -> Option<usize> {
        self.resources.lfe_index
    }

    /// Returns the common causal delay applied to an unrotated LFE path to
    /// align it with resampled custom HRIRs.
    #[must_use]
    pub fn lfe_alignment_delay_samples(&self) -> usize {
        self.resources.lfe_delay_samples
    }

    /// Returns `(input_channel_index, source_id)` pairs for non-LFE virtual
    /// sources, in the renderer's fixed registration order.
    pub fn source_mappings(&self) -> impl Iterator<Item = (usize, u64)> + '_ {
        self.resources
            .sources
            .iter()
            .map(|source| (source.channel_index, source.source_id.get()))
    }
}

fn max_resolved_custom_tap_count(bank: &HrirBank) -> Result<usize, OpenJocError> {
    let mut max_delay = 0_usize;
    let mut max_aligned_taps = 0_usize;
    for entry in bank.entries() {
        for (ear, taps) in [
            (openjoc_render::HrirEar::Left, entry.pair().left_taps()),
            (openjoc_render::HrirEar::Right, entry.pair().right_taps()),
        ] {
            let delay = entry.pair().delay_samples(ear);
            let aligned = taps.len().checked_sub(delay).ok_or_else(|| {
                OpenJocError::InvalidConfig("SOFA HRIR delay exceeds tap count".to_owned())
            })?;
            max_delay = max_delay.max(delay);
            max_aligned_taps = max_aligned_taps.max(aligned);
        }
    }
    max_delay
        .checked_add(max_aligned_taps)
        .filter(|tap_count| *tap_count > 0)
        .ok_or_else(|| OpenJocError::InvalidConfig("SOFA HRIR tap bound overflow".to_owned()))
}

fn orientation_resource_identity(
    config: &BinauralConfig,
    sample_rate_hz: u32,
) -> BinauralResourceIdentity {
    let hrtf_identity = if config.sofa_bytes.is_empty() {
        format!(
            "builtin:{}:{}",
            config.builtin_hrtf.id(),
            config.builtin_hrtf.asset_metadata().asset_sha256
        )
    } else {
        format!("custom:{}", super::sha256_hex(&config.sofa_bytes))
    };
    let mut hasher = Sha256::new();
    hasher.update(b"OpenJOC-listener-binaural-resource-v1\0");
    hasher.update(config.virtual_layout.as_bytes());
    hasher.update([0]);
    hasher.update(super::binaural_lfe_policy_name(config.lfe_policy).as_bytes());
    hasher.update([0]);
    hasher.update(sample_rate_hz.to_le_bytes());
    hasher.update(hrtf_identity.as_bytes());
    BinauralResourceIdentity::new(hasher.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limited_sofa_fixture_for_5_1() -> Vec<u8> {
        use hdf5_pure::{AttrValue, FileBuilder};

        // The SOFA spherical azimuth is zero on world +X. These five exact
        // measurements correspond to FL, FR, FC, Ls and Rs in OpenJOC's
        // listener basis; a pitch rotation moves them away from this plane.
        let directions = [
            [135.0, 0.0, 1.0],
            [45.0, 0.0, 1.0],
            [90.0, 0.0, 1.0],
            [-180.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ];
        let mut file = FileBuilder::new();
        for (name, value) in [
            ("Conventions", "SOFA"),
            ("SOFAConventions", "SimpleFreeFieldHRIR"),
            ("SOFAConventionsVersion", "1.2"),
            ("DataType", "FIR"),
            ("RoomType", "free field"),
        ] {
            file.set_attr(name, AttrValue::String(value.to_owned()));
        }
        let impulse_responses: Vec<f64> = directions
            .iter()
            .flat_map(|_| [1.0, 0.0, 0.75, 0.0])
            .collect();
        file.create_dataset("Data.IR")
            .with_f64_data(&impulse_responses)
            .with_shape(&[directions.len() as u64, 2, 2]);
        let rate = file.create_dataset("Data.SamplingRate");
        rate.with_f64_data(&[48_000.0]).with_shape(&[1]);
        rate.set_attr("Units", AttrValue::String("hertz".to_owned()));
        let delay = file.create_dataset("Data.Delay");
        delay.with_f64_data(&[0.0, 0.0]).with_shape(&[2]);
        delay.set_attr("Units", AttrValue::String("samples".to_owned()));
        let source_positions: Vec<f64> = directions.iter().flatten().copied().collect();
        let source = file.create_dataset("SourcePosition");
        source
            .with_f64_data(&source_positions)
            .with_shape(&[directions.len() as u64, 3]);
        source.set_attr("Type", AttrValue::String("spherical".to_owned()));
        source.set_attr(
            "Units",
            AttrValue::String("degree, degree, metre".to_owned()),
        );
        for (name, values) in [
            ("ListenerPosition", [0.0, 0.0, 0.0]),
            ("ListenerView", [0.0, 1.0, 0.0]),
            ("ListenerUp", [0.0, 0.0, 1.0]),
        ] {
            let dataset = file.create_dataset(name);
            dataset.with_f64_data(&values).with_shape(&[3]);
            dataset.set_attr("Type", AttrValue::String("cartesian".to_owned()));
            dataset.set_attr("Units", AttrValue::String("metre".to_owned()));
        }
        let receivers = file.create_dataset("ReceiverPosition");
        receivers
            .with_f64_data(&[0.09, 0.0, 0.0, -0.09, 0.0, 0.0])
            .with_shape(&[2, 3]);
        receivers.set_attr("Type", AttrValue::String("cartesian".to_owned()));
        receivers.set_attr("Units", AttrValue::String("metre".to_owned()));
        file.finish().expect("limited-coverage SOFA fixture")
    }

    #[test]
    fn inverse_rotation_moves_scene_front_left_for_right_turn() {
        let half_sqrt = std::f64::consts::FRAC_1_SQRT_2;
        let right_turn = ListenerOrientation::new(0.0, 0.0, -half_sqrt, half_sqrt).unwrap();
        let listener_front = right_turn.world_to_listener([0.0, 1.0, 0.0]);
        assert!((listener_front[0] + 1.0).abs() < 1.0e-12);
        assert!(listener_front[1].abs() < 1.0e-12);
        assert!(listener_front[2].abs() < 1.0e-12);
    }

    #[test]
    fn inverse_rotation_moves_scene_front_down_for_upward_pitch() {
        let half_sqrt = std::f64::consts::FRAC_1_SQRT_2;
        let pitch_up = ListenerOrientation::new(half_sqrt, 0.0, 0.0, half_sqrt).unwrap();
        let listener_front = pitch_up.world_to_listener([0.0, 1.0, 0.0]);
        assert!(listener_front[0].abs() < 1.0e-12);
        assert!(listener_front[1].abs() < 1.0e-12);
        assert!((listener_front[2] + 1.0).abs() < 1.0e-12);
    }

    #[test]
    fn roll_composition_and_inverse_rotation_are_vector_based() {
        let half_sqrt = std::f64::consts::FRAC_1_SQRT_2;
        let roll = ListenerOrientation::new(0.0, half_sqrt, 0.0, half_sqrt).unwrap();
        let listener_right = roll.world_to_listener([1.0, 0.0, 0.0]);
        assert!(listener_right[0].abs() < 1.0e-12);
        assert!(listener_right[1].abs() < 1.0e-12);
        assert!((listener_right[2] - 1.0).abs() < 1.0e-12);

        let combined = ListenerOrientation::new(0.2, -0.3, 0.4, 0.5).unwrap();
        let transformed = combined.world_to_listener([1.0, 2.0, -3.0]);
        let expected = [
            -1.444_444_444_444_444_4,
            -0.222_222_222_222_222_2,
            -3.444_444_444_444_444,
        ];
        for (actual, expected) in transformed.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 1.0e-12);
        }
        let length = transformed
            .iter()
            .map(|component| component * component)
            .sum::<f64>();
        assert!((length - 14.0).abs() < 1.0e-11);
    }

    #[test]
    fn normalization_is_scale_stable_and_q_neg_q_canonical() {
        let normal = ListenerOrientation::new(0.2, -0.3, 0.4, 0.5).unwrap();
        let opposite = ListenerOrientation::new(-0.2, 0.3, -0.4, -0.5).unwrap();
        assert_eq!(normal, opposite);

        let huge = ListenerOrientation::new(1.0e308, -1.0e308, 1.0e308, -1.0e308).unwrap();
        let small = ListenerOrientation::new(1.0e-308, -1.0e-308, 1.0e-308, -1.0e-308).unwrap();
        assert_eq!(huge, small);

        let half_turn = ListenerOrientation::new(0.0, 0.0, -1.0, 0.0).unwrap();
        assert_eq!(half_turn.as_xyzw(), [0.0, 0.0, 1.0, 0.0]);
    }

    #[test]
    fn rejects_non_finite_and_zero_quaternions() {
        for (components, expected_component) in [
            ([f64::NAN, 0.0, 0.0, 1.0], "x"),
            ([0.0, f64::INFINITY, 0.0, 1.0], "y"),
            ([0.0, 0.0, f64::NEG_INFINITY, 1.0], "z"),
            ([0.0, 0.0, 0.0, f64::NAN], "w"),
        ] {
            assert_eq!(
                ListenerOrientation::new(
                    components[0],
                    components[1],
                    components[2],
                    components[3]
                ),
                Err(ListenerOrientationError::NonFiniteComponent {
                    component: expected_component
                })
            );
        }
        assert_eq!(
            ListenerOrientation::new(0.0, 0.0, 0.0, 0.0),
            Err(ListenerOrientationError::ZeroLength)
        );
    }

    #[test]
    fn preparer_binds_layout_and_resource_and_builds_complete_updates() {
        for hrtf in [
            crate::BuiltinHrtf::SadieD1Ku100,
            crate::BuiltinHrtf::SadieD2Kemar,
        ] {
            let config = BinauralConfig::builtin(hrtf, "7.1.4");
            let preparer = ListenerOrientationPreparer::new(&config).unwrap();
            let identity = preparer
                .prepare(ListenerOrientation::IDENTITY, 7, 11)
                .unwrap();
            assert_eq!(identity.resource_identity(), preparer.resource_identity());
            assert_eq!(identity.stream_epoch(), 7);
            assert_eq!(identity.sequence(), 11);
            assert_eq!(identity.sample_rate_hz(), 48_000);
            assert_eq!(identity.kernels().len(), preparer.source_count());
            assert!(identity.max_tap_count() <= preparer.max_filter_taps());
            assert!(identity.kernels().iter().all(|kernel| {
                kernel.pair().sample_rate_hz() == 48_000
                    && !kernel.pair().left_taps().is_empty()
                    && kernel.pair().left_taps().len() == kernel.pair().right_taps().len()
            }));

            let q_preparer =
                ListenerOrientationPreparer::new(&BinauralConfig::builtin(hrtf, "2.0")).unwrap();
            let q =
                ListenerOrientation::new(0.0, 0.0, -0.043_619_387_365_336, 0.999_048_221_581_858_1)
                    .unwrap();
            let q_opposite =
                ListenerOrientation::new(0.0, 0.0, 0.043_619_387_365_336, -0.999_048_221_581_858_1)
                    .unwrap();
            assert_eq!(
                q_preparer.prepare(q, 7, 12).unwrap(),
                q_preparer.prepare(q_opposite, 7, 12).unwrap()
            );
        }
    }

    #[test]
    fn identity_prepared_kernels_equal_the_existing_static_hrir_selection() {
        for hrtf in [
            crate::BuiltinHrtf::SadieD1Ku100,
            crate::BuiltinHrtf::SadieD2Kemar,
        ] {
            for layout in ["5.1", "7.1.4", "9.1.6"] {
                let config = BinauralConfig::builtin(hrtf, layout);
                let static_state = crate::BinauralState::new(&config, None, None).unwrap();
                let bank = static_state.bank.as_ref().unwrap();
                let update = ListenerOrientationPreparer::new(&config)
                    .unwrap()
                    .prepare(ListenerOrientation::IDENTITY, 0, 0)
                    .unwrap();
                assert_eq!(update.kernels().len(), static_state.mappings.len());
                for (kernel, mapping) in update.kernels().iter().zip(&static_state.mappings) {
                    assert_eq!(kernel.source_id(), mapping.source_id);
                    let selected = bank
                        .entries()
                        .iter()
                        .find(|entry| entry.id() == mapping.hrir_entry)
                        .unwrap();
                    assert_eq!(kernel.pair(), selected.pair());
                }
            }
        }
    }

    #[test]
    fn custom_sofa_can_be_static_valid_but_rotation_fails_as_one_update() {
        let config = BinauralConfig::from_sofa_bytes(
            limited_sofa_fixture_for_5_1(),
            "5.1",
            crate::BinauralLfePolicy::Exclude,
        );
        crate::BinauralState::new(&config, None, None)
            .expect("static exact-direction 5.1 coverage");
        let preparer = ListenerOrientationPreparer::new(&config).unwrap();
        let identity_update = preparer
            .prepare(ListenerOrientation::IDENTITY, 3, 8)
            .unwrap();
        assert_eq!(identity_update.kernels().len(), 5);

        let half_sqrt = std::f64::consts::FRAC_1_SQRT_2;
        let pitch_up = ListenerOrientation::new(half_sqrt, 0.0, 0.0, half_sqrt).unwrap();
        let error = preparer.prepare(pitch_up, 3, 9).unwrap_err();
        assert!(matches!(
            error,
            ListenerOrientationPrepareError::HrirResolutionFailure { .. }
        ));
        assert_eq!(identity_update.sequence(), 8);
        assert_eq!(identity_update.stream_epoch(), 3);
    }

    #[test]
    fn post_resample_custom_resource_bound_covers_interpolated_delay_and_tail() {
        use openjoc_render::{HrirEntry, HrirEntryId, HrirPair};

        let bank = HrirBank::new(
            48_000,
            vec![
                HrirEntry::new(
                    HrirEntryId::new(1),
                    CartesianPosition::new(0.0, 1.0, 0.0),
                    HrirPair::new_with_delays(48_000, vec![1.0, 0.0], vec![1.0, 0.0], [0, 0])
                        .unwrap(),
                )
                .unwrap(),
                HrirEntry::new(
                    HrirEntryId::new(2),
                    CartesianPosition::new(1.0, 0.0, 0.0),
                    HrirPair::new_with_delays(48_000, vec![1.0, 0.0], vec![1.0, 0.0], [1, 1])
                        .unwrap(),
                )
                .unwrap(),
            ],
        )
        .unwrap();
        let bound = max_resolved_custom_tap_count(&bank).unwrap();
        let max_raw_taps = bank
            .entries()
            .iter()
            .map(|entry| entry.pair().tap_count())
            .max()
            .unwrap();
        let resolved =
            openjoc_sofa::resolve_hrir(&bank, CartesianPosition::new(1.0, 1.0, 0.0)).unwrap();
        assert_eq!(max_raw_taps, 2);
        assert_eq!(bound, 3);
        assert_eq!(resolved.pair.tap_count(), 3);
    }

    fn axis_angle(axis: [f64; 3], degrees: f64) -> ListenerOrientation {
        let half_angle = degrees.to_radians() * 0.5;
        let scale = half_angle.sin();
        ListenerOrientation::new(
            axis[0] * scale,
            axis[1] * scale,
            axis[2] * scale,
            half_angle.cos(),
        )
        .unwrap()
    }

    fn compose_rotations(
        first: ListenerOrientation,
        second: ListenerOrientation,
    ) -> ListenerOrientation {
        let [x1, y1, z1, w1] = first.as_xyzw();
        let [x2, y2, z2, w2] = second.as_xyzw();
        ListenerOrientation::new(
            w1 * x2 + x1 * w2 + y1 * z2 - z1 * y2,
            w1 * y2 - x1 * z2 + y1 * w2 + z1 * x2,
            w1 * z2 + x1 * y2 - y1 * x2 + z1 * w2,
            w1 * w2 - x1 * x2 - y1 * y2 - z1 * z2,
        )
        .unwrap()
    }

    fn print_timing_quantiles(label: &str, samples_ns: &[u128]) {
        let mut sorted = samples_ns.to_vec();
        sorted.sort_unstable();
        let percentile = |rank: usize| {
            let position = sorted.len().saturating_mul(rank).div_ceil(100);
            sorted[position.saturating_sub(1).min(sorted.len() - 1)]
        };
        let average = sorted.iter().sum::<u128>() / sorted.len() as u128;
        eprintln!(
            "{label}: n={} avg={:.3} us p50={:.3} us p95={:.3} us p99={:.3} us max={:.3} us",
            sorted.len(),
            average as f64 / 1_000.0,
            percentile(50) as f64 / 1_000.0,
            percentile(95) as f64 / 1_000.0,
            percentile(99) as f64 / 1_000.0,
            sorted.last().copied().unwrap_or(0) as f64 / 1_000.0,
        );
    }

    #[test]
    #[ignore = "manual built-in HRTF orientation coverage diagnostic; no pass/fail quality claim"]
    fn builtin_orientation_coverage_probe() {
        let poses = [
            ("+Z -1deg", [0.0, 0.0, 1.0], -1.0),
            ("+Z +1deg", [0.0, 0.0, 1.0], 1.0),
            ("+Z -5deg", [0.0, 0.0, 1.0], -5.0),
            ("+Z +5deg", [0.0, 0.0, 1.0], 5.0),
            ("+Z -15deg", [0.0, 0.0, 1.0], -15.0),
            ("+Z +15deg", [0.0, 0.0, 1.0], 15.0),
            ("+Z -30deg", [0.0, 0.0, 1.0], -30.0),
            ("+Z +30deg", [0.0, 0.0, 1.0], 30.0),
            ("+X -15deg", [1.0, 0.0, 0.0], -15.0),
            ("+X +15deg", [1.0, 0.0, 0.0], 15.0),
            ("+X -30deg", [1.0, 0.0, 0.0], -30.0),
            ("+X +30deg", [1.0, 0.0, 0.0], 30.0),
            ("+Y -15deg", [0.0, 1.0, 0.0], -15.0),
            ("+Y +15deg", [0.0, 1.0, 0.0], 15.0),
            ("+Y -30deg", [0.0, 1.0, 0.0], -30.0),
            ("+Y +30deg", [0.0, 1.0, 0.0], 30.0),
            ("+Z -45deg", [0.0, 0.0, 1.0], -45.0),
            ("+Z +45deg", [0.0, 0.0, 1.0], 45.0),
            ("+Z -90deg", [0.0, 0.0, 1.0], -90.0),
            ("+Z +90deg", [0.0, 0.0, 1.0], 90.0),
            ("+X -45deg", [1.0, 0.0, 0.0], -45.0),
            ("+X +45deg", [1.0, 0.0, 0.0], 45.0),
            ("+X -90deg", [1.0, 0.0, 0.0], -90.0),
            ("+X +90deg", [1.0, 0.0, 0.0], 90.0),
            ("+Y -45deg", [0.0, 1.0, 0.0], -45.0),
            ("+Y +45deg", [0.0, 1.0, 0.0], 45.0),
            ("+Y -90deg", [0.0, 1.0, 0.0], -90.0),
            ("+Y +90deg", [0.0, 1.0, 0.0], 90.0),
        ];
        for hrtf in [
            crate::BuiltinHrtf::SadieD1Ku100,
            crate::BuiltinHrtf::SadieD2Kemar,
        ] {
            for layout in ["7.1.4", "9.1.6"] {
                let preparer =
                    ListenerOrientationPreparer::new(&BinauralConfig::builtin(hrtf, layout))
                        .unwrap();
                eprintln!(
                    "coverage {hrtf:?} {layout} identity: {} sources",
                    preparer.source_count()
                );
                for (label, axis, degrees) in poses {
                    let orientation = axis_angle(axis, degrees);
                    match preparer.prepare(orientation, 0, 1) {
                        Ok(update) => eprintln!(
                            "coverage {hrtf:?} {layout} {label}: PASS {} sources, max_taps={}",
                            update.kernels().len(),
                            update.max_tap_count()
                        ),
                        Err(ListenerOrientationPrepareError::HrirResolutionFailure {
                            source_id,
                            world_direction,
                            listener_direction,
                            reason,
                        }) => eprintln!(
                            "coverage {hrtf:?} {layout} {label}: RESOLVER_REJECT source={source_id} world={world_direction:?} listener={listener_direction:?} reason={reason}"
                        ),
                        Err(error) => eprintln!(
                            "coverage {hrtf:?} {layout} {label}: PREPARATION_REJECT {error}"
                        ),
                    }
                }
                let composite = compose_rotations(
                    axis_angle([0.0, 0.0, 1.0], 30.0),
                    compose_rotations(
                        axis_angle([1.0, 0.0, 0.0], 20.0),
                        axis_angle([0.0, 1.0, 0.0], -15.0),
                    ),
                );
                match preparer.prepare(composite, 0, 99) {
                    Ok(update) => eprintln!(
                        "coverage {hrtf:?} {layout} composite yaw30/pitch20/roll-15: PASS {} sources, max_taps={}",
                        update.kernels().len(),
                        update.max_tap_count()
                    ),
                    Err(ListenerOrientationPrepareError::HrirResolutionFailure {
                        source_id,
                        world_direction,
                        listener_direction,
                        reason,
                    }) => eprintln!(
                        "coverage {hrtf:?} {layout} composite yaw30/pitch20/roll-15: RESOLVER_REJECT source={source_id} world={world_direction:?} listener={listener_direction:?} reason={reason}"
                    ),
                    Err(error) => eprintln!(
                        "coverage {hrtf:?} {layout} composite yaw30/pitch20/roll-15: PREPARATION_REJECT {error}"
                    ),
                }
            }
        }
    }

    #[test]
    #[ignore = "manual release-mode HRIR preparation latency probe; no CI timing threshold"]
    fn listener_orientation_prepare_latency_probe() {
        const UPDATE_RATE_HZ: usize = 120;
        for hrtf in [
            crate::BuiltinHrtf::SadieD1Ku100,
            crate::BuiltinHrtf::SadieD2Kemar,
        ] {
            for layout in ["7.1.4", "9.1.6"] {
                let config = BinauralConfig::builtin(hrtf, layout);
                let constructor_started = std::time::Instant::now();
                let preparer = ListenerOrientationPreparer::new(&config).unwrap();
                let constructor_ns = constructor_started.elapsed().as_nanos();
                let _ = preparer
                    .prepare(ListenerOrientation::IDENTITY, 0, 0)
                    .expect("identity warmup");

                let mut successful_ns = Vec::with_capacity(UPDATE_RATE_HZ);
                let mut rejected_ns = Vec::new();
                let mut first_error = None;
                for sequence in 0..UPDATE_RATE_HZ {
                    let phase = std::f64::consts::TAU * sequence as f64 / UPDATE_RATE_HZ as f64;
                    let yaw = 30.0 * phase.sin();
                    let pitch = 15.0 * (phase * 2.0).sin();
                    let roll = 10.0 * phase.cos();
                    let pose = compose_rotations(
                        axis_angle([0.0, 0.0, 1.0], yaw),
                        compose_rotations(
                            axis_angle([1.0, 0.0, 0.0], pitch),
                            axis_angle([0.0, 1.0, 0.0], roll),
                        ),
                    );
                    let started = std::time::Instant::now();
                    let result = preparer.prepare(pose, 0, sequence as u64 + 1);
                    let elapsed_ns = started.elapsed().as_nanos();
                    if let Err(error) = result {
                        rejected_ns.push(elapsed_ns);
                        if first_error.is_none() {
                            first_error = Some(error.to_string());
                        }
                    } else {
                        successful_ns.push(elapsed_ns);
                    }
                }
                eprintln!(
                    "prepare {hrtf:?} {layout}: constructor={:.3} ms query_tap_payload={} bytes input_rate={UPDATE_RATE_HZ} Hz rejected={}/{} first_error={:?}",
                    constructor_ns as f64 / 1_000_000.0,
                    preparer.query_hrir_tap_payload_bytes(),
                    rejected_ns.len(),
                    UPDATE_RATE_HZ,
                    first_error,
                );
                if !successful_ns.is_empty() {
                    print_timing_quantiles(
                        &format!("prepare {hrtf:?} {layout} successful"),
                        &successful_ns,
                    );
                }
                if !rejected_ns.is_empty() {
                    print_timing_quantiles(
                        &format!("prepare {hrtf:?} {layout} rejected"),
                        &rejected_ns,
                    );
                }
            }
        }
    }

    #[test]
    fn resource_identity_changes_with_layout_and_hrtf() {
        let d1_714 = ListenerOrientationPreparer::new(&BinauralConfig::builtin(
            crate::BuiltinHrtf::SadieD1Ku100,
            "7.1.4",
        ))
        .unwrap();
        let d1_916 = ListenerOrientationPreparer::new(&BinauralConfig::builtin(
            crate::BuiltinHrtf::SadieD1Ku100,
            "9.1.6",
        ))
        .unwrap();
        let d2_714 = ListenerOrientationPreparer::new(&BinauralConfig::builtin(
            crate::BuiltinHrtf::SadieD2Kemar,
            "7.1.4",
        ))
        .unwrap();
        assert_ne!(d1_714.resource_identity(), d1_916.resource_identity());
        assert_ne!(d1_714.resource_identity(), d2_714.resource_identity());
    }
}
