// pattern: Functional Core

//! Stable metadata and resource loading for OpenJOC's built-in HRTF library.

use super::{LoadedSofaHrirBank, SofaError, SofaHrirMetadata};
use openjoc_render::{
    CartesianPosition, HrirBank, HrirEar, HrirEntry, HrirEntryId, HrirPair, RenderError,
};
use sha2::{Digest, Sha256};

const ASSET_MAGIC: &[u8; 8] = b"OJHRTF2\0";
const ASSET_VERSION: u32 = 2;
const ASSET_HEADER_BYTES: usize = 56;
const PACKED_ASSET_MAGIC: &[u8; 8] = b"OJHRTF2\0";
const PACKED_ASSET_VERSION: u32 = 2;
const PACKED_PAYLOAD_MAGIC: &[u8; 8] = b"OJRIR2\0\0";
const PACKED_PAYLOAD_HEADER_BYTES: usize = 40;
const MAX_PACKED_ASSET_BYTES: usize = 256 * 1024 * 1024;

/// Stable identifier for a built-in HRTF resource.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BuiltinHrtf {
    SadieD1Ku100,
    SadieD2Kemar,
}

/// User-facing and provenance metadata for one built-in resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuiltinHrtfMetadata {
    pub id: &'static str,
    pub display_name: &'static str,
    pub dataset: &'static str,
    pub subject: &'static str,
    pub source: &'static str,
    pub doi: Option<&'static str>,
    pub license: &'static str,
    pub sample_rate_hz: u32,
    /// Number of directions in the upstream source before OpenJOC preparation.
    pub measurement_count: usize,
    pub ir_length: usize,
    pub notes: &'static str,
}

/// Versioned storage metadata and attribution fields added without changing the
/// original public [`BuiltinHrtfMetadata`] struct shape.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuiltinHrtfAssetMetadata {
    pub preset_id: &'static str,
    pub source_listener_name: &'static str,
    pub authors_institution: &'static str,
    pub asset_format_version: u32,
    pub asset_file: &'static str,
    pub asset_size_bytes: usize,
    pub asset_sha256: &'static str,
    pub asset_direction_count: usize,
}

/// Compact runtime representation for one built-in resource. Direction and
/// delay metadata stay f64/integer while the canonical published taps remain
/// f32 until a requested interpolation kernel is prepared.
#[derive(Clone, Debug, PartialEq)]
pub struct BuiltinHrirF32Bank {
    sample_rate_hz: u32,
    records: Vec<BuiltinHrirF32Record>,
    taps: Vec<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BuiltinHrirF32Record {
    pub(crate) direction: [f64; 3],
    pub(crate) tap_offset: u32,
    pub(crate) tap_count: u32,
    pub(crate) delays: [u32; 2],
}

/// Built-in asset loaded without widening its resident tap bank to f64.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadedBuiltinHrirF32Bank {
    pub bank: BuiltinHrirF32Bank,
    pub metadata: SofaHrirMetadata,
}

impl BuiltinHrirF32Bank {
    #[must_use]
    pub const fn sample_rate_hz(&self) -> u32 {
        self.sample_rate_hz
    }

    #[must_use]
    pub fn direction_count(&self) -> usize {
        self.records.len()
    }

    #[must_use]
    pub fn tap_sample_count(&self) -> usize {
        self.taps.len()
    }

    #[must_use]
    pub fn tap_storage_bytes(&self) -> usize {
        self.taps.capacity() * std::mem::size_of::<f32>()
    }

    #[must_use]
    pub fn direction_metadata_storage_bytes(&self) -> usize {
        self.records.capacity() * std::mem::size_of::<BuiltinHrirF32Record>()
    }

    /// Current resident capacity for record metadata and the canonical f32 taps.
    #[must_use]
    pub fn resident_storage_bytes(&self) -> usize {
        self.direction_metadata_storage_bytes() + self.tap_storage_bytes()
    }

    pub(crate) fn records(&self) -> &[BuiltinHrirF32Record] {
        &self.records
    }

    pub(crate) fn ear_taps(&self, record: usize, ear: usize) -> Option<&[f32]> {
        if ear > 1 {
            return None;
        }
        let metadata = self.records.get(record)?;
        let tap_count = usize::try_from(metadata.tap_count).ok()?;
        let tap_offset = usize::try_from(metadata.tap_offset).ok()?;
        let start = tap_offset.checked_add(ear.checked_mul(tap_count)?)?;
        self.taps.get(start..start.checked_add(tap_count)?)
    }
}

pub const BUILTIN_HRTF_REGISTRY: [BuiltinHrtf; 2] =
    [BuiltinHrtf::SadieD1Ku100, BuiltinHrtf::SadieD2Kemar];

impl BuiltinHrtf {
    #[must_use]
    pub const fn asset_code(self) -> u32 {
        match self {
            Self::SadieD1Ku100 => 1,
            Self::SadieD2Kemar => 2,
        }
    }

    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::SadieD1Ku100 => "sadie-ii-d1-ku100",
            Self::SadieD2Kemar => "sadie-ii-d2-kemar",
        }
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::SadieD1Ku100 => "SADIE II — KU100",
            Self::SadieD2Kemar => "SADIE II — KEMAR",
        }
    }

    #[must_use]
    pub const fn metadata(self) -> BuiltinHrtfMetadata {
        match self {
            Self::SadieD1Ku100 => BuiltinHrtfMetadata {
                id: "sadie-ii-d1-ku100",
                display_name: "SADIE II — KU100",
                dataset: "SADIE II Database, version 2-1",
                subject: "D1 / Neumann KU100",
                source: "https://zenodo.org/records/10886409",
                doi: Some("10.5281/zenodo.10886409"),
                license: "Apache-2.0",
                sample_rate_hz: 48_000,
                measurement_count: 8_802,
                ir_length: 256,
                notes: "Default/reference profile; OpenJOC retains the existing prepared resource and output path.",
            },
            Self::SadieD2Kemar => BuiltinHrtfMetadata {
                id: "sadie-ii-d2-kemar",
                display_name: "SADIE II — KEMAR",
                dataset: "SADIE II Database, version 2-1",
                subject: "D2 / KEMAR",
                source: "https://zenodo.org/records/10886409",
                doi: Some("10.5281/zenodo.10886409"),
                license: "Apache-2.0",
                sample_rate_hz: 48_000,
                measurement_count: 8_802,
                ir_length: 256,
                notes: "Official D2 HRIR grid; OpenJOC adds only the same exact virtual-speaker aliases used by D1.",
            },
        }
    }

    #[must_use]
    pub const fn asset_metadata(self) -> BuiltinHrtfAssetMetadata {
        match self {
            Self::SadieD1Ku100 => BuiltinHrtfAssetMetadata {
                preset_id: "sadie-ii-d1-ku100",
                source_listener_name: "D1",
                authors_institution: "University of York Audio Lab; SADIE II measurements",
                asset_format_version: ASSET_VERSION,
                asset_file: "sadie-ii-d1-ku100.ojhrtf",
                asset_size_bytes: 18_374_724,
                asset_sha256: "78d048a68f84d34051578c262e401e35baa0e718901f85349afe0232f985d4df",
                asset_direction_count: 8_817,
            },
            Self::SadieD2Kemar => BuiltinHrtfAssetMetadata {
                preset_id: "sadie-ii-d2-kemar",
                source_listener_name: "D2",
                authors_institution: "University of York Audio Lab; SADIE II measurements",
                asset_format_version: ASSET_VERSION,
                asset_file: "sadie-ii-d2-kemar.ojhrtf",
                asset_size_bytes: 18_374_724,
                asset_sha256: "b2f42ca2ce9ef2dfa7e3eff263543c4f306d0ac95bd684cf5ca344c88d6bd461",
                asset_direction_count: 8_817,
            },
        }
    }

    #[must_use]
    pub fn from_id(value: &str) -> Option<Self> {
        BUILTIN_HRTF_REGISTRY
            .into_iter()
            .find(|preset| preset.id() == value)
    }

    #[must_use]
    pub const fn all() -> &'static [Self; 2] {
        &BUILTIN_HRTF_REGISTRY
    }
}

/// Returns the checked-in packaged bytes when this build embeds resources.
pub fn builtin_hrtf_asset_bytes(preset: BuiltinHrtf) -> Result<&'static [u8], SofaError> {
    #[cfg(feature = "embedded-builtin-hrtf")]
    {
        Ok(match preset {
            BuiltinHrtf::SadieD1Ku100 => &include_bytes!("../assets/sadie-ii-d1-ku100.ojhrtf")[..],
            BuiltinHrtf::SadieD2Kemar => &include_bytes!("../assets/sadie-ii-d2-kemar.ojhrtf")[..],
        })
    }
    #[cfg(not(feature = "embedded-builtin-hrtf"))]
    {
        let _ = preset;
        Err(SofaError::InvalidBuiltinHrtfAsset(
            "this build requires the caller to provide the selected asset",
        ))
    }
}

/// Loads the default D1 built-in asset.
pub fn load_builtin_hrir(preset: BuiltinHrtf) -> Result<LoadedSofaHrirBank, SofaError> {
    load_builtin_hrir_from_asset(preset, builtin_hrtf_asset_bytes(preset)?)
}

/// Loads one built-in asset into the compact f32-resident representation.
pub fn load_builtin_hrir_f32(preset: BuiltinHrtf) -> Result<LoadedBuiltinHrirF32Bank, SofaError> {
    load_builtin_hrir_f32_from_asset(preset, builtin_hrtf_asset_bytes(preset)?)
}

/// Loads and validates a versioned OpenJOC built-in resource supplied by the caller.
pub fn load_builtin_hrir_from_asset(
    preset: BuiltinHrtf,
    asset: &[u8],
) -> Result<LoadedSofaHrirBank, SofaError> {
    parse_packed_hrtf_payload(preset, verified_builtin_payload(preset, asset)?)
}

/// Verifies and loads a caller-supplied built-in asset without creating a full
/// f64 copy of its taps.
pub fn load_builtin_hrir_f32_from_asset(
    preset: BuiltinHrtf,
    asset: &[u8],
) -> Result<LoadedBuiltinHrirF32Bank, SofaError> {
    parse_packed_hrtf_payload_f32(preset, verified_builtin_payload(preset, asset)?)
}

fn verified_builtin_payload(preset: BuiltinHrtf, asset: &[u8]) -> Result<&[u8], SofaError> {
    let metadata = preset.asset_metadata();
    if asset.len() != metadata.asset_size_bytes {
        return Err(SofaError::InvalidBuiltinHrtfAsset(
            "asset size does not match the built-in registry",
        ));
    }
    let actual_hash = Sha256::digest(asset);
    let mut hash_text = String::with_capacity(64);
    for byte in actual_hash {
        use std::fmt::Write as _;
        let _ = write!(hash_text, "{byte:02x}");
    }
    if hash_text != metadata.asset_sha256 {
        return Err(SofaError::InvalidBuiltinHrtfAsset(
            "asset checksum does not match the built-in registry",
        ));
    }
    decode_asset_payload(preset, asset)
}

/// Encodes a fully validated bank into the version-2, direct built-in format.
/// All source taps must round-trip exactly through f32; no quantization is done.
pub fn pack_builtin_hrir_asset(
    preset: BuiltinHrtf,
    loaded: &LoadedSofaHrirBank,
) -> Result<Vec<u8>, SofaError> {
    let metadata = preset.metadata();
    let asset_metadata = preset.asset_metadata();
    if loaded.metadata.sample_rate_hz != metadata.sample_rate_hz
        || loaded.bank.sample_rate_hz() != metadata.sample_rate_hz
        || loaded.metadata.original_fir_length != metadata.ir_length
        || loaded.bank.entries().len() != asset_metadata.asset_direction_count
        || !loaded
            .metadata
            .listener_short_name
            .as_deref()
            .is_some_and(|listener| {
                listener.eq_ignore_ascii_case(asset_metadata.source_listener_name)
            })
    {
        return Err(SofaError::InvalidBuiltinHrtfAsset(
            "packed bank shape does not match preset metadata",
        ));
    }
    let directions = loaded.bank.entries();
    let max_taps = directions
        .iter()
        .map(|entry| entry.pair().tap_count())
        .max()
        .ok_or(SofaError::InvalidBuiltinHrtfAsset("empty packed bank"))?;
    let mut payload_capacity = PACKED_PAYLOAD_HEADER_BYTES;
    for entry in directions {
        let pair = entry.pair();
        let expected_left = metadata
            .ir_length
            .checked_add(pair.delay_samples(HrirEar::Left))
            .ok_or(SofaError::ResourceLimitExceeded("packed FIR length"))?;
        let expected_right = metadata
            .ir_length
            .checked_add(pair.delay_samples(HrirEar::Right))
            .ok_or(SofaError::ResourceLimitExceeded("packed FIR length"))?;
        if pair.tap_count() != expected_left.max(expected_right) {
            return Err(SofaError::InvalidBuiltinHrtfAsset(
                "an HRIR row length does not match the preset tap count and Data.Delay",
            ));
        }
        let row_bytes = pair
            .tap_count()
            .checked_mul(8)
            .and_then(|taps| taps.checked_add(36))
            .ok_or(SofaError::ResourceLimitExceeded("packed asset size"))?;
        payload_capacity = payload_capacity
            .checked_add(row_bytes)
            .ok_or(SofaError::ResourceLimitExceeded("packed asset size"))?;
    }
    if payload_capacity
        .checked_add(ASSET_HEADER_BYTES)
        .is_none_or(|total| total > MAX_PACKED_ASSET_BYTES)
    {
        return Err(SofaError::ResourceLimitExceeded("packed asset bytes"));
    }
    let direction_count = u32::try_from(directions.len())
        .map_err(|_| SofaError::ResourceLimitExceeded("packed direction count"))?;
    let max_taps = u32::try_from(max_taps)
        .map_err(|_| SofaError::ResourceLimitExceeded("packed tap count"))?;
    let mut asset = Vec::with_capacity(ASSET_HEADER_BYTES + payload_capacity);
    asset.extend_from_slice(PACKED_ASSET_MAGIC);
    asset.extend_from_slice(&PACKED_ASSET_VERSION.to_le_bytes());
    asset.extend_from_slice(&preset.asset_code().to_le_bytes());
    asset.extend_from_slice(
        &u64::try_from(payload_capacity)
            .map_err(|_| SofaError::ResourceLimitExceeded("packed asset size"))?
            .to_le_bytes(),
    );
    asset.extend_from_slice(&[0; 32]);
    let payload_offset = asset.len();

    asset.extend_from_slice(PACKED_PAYLOAD_MAGIC);
    asset.extend_from_slice(&loaded.metadata.sample_rate_hz.to_le_bytes());
    asset.extend_from_slice(&direction_count.to_le_bytes());
    asset.extend_from_slice(&max_taps.to_le_bytes());
    // Coordinate code 1: listener-local Cartesian, x-left/y-front/z-up.
    asset.extend_from_slice(&1_u32.to_le_bytes());
    // Channel code 1: left ear followed by right ear.
    asset.extend_from_slice(&1_u32.to_le_bytes());
    // Direction code 1: three little-endian f64 unit-vector components.
    asset.extend_from_slice(&1_u32.to_le_bytes());
    // Tap code 1: little-endian f32; the source bank must round-trip exactly.
    asset.extend_from_slice(&1_u32.to_le_bytes());
    // Delay code 1: integer sample offsets retained separately per ear.
    asset.extend_from_slice(&1_u32.to_le_bytes());

    for entry in directions {
        for component in entry.direction() {
            if !component.is_finite() {
                return Err(SofaError::InvalidBuiltinHrtfAsset(
                    "non-finite packed direction",
                ));
            }
            asset.extend_from_slice(&component.to_le_bytes());
        }
        let pair = entry.pair();
        let tap_count = pair.tap_count();
        asset.extend_from_slice(
            &u32::try_from(tap_count)
                .map_err(|_| SofaError::ResourceLimitExceeded("packed tap count"))?
                .to_le_bytes(),
        );
        for ear in [HrirEar::Left, HrirEar::Right] {
            asset.extend_from_slice(
                &u32::try_from(pair.delay_samples(ear))
                    .map_err(|_| SofaError::ResourceLimitExceeded("packed delay"))?
                    .to_le_bytes(),
            );
        }
        for samples in [pair.left_taps(), pair.right_taps()] {
            for &sample in samples {
                if !sample.is_finite() || f64::from(sample as f32) != sample {
                    return Err(SofaError::InvalidBuiltinHrtfAsset(
                        "tap cannot be stored as f32 without changing its value",
                    ));
                }
                asset.extend_from_slice(&(sample as f32).to_le_bytes());
            }
        }
    }

    if asset.len() - payload_offset != payload_capacity {
        return Err(SofaError::InvalidBuiltinHrtfAsset(
            "packed payload size accounting failed",
        ));
    }
    let checksum = Sha256::digest(&asset[payload_offset..]);
    asset[24..ASSET_HEADER_BYTES].copy_from_slice(&checksum);
    Ok(asset)
}

fn decode_asset_payload(preset: BuiltinHrtf, asset: &[u8]) -> Result<&[u8], SofaError> {
    if asset.len() < ASSET_HEADER_BYTES {
        return Err(SofaError::InvalidBuiltinHrtfAsset("truncated header"));
    }
    if &asset[..8] != ASSET_MAGIC {
        return Err(SofaError::InvalidBuiltinHrtfAsset("bad magic"));
    }
    let version = u32::from_le_bytes(asset[8..12].try_into().expect("fixed header slice"));
    if version != ASSET_VERSION {
        return Err(SofaError::InvalidBuiltinHrtfAsset("unsupported version"));
    }
    let preset_code = u32::from_le_bytes(asset[12..16].try_into().expect("fixed header slice"));
    if preset_code != preset.asset_code() {
        return Err(SofaError::InvalidBuiltinHrtfAsset("preset id mismatch"));
    }
    let payload_len = u64::from_le_bytes(asset[16..24].try_into().expect("fixed header slice"));
    if usize::try_from(payload_len).ok() != Some(asset.len() - ASSET_HEADER_BYTES) {
        return Err(SofaError::InvalidBuiltinHrtfAsset(
            "payload length mismatch",
        ));
    }
    let payload = &asset[ASSET_HEADER_BYTES..];
    let expected_checksum = &asset[24..56];
    let actual_checksum = Sha256::digest(payload);
    if expected_checksum != actual_checksum.as_slice() {
        return Err(SofaError::InvalidBuiltinHrtfAsset(
            "payload checksum mismatch",
        ));
    }
    Ok(payload)
}

fn parse_packed_hrtf_payload_f32(
    preset: BuiltinHrtf,
    payload: &[u8],
) -> Result<LoadedBuiltinHrirF32Bank, SofaError> {
    let limits = super::SofaLoadLimits {
        max_measurements: 70_000,
        max_file_bytes: 256 * 1024 * 1024,
        ..super::SofaLoadLimits::default()
    };
    if payload.len() as u64 > limits.max_file_bytes {
        return Err(SofaError::ResourceLimitExceeded("packed asset bytes"));
    }
    if payload.len() < PACKED_PAYLOAD_HEADER_BYTES {
        return Err(SofaError::InvalidBuiltinHrtfAsset(
            "truncated packed-payload header",
        ));
    }
    let mut cursor = PackedCursor::new(payload);
    if cursor.bytes(8)? != PACKED_PAYLOAD_MAGIC {
        return Err(SofaError::InvalidBuiltinHrtfAsset(
            "bad packed-payload magic",
        ));
    }
    let sample_rate_hz = cursor.u32()?;
    let direction_count = usize::try_from(cursor.u32()?)
        .map_err(|_| SofaError::ResourceLimitExceeded("packed direction count"))?;
    let max_tap_count = usize::try_from(cursor.u32()?)
        .map_err(|_| SofaError::ResourceLimitExceeded("packed tap count"))?;
    let coordinate_code = cursor.u32()?;
    let ear_order_code = cursor.u32()?;
    let direction_encoding = cursor.u32()?;
    let tap_encoding = cursor.u32()?;
    let delay_encoding = cursor.u32()?;
    let metadata = preset.metadata();
    let asset_metadata = preset.asset_metadata();
    if sample_rate_hz != metadata.sample_rate_hz
        || direction_count != asset_metadata.asset_direction_count
        || direction_count > limits.max_measurements
        || max_tap_count == 0
        || max_tap_count
            > limits
                .max_fir_samples
                .saturating_add(limits.max_delay_samples)
        || coordinate_code != 1
        || ear_order_code != 1
        || direction_encoding != 1
        || tap_encoding != 1
        || delay_encoding != 1
    {
        return Err(SofaError::InvalidBuiltinHrtfAsset(
            "unsupported or inconsistent packed metadata",
        ));
    }

    let records_bytes = direction_count
        .checked_mul(36)
        .and_then(|bytes| PACKED_PAYLOAD_HEADER_BYTES.checked_add(bytes))
        .ok_or(SofaError::ResourceLimitExceeded("packed record table"))?;
    let tap_bytes = payload
        .len()
        .checked_sub(records_bytes)
        .filter(|bytes| bytes.checked_rem(std::mem::size_of::<f32>()) == Some(0))
        .ok_or(SofaError::InvalidBuiltinHrtfAsset(
            "packed payload length does not match its direction table",
        ))?;
    let tap_sample_count = tap_bytes / std::mem::size_of::<f32>();
    if tap_sample_count > limits.max_total_coefficients {
        return Err(SofaError::ResourceLimitExceeded("packed FIR coefficients"));
    }

    let mut records = Vec::with_capacity(direction_count);
    let mut taps = Vec::with_capacity(tap_sample_count);
    let mut observed_max_taps = 0usize;
    let mut total_coefficients = 0usize;
    for measurement in 0..direction_count {
        let direction = [cursor.f64()?, cursor.f64()?, cursor.f64()?];
        let norm = (direction[0] * direction[0]
            + direction[1] * direction[1]
            + direction[2] * direction[2])
            .sqrt();
        if !norm.is_finite() || (norm - 1.0).abs() > 1.0e-9 {
            return Err(SofaError::InvalidCoordinate(format!(
                "packed direction {measurement} is not a unit vector"
            )));
        }
        let tap_count = usize::try_from(cursor.u32()?)
            .map_err(|_| SofaError::ResourceLimitExceeded("packed tap count"))?;
        let left_delay = usize::try_from(cursor.u32()?)
            .map_err(|_| SofaError::ResourceLimitExceeded("packed delay"))?;
        let right_delay = usize::try_from(cursor.u32()?)
            .map_err(|_| SofaError::ResourceLimitExceeded("packed delay"))?;
        if tap_count == 0
            || tap_count > max_tap_count
            || tap_count
                > limits
                    .max_fir_samples
                    .saturating_add(limits.max_delay_samples)
            || left_delay > tap_count
            || right_delay > tap_count
        {
            return Err(SofaError::ResourceLimitExceeded("packed FIR record"));
        }
        let pair_coefficients = tap_count
            .checked_mul(2)
            .ok_or(SofaError::ResourceLimitExceeded("packed FIR coefficients"))?;
        total_coefficients = total_coefficients
            .checked_add(pair_coefficients)
            .ok_or(SofaError::ResourceLimitExceeded("packed FIR coefficients"))?;
        if total_coefficients > limits.max_total_coefficients {
            return Err(SofaError::ResourceLimitExceeded("packed FIR coefficients"));
        }
        let tap_offset = u32::try_from(taps.len())
            .map_err(|_| SofaError::ResourceLimitExceeded("packed tap offset"))?;
        let compact_tap_count = u32::try_from(tap_count)
            .map_err(|_| SofaError::ResourceLimitExceeded("packed tap count"))?;
        let compact_delays = [
            u32::try_from(left_delay)
                .map_err(|_| SofaError::ResourceLimitExceeded("packed delay"))?,
            u32::try_from(right_delay)
                .map_err(|_| SofaError::ResourceLimitExceeded("packed delay"))?,
        ];
        for _ear in 0..2 {
            for _ in 0..tap_count {
                let value = cursor.f32()?;
                if !value.is_finite() {
                    return Err(SofaError::InvalidImpulseResponse(format!(
                        "non-finite packed tap in measurement {measurement}"
                    )));
                }
                taps.push(value);
            }
        }
        observed_max_taps = observed_max_taps.max(tap_count);
        records.push(BuiltinHrirF32Record {
            direction,
            tap_offset,
            tap_count: compact_tap_count,
            delays: compact_delays,
        });
    }
    if observed_max_taps != max_tap_count
        || cursor.remaining() != 0
        || taps.len() != tap_sample_count
    {
        return Err(SofaError::InvalidBuiltinHrtfAsset(
            "packed payload length or tap count mismatch",
        ));
    }
    validate_unique_packed_directions(&records)?;
    Ok(LoadedBuiltinHrirF32Bank {
        bank: BuiltinHrirF32Bank {
            sample_rate_hz,
            records,
            taps,
        },
        metadata: packed_hrtf_metadata(preset, direction_count, sample_rate_hz, observed_max_taps),
    })
}

fn parse_packed_hrtf_payload(
    preset: BuiltinHrtf,
    payload: &[u8],
) -> Result<LoadedSofaHrirBank, SofaError> {
    let loaded_f32 = parse_packed_hrtf_payload_f32(preset, payload)?;
    let mut entries = Vec::with_capacity(loaded_f32.bank.records.len());
    for (index, record) in loaded_f32.bank.records.iter().enumerate() {
        let left = loaded_f32
            .bank
            .ear_taps(index, 0)
            .ok_or(SofaError::InvalidBuiltinHrtfAsset("left tap range"))?
            .iter()
            .copied()
            .map(f64::from)
            .collect();
        let right = loaded_f32
            .bank
            .ear_taps(index, 1)
            .ok_or(SofaError::InvalidBuiltinHrtfAsset("right tap range"))?
            .iter()
            .copied()
            .map(f64::from)
            .collect();
        let delays = [
            usize::try_from(record.delays[0])
                .map_err(|_| SofaError::ResourceLimitExceeded("packed delay"))?,
            usize::try_from(record.delays[1])
                .map_err(|_| SofaError::ResourceLimitExceeded("packed delay"))?,
        ];
        let pair = HrirPair::new_with_delays(loaded_f32.bank.sample_rate_hz, left, right, delays)
            .map_err(|_| {
            SofaError::InvalidImpulseResponse("packed HrirPair validation".to_owned())
        })?;
        entries.push(
            HrirEntry::new(
                HrirEntryId::new(index as u64),
                CartesianPosition::new(
                    record.direction[0],
                    record.direction[1],
                    record.direction[2],
                ),
                pair,
            )
            .map_err(|_| SofaError::InvalidCoordinate(format!("packed direction {index}")))?,
        );
    }
    let bank =
        HrirBank::new(loaded_f32.bank.sample_rate_hz, entries).map_err(|error| match error {
            RenderError::DuplicateHrirDirection { first, second } => {
                SofaError::DuplicateDirection { first, second }
            }
            _ => SofaError::InvalidImpulseResponse("packed HrirBank validation".to_owned()),
        })?;
    Ok(LoadedSofaHrirBank {
        bank,
        metadata: loaded_f32.metadata,
    })
}

fn packed_hrtf_metadata(
    preset: BuiltinHrtf,
    direction_count: usize,
    sample_rate_hz: u32,
    observed_max_taps: usize,
) -> SofaHrirMetadata {
    let metadata = preset.metadata();
    let asset_metadata = preset.asset_metadata();
    SofaHrirMetadata {
        convention_version: "1.0".to_owned(),
        title: Some(metadata.dataset.to_owned()),
        database_name: Some(metadata.dataset.to_owned()),
        listener_short_name: Some(asset_metadata.source_listener_name.to_owned()),
        license: Some(metadata.license.to_owned()),
        measurement_count: direction_count,
        original_fir_length: metadata.ir_length,
        expanded_max_tap_length: observed_max_taps,
        sample_rate_hz,
    }
}

fn validate_unique_packed_directions(records: &[BuiltinHrirF32Record]) -> Result<(), SofaError> {
    const MAX_COMPONENT_DELTA: f64 = 2.0e-6;
    const DIRECTION_DOT_TOLERANCE: f64 = 1.0e-12;
    let mut indices = (0..records.len()).collect::<Vec<_>>();
    indices.sort_by(|left, right| {
        records[*left].direction[0]
            .total_cmp(&records[*right].direction[0])
            .then_with(|| left.cmp(right))
    });
    for first_position in 0..indices.len() {
        let first_index = indices[first_position];
        let first = records[first_index].direction;
        for &second_index in indices.iter().skip(first_position + 1) {
            let second = records[second_index].direction;
            if second[0] - first[0] > MAX_COMPONENT_DELTA {
                break;
            }
            let dot = first
                .iter()
                .zip(second)
                .map(|(left, right)| left * right)
                .sum::<f64>();
            if (1.0 - dot).abs() <= DIRECTION_DOT_TOLERANCE {
                return Err(SofaError::DuplicateDirection {
                    first: first_index,
                    second: second_index,
                });
            }
        }
    }
    Ok(())
}

struct PackedCursor<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> PackedCursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn bytes(&mut self, length: usize) -> Result<&'a [u8], SofaError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(SofaError::TruncatedContainer)?;
        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or(SofaError::TruncatedContainer)?;
        self.position = end;
        Ok(bytes)
    }

    fn u32(&mut self) -> Result<u32, SofaError> {
        Ok(u32::from_le_bytes(
            self.bytes(4)?
                .try_into()
                .map_err(|_| SofaError::TruncatedContainer)?,
        ))
    }

    fn f32(&mut self) -> Result<f32, SofaError> {
        Ok(f32::from_bits(self.u32()?))
    }

    fn f64(&mut self) -> Result<f64, SofaError> {
        Ok(f64::from_bits(u64::from_le_bytes(
            self.bytes(8)?
                .try_into()
                .map_err(|_| SofaError::TruncatedContainer)?,
        )))
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }
}

#[cfg(test)]
mod asset_format_tests {
    use super::{
        BUILTIN_HRTF_REGISTRY, BuiltinHrtf, LoadedSofaHrirBank, decode_asset_payload,
        load_builtin_hrir, pack_builtin_hrir_asset, parse_packed_hrtf_payload,
    };
    use openjoc_render::{CartesianPosition, HrirBank, HrirEar, HrirEntry, HrirPair};
    use sha2::{Digest, Sha256};

    #[test]
    fn preset_ids_and_asset_format_versions_are_stable() {
        let preset_ids = BUILTIN_HRTF_REGISTRY
            .iter()
            .copied()
            .map(BuiltinHrtf::id)
            .collect::<Vec<_>>();
        assert_eq!(preset_ids, ["sadie-ii-d1-ku100", "sadie-ii-d2-kemar"]);
        assert!(BuiltinHrtf::from_id("aachen-high-resolution-kemar").is_none());
        for preset in BUILTIN_HRTF_REGISTRY {
            let metadata = preset.metadata();
            let asset_metadata = preset.asset_metadata();
            assert_eq!(asset_metadata.asset_format_version, 2);
            assert!(!asset_metadata.authors_institution.is_empty());
            assert_eq!(metadata.id, preset.id());
            assert_eq!(asset_metadata.preset_id, preset.id());
        }
    }

    #[test]
    fn packed_asset_round_trip_preserves_directions_delays_and_taps() {
        let preset = BuiltinHrtf::SadieD1Ku100;
        let source = load_builtin_hrir(preset).expect("default built-in asset");
        assert!(pack_builtin_hrir_asset(BuiltinHrtf::SadieD2Kemar, &source).is_err());
        let regenerated = pack_builtin_hrir_asset(preset, &source).expect("packed asset");
        let payload = decode_asset_payload(preset, &regenerated).expect("versioned payload");
        let restored = parse_packed_hrtf_payload(preset, payload).expect("restored packed bank");
        assert_eq!(restored.bank.entries().len(), source.bank.entries().len());
        for (original, packed) in source.bank.entries().iter().zip(restored.bank.entries()) {
            let dot = original
                .direction()
                .iter()
                .zip(packed.direction())
                .map(|(left, right)| left * right)
                .sum::<f64>();
            assert!(dot > 1.0 - 1.0e-12);
            assert_eq!(original.pair().left_taps(), packed.pair().left_taps());
            assert_eq!(original.pair().right_taps(), packed.pair().right_taps());
            assert_eq!(
                original.pair().delay_samples(HrirEar::Left),
                packed.pair().delay_samples(HrirEar::Left)
            );
            assert_eq!(
                original.pair().delay_samples(HrirEar::Right),
                packed.pair().delay_samples(HrirEar::Right)
            );
        }
    }

    #[test]
    fn packer_rejects_a_bank_row_with_the_wrong_tap_length() {
        let preset = BuiltinHrtf::SadieD1Ku100;
        let loaded = load_builtin_hrir(preset).expect("D1 asset");
        let mut entries = loaded.bank.entries().to_vec();
        let original = &entries[0];
        let direction = original.direction();
        entries[0] = HrirEntry::new(
            original.id(),
            CartesianPosition::new(direction[0], direction[1], direction[2]),
            HrirPair::new(48_000, vec![0.0; 255], vec![0.0; 255]).expect("short pair"),
        )
        .expect("short entry");
        let malformed = LoadedSofaHrirBank {
            bank: HrirBank::new(48_000, entries).expect("shape-valid bank"),
            metadata: loaded.metadata.clone(),
        };

        assert!(pack_builtin_hrir_asset(preset, &malformed).is_err());
    }

    fn asset(preset_code: u32, version: u32, payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(56 + payload.len());
        bytes.extend_from_slice(b"OJHRTF2\0");
        bytes.extend_from_slice(&version.to_le_bytes());
        bytes.extend_from_slice(&preset_code.to_le_bytes());
        bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&Sha256::digest(payload));
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn versioned_asset_checks_preset_length_and_payload_integrity() {
        let payload = b"OJRIR2\0\0openjoc-fixture";
        let valid = asset(1, 2, payload);
        assert_eq!(
            decode_asset_payload(BuiltinHrtf::SadieD1Ku100, &valid).expect("valid asset"),
            payload
        );
        assert!(decode_asset_payload(BuiltinHrtf::SadieD2Kemar, &valid).is_err());

        let mut corrupt = valid.clone();
        *corrupt.last_mut().expect("payload") ^= 0x01;
        assert!(decode_asset_payload(BuiltinHrtf::SadieD1Ku100, &corrupt).is_err());

        assert!(decode_asset_payload(BuiltinHrtf::SadieD1Ku100, &asset(1, 3, payload)).is_err());
        let mut truncated = valid;
        truncated.pop();
        assert!(decode_asset_payload(BuiltinHrtf::SadieD1Ku100, &truncated).is_err());
    }

    #[test]
    fn direct_payload_rejects_bad_metadata_nonfinite_values_and_truncation() {
        let preset = BuiltinHrtf::SadieD1Ku100;
        let bank = load_builtin_hrir(preset).expect("checked-in direct asset");
        let asset = pack_builtin_hrir_asset(preset, &bank).expect("re-encoded direct asset");
        let payload = decode_asset_payload(preset, &asset).expect("direct payload");

        let mut invalid_coordinate_code = payload.to_vec();
        invalid_coordinate_code[20..24].copy_from_slice(&99_u32.to_le_bytes());
        assert!(parse_packed_hrtf_payload(preset, &invalid_coordinate_code).is_err());

        let mut non_unit_direction = payload.to_vec();
        non_unit_direction[40..48].copy_from_slice(&f64::NAN.to_le_bytes());
        assert!(parse_packed_hrtf_payload(preset, &non_unit_direction).is_err());

        let mut oversized_record = payload.to_vec();
        oversized_record[64..68].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse_packed_hrtf_payload(preset, &oversized_record).is_err());

        let mut invalid_delay = payload.to_vec();
        invalid_delay[68..72].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse_packed_hrtf_payload(preset, &invalid_delay).is_err());

        let mut nonfinite_tap = payload.to_vec();
        nonfinite_tap[76..80].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(parse_packed_hrtf_payload(preset, &nonfinite_tap).is_err());

        assert!(parse_packed_hrtf_payload(preset, &payload[..payload.len() - 1]).is_err());
        let mut trailing_data = payload.to_vec();
        trailing_data.push(0);
        assert!(parse_packed_hrtf_payload(preset, &trailing_data).is_err());
    }
}
