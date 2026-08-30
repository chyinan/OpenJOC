// pattern: Functional Core

//! Clean-room AC-3 / Enhanced AC-3 frontend from ETSI TS 102 366.

// The reference frontend intentionally keeps literal ETSI tables, explicit
// checked index conversions, and long clause-shaped syntax/error functions.
// These lints are presentation-oriented and would require cosmetic rewrites
// of normative code; correctness checks remain enabled by the workspace.
#![allow(
    clippy::approx_constant,
    clippy::assigning_clones,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::doc_markdown,
    clippy::map_unwrap_or,
    clippy::match_same_arms,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::naive_bytecount,
    clippy::needless_pass_by_value,
    clippy::needless_range_loop,
    clippy::redundant_locals,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::type_complexity,
    clippy::unnecessary_wraps,
    clippy::unreadable_literal,
    clippy::useless_conversion
)]

mod ac3;
mod access_unit;
mod aht;
mod audio_block;
mod bit_allocation;
mod coding_tools;
mod dialnorm;
mod dynamic_range;
mod mantissa;
mod rematrix;
mod spx;
mod stereo_downmix;
mod timing;
mod transform;

use ac3::validate_ac3_crc;

pub use access_unit::{
    ChannelLocation, DecodedAccessUnitPcm, DecodedJocAccessUnitPcm, JocAccessUnitPcmDecoder,
    validate_joc_access_unit_decoder_contract,
};
pub use aht::{
    decode_aht_element_mantissas, decode_aht_gaq_mantissa, decode_aht_vq_vector,
    expand_aht_gaq_gains,
};
pub use audio_block::{
    AhtQuantizationInformation, AudioBlockCarrier, AudioBlockCarrierReport, AudioBlockPrefix,
    AudioPcmSynthesizer, BitAllocationParameters, CouplingInformation, CouplingLeak,
    DecodedAudioBlock, DecodedAudioPcm, DeltaBitAllocation, DeltaBitAllocationElement,
    DeltaBitAllocationSegment, EnhancedCouplingInformation, EnhancedCouplingReconstruction,
    ExponentInformation, FastGainCodes, InternalBasePolicy, MantissaElementTrace, SnrOffsets,
    SpectralExtensionCoordinates, SpectralExtensionInformation, StandardCouplingCoordinates,
    StandardCouplingInformation, TdacContribution, decode_audio_blocks,
    decode_audio_blocks_with_diagnostic_trace, decode_audio_blocks_with_parsed_frame,
    decode_audio_blocks_with_policy, decode_audio_frame_pcm, decode_audio_frame_pcm_with_policy,
    decode_first_audio_block, decode_first_audio_block_with_policy, inspect_audio_block_carriers,
    inverse_aht_dct, parse_first_audio_block_prefix, reconstruct_enhanced_coupling,
    reconstruct_standard_coupling, synthesize_audio_blocks,
};
pub use bit_allocation::{
    BitAllocationBand, FixedBitAllocationParameters, apply_delta_bit_allocation,
    bit_allocation_band, bit_allocation_band_for_bin, bit_allocation_pointer, calc_lowcomp,
    compute_bap, compute_element_bap, compute_excitation, compute_high_efficiency_bap,
    compute_high_efficiency_element_bap, compute_masking_curve, decode_bit_allocation_parameters,
    exponents_to_psd, high_efficiency_bit_allocation_pointer, integrate_psd, log_add, snr_offset,
    snr_offsets_are_zero,
};
pub use coding_tools::{
    CodingToolBlockInventory, CodingToolInventory, InventoryProvenance, SemanticChannel,
    emit_coding_tool_inventory,
};
pub use dialnorm::{DialnormMode, DialnormState};
pub use dynamic_range::{
    DynamicRangeControl, apply_dynamic_range_gains, compression_gain, dynamic_range_gain,
    scaled_dynamic_range_code,
};
pub use mantissa::{
    MantissaDecodeTrace, MantissaQuantizer, decode_mantissa_code, decode_mantissas,
    mantissa_quantizer, shift_mantissa, ungroup_mantissa_code,
};
pub use rematrix::rematrix_channels;
pub use spx::synthesize_spectral_extension;
pub use stereo_downmix::{
    StereoDownmixError, StereoDownmixMatrix, StereoDownmixMode, StereoDownmixRow,
    stereo_downmix_matrix,
};
pub use timing::Eac3DecodeStageTiming;
pub use transform::{
    InverseTransformTrace, OverlapAddTrace, inverse_transform, inverse_transform_with_trace,
    overlap_add, overlap_add_with_trace,
};

use core::fmt;
use openjoc_bitio::{BitError, BitRead, BitReader};
use openjoc_emdf::{
    CarrierClassification, EmdfContainer, EmdfError, JOC_PAYLOAD_ID, JocProfileDeviation,
    JocProfileValidationFailure, JocValidationProfile, JocValidationStatus, OAMD_PAYLOAD_ID,
    ParsedEmdf, classify_emdf_carrier, parse_emdf_sync, validate_joc_profile_for,
};

const EAC3_SYNCWORD: u16 = 0x0b77;

/// Spectral element that owns a conventional mantissa codeword.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MantissaElement {
    Channel,
    Coupling,
    Lfe,
}

/// Checked frontend failures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Eac3Error {
    Bit(BitError),
    InvalidSyncword {
        actual: u16,
    },
    UnsupportedBitstreamId {
        actual: u8,
    },
    ReservedStreamType,
    ReservedSampleRate,
    FrameSizeOverflow,
    TruncatedFrame {
        offset: usize,
        declared: usize,
        available: usize,
    },
    InvalidAddbsiLength {
        actual: usize,
    },
    NonzeroReservedData,
    InvalidDeltaBitAllocationStrategy {
        actual: u8,
    },
    InvalidBitAllocationParameterCode {
        parameter: &'static str,
        actual: u8,
    },
    InvalidBitAllocationTableIndex {
        table: &'static str,
        actual: u16,
    },
    InvalidPsdRange {
        start: usize,
        end: usize,
    },
    MissingJocExtensionFlag,
    ComplexityIndexOutOfRange {
        actual: u8,
    },
    ComplexityIndexMismatch {
        complexity: u8,
        objects: u16,
    },
    InvalidFrameExponentStrategy {
        actual: u8,
    },
    InvalidExponentStrategy {
        actual: u8,
    },
    InvalidChannelBandwidthCode {
        actual: u8,
    },
    InvalidGroupedExponent {
        actual: u8,
    },
    ExponentOutOfRange {
        actual: i16,
    },
    ExponentGroupCountMismatch {
        expected: usize,
        actual: usize,
    },
    InvalidExponentDimensions {
        end_mantissa: usize,
    },
    InvalidMantissaBap {
        actual: u8,
    },
    InvalidMantissaCode {
        bap: u8,
        actual: u16,
    },
    InvalidMantissaGroupCode {
        bap: u8,
        actual: u16,
    },
    InvalidMantissaDiagnostic {
        element: MantissaElement,
        channel: Option<u8>,
        block: usize,
        bap: u8,
        actual: u16,
        bit_width: u8,
        bit_offset_bits: usize,
        grouped: bool,
        spx_active: bool,
        coupling_active: bool,
        enhanced_coupling_active: bool,
        rematrix_active: bool,
        aht_active: bool,
        bin_index: usize,
        exponent: u8,
        psd: Option<i16>,
        mask: Option<i16>,
        quantizer_levels: u32,
        quantizer_group_size: u8,
        quantizer_group_bits: u8,
        quantizer_symmetric: bool,
        group_position: u8,
        dither: bool,
        block_switch: bool,
        exponent_strategy: u8,
        exponent_reused: bool,
        block_start_offset_bits: usize,
        element_start_offset_bits: usize,
    },
    MantissaExponentLengthMismatch {
        baps: usize,
        exponents: usize,
    },
    MantissaDitherLengthMismatch {
        expected: usize,
        actual: usize,
    },
    MissingDitherValue {
        index: usize,
    },
    InvalidSpectralExtensionCode {
        begin_code: u8,
        end_code: u8,
    },
    InvalidSpectralExtensionRange {
        begin: u8,
        end: u8,
    },
    InvalidSpectralExtensionCoordinateDimensions {
        expected: usize,
        actual: usize,
    },
    InvalidSpectralExtensionCoordinate {
        exponent: u8,
        mantissa: u8,
        master: u8,
    },
    MissingSpectralExtensionNoise {
        expected: usize,
        actual: usize,
    },
    NonFiniteSpectralExtensionCoefficient {
        index: usize,
    },
    InvalidCouplingRange {
        begin: i16,
        end: i16,
    },
    InvalidCouplingCoordinateDimensions {
        expected: usize,
        actual: usize,
    },
    InvalidCouplingCoordinate {
        exponent: u8,
        mantissa: u8,
        master: u8,
    },
    NonFiniteCouplingCoefficient,
    InvalidDynamicRangeGainCount {
        expected: usize,
        actual: usize,
    },
    NonFiniteDynamicRangeCoefficient {
        channel: usize,
        index: usize,
    },
    InvalidRematrixChannelCount {
        actual: usize,
    },
    InvalidRematrixFlagCount {
        expected: usize,
        actual: usize,
    },
    NonFiniteRematrixCoefficient {
        channel: usize,
        index: usize,
    },
    InvalidBlockStartDimensions {
        frame_size: usize,
        audio_blocks: u8,
    },
    ReservedSnrOffsetStrategy,
    UnsupportedAdaptiveHybridTransform,
    NonFiniteAhtCoefficient,
    InvalidAhtGaqMode {
        actual: u8,
    },
    InvalidAhtGaqGainWord {
        actual: u8,
    },
    InvalidAhtGaqHebap {
        actual: u8,
    },
    InvalidAhtGaqGain {
        actual: u8,
    },
    InvalidAhtGaqCode {
        actual: u16,
    },
    InvalidAhtVqHebap {
        actual: u8,
    },
    InvalidAhtVqIndex {
        hebap: u8,
        actual: u16,
    },
    InvalidTransformCoefficientLength {
        expected: usize,
        actual: usize,
    },
    InvalidTransformWindowLength {
        actual: usize,
    },
    InvalidAudioBlockChannelCount {
        expected: usize,
        actual: usize,
    },
    InvalidAudioBlockSwitchCount {
        expected: usize,
        actual: usize,
    },
    InvalidAudioBlockLfePresence {
        expected: bool,
        actual: bool,
    },
    NonFiniteTransformCoefficient {
        index: usize,
    },
    MissingIndependentSubstreamZero {
        frame: usize,
    },
    NonsequentialIndependentSubstream {
        expected: u8,
        actual: u8,
    },
    NonsequentialDependentSubstream {
        expected: u8,
        actual: u8,
    },
    DependentAfterConvertedSubstream {
        frame: usize,
    },
    SubstreamTimingMismatch {
        frame: usize,
    },
    AuxDataLengthOutOfRange {
        declared: usize,
        available: usize,
    },
    AuxDataNotByteAligned {
        bits: usize,
    },
    /// A bounded skip-field began with an EMDF container that ended before
    /// the declared carrier range. Annex H does not define implicit carrier
    /// padding for this API, so the candidate is not accepted.
    EmdfCarrierTrailingData {
        container_bytes: usize,
        carrier_bytes: usize,
    },
    AudioBlockCarrierTraversalUnresolved {
        examined_blocks: usize,
        unresolved_blocks: usize,
    },
    Emdf(EmdfError),
    JocProfileValidation(JocProfileValidationFailure),
    InvalidAccessUnitRange,
    UnsupportedJocAccessUnitFrameCount {
        actual: usize,
    },
    UnsupportedJocAudioBlockCount {
        actual: u8,
    },
    UnsupportedJocChannelTopology {
        full_band_channels: usize,
        lfe_present: bool,
    },
    InvalidDependentChannelMap {
        expected: usize,
        actual: usize,
    },
    MultipleLfeChannels,
    AccessUnitPcmSampleCountMismatch {
        expected: usize,
        actual: usize,
    },
    MultipleJocCarriers,
    MissingJocAddbsi {
        frame: usize,
    },
    InvalidJocCarrierPlacement {
        carrier_frame: usize,
        required_frame: usize,
    },
    InvalidAc3Crc {
        region: &'static str,
    },
    UnsupportedAc3CodingTool {
        tool: &'static str,
    },
    Ac3AudioBlock {
        block: usize,
        source: Box<Eac3Error>,
    },
    Ac3SyntaxField {
        field: &'static str,
        source: Box<Eac3Error>,
    },
    InvalidAc3Syntax {
        field: &'static str,
    },
}

impl Eac3Error {
    fn static_message(&self) -> Option<&'static str> {
        match self {
            Self::ReservedStreamType => Some("reserved E-AC-3 stream type"),
            Self::ReservedSampleRate => Some("reserved E-AC-3 sample-rate code"),
            Self::FrameSizeOverflow => Some("E-AC-3 frame-size overflow"),
            Self::NonzeroReservedData => Some("nonzero reserved E-AC-3 data"),
            Self::MissingJocExtensionFlag => Some("missing E-AC-3 JOC extension flag"),
            Self::ReservedSnrOffsetStrategy => Some("reserved E-AC-3 SNR offset strategy"),
            Self::UnsupportedAdaptiveHybridTransform => {
                Some("E-AC-3 adaptive hybrid transform mantissas are not yet supported")
            }
            Self::NonFiniteAhtCoefficient => Some("non-finite E-AC-3 AHT coefficient"),
            Self::InvalidAccessUnitRange => Some("invalid E-AC-3 access-unit range"),
            Self::MultipleLfeChannels => Some("multiple E-AC-3 LFE channels are not supported"),
            Self::MultipleJocCarriers => {
                Some("multiple JOC EMDF carriers in one E-AC-3 access unit")
            }
            _ => None,
        }
    }

    fn format_exponent_error(&self, formatter: &mut fmt::Formatter<'_>) -> Option<fmt::Result> {
        match self {
            Self::InvalidGroupedExponent { actual } => Some(write!(
                formatter,
                "invalid E-AC-3 grouped exponent {actual}"
            )),
            Self::ExponentOutOfRange { actual } => Some(write!(
                formatter,
                "decoded E-AC-3 exponent {actual} is out of range"
            )),
            Self::ExponentGroupCountMismatch { expected, actual } => Some(write!(
                formatter,
                "E-AC-3 exponent group count mismatch: expected {expected}, got {actual}"
            )),
            Self::InvalidExponentDimensions { end_mantissa } => Some(write!(
                formatter,
                "invalid E-AC-3 exponent end mantissa {end_mantissa}"
            )),
            _ => None,
        }
    }

    fn format_structure_error(&self, formatter: &mut fmt::Formatter<'_>) -> Option<fmt::Result> {
        match self {
            Self::InvalidBlockStartDimensions {
                frame_size,
                audio_blocks,
            } => Some(write!(
                formatter,
                "invalid E-AC-3 block-start dimensions: {frame_size} frame bytes and {audio_blocks} blocks"
            )),
            Self::MissingIndependentSubstreamZero { frame } => Some(write!(
                formatter,
                "E-AC-3 access unit at frame {frame} does not begin with independent substream 0"
            )),
            Self::NonsequentialIndependentSubstream { expected, actual } => Some(write!(
                formatter,
                "nonsequential E-AC-3 independent substream: expected {expected}, got {actual}"
            )),
            Self::NonsequentialDependentSubstream { expected, actual } => Some(write!(
                formatter,
                "nonsequential E-AC-3 dependent substream: expected {expected}, got {actual}"
            )),
            Self::DependentAfterConvertedSubstream { frame } => Some(write!(
                formatter,
                "dependent E-AC-3 frame {frame} follows a converted independent substream"
            )),
            Self::UnsupportedJocAccessUnitFrameCount { actual } => Some(write!(
                formatter,
                "JOC E-AC-3 access unit contains {actual} frames; expected one independent and at most one dependent frame"
            )),
            Self::UnsupportedJocAudioBlockCount { actual } => Some(write!(
                formatter,
                "JOC E-AC-3 syncframe contains {actual} audio blocks; expected six"
            )),
            Self::UnsupportedJocChannelTopology {
                full_band_channels,
                lfe_present,
            } => Some(write!(
                formatter,
                "JOC E-AC-3 access unit exposes unsupported Table 47 topology: {full_band_channels} full-band channels, LFE present={lfe_present}"
            )),
            Self::InvalidDependentChannelMap { expected, actual } => Some(write!(
                formatter,
                "dependent E-AC-3 channel map contains {expected} channels but audio carries {actual}"
            )),
            Self::AccessUnitPcmSampleCountMismatch { expected, actual } => Some(write!(
                formatter,
                "E-AC-3 access-unit PCM contains {actual} samples; expected {expected}"
            )),
            Self::InvalidSpectralExtensionCode {
                begin_code,
                end_code,
            } => Some(write!(
                formatter,
                "invalid E-AC-3 spectral-extension codes {begin_code}, {end_code}"
            )),
            Self::InvalidSpectralExtensionRange { begin, end } => Some(write!(
                formatter,
                "invalid E-AC-3 spectral-extension subband range {begin}..{end}"
            )),
            Self::InvalidCouplingRange { begin, end } => Some(write!(
                formatter,
                "invalid E-AC-3 coupling subband range {begin}..{end}"
            )),
            Self::InvalidCouplingCoordinateDimensions { expected, actual } => Some(write!(
                formatter,
                "invalid E-AC-3 coupling coordinate dimensions: expected {expected}, got {actual}"
            )),
            Self::InvalidSpectralExtensionCoordinateDimensions { expected, actual } => {
                Some(write!(
                    formatter,
                    "invalid E-AC-3 spectral-extension coordinate dimensions: expected {expected}, got {actual}"
                ))
            }
            Self::InvalidSpectralExtensionCoordinate {
                exponent,
                mantissa,
                master,
            } => Some(write!(
                formatter,
                "invalid E-AC-3 spectral-extension coordinate {exponent}/{mantissa}/{master}"
            )),
            Self::MissingSpectralExtensionNoise { expected, actual } => Some(write!(
                formatter,
                "missing E-AC-3 spectral-extension noise: expected {expected}, got {actual}"
            )),
            Self::NonFiniteSpectralExtensionCoefficient { index } => Some(write!(
                formatter,
                "non-finite E-AC-3 spectral-extension coefficient at index {index}"
            )),
            Self::InvalidCouplingCoordinate {
                exponent,
                mantissa,
                master,
            } => Some(write!(
                formatter,
                "invalid E-AC-3 coupling coordinate {exponent}/{mantissa}/{master}"
            )),
            Self::NonFiniteCouplingCoefficient => {
                Some(write!(formatter, "non-finite E-AC-3 coupling coefficient"))
            }
            Self::InvalidRematrixFlagCount { expected, actual } => Some(write!(
                formatter,
                "invalid E-AC-3 rematrix flag count: expected {expected}, got {actual}"
            )),
            Self::InvalidRematrixChannelCount { actual } => Some(write!(
                formatter,
                "invalid E-AC-3 rematrix channel count {actual}; expected 2"
            )),
            Self::InvalidDynamicRangeGainCount { expected, actual } => Some(write!(
                formatter,
                "invalid E-AC-3 dynamic-range gain count: expected {expected}, got {actual}"
            )),
            Self::NonFiniteDynamicRangeCoefficient { channel, index } => Some(write!(
                formatter,
                "non-finite E-AC-3 dynamic-range coefficient at channel {channel}, index {index}"
            )),
            Self::NonFiniteRematrixCoefficient { channel, index } => Some(write!(
                formatter,
                "non-finite E-AC-3 rematrix coefficient at channel {channel}, index {index}"
            )),
            Self::InvalidBitAllocationParameterCode { parameter, actual } => Some(write!(
                formatter,
                "invalid E-AC-3 {parameter} bit allocation parameter code {actual}"
            )),
            Self::InvalidBitAllocationTableIndex { table, actual } => Some(write!(
                formatter,
                "invalid E-AC-3 bit allocation {table} index {actual}"
            )),
            Self::InvalidPsdRange { start, end } => Some(write!(
                formatter,
                "invalid E-AC-3 PSD integration range {start}..{end}"
            )),
            Self::InvalidDeltaBitAllocationStrategy { actual } => Some(write!(
                formatter,
                "invalid first-block E-AC-3 delta bit allocation strategy {actual}"
            )),
            Self::InvalidMantissaBap { actual } => Some(write!(
                formatter,
                "invalid E-AC-3 mantissa bit allocation pointer {actual}"
            )),
            Self::InvalidMantissaCode { bap, actual } => Some(write!(
                formatter,
                "invalid E-AC-3 mantissa code {actual} for bap {bap}"
            )),
            Self::InvalidMantissaGroupCode { bap, actual } => Some(write!(
                formatter,
                "invalid E-AC-3 mantissa group code {actual} for bap {bap}"
            )),
            Self::InvalidMantissaDiagnostic {
                element,
                channel,
                block,
                bap,
                actual,
                bit_width,
                bit_offset_bits,
                grouped,
                spx_active,
                coupling_active,
                enhanced_coupling_active,
                rematrix_active,
                aht_active,
                bin_index,
                exponent,
                psd,
                mask,
                quantizer_levels,
                quantizer_group_size,
                quantizer_group_bits,
                quantizer_symmetric,
                group_position,
                dither,
                block_switch,
                exponent_strategy,
                exponent_reused,
                block_start_offset_bits,
                element_start_offset_bits,
            } => Some(write!(
                formatter,
                "invalid E-AC-3 mantissa code {actual} for bap {bap}; element {element:?}, channel {channel:?}, block {block}, bin {bin_index}, exponent {exponent}, width {bit_width}, bit offset {bit_offset_bits}, block offset {block_start_offset_bits}, element offset {element_start_offset_bits}, grouped {grouped}, group position {group_position}, quantizer levels {quantizer_levels}, group {quantizer_group_size}x{quantizer_group_bits}, symmetric {quantizer_symmetric}, psd {psd:?}, mask {mask:?}, dither {dither}, block switch {block_switch}, exponent strategy {exponent_strategy}, exponent reused {exponent_reused}, spx {spx_active}, coupling {coupling_active}, enhanced coupling {enhanced_coupling_active}, rematrix {rematrix_active}, aht {aht_active}"
            )),
            Self::MantissaExponentLengthMismatch { baps, exponents } => Some(write!(
                formatter,
                "E-AC-3 mantissa/exponent length mismatch: {baps} baps and {exponents} exponents"
            )),
            Self::MantissaDitherLengthMismatch { expected, actual } => Some(write!(
                formatter,
                "E-AC-3 mantissa/dither length mismatch: expected {expected}, got {actual}"
            )),
            Self::MissingDitherValue { index } => Some(write!(
                formatter,
                "missing E-AC-3 dither value at mantissa {index}"
            )),
            Self::InvalidTransformCoefficientLength { expected, actual } => Some(write!(
                formatter,
                "invalid E-AC-3 transform coefficient length: expected {expected}, got {actual}"
            )),
            Self::InvalidTransformWindowLength { actual } => Some(write!(
                formatter,
                "invalid E-AC-3 transform window length {actual}"
            )),
            Self::InvalidAudioBlockChannelCount { expected, actual } => Some(write!(
                formatter,
                "invalid E-AC-3 audio-block channel count: expected {expected}, got {actual}"
            )),
            Self::InvalidAudioBlockSwitchCount { expected, actual } => Some(write!(
                formatter,
                "invalid E-AC-3 audio-block switch count: expected {expected}, got {actual}"
            )),
            Self::InvalidAudioBlockLfePresence { expected, actual } => Some(write!(
                formatter,
                "invalid E-AC-3 audio-block LFE presence: expected {expected}, got {actual}"
            )),
            Self::NonFiniteTransformCoefficient { index } => Some(write!(
                formatter,
                "non-finite E-AC-3 transform coefficient at index {index}"
            )),
            _ => None,
        }
    }
}

impl fmt::Display for Eac3Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(message) = self.static_message() {
            return formatter.write_str(message);
        }
        if let Some(result) = self.format_exponent_error(formatter) {
            return result;
        }
        if let Some(result) = self.format_structure_error(formatter) {
            return result;
        }
        match self {
            Self::Bit(error) => write!(formatter, "failed to read E-AC-3 bitstream: {error}"),
            Self::InvalidSyncword { actual } => {
                write!(formatter, "invalid AC-3/E-AC-3 syncword 0x{actual:04x}")
            }
            Self::UnsupportedBitstreamId { actual } => {
                write!(formatter, "unsupported AC-3/E-AC-3 bitstream id {actual}")
            }
            Self::TruncatedFrame {
                offset,
                declared,
                available,
            } => write!(
                formatter,
                "truncated E-AC-3 frame at byte {offset}: declared {declared} bytes with {available} available"
            ),
            Self::InvalidAddbsiLength { actual } => {
                write!(formatter, "invalid JOC addbsi length {actual}; expected 2")
            }
            Self::InvalidAhtGaqMode { actual } => {
                write!(formatter, "invalid E-AC-3 AHT GAQ mode {actual}")
            }
            Self::InvalidAhtGaqGainWord { actual } => {
                write!(formatter, "invalid E-AC-3 AHT GAQ gain word {actual}")
            }
            Self::InvalidAhtGaqHebap { actual } => {
                write!(formatter, "invalid E-AC-3 AHT hebap {actual}")
            }
            Self::InvalidAhtGaqGain { actual } => {
                write!(formatter, "invalid E-AC-3 AHT GAQ gain {actual}")
            }
            Self::InvalidAhtGaqCode { actual } => {
                write!(formatter, "invalid E-AC-3 AHT GAQ code {actual}")
            }
            Self::InvalidAhtVqHebap { actual } => {
                write!(formatter, "invalid E-AC-3 AHT VQ hebap {actual}")
            }
            Self::InvalidAhtVqIndex { hebap, actual } => {
                write!(
                    formatter,
                    "invalid E-AC-3 AHT VQ index {actual} for hebap {hebap}"
                )
            }
            Self::ComplexityIndexOutOfRange { actual } => {
                write!(formatter, "E-AC-3 JOC complexity index {actual} exceeds 16")
            }
            Self::ComplexityIndexMismatch {
                complexity,
                objects,
            } => write!(
                formatter,
                "E-AC-3 JOC complexity index {complexity} does not equal OAMD object count {objects}"
            ),
            Self::InvalidFrameExponentStrategy { actual } => {
                write!(formatter, "invalid E-AC-3 frame exponent strategy {actual}")
            }
            Self::InvalidExponentStrategy { actual } => {
                write!(formatter, "invalid E-AC-3 exponent strategy {actual}")
            }
            Self::InvalidChannelBandwidthCode { actual } => {
                write!(formatter, "invalid E-AC-3 channel bandwidth code {actual}")
            }
            Self::SubstreamTimingMismatch { frame } => {
                write!(
                    formatter,
                    "E-AC-3 substream timing mismatch at frame {frame}"
                )
            }
            Self::AuxDataLengthOutOfRange {
                declared,
                available,
            } => write!(
                formatter,
                "E-AC-3 auxiliary-data length {declared} exceeds {available} available bits"
            ),
            Self::AuxDataNotByteAligned { bits } => write!(
                formatter,
                "E-AC-3 EMDF auxiliary data is not byte-aligned: {bits} bits"
            ),
            Self::Emdf(error) => write!(formatter, "failed to decode carried EMDF: {error}"),
            Self::JocProfileValidation(error) => write!(formatter, "{error}"),
            Self::EmdfCarrierTrailingData {
                container_bytes,
                carrier_bytes,
            } => write!(
                formatter,
                "EMDF carrier has {carrier_bytes} bytes but its bounded container ends at {container_bytes} bytes"
            ),
            Self::AudioBlockCarrierTraversalUnresolved {
                examined_blocks,
                unresolved_blocks,
            } => write!(
                formatter,
                "audio-block carrier traversal reached {examined_blocks} blocks and left {unresolved_blocks} unresolved"
            ),
            Self::MissingJocAddbsi { frame } => {
                write!(formatter, "missing JOC addbsi in carrier frame {frame}")
            }
            Self::InvalidJocCarrierPlacement {
                carrier_frame,
                required_frame,
            } => write!(
                formatter,
                "JOC EMDF carrier frame {carrier_frame} is not required last dependent frame {required_frame}"
            ),
            Self::InvalidAc3Crc { region } => {
                write!(formatter, "invalid original-syntax AC-3 {region} coverage")
            }
            Self::UnsupportedAc3CodingTool { tool } => {
                write!(
                    formatter,
                    "unsupported original-syntax AC-3 coding tool: {tool}"
                )
            }
            Self::Ac3AudioBlock { block, source } => {
                write!(
                    formatter,
                    "failed to decode original-syntax AC-3 block {block}: {source}"
                )
            }
            Self::Ac3SyntaxField { field, source } => {
                write!(
                    formatter,
                    "failed to decode original-syntax AC-3 {field}: {source}"
                )
            }
            Self::InvalidAc3Syntax { field } => {
                write!(formatter, "invalid original-syntax AC-3 {field}")
            }
            Self::ReservedStreamType
            | Self::ReservedSampleRate
            | Self::FrameSizeOverflow
            | Self::NonzeroReservedData
            | Self::MissingJocExtensionFlag
            | Self::ReservedSnrOffsetStrategy
            | Self::UnsupportedAdaptiveHybridTransform
            | Self::NonFiniteAhtCoefficient
            | Self::InvalidAccessUnitRange
            | Self::UnsupportedJocAccessUnitFrameCount { .. }
            | Self::UnsupportedJocAudioBlockCount { .. }
            | Self::UnsupportedJocChannelTopology { .. }
            | Self::InvalidDependentChannelMap { .. }
            | Self::MultipleLfeChannels
            | Self::AccessUnitPcmSampleCountMismatch { .. }
            | Self::MultipleJocCarriers
            | Self::InvalidGroupedExponent { .. }
            | Self::ExponentOutOfRange { .. }
            | Self::ExponentGroupCountMismatch { .. }
            | Self::InvalidExponentDimensions { .. }
            | Self::InvalidMantissaBap { .. }
            | Self::InvalidMantissaCode { .. }
            | Self::InvalidMantissaGroupCode { .. }
            | Self::InvalidMantissaDiagnostic { .. }
            | Self::MantissaExponentLengthMismatch { .. }
            | Self::MantissaDitherLengthMismatch { .. }
            | Self::MissingDitherValue { .. }
            | Self::InvalidTransformCoefficientLength { .. }
            | Self::InvalidTransformWindowLength { .. }
            | Self::InvalidAudioBlockChannelCount { .. }
            | Self::InvalidAudioBlockSwitchCount { .. }
            | Self::InvalidAudioBlockLfePresence { .. }
            | Self::NonFiniteTransformCoefficient { .. }
            | Self::InvalidBlockStartDimensions { .. }
            | Self::MissingIndependentSubstreamZero { .. }
            | Self::NonsequentialIndependentSubstream { .. }
            | Self::NonsequentialDependentSubstream { .. }
            | Self::DependentAfterConvertedSubstream { .. }
            | Self::InvalidSpectralExtensionCode { .. }
            | Self::InvalidSpectralExtensionRange { .. }
            | Self::InvalidCouplingRange { .. }
            | Self::InvalidCouplingCoordinateDimensions { .. }
            | Self::InvalidCouplingCoordinate { .. }
            | Self::NonFiniteCouplingCoefficient
            | Self::InvalidSpectralExtensionCoordinateDimensions { .. }
            | Self::InvalidSpectralExtensionCoordinate { .. }
            | Self::MissingSpectralExtensionNoise { .. }
            | Self::NonFiniteSpectralExtensionCoefficient { .. }
            | Self::InvalidRematrixChannelCount { .. }
            | Self::InvalidRematrixFlagCount { .. }
            | Self::InvalidDynamicRangeGainCount { .. }
            | Self::NonFiniteDynamicRangeCoefficient { .. }
            | Self::NonFiniteRematrixCoefficient { .. }
            | Self::InvalidBitAllocationParameterCode { .. }
            | Self::InvalidBitAllocationTableIndex { .. }
            | Self::InvalidPsdRange { .. }
            | Self::InvalidDeltaBitAllocationStrategy { .. } => {
                unreachable!("handled E-AC-3 error message")
            }
        }
    }
}

impl std::error::Error for Eac3Error {}

impl From<BitError> for Eac3Error {
    fn from(value: BitError) -> Self {
        Self::Bit(value)
    }
}

impl From<EmdfError> for Eac3Error {
    fn from(value: EmdfError) -> Self {
        Self::Emdf(value)
    }
}

impl From<JocProfileValidationFailure> for Eac3Error {
    fn from(value: JocProfileValidationFailure) -> Self {
        Self::JocProfileValidation(value)
    }
}

/// Table E.1.1 stream identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamType {
    /// Original AC-3 syncframe syntax used as Annex-J independent substream 0.
    LegacyIndependent,
    Independent,
    Dependent,
    ConvertedIndependent,
}

/// Fixed-length acquisition fields at the start of one syncframe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyncframeHeader {
    pub stream_type: StreamType,
    pub substream_id: u8,
    pub bitstream_id: u8,
    pub frame_size: usize,
    pub sample_rate: u32,
    pub audio_blocks: u8,
    pub samples: u16,
}

/// Byte location and acquisition header for one complete syncframe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyncframeIndexEntry {
    pub offset: usize,
    pub header: SyncframeHeader,
}

/// One time-aligned set of independent and dependent substream frames.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccessUnitIndex {
    pub first_frame: usize,
    pub frame_count: usize,
    pub sample_rate: u32,
    pub samples: u16,
}

/// Forward-ordered auxiliary user bits, packed from the first bit into the MSB.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuxiliaryData {
    pub bit_len: usize,
    pub bytes: Vec<u8>,
}

/// Parsed JOC-candidate representation before profile validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedJocAccessUnit {
    pub carrier_frame: usize,
    pub sample_rate: u32,
    pub samples: u16,
    pub complexity_index: u8,
    pub emdf: EmdfContainer,
}

/// One explicitly validated TS 103 420 metadata frame extracted from E-AC-3.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JocMetadataFrame {
    pub carrier_frame: usize,
    pub sample_rate: u32,
    pub samples: u16,
    pub complexity_index: u8,
    pub validation_profile: JocValidationProfile,
    pub validation_status: JocValidationStatus,
    pub deviations: Vec<JocProfileDeviation>,
    /// Complete original parsed EMDF representation; no profile normalization.
    pub emdf: EmdfContainer,
    pub oamd: Vec<u8>,
    pub joc: Vec<u8>,
}

/// TS 103 420 clause 8.3 type-A extension fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JocAddbsi {
    pub complexity_index: u8,
}

/// Decoded E.1.2.2 fields required by the JOC frontend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BitstreamInformation {
    pub header: SyncframeHeader,
    /// Original-syntax `bsmod`; absent for E-AC-3 syntax.
    pub bitstream_mode: Option<u8>,
    pub audio_coding_mode: u8,
    pub lfe_on: bool,
    pub bitstream_id: u8,
    /// Raw dialogue-normalization code for the primary programme channel.
    /// The accepted independent value is converted into the frame's prepared
    /// calibrated program scalar at the common render boundary.
    pub dialnorm: u8,
    /// Raw dual-mono dialogue-normalization code, when `acmod == 0`.
    pub dialnorm_2: Option<u8>,
    /// Syncframe-level RF/heavy-compression word for the primary programme.
    pub compr: Option<u8>,
    /// Syncframe-level RF/heavy-compression word for dual-mono channel 2.
    pub compr_2: Option<u8>,
    /// E-AC-3 mixing metadata used by an admitted stereo downmix.
    pub downmix: DownmixMetadata,
    /// Custom channel map for a dependent substream, in the MSB-first table
    /// E.1.4 representation. `None` means the `acmod`/`lfeon` mapping applies.
    pub channel_map: Option<u16>,
    pub addbsi: Option<Vec<u8>>,
}

/// Raw E-AC-3 stereo-downmix metadata from E.1.2.2.
///
/// The fields remain raw bitstream codes. Decoder/render policy converts them
/// to coefficients only at the 2.0 speaker-output boundary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DownmixMetadata {
    pub dmixmod: Option<u8>,
    pub ltrt_center_mix_level: Option<u8>,
    pub loro_center_mix_level: Option<u8>,
    pub ltrt_surround_mix_level: Option<u8>,
    pub loro_surround_mix_level: Option<u8>,
    /// `Some(code)` means `lfemixlevcode` was present and enabled.
    pub lfe_mix_level_code: Option<u8>,
}

/// E.1.2.3 frame state required to decode each following audio block.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioFrameInformation {
    pub bsi: BitstreamInformation,
    pub full_bandwidth_channels: u8,
    pub snr_offset_strategy: u8,
    /// Frame-wide coarse SNR code used by strategy 1 (frame strategy 00).
    pub frame_coarse_snr_code: Option<u8>,
    /// Frame-wide fine SNR code used by strategy 1 (frame strategy 00).
    pub frame_fine_snr_code: Option<u8>,
    pub syntax: AudioFrameSyntaxFlags,
    pub coupling_in_use: Vec<bool>,
    /// Whether `cplstre[blk]` introduces new coupling strategy fields.
    pub coupling_strategy_exists: Vec<bool>,
    pub coupling_exponent_strategy: Vec<u8>,
    pub channel_exponent_strategy: Vec<Vec<u8>>,
    pub lfe_exponent_strategy: Vec<bool>,
    pub coupling_aht_in_use: bool,
    pub channel_aht_in_use: Vec<bool>,
    pub lfe_aht_in_use: bool,
    /// Frame-level `chinspxatten`/`spxattencod` values from E.1.3.2.24-25.
    /// `None` means the five-tap spectral-extension attenuation notch is off.
    pub spx_attenuation_codes: Vec<Option<u8>>,
    pub block_start_information: Option<AuxiliaryData>,
    pub audio_blocks_offset_bits: usize,
}

/// Compact E.1.2.3 syntax-enable fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioFrameSyntaxFlags(u8);

impl AudioFrameSyntaxFlags {
    #[must_use]
    pub fn block_switch(self) -> bool {
        self.0 & (1 << 0) != 0
    }
    #[must_use]
    pub fn dither(self) -> bool {
        self.0 & (1 << 1) != 0
    }
    #[must_use]
    pub fn bit_allocation(self) -> bool {
        self.0 & (1 << 2) != 0
    }
    #[must_use]
    pub fn frame_fast_gain(self) -> bool {
        self.0 & (1 << 3) != 0
    }
    #[must_use]
    pub fn delta_bit_allocation(self) -> bool {
        self.0 & (1 << 4) != 0
    }
    #[must_use]
    pub fn skip_field(self) -> bool {
        self.0 & (1 << 5) != 0
    }
    #[must_use]
    pub fn spx_attenuation(self) -> bool {
        self.0 & (1 << 6) != 0
    }
}

/// Parses the fixed acquisition prefix from clauses E.1.2.1 and E.1.2.2.
///
/// # Errors
/// Returns an error for truncation, invalid synchronization, or reserved table
/// values.
pub fn parse_syncframe_header(bytes: &[u8]) -> Result<SyncframeHeader, Eac3Error> {
    let mut probe = BitReader::new(bytes);
    let syncword = u16::try_from(probe.read_bits(16)?).map_err(|_| Eac3Error::FrameSizeOverflow)?;
    if syncword != EAC3_SYNCWORD {
        return Err(Eac3Error::InvalidSyncword { actual: syncword });
    }
    let _common_prefix = probe.read_bits(24)?;
    let bitstream_id = read_u8(&mut probe, 5)?;
    let mut bits = BitReader::new(bytes);
    match bitstream_id {
        0..=8 => parse_ac3_header_reader(&mut bits),
        16 => {
            let mut header = parse_header_reader(&mut bits)?.0;
            header.bitstream_id = bitstream_id;
            Ok(header)
        }
        actual => Err(Eac3Error::UnsupportedBitstreamId { actual }),
    }
}

fn parse_ac3_header_reader(bits: &mut BitReader<'_>) -> Result<SyncframeHeader, Eac3Error> {
    const FRAME_SIZE_WORDS: [[u16; 3]; 38] = [
        [64, 69, 96],
        [64, 70, 96],
        [80, 87, 120],
        [80, 88, 120],
        [96, 104, 144],
        [96, 105, 144],
        [112, 121, 168],
        [112, 122, 168],
        [128, 139, 192],
        [128, 140, 192],
        [160, 174, 240],
        [160, 175, 240],
        [192, 208, 288],
        [192, 209, 288],
        [224, 243, 336],
        [224, 244, 336],
        [256, 278, 384],
        [256, 279, 384],
        [320, 348, 480],
        [320, 349, 480],
        [384, 417, 576],
        [384, 418, 576],
        [448, 487, 672],
        [448, 488, 672],
        [512, 557, 768],
        [512, 558, 768],
        [640, 696, 960],
        [640, 697, 960],
        [768, 835, 1152],
        [768, 836, 1152],
        [896, 975, 1344],
        [896, 976, 1344],
        [1024, 1114, 1536],
        [1024, 1115, 1536],
        [1152, 1253, 1728],
        [1152, 1254, 1728],
        [1280, 1393, 1920],
        [1280, 1394, 1920],
    ];
    let syncword = u16::try_from(bits.read_bits(16)?).map_err(|_| Eac3Error::FrameSizeOverflow)?;
    if syncword != EAC3_SYNCWORD {
        return Err(Eac3Error::InvalidSyncword { actual: syncword });
    }
    let _crc1 = bits.read_bits(16)?;
    let sample_rate_code = read_u8(bits, 2)?;
    let sample_rate = match sample_rate_code {
        0 => 48_000,
        1 => 44_100,
        2 => 32_000,
        _ => return Err(Eac3Error::ReservedSampleRate),
    };
    let frame_size_code = read_u8(bits, 6)?;
    let bitstream_id = read_u8(bits, 5)?;
    let frame_words = FRAME_SIZE_WORDS
        .get(usize::from(frame_size_code))
        .and_then(|sizes| sizes.get(usize::from(sample_rate_code)))
        .copied()
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    let frame_size = usize::from(frame_words)
        .checked_mul(2)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    Ok(SyncframeHeader {
        stream_type: StreamType::LegacyIndependent,
        substream_id: 0,
        bitstream_id,
        frame_size,
        sample_rate,
        audio_blocks: 6,
        samples: 1536,
    })
}

fn parse_header_reader(bits: &mut BitReader<'_>) -> Result<(SyncframeHeader, u8), Eac3Error> {
    let syncword = u16::try_from(bits.read_bits(16)?).map_err(|_| Eac3Error::FrameSizeOverflow)?;
    if syncword != EAC3_SYNCWORD {
        return Err(Eac3Error::InvalidSyncword { actual: syncword });
    }
    let stream_type = match bits.read_bits(2)? {
        0 => StreamType::Independent,
        1 => StreamType::Dependent,
        2 => StreamType::ConvertedIndependent,
        3 => return Err(Eac3Error::ReservedStreamType),
        _ => unreachable!("two-bit field"),
    };
    let substream_id =
        u8::try_from(bits.read_bits(3)?).map_err(|_| Eac3Error::FrameSizeOverflow)?;
    let frame_words = usize::try_from(bits.read_bits(11)?)
        .map_err(|_| Eac3Error::FrameSizeOverflow)?
        .checked_add(1)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    let frame_size = frame_words
        .checked_mul(2)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    let sample_rate = match bits.read_bits(2)? {
        0 => 48_000,
        1 => 44_100,
        2 => 32_000,
        3 => return Err(Eac3Error::ReservedSampleRate),
        _ => unreachable!("two-bit field"),
    };
    let num_blocks_code =
        u8::try_from(bits.read_bits(2)?).map_err(|_| Eac3Error::FrameSizeOverflow)?;
    let audio_blocks = match num_blocks_code {
        0 => 1,
        1 => 2,
        2 => 3,
        3 => 6,
        _ => unreachable!("two-bit field"),
    };
    Ok((
        SyncframeHeader {
            stream_type,
            substream_id,
            bitstream_id: 16,
            frame_size,
            sample_rate,
            audio_blocks,
            samples: u16::from(audio_blocks) * 256,
        },
        num_blocks_code,
    ))
}

/// Parses complete E.1.2.2 conditional syntax through the terminal `addbsi`.
///
/// # Errors
/// Returns an error for any truncated conditional branch, invalid acquisition
/// field, or frame shorter than its declared size.
pub fn parse_bsi(bytes: &[u8]) -> Result<BitstreamInformation, Eac3Error> {
    let header = parse_syncframe_header(bytes)?;
    if header.frame_size > bytes.len() {
        return Err(Eac3Error::TruncatedFrame {
            offset: 0,
            declared: header.frame_size,
            available: bytes.len(),
        });
    }
    let mut bits = BitReader::new(&bytes[..header.frame_size]);
    if header.stream_type == StreamType::LegacyIndependent {
        parse_ac3_bsi_reader(&mut bits)
    } else {
        Ok(parse_bsi_reader(&mut bits)?.0)
    }
}

fn parse_ac3_bsi_reader(bits: &mut BitReader<'_>) -> Result<BitstreamInformation, Eac3Error> {
    let header = parse_ac3_header_reader(bits)?;
    let bitstream_id = header.bitstream_id;
    let bitstream_mode = read_u8(bits, 3)?;
    let acmod = read_u8(bits, 3)?;
    let centre_mix = if acmod & 1 != 0 && acmod != 1 {
        Some(read_u8(bits, 2)?)
    } else {
        None
    };
    let surround_mix = if acmod & 4 != 0 {
        Some(read_u8(bits, 2)?)
    } else {
        None
    };
    let dolby_surround_mode = if acmod == 2 {
        Some(read_u8(bits, 2)?)
    } else {
        None
    };
    let lfe_on = bits.read_bit()?;
    let dialnorm = match read_u8(bits, 5)? {
        0 => 31,
        value => value,
    };
    let compr = bits.read_bit()?.then(|| read_u8(bits, 8)).transpose()?;
    skip_optional(bits, 8)?; // langcod
    if bits.read_bit()? {
        skip(bits, 7)?; // mixlevel + roomtyp
    }
    let (dialnorm_2, compr_2) = if acmod == 0 {
        let dialnorm_2 = Some(match read_u8(bits, 5)? {
            0 => 31,
            value => value,
        });
        let compr_2 = bits.read_bit()?.then(|| read_u8(bits, 8)).transpose()?;
        skip_optional(bits, 8)?;
        if bits.read_bit()? {
            skip(bits, 7)?;
        }
        (dialnorm_2, compr_2)
    } else {
        (None, None)
    };
    skip(bits, 1)?; // copyrightb
    skip(bits, 1)?; // origbs
    let extended_downmix = if bitstream_id == 6 {
        let downmix = if bits.read_bit()? {
            Some(DownmixMetadata {
                dmixmod: Some(read_u8(bits, 2)?),
                ltrt_center_mix_level: Some(read_u8(bits, 3)?),
                ltrt_surround_mix_level: Some(read_u8(bits, 3)?),
                loro_center_mix_level: Some(read_u8(bits, 3)?),
                loro_surround_mix_level: Some(read_u8(bits, 3)?),
                lfe_mix_level_code: None,
            })
        } else {
            None
        };
        if bits.read_bit()? {
            let _dsurexmod = read_u8(bits, 2)?;
            let _dheadphonmod = read_u8(bits, 2)?;
            let _adconvtyp = bits.read_bit()?;
            let _xbsi2 = read_u8(bits, 8)?;
            let _encinfo = bits.read_bit()?;
        }
        downmix
    } else {
        skip_optional(bits, 14)?; // timecod1
        skip_optional(bits, 14)?; // timecod2
        None
    };
    let addbsi = if bits.read_bit()? {
        let length = usize::from(read_u8(bits, 6)?) + 1;
        let mut data = Vec::with_capacity(length);
        for _ in 0..length {
            data.push(read_u8(bits, 8)?);
        }
        Some(data)
    } else {
        None
    };
    let map_center = |code: u8| match code {
        0 => 4,
        1 => 5,
        2 => 6,
        _ => 5,
    };
    let map_surround = |code: u8| match code {
        0 => 4,
        1 => 6,
        2 => 7,
        _ => 6,
    };
    let center = centre_mix.map(map_center);
    let surround = surround_mix.map(map_surround);
    Ok(BitstreamInformation {
        header,
        bitstream_mode: Some(bitstream_mode),
        audio_coding_mode: acmod,
        lfe_on,
        bitstream_id,
        dialnorm,
        dialnorm_2,
        compr,
        compr_2,
        downmix: extended_downmix.unwrap_or(DownmixMetadata {
            dmixmod: dolby_surround_mode.map(|mode| if mode == 2 { 1 } else { 2 }),
            ltrt_center_mix_level: center,
            loro_center_mix_level: center,
            ltrt_surround_mix_level: surround,
            loro_surround_mix_level: surround,
            lfe_mix_level_code: None,
        }),
        channel_map: None,
        addbsi,
    })
}

fn parse_bsi_reader(bits: &mut BitReader<'_>) -> Result<(BitstreamInformation, u8), Eac3Error> {
    let (header, num_blocks_code) = parse_header_reader(bits)?;
    let acmod = read_u8(bits, 3)?;
    let lfe_on = bits.read_bit()?;
    let bitstream_id = read_u8(bits, 5)?;
    let dialnorm = read_u8(bits, 5)?;
    let compr = bits.read_bit()?.then(|| read_u8(bits, 8)).transpose()?;
    let (dialnorm_2, compr_2) = if acmod == 0 {
        let dialnorm_2 = Some(read_u8(bits, 5)?);
        let compr_2 = bits.read_bit()?.then(|| read_u8(bits, 8)).transpose()?;
        (dialnorm_2, compr_2)
    } else {
        (None, None)
    };
    let channel_map = if header.stream_type == StreamType::Dependent && bits.read_bit()? {
        Some(u16::try_from(bits.read_bits(16)?).map_err(|_| Eac3Error::FrameSizeOverflow)?)
    } else {
        None
    };
    let downmix = if bits.read_bit()? {
        parse_mixing_metadata(bits, header.stream_type, acmod, lfe_on, num_blocks_code)?
    } else {
        DownmixMetadata::default()
    };
    if bits.read_bit()? {
        parse_informational_metadata(bits, acmod)?;
    }
    if header.stream_type == StreamType::Independent && num_blocks_code != 3 {
        skip(bits, 1)?;
    }
    if header.stream_type == StreamType::ConvertedIndependent {
        let block_id = num_blocks_code == 3 || bits.read_bit()?;
        if block_id {
            skip(bits, 6)?;
        }
    }
    let addbsi = if bits.read_bit()? {
        let length = usize::from(read_u8(bits, 6)?) + 1;
        let mut data = Vec::with_capacity(length);
        for _ in 0..length {
            data.push(read_u8(bits, 8)?);
        }
        Some(data)
    } else {
        None
    };
    Ok((
        BitstreamInformation {
            header,
            bitstream_mode: None,
            audio_coding_mode: acmod,
            lfe_on,
            bitstream_id,
            dialnorm,
            dialnorm_2,
            compr,
            compr_2,
            downmix,
            channel_map,
            addbsi,
        },
        num_blocks_code,
    ))
}

/// Parses E.1.2.2 and the following E.1.2.3 audio-frame state.
///
/// # Errors
/// Returns an error for malformed or truncated bounded syntax.
#[allow(clippy::too_many_lines)] // Mirrors E.1.2.3 in one auditable clause order.
pub fn parse_audio_frame(bytes: &[u8]) -> Result<AudioFrameInformation, Eac3Error> {
    let header = parse_syncframe_header(bytes)?;
    if header.frame_size > bytes.len() {
        return Err(Eac3Error::TruncatedFrame {
            offset: 0,
            declared: header.frame_size,
            available: bytes.len(),
        });
    }
    if header.stream_type == StreamType::LegacyIndependent {
        let frame_bits = header
            .frame_size
            .checked_mul(8)
            .ok_or(Eac3Error::FrameSizeOverflow)?;
        let mut bits = BitReader::new(&bytes[..header.frame_size]);
        let bsi = parse_ac3_bsi_reader(&mut bits)?;
        let channel_count = full_bandwidth_channels(bsi.audio_coding_mode);
        let block_count = usize::from(bsi.header.audio_blocks);
        let audio_blocks_offset_bits = frame_bits
            .checked_sub(bits.bits_remaining())
            .ok_or(Eac3Error::FrameSizeOverflow)?;
        return Ok(AudioFrameInformation {
            bsi,
            full_bandwidth_channels: channel_count,
            snr_offset_strategy: 2,
            frame_coarse_snr_code: None,
            frame_fine_snr_code: None,
            // Original syntax carries these controls in every audio block.
            syntax: AudioFrameSyntaxFlags(0b0011_0111),
            coupling_in_use: vec![false; block_count],
            coupling_strategy_exists: vec![false; block_count],
            coupling_exponent_strategy: vec![0; block_count],
            channel_exponent_strategy: vec![vec![0; usize::from(channel_count)]; block_count],
            lfe_exponent_strategy: vec![false; block_count],
            coupling_aht_in_use: false,
            channel_aht_in_use: vec![false; usize::from(channel_count)],
            lfe_aht_in_use: false,
            spx_attenuation_codes: vec![None; usize::from(channel_count)],
            block_start_information: None,
            audio_blocks_offset_bits,
        });
    }
    let frame_bits = header
        .frame_size
        .checked_mul(8)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    let mut bits = BitReader::new(&bytes[..header.frame_size]);
    let (bsi, num_blocks_code) = parse_bsi_reader(&mut bits)?;
    let (exponent_strategy_exists, aht_exists) = if num_blocks_code == 3 {
        (bits.read_bit()?, bits.read_bit()?)
    } else {
        (true, false)
    };
    let snr_offset_strategy = read_u8(&mut bits, 2)?;
    if snr_offset_strategy == 3 {
        return Err(Eac3Error::ReservedSnrOffsetStrategy);
    }
    let transient_processing = bits.read_bit()?;
    let mut syntax = 0_u8;
    for index in 0..7 {
        syntax |= u8::from(bits.read_bit()?) << index;
    }
    let syntax = AudioFrameSyntaxFlags(syntax);
    let channel_count = full_bandwidth_channels(bsi.audio_coding_mode);
    let block_count = usize::from(bsi.header.audio_blocks);
    let mut coupling_in_use = vec![false; block_count];
    let mut coupling_strategy_exists = vec![false; block_count];
    if bsi.audio_coding_mode > 1 {
        coupling_strategy_exists[0] = true;
        coupling_in_use[0] = bits.read_bit()?;
        for block in 1..block_count {
            coupling_strategy_exists[block] = bits.read_bit()?;
            if coupling_strategy_exists[block] {
                coupling_in_use[block] = bits.read_bit()?;
            } else {
                coupling_in_use[block] = coupling_in_use[block - 1];
            }
        }
    }
    let mut coupling_exponent_strategy = vec![0_u8; block_count];
    let mut channel_exponent_strategy = vec![vec![0_u8; usize::from(channel_count)]; block_count];
    if exponent_strategy_exists {
        for block in 0..block_count {
            if coupling_in_use[block] {
                coupling_exponent_strategy[block] = read_u8(&mut bits, 2)?;
            }
            for channel in 0..usize::from(channel_count) {
                channel_exponent_strategy[block][channel] = read_u8(&mut bits, 2)?;
            }
        }
    } else {
        if bsi.audio_coding_mode > 1 && coupling_in_use.iter().any(|in_use| *in_use) {
            coupling_exponent_strategy = decode_frame_exponent_strategy(read_u8(&mut bits, 5)?)?
                .into_iter()
                .collect();
        }
        for channel in 0..usize::from(channel_count) {
            let strategies = decode_frame_exponent_strategy(read_u8(&mut bits, 5)?)?;
            for (block, strategy) in strategies.into_iter().enumerate() {
                channel_exponent_strategy[block][channel] = strategy;
            }
        }
    }
    let mut lfe_exponent_strategy = vec![false; block_count];
    if bsi.lfe_on {
        for strategy in &mut lfe_exponent_strategy {
            *strategy = bits.read_bit()?;
        }
    }
    if bsi.header.stream_type == StreamType::Independent {
        let converter_exponent_exists = if num_blocks_code == 3 {
            true
        } else {
            bits.read_bit()?
        };
        if converter_exponent_exists {
            for _ in 0..channel_count {
                skip(&mut bits, 5)?;
            }
        }
    }
    let mut coupling_aht_in_use = false;
    let mut channel_aht_in_use = vec![false; usize::from(channel_count)];
    let mut lfe_aht_in_use = false;
    if aht_exists {
        let coupling_regions = coupling_strategy_exists
            .iter()
            .zip(&coupling_exponent_strategy)
            .filter(|(exists, strategy)| **exists || **strategy != 0)
            .count();
        if coupling_in_use.iter().all(|in_use| *in_use) && coupling_regions == 1 {
            coupling_aht_in_use = bits.read_bit()?;
        }
        for channel in 0..usize::from(channel_count) {
            let regions = channel_exponent_strategy
                .iter()
                .filter(|strategies| strategies[channel] != 0)
                .count();
            if regions == 1 {
                channel_aht_in_use[channel] = bits.read_bit()?;
            }
        }
        if bsi.lfe_on && lfe_exponent_strategy.iter().filter(|value| **value).count() == 1 {
            lfe_aht_in_use = bits.read_bit()?;
        }
    }
    let (frame_coarse_snr_code, frame_fine_snr_code) = if snr_offset_strategy == 0 {
        (Some(read_u8(&mut bits, 6)?), Some(read_u8(&mut bits, 4)?))
    } else {
        (None, None)
    };
    if transient_processing {
        for _ in 0..channel_count {
            if bits.read_bit()? {
                skip(&mut bits, 18)?;
            }
        }
    }
    let spx_attenuation_codes = if syntax.spx_attenuation() {
        (0..channel_count)
            .map(|_| {
                if bits.read_bit()? {
                    Ok(Some(read_u8(&mut bits, 5)?))
                } else {
                    Ok(None)
                }
            })
            .collect::<Result<Vec<_>, Eac3Error>>()?
    } else {
        vec![None; usize::from(channel_count)]
    };
    let block_start_information = if num_blocks_code != 0 && bits.read_bit()? {
        let length =
            block_start_information_length(bsi.header.frame_size, bsi.header.audio_blocks)?;
        Some(read_raw_bits(&mut bits, length)?)
    } else {
        None
    };
    let audio_blocks_offset_bits = frame_bits
        .checked_sub(bits.bits_remaining())
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    Ok(AudioFrameInformation {
        bsi,
        full_bandwidth_channels: channel_count,
        snr_offset_strategy,
        frame_coarse_snr_code,
        frame_fine_snr_code,
        syntax,
        coupling_in_use,
        coupling_strategy_exists,
        coupling_exponent_strategy,
        channel_exponent_strategy,
        lfe_exponent_strategy,
        coupling_aht_in_use,
        channel_aht_in_use,
        lfe_aht_in_use,
        spx_attenuation_codes,
        block_start_information,
        audio_blocks_offset_bits,
    })
}

fn full_bandwidth_channels(audio_coding_mode: u8) -> u8 {
    [2, 1, 2, 3, 3, 4, 4, 5][usize::from(audio_coding_mode)]
}

fn read_raw_bits(bits: &mut BitReader<'_>, bit_len: usize) -> Result<AuxiliaryData, Eac3Error> {
    let byte_len = bit_len.checked_add(7).ok_or(Eac3Error::FrameSizeOverflow)? / 8;
    let mut bytes = vec![0_u8; byte_len];
    for index in 0..bit_len {
        if bits.read_bit()? {
            bytes[index / 8] |= 0x80 >> (index % 8);
        }
    }
    Ok(AuxiliaryData { bit_len, bytes })
}

/// Applies TS 102 366 table E.1.9 (`R`, `D15`, `D25`, `D45` = 0..=3).
///
/// # Errors
/// Returns [`Eac3Error::InvalidFrameExponentStrategy`] for values wider than
/// the normative five-bit field.
pub fn decode_frame_exponent_strategy(code: u8) -> Result<[u8; 6], Eac3Error> {
    const TABLE: [[u8; 6]; 32] = [
        [1, 0, 0, 0, 0, 0],
        [1, 0, 0, 0, 0, 3],
        [1, 0, 0, 0, 2, 0],
        [1, 0, 0, 0, 3, 3],
        [2, 0, 0, 2, 0, 0],
        [2, 0, 0, 2, 0, 3],
        [2, 0, 0, 3, 2, 0],
        [2, 0, 0, 3, 3, 3],
        [2, 0, 1, 0, 0, 0],
        [2, 0, 2, 0, 0, 3],
        [2, 0, 2, 0, 2, 0],
        [2, 0, 2, 0, 3, 3],
        [2, 0, 3, 2, 0, 0],
        [2, 0, 3, 2, 0, 3],
        [2, 0, 3, 3, 2, 0],
        [2, 0, 3, 3, 3, 3],
        [3, 1, 0, 0, 0, 0],
        [3, 1, 0, 0, 0, 3],
        [3, 2, 0, 0, 2, 0],
        [3, 2, 0, 0, 3, 3],
        [3, 2, 0, 2, 0, 0],
        [3, 2, 0, 2, 0, 3],
        [3, 2, 0, 3, 2, 0],
        [3, 2, 0, 3, 3, 3],
        [3, 3, 1, 0, 0, 0],
        [3, 3, 2, 0, 0, 3],
        [3, 3, 2, 0, 2, 0],
        [3, 3, 2, 0, 3, 3],
        [3, 3, 3, 2, 0, 0],
        [3, 3, 3, 2, 0, 3],
        [3, 3, 3, 3, 2, 0],
        [3, 3, 3, 3, 3, 3],
    ];
    TABLE
        .get(usize::from(code))
        .copied()
        .ok_or(Eac3Error::InvalidFrameExponentStrategy { actual: code })
}

/// Evaluates the page-138 clause E.1.3.2.27 `nblkstrtbits` equation.
///
/// # Errors
/// Returns a dimension or overflow error for an impossible Enhanced AC-3
/// frame size/block count.
pub fn block_start_information_length(
    frame_size: usize,
    audio_blocks: u8,
) -> Result<usize, Eac3Error> {
    if frame_size == 0 || frame_size % 2 != 0 || !matches!(audio_blocks, 1 | 2 | 3 | 6) {
        return Err(Eac3Error::InvalidBlockStartDimensions {
            frame_size,
            audio_blocks,
        });
    }
    let words = frame_size / 2;
    let ceiling_log2 = usize::try_from(usize::BITS - (words - 1).leading_zeros())
        .map_err(|_| Eac3Error::FrameSizeOverflow)?;
    usize::from(audio_blocks - 1)
        .checked_mul(
            4_usize
                .checked_add(ceiling_log2)
                .ok_or(Eac3Error::FrameSizeOverflow)?,
        )
        .ok_or(Eac3Error::FrameSizeOverflow)
}

/// Derives an uncoupled channel's end mantissa from clause 6.1.3.
///
/// # Errors
/// Returns an error for the prohibited channel-bandwidth codes 61 through 63.
pub fn channel_end_mantissa(channel_bandwidth_code: u8) -> Result<usize, Eac3Error> {
    if channel_bandwidth_code > 60 {
        return Err(Eac3Error::InvalidChannelBandwidthCode {
            actual: channel_bandwidth_code,
        });
    }
    Ok((usize::from(channel_bandwidth_code) + 12) * 3 + 37)
}

/// Derives the number of seven-bit exponent groups from clause 6.1.3.
///
/// Strategies 1, 2, and 3 denote D15, D25, and D45 respectively.
///
/// # Errors
/// Returns an error when called with the reuse strategy or an invalid value.
pub fn channel_exponent_group_count(
    end_mantissa: usize,
    exponent_strategy: u8,
) -> Result<usize, Eac3Error> {
    match exponent_strategy {
        1 => Ok(end_mantissa.saturating_sub(1) / 3),
        2 => Ok(end_mantissa.saturating_add(2) / 6),
        3 => Ok(end_mantissa.saturating_add(8) / 12),
        actual => Err(Eac3Error::InvalidExponentStrategy { actual }),
    }
}

/// Decodes clause 6.1.3 base-25 grouped differential exponents.
///
/// The result contains one exponent for every mantissa bin, including the
/// initial absolute exponent at bin zero. Strategies 1, 2, and 3 denote D15,
/// D25, and D45.
///
/// # Errors
/// Returns an error for invalid dimensions, strategy, group count/code, or an
/// exponent outside the normative 0 through 24 range.
pub fn decode_exponents(
    initial_exponent: u8,
    grouped_exponents: &[u8],
    exponent_strategy: u8,
    end_mantissa: usize,
) -> Result<Vec<u8>, Eac3Error> {
    if end_mantissa == 0 || end_mantissa > 253 {
        return Err(Eac3Error::InvalidExponentDimensions { end_mantissa });
    }
    let expected = channel_exponent_group_count(end_mantissa, exponent_strategy)?;
    if grouped_exponents.len() != expected {
        return Err(Eac3Error::ExponentGroupCountMismatch {
            expected,
            actual: grouped_exponents.len(),
        });
    }
    let mut exponent = i16::from(initial_exponent);
    if exponent > 24 {
        return Err(Eac3Error::ExponentOutOfRange { actual: exponent });
    }
    let repeats = 1_usize << usize::from(exponent_strategy - 1);
    let mut decoded = Vec::with_capacity(end_mantissa);
    decoded.push(initial_exponent);
    for &group in grouped_exponents {
        if group > 124 {
            return Err(Eac3Error::InvalidGroupedExponent { actual: group });
        }
        for value in [group / 25, (group % 25) / 5, group % 5] {
            exponent += i16::from(value) - 2;
            if !(0..=24).contains(&exponent) {
                return Err(Eac3Error::ExponentOutOfRange { actual: exponent });
            }
            for _ in 0..repeats {
                if decoded.len() < end_mantissa {
                    decoded.push(
                        u8::try_from(exponent)
                            .map_err(|_| Eac3Error::ExponentOutOfRange { actual: exponent })?,
                    );
                }
            }
        }
    }
    Ok(decoded)
}

/// Derives the active SPX subband interval from clause E.1.2.4.
///
/// # Errors
/// Returns an error for values wider than the two three-bit fields, or when
/// the derived half-open interval is empty or reversed.
pub fn spx_subband_range(begin_code: u8, end_code: u8) -> Result<(u8, u8), Eac3Error> {
    if begin_code > 7 || end_code > 7 {
        return Err(Eac3Error::InvalidSpectralExtensionCode {
            begin_code,
            end_code,
        });
    }
    let begin = if begin_code < 6 {
        begin_code + 2
    } else {
        begin_code * 2 - 3
    };
    let end = if end_code < 3 {
        end_code + 5
    } else {
        end_code * 2 + 3
    };
    if begin >= end {
        return Err(Eac3Error::InvalidSpectralExtensionRange { begin, end });
    }
    Ok((begin, end))
}

fn parse_mixing_metadata(
    bits: &mut BitReader<'_>,
    stream_type: StreamType,
    acmod: u8,
    lfe_on: bool,
    num_blocks_code: u8,
) -> Result<DownmixMetadata, Eac3Error> {
    let dmixmod = (acmod > 2).then(|| read_u8(bits, 2)).transpose()?;
    let (ltrt_center_mix_level, loro_center_mix_level) = if acmod & 1 != 0 && acmod > 2 {
        (Some(read_u8(bits, 3)?), Some(read_u8(bits, 3)?))
    } else {
        (None, None)
    };
    let (ltrt_surround_mix_level, loro_surround_mix_level) = if acmod & 4 != 0 {
        (Some(read_u8(bits, 3)?), Some(read_u8(bits, 3)?))
    } else {
        (None, None)
    };
    let lfe_mix_level_code = if lfe_on && bits.read_bit()? {
        Some(read_u8(bits, 5)?)
    } else {
        None
    };
    let metadata = DownmixMetadata {
        dmixmod,
        ltrt_center_mix_level,
        loro_center_mix_level,
        ltrt_surround_mix_level,
        loro_surround_mix_level,
        lfe_mix_level_code,
    };
    if stream_type != StreamType::Independent {
        return Ok(metadata);
    }
    skip_optional(bits, 6)?;
    if acmod == 0 {
        skip_optional(bits, 6)?;
    }
    skip_optional(bits, 6)?;
    match bits.read_bits(2)? {
        0 => {}
        1 => skip(bits, 5)?,
        2 => skip(bits, 12)?,
        3 => {
            let length_code = usize::from(read_u8(bits, 5)?);
            let region_bits = length_code
                .checked_add(2)
                .and_then(|bytes| bytes.checked_mul(8))
                .and_then(|total| total.checked_sub(5))
                .ok_or(Eac3Error::FrameSizeOverflow)?;
            let _region = bits.take_bits(region_bits)?;
        }
        _ => unreachable!("two-bit field"),
    }
    if acmod < 2 {
        skip_optional(bits, 14)?;
        if acmod == 0 {
            skip_optional(bits, 14)?;
        }
    }
    if bits.read_bit()? {
        if num_blocks_code == 0 {
            skip(bits, 5)?;
        } else {
            for _ in 0..blocks_from_code(num_blocks_code) {
                skip_optional(bits, 5)?;
            }
        }
    }
    Ok(metadata)
}

fn parse_informational_metadata(bits: &mut BitReader<'_>, acmod: u8) -> Result<(), Eac3Error> {
    skip(bits, 5)?;
    if acmod == 2 {
        skip(bits, 4)?;
    }
    if acmod >= 6 {
        skip(bits, 2)?;
    }
    skip_optional(bits, 8)?;
    if acmod == 0 {
        skip_optional(bits, 8)?;
    }
    skip(bits, 1)?;
    Ok(())
}

fn blocks_from_code(code: u8) -> u8 {
    [1, 2, 3, 6][usize::from(code)]
}

fn skip_optional(bits: &mut BitReader<'_>, width: u8) -> Result<(), Eac3Error> {
    if bits.read_bit()? {
        skip(bits, width)?;
    }
    Ok(())
}

fn skip(bits: &mut BitReader<'_>, width: u8) -> Result<(), Eac3Error> {
    let _ignored = bits.read_bits(width)?;
    Ok(())
}

fn read_u8(bits: &mut BitReader<'_>, width: u8) -> Result<u8, Eac3Error> {
    u8::try_from(bits.read_bits(width)?).map_err(|_| Eac3Error::FrameSizeOverflow)
}

/// Indexes a byte-concatenated sequence using declared frame sizes only.
///
/// # Errors
/// Returns an error at the first malformed header or incomplete declared frame.
pub fn index_syncframes(bytes: &[u8]) -> Result<Vec<SyncframeIndexEntry>, Eac3Error> {
    let mut entries = Vec::new();
    let mut offset = 0_usize;
    while offset < bytes.len() {
        let header = parse_syncframe_header(&bytes[offset..])?;
        let end = offset
            .checked_add(header.frame_size)
            .ok_or(Eac3Error::FrameSizeOverflow)?;
        if end > bytes.len() {
            return Err(Eac3Error::TruncatedFrame {
                offset,
                declared: header.frame_size,
                available: bytes.len() - offset,
            });
        }
        entries.push(SyncframeIndexEntry { offset, header });
        offset = end;
    }
    Ok(entries)
}

/// Groups E.1.3.1.2 ordered substreams into time-aligned access units.
///
/// # Errors
/// Returns an error unless each unit starts with independent substream zero,
/// independent and dependent IDs are sequential, dependent streams immediately
/// follow their parent, and every substream has identical rate/block timing.
pub fn group_access_units(
    frames: &[SyncframeIndexEntry],
) -> Result<Vec<AccessUnitIndex>, Eac3Error> {
    let mut units = Vec::new();
    let mut first = 0_usize;
    while first < frames.len() {
        let base = frames[first].header;
        if base.stream_type == StreamType::Dependent || base.substream_id != 0 {
            return Err(Eac3Error::MissingIndependentSubstreamZero { frame: first });
        }
        let mut expected_independent = 0_u8;
        let mut expected_dependent = 0_u8;
        let mut dependent_allowed = false;
        let mut index = first;
        while index < frames.len() {
            let header = frames[index].header;
            if index > first
                && header.stream_type != StreamType::Dependent
                && header.substream_id == 0
            {
                break;
            }
            if header.sample_rate != base.sample_rate || header.audio_blocks != base.audio_blocks {
                return Err(Eac3Error::SubstreamTimingMismatch { frame: index });
            }
            match header.stream_type {
                StreamType::Dependent => {
                    if !dependent_allowed {
                        return Err(Eac3Error::DependentAfterConvertedSubstream { frame: index });
                    }
                    if header.substream_id != expected_dependent {
                        return Err(Eac3Error::NonsequentialDependentSubstream {
                            expected: expected_dependent,
                            actual: header.substream_id,
                        });
                    }
                    expected_dependent += 1;
                }
                StreamType::LegacyIndependent
                | StreamType::Independent
                | StreamType::ConvertedIndependent => {
                    if header.substream_id != expected_independent {
                        return Err(Eac3Error::NonsequentialIndependentSubstream {
                            expected: expected_independent,
                            actual: header.substream_id,
                        });
                    }
                    expected_independent += 1;
                    expected_dependent = 0;
                    dependent_allowed = matches!(
                        header.stream_type,
                        StreamType::LegacyIndependent | StreamType::Independent
                    );
                }
            }
            index += 1;
        }
        units.push(AccessUnitIndex {
            first_frame: first,
            frame_count: index - first,
            sample_rate: base.sample_rate,
            samples: base.samples,
        });
        first = index;
    }
    Ok(units)
}

/// Extracts E.1.2.5 auxiliary user data backward from the fixed frame end.
///
/// This follows clause 4.4.4.1 and does not require decoding audio blocks.
///
/// # Errors
/// Returns an error for malformed acquisition data, a truncated declared
/// frame, or an auxiliary length that reaches before the frame start.
pub fn extract_auxdata(frame: &[u8]) -> Result<Option<AuxiliaryData>, Eac3Error> {
    let header = parse_syncframe_header(frame)?;
    if header.frame_size > frame.len() {
        return Err(Eac3Error::TruncatedFrame {
            offset: 0,
            declared: header.frame_size,
            available: frame.len(),
        });
    }
    let frame = &frame[..header.frame_size];
    let frame_bits = header
        .frame_size
        .checked_mul(8)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    let auxdatae_position = frame_bits
        .checked_sub(18)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    if !bit_at(frame, auxdatae_position) {
        return Ok(None);
    }
    let length_position = auxdatae_position
        .checked_sub(14)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    let declared = usize::try_from(bits_at(frame, length_position, 14))
        .map_err(|_| Eac3Error::FrameSizeOverflow)?;
    if declared > length_position {
        return Err(Eac3Error::AuxDataLengthOutOfRange {
            declared,
            available: length_position,
        });
    }
    let start = length_position - declared;
    let byte_len = declared
        .checked_add(7)
        .ok_or(Eac3Error::FrameSizeOverflow)?
        / 8;
    let mut bytes = vec![0_u8; byte_len];
    for index in 0..declared {
        if bit_at(frame, start + index) {
            bytes[index / 8] |= 1 << (7 - index % 8);
        }
    }
    Ok(Some(AuxiliaryData {
        bit_len: declared,
        bytes,
    }))
}

/// Parses an EMDF synchronization unit carried as complete auxiliary user data.
///
/// # Errors
/// Returns an error for malformed auxiliary syntax, a non-octet user payload,
/// or malformed bounded EMDF data.
pub fn extract_aux_emdf(frame: &[u8]) -> Result<Option<ParsedEmdf>, Eac3Error> {
    let Some(auxdata) = extract_auxdata(frame)? else {
        return Ok(None);
    };
    if auxdata.bit_len % 8 != 0 {
        return Err(Eac3Error::AuxDataNotByteAligned {
            bits: auxdata.bit_len,
        });
    }
    let parsed = parse_emdf_sync(&auxdata.bytes)?;
    if parsed.bytes_consumed != auxdata.bytes.len() {
        return Err(Eac3Error::EmdfCarrierTrailingData {
            container_bytes: parsed.bytes_consumed,
            carrier_bytes: auxdata.bytes.len(),
        });
    }
    Ok(Some(parsed))
}

/// Classifies the exact, byte-bounded frame-end `auxdata` carrier.
///
/// The caller receives `NonEmdf` for ordinary auxiliary data, while a
/// synchronization word commits the whole declared range to the bounded
/// Annex H parser. No later byte search or implicit padding is performed.
pub fn classify_aux_emdf(frame: &[u8]) -> Result<Option<CarrierClassification>, Eac3Error> {
    let Some(auxdata) = extract_auxdata(frame)? else {
        return Ok(None);
    };
    if auxdata.bit_len % 8 != 0 {
        return Err(Eac3Error::AuxDataNotByteAligned {
            bits: auxdata.bit_len,
        });
    }
    Ok(Some(classify_emdf_carrier(&auxdata.bytes)))
}

/// Classifies one exact audio-block `skipfld` byte range using the bounded
/// Annex H parser as a diagnostic candidate. TS 102 366 describes these bytes
/// as dummy data, so this function does not assert normative JOC carriage. The
/// bytes have already been unpacked from their declared bit range; no frame or
/// neighbouring carrier data is visible here.
#[must_use]
pub fn classify_skip_field_emdf(auxdata: &AuxiliaryData) -> CarrierClassification {
    classify_emdf_carrier(&auxdata.bytes)
}

/// Parses one TS 103 420 candidate carried through frame-end auxiliary data.
///
/// # Errors
/// Returns an error for invalid unit bounds, malformed frame/EMDF syntax,
/// multiple profile carriers, missing same-frame `addbsi`, or violation of the
/// mandatory last-dependent-substream placement rule.
pub fn parse_aux_joc_access_unit(
    stream: &[u8],
    frames: &[SyncframeIndexEntry],
    unit: AccessUnitIndex,
) -> Result<Option<ParsedJocAccessUnit>, Eac3Error> {
    parse_joc_access_unit_impl(stream, frames, unit, false)
}

/// Strictly validates one frame-end candidate for backward API compatibility.
pub fn extract_aux_joc_access_unit(
    stream: &[u8],
    frames: &[SyncframeIndexEntry],
    unit: AccessUnitIndex,
) -> Result<Option<JocMetadataFrame>, Eac3Error> {
    parse_aux_joc_access_unit(stream, frames, unit)?
        .as_ref()
        .map(|parsed| validate_joc_access_unit(parsed, JocValidationProfile::EtsiStrict))
        .transpose()
}

/// Parses one JOC candidate from the currently examined
/// bounded E-AC-3 ranges: frame-end `auxdata` and each reached audio-block
/// `skipfld` diagnostic candidate.
///
/// This parser never combines
/// payloads from separate carriers; a duplicate or incomplete placement is a
/// structured parse error. It does not apply table 55/56 profile validation.
pub fn parse_joc_access_unit(
    stream: &[u8],
    frames: &[SyncframeIndexEntry],
    unit: AccessUnitIndex,
) -> Result<Option<ParsedJocAccessUnit>, Eac3Error> {
    parse_joc_access_unit_impl(stream, frames, unit, true)
}

/// Applies one explicit validation profile to a parsed JOC candidate.
///
/// The returned decoder representation retains the complete original EMDF
/// container and every compatibility deviation.
pub fn validate_joc_access_unit(
    parsed: &ParsedJocAccessUnit,
    profile: JocValidationProfile,
) -> Result<JocMetadataFrame, Eac3Error> {
    let validated = validate_joc_profile_for(&parsed.emdf, profile)?;
    let oamd = validated.oamd.data.clone();
    let joc = validated.joc.data.clone();
    let validation_status = validated.status;
    let deviations = validated.deviations;
    Ok(JocMetadataFrame {
        carrier_frame: parsed.carrier_frame,
        sample_rate: parsed.sample_rate,
        samples: parsed.samples,
        complexity_index: parsed.complexity_index,
        validation_profile: profile,
        validation_status,
        deviations,
        emdf: parsed.emdf.clone(),
        oamd,
        joc,
    })
}

/// Parses and validates with an explicit profile.
pub fn extract_joc_access_unit_for_profile(
    stream: &[u8],
    frames: &[SyncframeIndexEntry],
    unit: AccessUnitIndex,
    profile: JocValidationProfile,
) -> Result<Option<JocMetadataFrame>, Eac3Error> {
    parse_joc_access_unit(stream, frames, unit)?
        .as_ref()
        .map(|parsed| validate_joc_access_unit(parsed, profile))
        .transpose()
}

/// Parses and validates with the normative strict profile.
pub fn extract_joc_access_unit(
    stream: &[u8],
    frames: &[SyncframeIndexEntry],
    unit: AccessUnitIndex,
) -> Result<Option<JocMetadataFrame>, Eac3Error> {
    extract_joc_access_unit_for_profile(stream, frames, unit, JocValidationProfile::EtsiStrict)
}

fn parse_joc_access_unit_impl(
    stream: &[u8],
    frames: &[SyncframeIndexEntry],
    unit: AccessUnitIndex,
    include_skip_fields: bool,
) -> Result<Option<ParsedJocAccessUnit>, Eac3Error> {
    let end_frame = unit
        .first_frame
        .checked_add(unit.frame_count)
        .ok_or(Eac3Error::InvalidAccessUnitRange)?;
    let unit_frames = frames
        .get(unit.first_frame..end_frame)
        .ok_or(Eac3Error::InvalidAccessUnitRange)?;
    let required_dependent = unit_frames
        .iter()
        .enumerate()
        .filter(|(_, frame)| frame.header.stream_type == StreamType::Dependent)
        .map(|(relative, _)| unit.first_frame + relative)
        .next_back();
    let mut found: Option<(usize, ParsedEmdf)> = None;
    for (relative, entry) in unit_frames.iter().enumerate() {
        let frame_index = unit.first_frame + relative;
        let frame = frame_bytes(stream, *entry)?;
        if let Some(classification) = classify_aux_emdf(frame)? {
            match classification {
                CarrierClassification::NonEmdf => {}
                CarrierClassification::Parsed(parsed) => {
                    register_joc_carrier(&mut found, frame_index, parsed)?;
                }
                CarrierClassification::Malformed(error) => {
                    return Err(Eac3Error::Emdf(error));
                }
                CarrierClassification::TrailingData {
                    container_bytes,
                    carrier_bytes,
                } => {
                    return Err(Eac3Error::EmdfCarrierTrailingData {
                        container_bytes,
                        carrier_bytes,
                    });
                }
            }
        }
        if include_skip_fields {
            inspect_skip_joc_carriers(frame, frame_index, &mut found)?;
        }
    }
    let Some((carrier_frame, parsed)) = found else {
        return Ok(None);
    };
    if let Some(required_frame) = required_dependent {
        if carrier_frame != required_frame {
            return Err(Eac3Error::InvalidJocCarrierPlacement {
                carrier_frame,
                required_frame,
            });
        }
    }
    let carrier_entry = frames
        .get(carrier_frame)
        .ok_or(Eac3Error::InvalidAccessUnitRange)?;
    let frame = frame_bytes(stream, *carrier_entry)?;
    let bsi = parse_bsi(frame)?;
    let addbsi = bsi.addbsi.as_deref().ok_or(Eac3Error::MissingJocAddbsi {
        frame: carrier_frame,
    })?;
    let extension = parse_joc_addbsi(addbsi)?;
    Ok(Some(ParsedJocAccessUnit {
        carrier_frame,
        sample_rate: unit.sample_rate,
        samples: unit.samples,
        complexity_index: extension.complexity_index,
        emdf: parsed.container,
    }))
}

fn register_joc_carrier(
    found: &mut Option<(usize, ParsedEmdf)>,
    frame_index: usize,
    parsed: ParsedEmdf,
) -> Result<(), Eac3Error> {
    let carries_profile = parsed
        .container
        .payloads
        .iter()
        .any(|payload| payload.id == OAMD_PAYLOAD_ID || payload.id == JOC_PAYLOAD_ID);
    if !carries_profile {
        return Ok(());
    }
    if found.is_some() {
        return Err(Eac3Error::MultipleJocCarriers);
    }
    *found = Some((frame_index, parsed));
    Ok(())
}

fn inspect_skip_joc_carriers(
    frame: &[u8],
    frame_index: usize,
    found: &mut Option<(usize, ParsedEmdf)>,
) -> Result<(), Eac3Error> {
    let parsed_frame = parse_audio_frame(frame)?;
    if parsed_frame.bsi.header.stream_type == StreamType::LegacyIndependent {
        // TS 103 420 places the profile carrier in the last D0. The AC-3
        // compatibility core is CRC-validated but never scanned as a carrier.
        return validate_ac3_crc(frame);
    }
    if !parsed_frame.syntax.skip_field() {
        return Ok(());
    }
    let mut carrier_error = None;
    let report = inspect_audio_block_carriers(frame, |carrier| {
        let Some(skip) = carrier.skip_field.as_ref() else {
            return;
        };
        match classify_skip_field_emdf(skip) {
            CarrierClassification::NonEmdf => {}
            CarrierClassification::Parsed(parsed) => {
                if carrier_error.is_none() {
                    carrier_error = register_joc_carrier(found, frame_index, parsed).err();
                }
            }
            CarrierClassification::Malformed(error) => {
                if carrier_error.is_none() {
                    carrier_error = Some(Eac3Error::Emdf(error));
                }
            }
            CarrierClassification::TrailingData {
                container_bytes,
                carrier_bytes,
            } => {
                if carrier_error.is_none() {
                    carrier_error = Some(Eac3Error::EmdfCarrierTrailingData {
                        container_bytes,
                        carrier_bytes,
                    });
                }
            }
        }
    })?;
    if let Some(error) = carrier_error {
        return Err(error);
    }
    if report.unresolved_blocks != 0 {
        return Err(Eac3Error::AudioBlockCarrierTraversalUnresolved {
            examined_blocks: report.examined_blocks,
            unresolved_blocks: report.unresolved_blocks,
        });
    }
    Ok(())
}

/// Extracts the TS 103 420 `addbsi` extension from one E-AC-3 access unit.
///
/// This helper is intentionally independent of EMDF extraction. It is used by
/// diagnostics to distinguish a stream that signals the JOC extension from a
/// stream that actually carries the required OAMD/JOC EMDF payloads.
///
/// # Errors
/// Returns an error for an invalid access-unit range, truncated frame, or
/// malformed `addbsi` syntax.
pub fn extract_joc_addbsi_access_unit(
    stream: &[u8],
    frames: &[SyncframeIndexEntry],
    unit: AccessUnitIndex,
) -> Result<Option<JocAddbsi>, Eac3Error> {
    let end_frame = unit
        .first_frame
        .checked_add(unit.frame_count)
        .ok_or(Eac3Error::InvalidAccessUnitRange)?;
    let unit_frames = frames
        .get(unit.first_frame..end_frame)
        .ok_or(Eac3Error::InvalidAccessUnitRange)?;
    for entry in unit_frames {
        let frame = frame_bytes(stream, *entry)?;
        let bsi = parse_bsi(frame)?;
        let Some(addbsi) = bsi.addbsi else {
            continue;
        };
        return parse_joc_addbsi(&addbsi).map(Some);
    }
    Ok(None)
}

fn frame_bytes(stream: &[u8], entry: SyncframeIndexEntry) -> Result<&[u8], Eac3Error> {
    let end = entry
        .offset
        .checked_add(entry.header.frame_size)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    stream
        .get(entry.offset..end)
        .ok_or(Eac3Error::TruncatedFrame {
            offset: entry.offset,
            declared: entry.header.frame_size,
            available: stream.len().saturating_sub(entry.offset),
        })
}

fn bits_at(bytes: &[u8], position: usize, width: u8) -> u64 {
    let mut value = 0_u64;
    for index in 0..usize::from(width) {
        value = (value << 1) | u64::from(bit_at(bytes, position + index));
    }
    value
}

fn bit_at(bytes: &[u8], position: usize) -> bool {
    bytes[position / 8] & (1 << (7 - position % 8)) != 0
}

/// Parses the exact two-byte TS 103 420 clause 8.3 `addbsi` payload.
///
/// # Errors
/// Returns an error for the wrong length, nonzero reserved bits, an absent
/// extension flag, or complexity above the normative maximum of 16.
pub fn parse_joc_addbsi(bytes: &[u8]) -> Result<JocAddbsi, Eac3Error> {
    if bytes.len() != 2 {
        return Err(Eac3Error::InvalidAddbsiLength {
            actual: bytes.len(),
        });
    }
    if bytes[0] >> 1 != 0 {
        return Err(Eac3Error::NonzeroReservedData);
    }
    if bytes[0] & 1 == 0 {
        return Err(Eac3Error::MissingJocExtensionFlag);
    }
    let complexity_index = bytes[1];
    if complexity_index > 16 {
        return Err(Eac3Error::ComplexityIndexOutOfRange {
            actual: complexity_index,
        });
    }
    Ok(JocAddbsi { complexity_index })
}

/// Enforces TS 103 420 clause 8.3.2.2 against the decoded OAMD programme.
///
/// # Errors
/// Returns [`Eac3Error::ComplexityIndexMismatch`] unless the extension index
/// equals the total OAMD bed, ISF, and dynamic object count.
pub fn validate_complexity_index(complexity: u8, objects: u16) -> Result<(), Eac3Error> {
    if u16::from(complexity) != objects {
        return Err(Eac3Error::ComplexityIndexMismatch {
            complexity,
            objects,
        });
    }
    Ok(())
}
