// pattern: Functional Core

//! Clean-room Enhanced AC-3 frontend from ETSI TS 102 366 Annex E.

mod aht;
mod audio_block;
mod bit_allocation;
mod mantissa;

pub use aht::{decode_aht_gaq_mantissa, decode_aht_vq_vector, expand_aht_gaq_gains};
pub use audio_block::{
    AudioBlockPrefix, BitAllocationParameters, CouplingInformation, CouplingLeak,
    DecodedAudioBlock, DeltaBitAllocation, DeltaBitAllocationElement, DeltaBitAllocationSegment,
    EnhancedCouplingInformation, EnhancedCouplingReconstruction, ExponentInformation,
    FastGainCodes, SnrOffsets, SpectralExtensionCoordinates, SpectralExtensionInformation,
    StandardCouplingCoordinates, StandardCouplingInformation, decode_audio_blocks,
    decode_first_audio_block, inverse_aht_dct, parse_first_audio_block_prefix,
    reconstruct_enhanced_coupling,
};
pub use bit_allocation::{
    BitAllocationBand, FixedBitAllocationParameters, apply_delta_bit_allocation,
    bit_allocation_band, bit_allocation_band_for_bin, bit_allocation_pointer, calc_lowcomp,
    compute_bap, compute_element_bap, compute_excitation, compute_high_efficiency_bap,
    compute_masking_curve, decode_bit_allocation_parameters, exponents_to_psd,
    high_efficiency_bit_allocation_pointer, integrate_psd, log_add, snr_offset,
    snr_offsets_are_zero,
};
pub use mantissa::{
    MantissaQuantizer, decode_mantissa_code, decode_mantissas, mantissa_quantizer, shift_mantissa,
    ungroup_mantissa_code,
};

use core::fmt;
use openjoc_bitio::{BitError, BitRead, BitReader};
use openjoc_emdf::{
    EmdfError, JOC_PAYLOAD_ID, OAMD_PAYLOAD_ID, ParsedEmdf, parse_emdf_sync, validate_joc_profile,
};

const EAC3_SYNCWORD: u16 = 0x0b77;

/// Checked frontend failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Eac3Error {
    Bit(BitError),
    InvalidSyncword {
        actual: u16,
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
    InvalidCouplingRange {
        begin: i16,
        end: i16,
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
    Emdf(EmdfError),
    InvalidAccessUnitRange,
    MultipleJocCarriers,
    MissingJocAddbsi {
        frame: usize,
    },
    InvalidJocCarrierPlacement {
        carrier_frame: usize,
        required_frame: usize,
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
                write!(formatter, "invalid E-AC-3 syncword 0x{actual:04x}")
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
            Self::ReservedStreamType
            | Self::ReservedSampleRate
            | Self::FrameSizeOverflow
            | Self::NonzeroReservedData
            | Self::MissingJocExtensionFlag
            | Self::ReservedSnrOffsetStrategy
            | Self::UnsupportedAdaptiveHybridTransform
            | Self::NonFiniteAhtCoefficient
            | Self::InvalidAccessUnitRange
            | Self::MultipleJocCarriers
            | Self::InvalidGroupedExponent { .. }
            | Self::ExponentOutOfRange { .. }
            | Self::ExponentGroupCountMismatch { .. }
            | Self::InvalidExponentDimensions { .. }
            | Self::InvalidMantissaBap { .. }
            | Self::InvalidMantissaCode { .. }
            | Self::InvalidMantissaGroupCode { .. }
            | Self::MantissaExponentLengthMismatch { .. }
            | Self::MantissaDitherLengthMismatch { .. }
            | Self::MissingDitherValue { .. }
            | Self::InvalidBlockStartDimensions { .. }
            | Self::MissingIndependentSubstreamZero { .. }
            | Self::NonsequentialIndependentSubstream { .. }
            | Self::NonsequentialDependentSubstream { .. }
            | Self::DependentAfterConvertedSubstream { .. }
            | Self::InvalidSpectralExtensionCode { .. }
            | Self::InvalidSpectralExtensionRange { .. }
            | Self::InvalidCouplingRange { .. }
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

/// Table E.1.1 stream identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamType {
    Independent,
    Dependent,
    ConvertedIndependent,
}

/// Fixed-length acquisition fields at the start of one syncframe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyncframeHeader {
    pub stream_type: StreamType,
    pub substream_id: u8,
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

/// One validated TS 103 420 metadata frame extracted from E-AC-3.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JocMetadataFrame {
    pub carrier_frame: usize,
    pub sample_rate: u32,
    pub samples: u16,
    pub complexity_index: u8,
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
    pub audio_coding_mode: u8,
    pub lfe_on: bool,
    pub bitstream_id: u8,
    pub addbsi: Option<Vec<u8>>,
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
    let mut bits = BitReader::new(bytes);
    Ok(parse_header_reader(&mut bits)?.0)
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
    Ok(parse_bsi_reader(&mut bits)?.0)
}

fn parse_bsi_reader(bits: &mut BitReader<'_>) -> Result<(BitstreamInformation, u8), Eac3Error> {
    let (header, num_blocks_code) = parse_header_reader(bits)?;
    let acmod = read_u8(bits, 3)?;
    let lfe_on = bits.read_bit()?;
    let bitstream_id = read_u8(bits, 5)?;
    skip(bits, 5)?; // dialnorm
    if bits.read_bit()? {
        skip(bits, 8)?;
    }
    if acmod == 0 {
        skip(bits, 5)?;
        if bits.read_bit()? {
            skip(bits, 8)?;
        }
    }
    if header.stream_type == StreamType::Dependent && bits.read_bit()? {
        skip(bits, 16)?;
    }
    if bits.read_bit()? {
        parse_mixing_metadata(bits, header.stream_type, acmod, lfe_on, num_blocks_code)?;
    }
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
            audio_coding_mode: acmod,
            lfe_on,
            bitstream_id,
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
    if syntax.spx_attenuation() {
        for _ in 0..channel_count {
            if bits.read_bit()? {
                skip(&mut bits, 5)?;
            }
        }
    }
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
) -> Result<(), Eac3Error> {
    if acmod > 2 {
        skip(bits, 2)?;
    }
    if acmod & 1 != 0 && acmod > 2 {
        skip(bits, 6)?;
    }
    if acmod & 4 != 0 {
        skip(bits, 6)?;
    }
    if lfe_on && bits.read_bit()? {
        skip(bits, 5)?;
    }
    if stream_type != StreamType::Independent {
        return Ok(());
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
    Ok(())
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
                StreamType::Independent | StreamType::ConvertedIndependent => {
                    if header.substream_id != expected_independent {
                        return Err(Eac3Error::NonsequentialIndependentSubstream {
                            expected: expected_independent,
                            actual: header.substream_id,
                        });
                    }
                    expected_independent += 1;
                    expected_dependent = 0;
                    dependent_allowed = header.stream_type == StreamType::Independent;
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
    Ok(Some(parse_emdf_sync(&auxdata.bytes)?))
}

/// Extracts and validates one TS 103 420 profile carried through `auxdata`.
///
/// # Errors
/// Returns an error for invalid unit bounds, malformed frame/EMDF syntax,
/// multiple profile carriers, missing same-frame `addbsi`, or violation of the
/// mandatory last-dependent-substream placement rule.
pub fn extract_aux_joc_access_unit(
    stream: &[u8],
    frames: &[SyncframeIndexEntry],
    unit: AccessUnitIndex,
) -> Result<Option<JocMetadataFrame>, Eac3Error> {
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
    let mut found = None;
    for (relative, entry) in unit_frames.iter().enumerate() {
        let frame_index = unit.first_frame + relative;
        let frame = frame_bytes(stream, *entry)?;
        let Some(parsed) = extract_aux_emdf(frame)? else {
            continue;
        };
        let carries_profile = parsed
            .container
            .payloads
            .iter()
            .any(|payload| payload.id == OAMD_PAYLOAD_ID || payload.id == JOC_PAYLOAD_ID);
        if !carries_profile {
            continue;
        }
        if found.is_some() {
            return Err(Eac3Error::MultipleJocCarriers);
        }
        found = Some((frame_index, frame, parsed));
    }
    let Some((carrier_frame, frame, parsed)) = found else {
        return Ok(None);
    };
    if let Some(required_frame) = required_dependent
        && carrier_frame != required_frame
    {
        return Err(Eac3Error::InvalidJocCarrierPlacement {
            carrier_frame,
            required_frame,
        });
    }
    let bsi = parse_bsi(frame)?;
    let addbsi = bsi.addbsi.as_deref().ok_or(Eac3Error::MissingJocAddbsi {
        frame: carrier_frame,
    })?;
    let extension = parse_joc_addbsi(addbsi)?;
    let payloads = validate_joc_profile(&parsed.container)?;
    Ok(Some(JocMetadataFrame {
        carrier_frame,
        sample_rate: unit.sample_rate,
        samples: unit.samples,
        complexity_index: extension.complexity_index,
        oamd: payloads.oamd.to_vec(),
        joc: payloads.joc.to_vec(),
    }))
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
