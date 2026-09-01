// pattern: Functional Core

//! Pure validation for the ETSI E-AC-3 JOC CMAF subset.

use openjoc_eac3::{
    Eac3Error, StreamType, SyncframeIndexEntry, extract_joc_addbsi_access_unit, group_access_units,
    index_syncframes, parse_bsi,
};
use std::fmt;

/// The sample duration required by the E-AC-3 CMAF sample definition.
pub const CMAF_SAMPLE_AUDIO_DURATION: u16 = 1536;
/// The sample rate supported by OpenJOC's Core CMAF decoder path.
pub const CMAF_SAMPLE_RATE: u32 = 48_000;
/// CMAF JOC has exactly one independent substream.
pub const CMAF_INDEPENDENT_SUBSTREAMS: usize = 1;
/// CMAF JOC permits I0 and at most D0.
pub const CMAF_MAX_DEPENDENT_SUBSTREAMS: usize = 1;

/// One substream descriptor from an ISO-BMFF `dec3` box.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ec3SubstreamConfig {
    pub fscod: u8,
    pub bsid: u8,
    pub asvc: bool,
    pub bsmod: u8,
    pub acmod: u8,
    pub lfe_on: bool,
    /// The `num_dep_sub` value. For the CMAF subset this is zero or one.
    pub dependent_substreams: u8,
    pub chan_loc: Option<u16>,
}

/// Parsed ISO-BMFF `EC3SpecificBox` (`dec3`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ec3SpecificBox {
    pub data_rate_kbps: u16,
    pub independent_substreams: Vec<Ec3SubstreamConfig>,
    pub flag_ec3_extension_type_a: bool,
    pub complexity_index_type_a: u8,
}

/// The container facts needed to validate the supported CMAF JOC profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CmafTrackMetadata {
    pub sample_entry: [u8; 4],
    pub timescale: u32,
    pub sample_rate: u32,
    pub decoder_config: Option<Ec3SpecificBox>,
    /// Compatibility brands are retained for audit/reporting. `ceao` is a
    /// recommended object-audio hint, not the source of JOC truth.
    pub compatibility_brands: Vec<[u8; 4]>,
}

/// A track whose mandatory CMAF JOC metadata has passed validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CmafJocTrack {
    metadata: CmafTrackMetadata,
}

/// A validated CMAF sample borrowing the exact demuxed bytes.
#[derive(Debug, Eq, PartialEq)]
pub struct ValidatedCmafSample<'a> {
    pub bytes: &'a [u8],
    pub frame_offsets: Vec<usize>,
    pub audio_duration: u16,
}

/// Layer-specific CMAF validation failure.
#[derive(Debug)]
pub enum CmafError {
    MalformedDecoderConfig(String),
    WrongSampleEntry { actual: [u8; 4] },
    MissingDecoderConfig,
    TrackTimescaleMismatch { timescale: u32, sample_rate: u32 },
    UnsupportedSampleRate { actual: u32 },
    IndependentSubstreamCount { actual: usize },
    UnsupportedDependentSubstreamCount { actual: u8 },
    MissingJocExtensionSignal,
    ComplexityIndexOutOfRange { actual: u8 },
    UnsupportedBitrate { actual: u16 },
    InvalidTrackSubstream(String),
    InvalidSample(String),
    Eac3(Eac3Error),
}

impl fmt::Display for CmafError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedDecoderConfig(detail) => {
                write!(formatter, "malformed E-AC-3 dec3 configuration: {detail}")
            }
            Self::WrongSampleEntry { actual } => write!(
                formatter,
                "CMAF JOC requires EC3SampleEntry ec-3, got {}",
                format_fourcc(*actual)
            ),
            Self::MissingDecoderConfig => {
                formatter.write_str("CMAF JOC requires an EC3SpecificBox dec3 configuration")
            }
            Self::TrackTimescaleMismatch {
                timescale,
                sample_rate,
            } => write!(
                formatter,
                "CMAF E-AC-3 timescale {timescale} does not match sample rate {sample_rate}"
            ),
            Self::UnsupportedSampleRate { actual } => write!(
                formatter,
                "OpenJOC CMAF JOC support requires 48000 Hz, got {actual} Hz"
            ),
            Self::IndependentSubstreamCount { actual } => write!(
                formatter,
                "CMAF JOC requires exactly one independent substream, got {actual}"
            ),
            Self::UnsupportedDependentSubstreamCount { actual } => write!(
                formatter,
                "CMAF JOC permits at most dependent substream D0, got {actual}"
            ),
            Self::MissingJocExtensionSignal => {
                formatter.write_str("CMAF JOC requires the EC3SpecificBox JOC extension signal")
            }
            Self::ComplexityIndexOutOfRange { actual } => write!(
                formatter,
                "CMAF JOC complexity index {actual} exceeds the ETSI maximum of 16"
            ),
            Self::UnsupportedBitrate { actual } => write!(
                formatter,
                "CMAF JOC bitrate {actual} kbps exceeds the Core CMAF limit of 3024 kbps"
            ),
            Self::InvalidTrackSubstream(detail) => {
                write!(formatter, "invalid CMAF JOC track substream: {detail}")
            }
            Self::InvalidSample(detail) => write!(formatter, "invalid CMAF JOC sample: {detail}"),
            Self::Eac3(error) => write!(formatter, "invalid E-AC-3 sample: {error}"),
        }
    }
}

impl std::error::Error for CmafError {}

impl From<Eac3Error> for CmafError {
    fn from(error: Eac3Error) -> Self {
        Self::Eac3(error)
    }
}

/// Parses one complete ISO-BMFF `dec3` box, including its 8-byte box header.
pub fn parse_ec3_specific_box(bytes: &[u8]) -> Result<Ec3SpecificBox, CmafError> {
    let size = read_u32(bytes, 0)? as usize;
    if size != bytes.len() || size < 8 {
        return Err(config_error("box size does not cover exactly one dec3 box"));
    }
    if bytes.get(4..8) != Some(b"dec3") {
        return Err(config_error("box type is not dec3"));
    }
    let mut reader = BitReader::new(&bytes[8..]);
    let data_rate_kbps = reader.read(13)? as u16;
    let num_ind_sub = reader.read(3)? as usize;
    let mut independent_substreams = Vec::with_capacity(num_ind_sub + 1);
    for _ in 0..=num_ind_sub {
        let fscod = reader.read(2)? as u8;
        let bsid = reader.read(5)? as u8;
        reader.expect_zero(1, "reserved dec3 bit")?;
        let asvc = reader.read(1)? != 0;
        let bsmod = reader.read(3)? as u8;
        let acmod = reader.read(3)? as u8;
        let lfe_on = reader.read(1)? != 0;
        reader.expect_zero(3, "reserved dec3 bits")?;
        let dependent_substreams = reader.read(4)? as u8;
        let chan_loc = if dependent_substreams > 0 {
            Some(reader.read(9)? as u16)
        } else {
            reader.expect_zero(1, "reserved dec3 bit")?;
            None
        };
        independent_substreams.push(Ec3SubstreamConfig {
            fscod,
            bsid,
            asvc,
            bsmod,
            acmod,
            lfe_on,
            dependent_substreams,
            chan_loc,
        });
    }

    let mut flag_ec3_extension_type_a = false;
    let mut complexity_index_type_a = 0;
    let remaining = reader.remaining();
    if remaining != 0 {
        if remaining % 8 != 0 {
            return Err(config_error("dec3 trailing fields are not byte-aligned"));
        }
        if remaining == 8 {
            reader.expect_zero(8, "reserved dec3 extension byte")?;
        } else {
            // F.6.2.14 explicitly permits additional reserved bytes after
            // the defined fields. They are accepted only when zero.
            reader.expect_zero(7, "reserved dec3 extension bits")?;
            flag_ec3_extension_type_a = reader.read(1)? != 0;
            complexity_index_type_a = reader.read(8)? as u8;
            if !flag_ec3_extension_type_a && complexity_index_type_a != 0 {
                return Err(config_error(
                    "dec3 complexity is nonzero without the type-A extension",
                ));
            }
            while reader.remaining() > 0 {
                reader.expect_zero(8, "reserved dec3 extension byte")?;
            }
        }
    }

    Ok(Ec3SpecificBox {
        data_rate_kbps,
        independent_substreams,
        flag_ec3_extension_type_a,
        complexity_index_type_a,
    })
}

impl CmafJocTrack {
    /// Validates the mandatory metadata for OpenJOC's supported CMAF JOC path.
    pub fn new(metadata: CmafTrackMetadata) -> Result<Self, CmafError> {
        if metadata.sample_entry != *b"ec-3" {
            return Err(CmafError::WrongSampleEntry {
                actual: metadata.sample_entry,
            });
        }
        if metadata.decoder_config.is_none() {
            return Err(CmafError::MissingDecoderConfig);
        }
        if metadata.timescale != metadata.sample_rate {
            return Err(CmafError::TrackTimescaleMismatch {
                timescale: metadata.timescale,
                sample_rate: metadata.sample_rate,
            });
        }
        if metadata.sample_rate != CMAF_SAMPLE_RATE {
            return Err(CmafError::UnsupportedSampleRate {
                actual: metadata.sample_rate,
            });
        }
        let config = metadata
            .decoder_config
            .as_ref()
            .ok_or(CmafError::MissingDecoderConfig)?;
        if config.independent_substreams.len() != CMAF_INDEPENDENT_SUBSTREAMS {
            return Err(CmafError::IndependentSubstreamCount {
                actual: config.independent_substreams.len(),
            });
        }
        if !config.flag_ec3_extension_type_a {
            return Err(CmafError::MissingJocExtensionSignal);
        }
        if config.complexity_index_type_a > 16 {
            return Err(CmafError::ComplexityIndexOutOfRange {
                actual: config.complexity_index_type_a,
            });
        }
        if config.data_rate_kbps > 3024 {
            return Err(CmafError::UnsupportedBitrate {
                actual: config.data_rate_kbps,
            });
        }
        let independent = &config.independent_substreams[0];
        if independent.fscod != 0 {
            return Err(CmafError::InvalidTrackSubstream(
                "the supported CMAF JOC profile requires fscod=0".to_owned(),
            ));
        }
        if independent.acmod == 0 {
            return Err(CmafError::InvalidTrackSubstream(
                "acmod=0 is forbidden in CMAF".to_owned(),
            ));
        }
        if independent.dependent_substreams > 1 {
            return Err(CmafError::UnsupportedDependentSubstreamCount {
                actual: independent.dependent_substreams,
            });
        }
        if independent.dependent_substreams > 0 && independent.chan_loc.is_none() {
            return Err(CmafError::InvalidTrackSubstream(
                "D0 configuration is missing chan_loc".to_owned(),
            ));
        }
        if independent.dependent_substreams == 0 && independent.bsid != 16 {
            return Err(CmafError::InvalidTrackSubstream(
                "I0-only CMAF E-AC-3 requires bsid=16".to_owned(),
            ));
        }
        if independent.dependent_substreams > 0 && !matches!(independent.bsid, 6 | 8 | 16) {
            return Err(CmafError::InvalidTrackSubstream(
                "I0+D0 CMAF E-AC-3 requires I0 bsid 6, 8, or 16".to_owned(),
            ));
        }
        Ok(Self { metadata })
    }

    /// Returns the validated track metadata without transferring ownership.
    #[must_use]
    pub const fn metadata(&self) -> &CmafTrackMetadata {
        &self.metadata
    }

    /// Validates one complete CMAF sample while borrowing its bytes unchanged.
    pub fn validate_sample<'a>(
        &self,
        bytes: &'a [u8],
    ) -> Result<ValidatedCmafSample<'a>, CmafError> {
        let frames = index_syncframes(bytes)?;
        let units = group_access_units(&frames)?;
        let unit = units.first().copied().ok_or_else(|| {
            CmafError::InvalidSample("sample contains no complete E-AC-3 access unit".to_owned())
        })?;
        if units.len() != 1 || unit.first_frame != 0 || unit.frame_count != frames.len() {
            return Err(CmafError::InvalidSample(
                "sample must contain exactly one complete access unit".to_owned(),
            ));
        }
        if unit.samples != CMAF_SAMPLE_AUDIO_DURATION {
            return Err(CmafError::InvalidSample(format!(
                "sample duration is {} decoded samples, expected {CMAF_SAMPLE_AUDIO_DURATION}",
                unit.samples
            )));
        }
        let config = self
            .metadata
            .decoder_config
            .as_ref()
            .ok_or(CmafError::MissingDecoderConfig)?;
        let independent = &config.independent_substreams[0];
        let expected_frame_count = usize::from(independent.dependent_substreams) + 1;
        if frames.len() != expected_frame_count {
            return Err(CmafError::InvalidSample(format!(
                "sample has {} syncframes, expected {expected_frame_count}",
                frames.len()
            )));
        }
        for (index, entry) in frames.iter().enumerate() {
            validate_frame_shape(bytes, *entry, index, independent)?;
        }
        if let Some(addbsi) = extract_joc_addbsi_access_unit(bytes, &frames, unit)? {
            if addbsi.complexity_index != config.complexity_index_type_a {
                return Err(CmafError::InvalidSample(format!(
                    "in-band JOC complexity {} disagrees with dec3 complexity {}",
                    addbsi.complexity_index, config.complexity_index_type_a
                )));
            }
        }
        let frame_offsets = frames.iter().map(|entry| entry.offset).collect();
        Ok(ValidatedCmafSample {
            bytes,
            frame_offsets,
            audio_duration: CMAF_SAMPLE_AUDIO_DURATION,
        })
    }
}

fn validate_frame_shape(
    stream: &[u8],
    entry: SyncframeIndexEntry,
    index: usize,
    independent: &Ec3SubstreamConfig,
) -> Result<(), CmafError> {
    let end = entry
        .offset
        .checked_add(entry.header.frame_size)
        .ok_or_else(|| CmafError::InvalidSample("syncframe offset overflow".to_owned()))?;
    let bytes = stream.get(entry.offset..end).ok_or_else(|| {
        CmafError::InvalidSample("syncframe exceeds the CMAF sample boundary".to_owned())
    })?;
    if entry.header.stream_type == StreamType::ConvertedIndependent {
        return Err(CmafError::InvalidSample(
            "converted Type2 E-AC-3 is forbidden in CMAF".to_owned(),
        ));
    }
    if entry.header.stream_type == StreamType::LegacyIndependent {
        return Err(CmafError::InvalidSample(
            "legacy AC-3 core carriage is outside the CMAF JOC profile".to_owned(),
        ));
    }
    if entry.header.audio_blocks != 6 || entry.header.samples != CMAF_SAMPLE_AUDIO_DURATION {
        return Err(CmafError::InvalidSample(
            "CMAF JOC requires six audio blocks per syncframe".to_owned(),
        ));
    }
    if entry.header.sample_rate != CMAF_SAMPLE_RATE {
        return Err(CmafError::InvalidSample(
            "CMAF JOC sample rate disagrees with the supported track".to_owned(),
        ));
    }
    let info = parse_bsi(bytes)?;
    if info.header.stream_type != entry.header.stream_type
        || info.header.substream_id != entry.header.substream_id
    {
        return Err(CmafError::InvalidSample(
            "indexed and in-band E-AC-3 headers disagree".to_owned(),
        ));
    }
    if info.audio_coding_mode == 0 {
        return Err(CmafError::InvalidSample(
            "acmod=0 is forbidden in CMAF".to_owned(),
        ));
    }
    if index == 0 {
        if info.header.stream_type != StreamType::Independent || info.header.substream_id != 0 {
            return Err(CmafError::InvalidSample(
                "the first CMAF syncframe must be independent substream I0".to_owned(),
            ));
        }
        if info.bitstream_id != independent.bsid
            || info.audio_coding_mode != independent.acmod
            || info.lfe_on != independent.lfe_on
        {
            return Err(CmafError::InvalidSample(format!(
                "I0 in-band headers disagree with dec3 configuration (in-band bsid={}, acmod={}, lfeon={}; dec3 bsid={}, acmod={}, lfeon={})",
                info.bitstream_id,
                info.audio_coding_mode,
                info.lfe_on,
                independent.bsid,
                independent.acmod,
                independent.lfe_on
            )));
        }
    } else {
        if info.header.stream_type != StreamType::Dependent || info.header.substream_id != 0 {
            return Err(CmafError::InvalidSample(
                "the optional second CMAF syncframe must be dependent substream D0".to_owned(),
            ));
        }
        if info.bitstream_id != 16 {
            return Err(CmafError::InvalidSample(
                "D0 in CMAF must use bsid=16".to_owned(),
            ));
        }
        let channel_map = info.channel_map.ok_or_else(|| {
            CmafError::InvalidSample("D0 is missing its required channel map".to_owned())
        })?;
        if channel_map == 0 {
            return Err(CmafError::InvalidSample(
                "D0 channel map must contain at least one channel".to_owned(),
            ));
        }
        if let Some(expected) = independent.chan_loc {
            let actual = chan_loc_from_channel_map(channel_map)?;
            if actual != expected {
                return Err(CmafError::InvalidSample(format!(
                    "D0 chanmap disagrees with dec3 chan_loc (in-band {actual}, dec3 {expected})"
                )));
            }
        }
    }
    Ok(())
}

fn chan_loc_from_channel_map(channel_map: u16) -> Result<u16, CmafError> {
    // TS 102 366 F.6.2.13 has no chan_loc bit for Lts/Rts or LFE1. Those
    // positions are outside the constrained Table-47 CMAF shapes and must
    // not be silently discarded during cross-validation.
    if channel_map & 0x0005 != 0 {
        return Err(CmafError::InvalidSample(
            "D0 chanmap contains a CMAF-unsupported channel position".to_owned(),
        ));
    }
    let mut chan_loc = 0_u16;
    for (channel_map_bit, chan_loc_bit) in [
        (5_u8, 0_u8),
        (6, 1),
        (7, 2),
        (8, 3),
        (9, 4),
        (10, 5),
        (11, 6),
        (12, 7),
        (14, 8),
    ] {
        if channel_map & (1 << (15 - channel_map_bit)) != 0 {
            chan_loc |= 1 << chan_loc_bit;
        }
    }
    Ok(chan_loc)
}

fn format_fourcc(bytes: [u8; 4]) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}

fn config_error(detail: &str) -> CmafError {
    CmafError::MalformedDecoderConfig(detail.to_owned())
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, CmafError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| config_error("truncated box header"))?;
    Ok(u32::from_be_bytes([value[0], value[1], value[2], value[3]]))
}

struct BitReader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> BitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len() * 8 - self.position
    }

    fn read(&mut self, width: usize) -> Result<u64, CmafError> {
        if width > 64 || self.remaining() < width {
            return Err(config_error("truncated dec3 bit field"));
        }
        let mut value = 0_u64;
        for _ in 0..width {
            let byte = self.bytes[self.position / 8];
            let bit = (byte >> (7 - (self.position % 8))) & 1;
            value = (value << 1) | u64::from(bit);
            self.position += 1;
        }
        Ok(value)
    }

    fn expect_zero(&mut self, width: usize, field: &str) -> Result<(), CmafError> {
        if self.read(width)? != 0 {
            return Err(config_error(field));
        }
        Ok(())
    }
}
