//! Prepared, immutable HRIR sets for bounded dynamic binaural updates.

use crate::{HrirPair, RenderError, SourceId};

/// Stable identity for the HRTF resource and virtual-speaker layout that
/// produced a prepared update.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BinauralResourceIdentity([u8; 32]);

impl BinauralResourceIdentity {
    /// Creates an identity from a content/configuration digest.
    #[must_use]
    pub const fn new(digest: [u8; 32]) -> Self {
        Self(digest)
    }

    /// Returns the complete identity digest.
    #[must_use]
    pub const fn digest(self) -> [u8; 32] {
        self.0
    }
}

/// One source's already-resolved left/right HRIR pair.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedBinauralKernel {
    source_id: SourceId,
    pair: HrirPair,
}

impl PreparedBinauralKernel {
    /// Creates one immutable prepared kernel for a registered source.
    #[must_use]
    pub const fn new(source_id: SourceId, pair: HrirPair) -> Self {
        Self { source_id, pair }
    }

    /// Returns the registered source identity.
    #[must_use]
    pub const fn source_id(&self) -> SourceId {
        self.source_id
    }

    /// Returns the validated causal HRIR pair.
    #[must_use]
    pub const fn pair(&self) -> &HrirPair {
        &self.pair
    }
}

/// An immutable all-source update prepared away from the render path.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedBinauralUpdate {
    resource_identity: BinauralResourceIdentity,
    stream_epoch: u64,
    sequence: u64,
    sample_rate_hz: u32,
    kernels: Vec<PreparedBinauralKernel>,
}

impl PreparedBinauralUpdate {
    /// Creates a complete prepared update, rejecting duplicate sources or
    /// mixed sample rates before it can be applied to a renderer.
    pub fn new(
        resource_identity: BinauralResourceIdentity,
        stream_epoch: u64,
        sequence: u64,
        kernels: Vec<PreparedBinauralKernel>,
    ) -> Result<Self, RenderError> {
        let Some(first) = kernels.first() else {
            return Err(RenderError::EmptyBinauralSourceSet);
        };
        let sample_rate_hz = first.pair.sample_rate_hz();
        for (index, kernel) in kernels.iter().enumerate() {
            if kernel.pair.sample_rate_hz() != sample_rate_hz {
                return Err(RenderError::HrirSampleRateMismatch {
                    expected: sample_rate_hz,
                    actual: kernel.pair.sample_rate_hz(),
                });
            }
            if kernels[..index]
                .iter()
                .any(|previous| previous.source_id == kernel.source_id)
            {
                return Err(RenderError::DuplicateSourceId {
                    id: kernel.source_id,
                });
            }
        }
        Ok(Self {
            resource_identity,
            stream_epoch,
            sequence,
            sample_rate_hz,
            kernels,
        })
    }

    /// Returns the resource/layout identity associated with these filters.
    #[must_use]
    pub const fn resource_identity(&self) -> BinauralResourceIdentity {
        self.resource_identity
    }

    /// Returns the stream epoch captured when the update was prepared.
    #[must_use]
    pub const fn stream_epoch(&self) -> u64 {
        self.stream_epoch
    }

    /// Returns the caller-assigned monotonic orientation sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Returns the exact sample rate of every prepared HRIR.
    #[must_use]
    pub const fn sample_rate_hz(&self) -> u32 {
        self.sample_rate_hz
    }

    /// Returns all non-LFE source kernels in registered order.
    #[must_use]
    pub fn kernels(&self) -> &[PreparedBinauralKernel] {
        &self.kernels
    }

    /// Returns the largest left/right tap count in the update.
    #[must_use]
    pub fn max_tap_count(&self) -> usize {
        self.kernels
            .iter()
            .map(|kernel| kernel.pair.tap_count())
            .max()
            .unwrap_or(0)
    }

    /// Returns the owned kernel allocations, primarily for an adapter that
    /// must explicitly recycle a superseded prepared update.
    pub fn into_kernels(self) -> Vec<PreparedBinauralKernel> {
        self.kernels
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        BinauralResourceIdentity,
        u64,
        u64,
        u32,
        Vec<PreparedBinauralKernel>,
    ) {
        (
            self.resource_identity,
            self.stream_epoch,
            self.sequence,
            self.sample_rate_hz,
            self.kernels,
        )
    }
}
