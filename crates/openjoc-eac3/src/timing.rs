//! Opt-in E-AC-3 core stage timing used by performance diagnostics.

use std::time::{Duration, Instant};

/// Accumulated wall-clock time for real implementation boundaries inside one
/// or more successful E-AC-3 access-unit decodes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Eac3DecodeStageTiming {
    pub total: Duration,
    pub syncframe_and_header_parsing: Duration,
    pub audio_block_syntax_and_exponents: Duration,
    pub bit_allocation: Duration,
    pub mantissa_unpack_and_dequantization: Duration,
    pub coupling_rematrix_and_spx: Duration,
    pub inverse_transform: Duration,
    pub window_and_overlap_add: Duration,
    pub pcm_assembly: Duration,
    pub allocation_and_copy: Duration,
    pub decoder_state_commit: Duration,
    pub syncframes: u64,
    pub audio_blocks: u64,
    pub full_bandwidth_channel_blocks: u64,
    pub lfe_blocks: u64,
    pub long_transforms: u64,
    pub short_transforms: u64,
    pub aht_elements: u64,
    pub coupling_blocks: u64,
    pub spx_blocks: u64,
}

impl Eac3DecodeStageTiming {
    /// Adds another timing record into this aggregate.
    pub fn add_assign(&mut self, other: &Self) {
        self.total += other.total;
        self.syncframe_and_header_parsing += other.syncframe_and_header_parsing;
        self.audio_block_syntax_and_exponents += other.audio_block_syntax_and_exponents;
        self.bit_allocation += other.bit_allocation;
        self.mantissa_unpack_and_dequantization += other.mantissa_unpack_and_dequantization;
        self.coupling_rematrix_and_spx += other.coupling_rematrix_and_spx;
        self.inverse_transform += other.inverse_transform;
        self.window_and_overlap_add += other.window_and_overlap_add;
        self.pcm_assembly += other.pcm_assembly;
        self.allocation_and_copy += other.allocation_and_copy;
        self.decoder_state_commit += other.decoder_state_commit;
        self.syncframes += other.syncframes;
        self.audio_blocks += other.audio_blocks;
        self.full_bandwidth_channel_blocks += other.full_bandwidth_channel_blocks;
        self.lfe_blocks += other.lfe_blocks;
        self.long_transforms += other.long_transforms;
        self.short_transforms += other.short_transforms;
        self.aht_elements += other.aht_elements;
        self.coupling_blocks += other.coupling_blocks;
        self.spx_blocks += other.spx_blocks;
    }

    pub(crate) fn measure<T, E>(
        timing: Option<&mut Self>,
        stage: fn(&mut Self) -> &mut Duration,
        operation: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, E> {
        let start = timing.is_some().then(Instant::now);
        let result = operation();
        if let (Some(timing), Some(start)) = (timing, start) {
            *stage(timing) += start.elapsed();
        }
        result
    }
}
