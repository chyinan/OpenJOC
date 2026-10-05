//! Strict, read-only ingestion for the `SimpleFreeFieldHRIR` SOFA subset.
//!
//! The reader accepts a bounded SimpleFreeFieldHRIR subset from NetCDF classic
//! CDF-1 or NetCDF-4/HDF5 containers. It intentionally does not implement a
//! general-purpose SOFA or HDF5 API.

// pattern: Mixed (unavoidable)
// Reason: the public adapter retains its thin local-file loader alongside the
// in-memory parser and pure HRIR operations; rendering performs no file I/O.
use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
    fmt, fs,
    path::Path,
};

use openjoc_render::{CartesianPosition, HrirBank, HrirEar, HrirEntry, HrirEntryId, HrirPair};

mod builtin_hrtf;

pub use builtin_hrtf::{
    BUILTIN_HRTF_REGISTRY, BuiltinHrirF32Bank, BuiltinHrtf, BuiltinHrtfAssetMetadata,
    BuiltinHrtfMetadata, LoadedBuiltinHrirF32Bank, builtin_hrtf_asset_bytes, load_builtin_hrir,
    load_builtin_hrir_f32, load_builtin_hrir_f32_from_asset, load_builtin_hrir_from_asset,
    pack_builtin_hrir_asset,
};

const MAX_COORDINATE_TOLERANCE: f64 = 1.0e-9;
const NC_DIMENSION_TAG: u32 = 10;
const NC_ATTRIBUTE_TAG: u32 = 12;
const NC_VARIABLE_TAG: u32 = 11;
const NC_BYTE: u32 = 1;
const NC_CHAR: u32 = 2;
const NC_SHORT: u32 = 3;
const NC_INT: u32 = 4;
const NC_FLOAT: u32 = 5;
const NC_DOUBLE: u32 = 6;
const MAX_HRIR_RATE_CONVERSION_RATIO: u32 = 16;
const HRIR_RESAMPLER_LOBES: u128 = 16;
const MAX_HRIR_RESAMPLER_PHASES: usize = 4096;

/// Resource limits applied before any large allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SofaLoadLimits {
    /// Maximum input file bytes.
    pub max_file_bytes: u64,
    /// Maximum number of measurements.
    pub max_measurements: usize,
    /// Maximum FIR taps per measurement and receiver.
    pub max_fir_samples: usize,
    /// Maximum integer delay in samples.
    pub max_delay_samples: usize,
    /// Maximum expanded FIR coefficients across the whole bank.
    pub max_total_coefficients: usize,
    /// Maximum bytes retained by one attribute string.
    pub max_metadata_bytes: usize,
}

impl Default for SofaLoadLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 64 * 1024 * 1024,
            max_measurements: 4096,
            max_fir_samples: 65_536,
            max_delay_samples: 1_048_576,
            max_total_coefficients: 268_435_456,
            max_metadata_bytes: 1_048_576,
        }
    }
}

/// Metadata deliberately kept small and stable by the loader API.
#[derive(Clone, Debug, PartialEq)]
pub struct SofaHrirMetadata {
    pub convention_version: String,
    pub title: Option<String>,
    pub database_name: Option<String>,
    pub listener_short_name: Option<String>,
    pub license: Option<String>,
    pub measurement_count: usize,
    pub original_fir_length: usize,
    pub expanded_max_tap_length: usize,
    pub sample_rate_hz: u32,
}

/// A validated SOFA file converted into the renderer's exact-direction bank.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadedSofaHrirBank {
    pub bank: HrirBank,
    pub metadata: SofaHrirMetadata,
}

/// Official dataset identity for the offline default renderer resource.
pub const BUILTIN_GENERIC_HRTF_DATASET: &str = "SADIE II D1 (KU100), v2-2";
/// The built-in resource is the official D1 48 kHz, 256-tap HRIR set packed
/// into the versioned direct-record asset; Custom SOFA remains the supported
/// `SimpleFreeFieldHRIR` import format in CDF-1 or NetCDF-4/HDF5 containers.
pub const BUILTIN_GENERIC_HRTF_SAMPLE_RATE_HZ: u32 = 48_000;
pub const BUILTIN_GENERIC_HRTF_TAP_COUNT: usize = 256;

/// Compatibility wrapper for the original single built-in API.
pub fn load_builtin_generic_hrir() -> Result<LoadedSofaHrirBank, SofaError> {
    load_builtin_hrir(BuiltinHrtf::SadieD1Ku100)
}

/// Result of resolving one virtual-speaker direction against a SOFA bank.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedHrir {
    /// The exact or prepared interpolated pair for the requested direction.
    pub pair: HrirPair,
    /// The source entry identity when the exact path was selected.
    pub exact_entry: Option<HrirEntryId>,
    /// Number of source measurements used for an interpolated pair.
    pub neighbor_count: usize,
}

/// Typed failures from the narrow SOFA ingestion boundary.
#[derive(Debug, PartialEq)]
pub enum SofaError {
    Io(String),
    UnsupportedContainerOrEncoding,
    TruncatedContainer,
    InvalidContainer(&'static str),
    UnsupportedSofaConvention(String),
    UnsupportedSofaConventionVersion(String),
    MissingAttribute(&'static str),
    MissingVariable(&'static str),
    InvalidDimension(String),
    InvalidCoordinate(String),
    InvalidReceiverGeometry,
    InvalidSamplingRate(String),
    UnsupportedFractionalSofaDelay {
        measurement: usize,
        receiver: usize,
        value: f64,
    },
    InvalidImpulseResponse(String),
    InvalidBuiltinHrtfAsset(&'static str),
    InsufficientInterpolationData {
        required: usize,
        available: usize,
    },
    InterpolationOutsideCoverage(String),
    DegenerateInterpolationGeometry,
    InvalidInterpolationResult(String),
    DuplicateDirection {
        first: usize,
        second: usize,
    },
    ResourceLimitExceeded(&'static str),
    UnsupportedAttributeType(String),
}

impl fmt::Display for SofaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) => write!(f, "SOFA I/O error: {message}"),
            Self::UnsupportedContainerOrEncoding => f.write_str(
                "unsupported SOFA container: expected NetCDF classic CDF-1 or NetCDF-4/HDF5",
            ),
            Self::TruncatedContainer => f.write_str("truncated SOFA container"),
            Self::InvalidContainer(message) => write!(f, "invalid SOFA container: {message}"),
            Self::UnsupportedSofaConvention(value) => {
                write!(f, "unsupported SOFA convention: {value}")
            }
            Self::UnsupportedSofaConventionVersion(value) => {
                write!(f, "unsupported SOFA convention version: {value}")
            }
            Self::MissingAttribute(name) => write!(f, "missing SOFA attribute: {name}"),
            Self::MissingVariable(name) => write!(f, "missing SOFA variable: {name}"),
            Self::InvalidDimension(message) => write!(f, "invalid SOFA dimension: {message}"),
            Self::InvalidCoordinate(message) => write!(f, "invalid SOFA coordinate: {message}"),
            Self::InvalidReceiverGeometry => f.write_str("invalid or ambiguous receiver geometry"),
            Self::InvalidSamplingRate(message) => {
                write!(f, "invalid SOFA sampling rate: {message}")
            }
            Self::UnsupportedFractionalSofaDelay {
                measurement,
                receiver,
                value,
            } => write!(
                f,
                "fractional SOFA delay at measurement {measurement}, receiver {receiver}: {value}"
            ),
            Self::InvalidImpulseResponse(message) => {
                write!(f, "invalid SOFA impulse response: {message}")
            }
            Self::InvalidBuiltinHrtfAsset(message) => {
                write!(f, "invalid built-in HRTF asset: {message}")
            }
            Self::InsufficientInterpolationData {
                required,
                available,
            } => write!(
                f,
                "SOFA interpolation needs at least {required} local measurements; only {available} are available"
            ),
            Self::InterpolationOutsideCoverage(message) => {
                write!(
                    f,
                    "SOFA interpolation request is outside measured coverage: {message}"
                )
            }
            Self::DegenerateInterpolationGeometry => {
                f.write_str("SOFA interpolation neighborhood is geometrically degenerate")
            }
            Self::InvalidInterpolationResult(message) => {
                write!(f, "invalid SOFA interpolation result: {message}")
            }
            Self::DuplicateDirection { first, second } => {
                write!(
                    f,
                    "duplicate SOFA directions at measurements {first} and {second}"
                )
            }
            Self::ResourceLimitExceeded(name) => write!(f, "SOFA resource limit exceeded: {name}"),
            Self::UnsupportedAttributeType(name) => {
                write!(f, "unsupported SOFA attribute type: {name}")
            }
        }
    }
}

impl std::error::Error for SofaError {}

/// Loads a bounded local CDF-1 or NetCDF-4/HDF5 SOFA file.
pub fn load_simple_free_field_hrir<P: AsRef<Path>>(
    path: P,
    limits: SofaLoadLimits,
) -> Result<LoadedSofaHrirBank, SofaError> {
    let path = path.as_ref();
    let size = fs::metadata(path)
        .map_err(|error| SofaError::Io(error.to_string()))?
        .len();
    if size > limits.max_file_bytes {
        return Err(SofaError::ResourceLimitExceeded("file bytes"));
    }
    let data = fs::read(path).map_err(|error| SofaError::Io(error.to_string()))?;
    parse_simple_free_field_hrir(&data, limits)
}

/// Returns the common causal filter delay added by one HRIR rate conversion.
///
/// This is in target samples and excludes the SOFA's measured source delays.
/// Callers mixing an unfiltered LFE path must delay it by this amount as well.
/// Equal nonzero rates return zero. Conversion ratios are limited to 16:1.
pub fn hrir_resampling_delay_samples(
    source_sample_rate_hz: u32,
    target_sample_rate_hz: u32,
) -> Result<usize, SofaError> {
    if source_sample_rate_hz == 0 || target_sample_rate_hz == 0 {
        return Err(SofaError::InvalidSamplingRate("rate is zero".to_owned()));
    }
    if source_sample_rate_hz == target_sample_rate_hz {
        return Ok(0);
    }
    let larger_rate = source_sample_rate_hz.max(target_sample_rate_hz);
    let smaller_rate = source_sample_rate_hz.min(target_sample_rate_hz);
    if u64::from(larger_rate) > u64::from(smaller_rate) * u64::from(MAX_HRIR_RATE_CONVERSION_RATIO)
    {
        return Err(SofaError::InvalidSamplingRate(
            "sample-rate conversion ratio exceeds 16:1".to_owned(),
        ));
    }
    usize::try_from(
        (HRIR_RESAMPLER_LOBES * u128::from(larger_rate))
            .div_ceil(u128::from(source_sample_rate_hz))
            + 1,
    )
    .map_err(|_| SofaError::ResourceLimitExceeded("resampling filter delay"))
}

/// Resamples an HRIR bank while preserving its convolution gain.
///
/// Matching-rate banks are returned unchanged, including every coefficient bit.
/// Other banks include a common causal filter delay of
/// `ceil(16 * max(target_rate / source_rate, 1)) + 1` target samples. This keeps
/// the complete sinc precursor, including for a nonzero first input tap. The
/// extra sample also keeps every tap before the rounded source-delay metadata
/// zero, so spatial interpolation can safely remove that prefix. The common
/// filter delay stays in the FIR shape, not in the measured delay metadata.
/// SOFA metadata retains the source rate; the bank records the target rate.
pub fn resample_loaded_hrir_bank(
    mut loaded: LoadedSofaHrirBank,
    target_sample_rate_hz: u32,
    limits: SofaLoadLimits,
) -> Result<LoadedSofaHrirBank, SofaError> {
    let source_sample_rate_hz = loaded.bank.sample_rate_hz();
    let filter_delay = hrir_resampling_delay_samples(source_sample_rate_hz, target_sample_rate_hz)?;
    if filter_delay == 0 {
        return Ok(loaded);
    }
    let divisor = gcd(source_sample_rate_hz, target_sample_rate_hz);
    let source_step = u64::from(source_sample_rate_hz / divisor);
    let target_step = u64::from(target_sample_rate_hz / divisor);
    let cutoff = (target_sample_rate_hz as f64 / source_sample_rate_hz as f64).min(1.0);
    let radius = HRIR_RESAMPLER_LOBES as f64 / cutoff;
    let half_width = usize::try_from(
        (HRIR_RESAMPLER_LOBES * u128::from(target_step.max(source_step)))
            .div_ceil(u128::from(target_step)),
    )
    .map_err(|_| SofaError::ResourceLimitExceeded("resampling kernel"))?;
    let phases = (usize::try_from(target_step).ok())
        .filter(|count| *count <= MAX_HRIR_RESAMPLER_PHASES)
        .map(|count| {
            (0..count)
                .map(|phase| {
                    resampler_phase_weights(phase as f64 / count as f64, cutoff, radius, half_width)
                })
                .collect::<Vec<_>>()
        });
    let mut entries = Vec::with_capacity(loaded.bank.entries().len());
    let mut total_coefficients = 0_usize;
    let expanded_tap_limit = limits
        .max_fir_samples
        .checked_add(limits.max_delay_samples)
        .ok_or(SofaError::ResourceLimitExceeded("expanded taps"))?;
    for entry in loaded.bank.entries() {
        let pair = entry.pair();
        let output_len = resampled_tap_count(pair.tap_count(), target_step, source_step)?
            .checked_add(filter_delay)
            .ok_or(SofaError::ResourceLimitExceeded("resampled FIR samples"))?;
        if output_len > expanded_tap_limit {
            return Err(SofaError::ResourceLimitExceeded("resampled FIR samples"));
        }
        let delays = [
            resampled_delay(
                pair.delay_samples(HrirEar::Left),
                target_sample_rate_hz,
                source_sample_rate_hz,
                limits,
            )?,
            resampled_delay(
                pair.delay_samples(HrirEar::Right),
                target_sample_rate_hz,
                source_sample_rate_hz,
                limits,
            )?,
        ];
        if delays.into_iter().any(|delay| delay > output_len) {
            return Err(SofaError::ResourceLimitExceeded("resampled delay samples"));
        }
        let pair_coefficients =
            output_len
                .checked_mul(2)
                .ok_or(SofaError::ResourceLimitExceeded(
                    "resampled FIR coefficients",
                ))?;
        total_coefficients = total_coefficients.checked_add(pair_coefficients).ok_or(
            SofaError::ResourceLimitExceeded("resampled FIR coefficients"),
        )?;
        if total_coefficients > limits.max_total_coefficients {
            return Err(SofaError::ResourceLimitExceeded(
                "resampled FIR coefficients",
            ));
        }
        let left = resample_fir(
            pair.left_taps(),
            output_len,
            target_step,
            source_step,
            cutoff,
            half_width,
            filter_delay,
            phases.as_deref(),
        )?;
        let right = resample_fir(
            pair.right_taps(),
            output_len,
            target_step,
            source_step,
            cutoff,
            half_width,
            filter_delay,
            phases.as_deref(),
        )?;
        let pair = HrirPair::new_with_delays(target_sample_rate_hz, left, right, delays)
            .map_err(|error| SofaError::InvalidImpulseResponse(error.to_string()))?;
        let direction = entry.direction();
        entries.push(
            HrirEntry::new(
                entry.id(),
                CartesianPosition::new(direction[0], direction[1], direction[2]),
                pair,
            )
            .map_err(|error| SofaError::InvalidCoordinate(error.to_string()))?,
        );
    }
    loaded.bank = HrirBank::new(target_sample_rate_hz, entries)
        .map_err(|error| SofaError::InvalidImpulseResponse(error.to_string()))?;
    loaded.metadata.expanded_max_tap_length = loaded
        .bank
        .entries()
        .iter()
        .map(|entry| entry.pair().tap_count())
        .max()
        .unwrap_or(0);
    Ok(loaded)
}

fn gcd(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

fn resampled_tap_count(
    input_samples: usize,
    target_step: u64,
    source_step: u64,
) -> Result<usize, SofaError> {
    if input_samples == 0 || source_step == 0 {
        return Err(SofaError::InvalidImpulseResponse(
            "empty input HRIR".to_owned(),
        ));
    }
    let input_last = u128::try_from(input_samples - 1)
        .map_err(|_| SofaError::ResourceLimitExceeded("resampled FIR samples"))?;
    let target = u128::from(target_step);
    let source = u128::from(source_step);
    let output_filter_tail = HRIR_RESAMPLER_LOBES * target.max(source);
    let last_center_numerator = input_last
        .checked_mul(target)
        .and_then(|length| length.checked_add(output_filter_tail))
        .ok_or(SofaError::ResourceLimitExceeded("resampled FIR samples"))?;
    let count = last_center_numerator / source + 1;
    usize::try_from(count).map_err(|_| SofaError::ResourceLimitExceeded("resampled FIR samples"))
}

fn resampled_delay(
    delay: usize,
    target_sample_rate_hz: u32,
    source_sample_rate_hz: u32,
    limits: SofaLoadLimits,
) -> Result<usize, SofaError> {
    if delay > limits.max_delay_samples {
        return Err(SofaError::ResourceLimitExceeded("delay samples"));
    }
    let numerator = (delay as u128)
        .checked_mul(u128::from(target_sample_rate_hz))
        .and_then(|value| value.checked_add(u128::from(source_sample_rate_hz / 2)))
        .ok_or(SofaError::ResourceLimitExceeded("resampled delay samples"))?;
    let resampled = usize::try_from(numerator / u128::from(source_sample_rate_hz))
        .map_err(|_| SofaError::ResourceLimitExceeded("resampled delay samples"))?;
    if resampled > limits.max_delay_samples {
        return Err(SofaError::ResourceLimitExceeded("resampled delay samples"));
    }
    Ok(resampled)
}

#[allow(clippy::too_many_arguments)]
fn resample_fir(
    input: &[f64],
    output_len: usize,
    target_step: u64,
    source_step: u64,
    cutoff: f64,
    half_width: usize,
    filter_delay: usize,
    phases: Option<&[Vec<f64>]>,
) -> Result<Vec<f64>, SofaError> {
    let mut output = Vec::with_capacity(output_len);
    // Audio sample interpolation preserves amplitude, whereas FIR resampling
    // must preserve the discrete convolution gain (the sum of coefficients).
    let fir_scale = source_step as f64 / target_step as f64;
    for output_index in 0..output_len {
        let position_numerator = (output_index as i128 - filter_delay as i128)
            .checked_mul(i128::from(source_step))
            .ok_or(SofaError::ResourceLimitExceeded("resampling phase"))?;
        // Negative positions hold the precursor. Euclidean division keeps
        // the fractional phase in [0, 1), including before source sample zero.
        let base = position_numerator.div_euclid(i128::from(target_step));
        let phase_index = usize::try_from(position_numerator.rem_euclid(i128::from(target_step)))
            .map_err(|_| SofaError::ResourceLimitExceeded("resampling phase"))?;
        let phase = phase_index as f64 / target_step as f64;
        let temporary;
        let weights = if let Some(phases) = phases {
            phases
                .get(phase_index)
                .ok_or(SofaError::InvalidImpulseResponse(
                    "resampling phase table".to_owned(),
                ))?
        } else {
            temporary = resampler_phase_weights(
                phase,
                cutoff,
                HRIR_RESAMPLER_LOBES as f64 / cutoff,
                half_width,
            );
            &temporary
        };
        let mut sum = 0.0_f64;
        for (kernel_index, weight) in weights.iter().enumerate() {
            let offset = kernel_index as i128 - half_width as i128;
            if let Ok(input_index) = usize::try_from(base + offset) {
                if let Some(value) = input.get(input_index) {
                    sum += value * weight;
                }
            }
        }
        sum *= fir_scale;
        if !sum.is_finite() {
            return Err(SofaError::InvalidImpulseResponse(
                "non-finite resampled tap".to_owned(),
            ));
        }
        output.push(sum);
    }
    Ok(output)
}

fn resampler_phase_weights(phase: f64, cutoff: f64, radius: f64, half_width: usize) -> Vec<f64> {
    let mut weights = Vec::with_capacity(half_width.saturating_mul(2).saturating_add(1));
    let mut sum = 0.0;
    for offset in -(half_width as i128)..=(half_width as i128) {
        let distance = phase - offset as f64;
        let scaled_distance = cutoff * distance;
        let sinc = if scaled_distance == 0.0 {
            1.0
        } else {
            let angle = std::f64::consts::PI * scaled_distance;
            angle.sin() / angle
        };
        let window = if distance.abs() <= radius {
            let window_position = distance / radius;
            1.0_f64.midpoint((std::f64::consts::PI * window_position).cos())
        } else {
            0.0
        };
        let weight = cutoff * sinc * window;
        weights.push(weight);
        sum += weight;
    }
    if sum.is_finite() && sum.abs() > f64::EPSILON {
        for weight in &mut weights {
            *weight /= sum;
        }
    }
    weights
}

/// Parses a complete in-memory CDF-1 or NetCDF-4/HDF5 buffer.
///
/// The parser admits only the supported SimpleFreeFieldHRIR field profile,
/// regardless of the underlying container.
pub fn parse_simple_free_field_hrir(
    data: &[u8],
    limits: SofaLoadLimits,
) -> Result<LoadedSofaHrirBank, SofaError> {
    if data.len() as u64 > limits.max_file_bytes {
        return Err(SofaError::ResourceLimitExceeded("file bytes"));
    }
    let file = if data.starts_with(b"CDF") {
        NetcdfFile::parse(data, limits)?
    } else if hdf5_pure::is_hdf5_bytes(data) {
        let hdf5 = hdf5_pure::File::from_bytes(data.to_vec())
            .map_err(|_| SofaError::InvalidContainer("HDF5 file image"))?;
        NetcdfFile::from_hdf5(data, &hdf5, limits)?
    } else {
        return Err(SofaError::UnsupportedContainerOrEncoding);
    };
    validate_and_build(&file, limits)
}

#[derive(Clone, Debug)]
struct Dimension {
    len: usize,
}

#[derive(Clone, Debug)]
enum AttributeValue {
    Text(String),
    Numbers,
}

#[derive(Clone, Debug)]
struct Attribute {
    name: String,
    value: AttributeValue,
}

#[derive(Clone, Debug)]
struct Variable {
    name: String,
    dims: Vec<usize>,
    attrs: Vec<Attribute>,
    ty: u32,
    begin: usize,
    elements: usize,
    bytes: usize,
    hdf5_dataset: Option<hdf5_pure::Dataset>,
}

#[derive(Clone, Debug)]
struct NetcdfFile<'a> {
    data: &'a [u8],
    dimensions: Vec<Dimension>,
    globals: Vec<Attribute>,
    variables: Vec<Variable>,
}

impl<'a> NetcdfFile<'a> {
    fn parse(data: &'a [u8], limits: SofaLoadLimits) -> Result<Self, SofaError> {
        if data.len() < 8 {
            return Err(SofaError::TruncatedContainer);
        }
        if &data[..4] != b"CDF\x01" {
            return Err(SofaError::UnsupportedContainerOrEncoding);
        }
        let mut cursor = Cursor::new(data);
        cursor.skip(4)?;
        let record_count = cursor.u32()?;
        if record_count != 0 {
            return Err(SofaError::InvalidContainer(
                "record dimensions are unsupported",
            ));
        }
        let dimensions = parse_dimensions(&mut cursor)?;
        let globals = parse_attributes(&mut cursor, limits.max_metadata_bytes)?;
        let variables = parse_variables(&mut cursor, &dimensions, limits)?;
        for variable in &variables {
            let end = variable
                .begin
                .checked_add(variable.bytes)
                .ok_or(SofaError::ResourceLimitExceeded("variable byte range"))?;
            if end > data.len() {
                return Err(SofaError::TruncatedContainer);
            }
        }
        Ok(Self {
            data,
            dimensions,
            globals,
            variables,
        })
    }

    fn from_hdf5(
        data: &'a [u8],
        hdf5: &hdf5_pure::File,
        limits: SofaLoadLimits,
    ) -> Result<Self, SofaError> {
        let root = hdf5.root();
        let globals = read_hdf5_attributes(
            root.attrs()
                .map_err(|_| SofaError::InvalidContainer("HDF5 root attributes"))?,
            limits,
        )?;
        let mut dimensions = Vec::new();
        let mut variables = Vec::new();
        for name in [
            "Data.IR",
            "Data.SamplingRate",
            "Data.Delay",
            "ListenerPosition",
            "ListenerView",
            "ListenerUp",
            "ReceiverPosition",
            "SourcePosition",
            "EmitterPosition",
        ] {
            let candidate_paths: &[&str] = match name {
                "Data.IR" => &["Data.IR", "Data/IR"],
                "Data.SamplingRate" => &["Data.SamplingRate", "Data/SamplingRate"],
                "Data.Delay" => &["Data.Delay", "Data/Delay"],
                "ListenerPosition" => &["ListenerPosition", "Listener/Position"],
                "ListenerView" => &["ListenerView", "Listener/View"],
                "ListenerUp" => &["ListenerUp", "Listener/Up"],
                "ReceiverPosition" => &["ReceiverPosition", "Receiver/Position"],
                "SourcePosition" => &["SourcePosition", "Source/Position"],
                "EmitterPosition" => &["EmitterPosition", "Emitter/Position"],
                _ => &[],
            };
            let mut resolved = None;
            for path in candidate_paths.iter().copied() {
                if let Ok(dataset) = hdf5.dataset(path) {
                    if resolved.is_some() {
                        return Err(SofaError::InvalidContainer(
                            "duplicate HDF5 variable aliases",
                        ));
                    }
                    resolved = Some((path, dataset));
                }
            }
            let Some((_path, dataset)) = resolved else {
                continue;
            };
            let shape = dataset
                .shape()
                .map_err(|_| SofaError::InvalidContainer("HDF5 dataset dimensions"))?;
            if shape.len() > 16 {
                return Err(SofaError::ResourceLimitExceeded("dimensions"));
            }
            let elements = shape.iter().try_fold(1_usize, |total, &dimension| {
                let dimension = usize::try_from(dimension)
                    .map_err(|_| SofaError::ResourceLimitExceeded("dimension product"))?;
                total
                    .checked_mul(dimension)
                    .ok_or(SofaError::ResourceLimitExceeded("dimension product"))
            })?;
            if elements > limits.max_total_coefficients {
                return Err(SofaError::ResourceLimitExceeded("variable values"));
            }
            let ty = match dataset
                .dtype()
                .map_err(|_| SofaError::InvalidContainer("HDF5 dataset datatype"))?
            {
                hdf5_pure::DType::F32 => NC_FLOAT,
                hdf5_pure::DType::F64 => NC_DOUBLE,
                _ => return Err(SofaError::UnsupportedAttributeType(name.to_owned())),
            };
            // A compressed chunk may be much larger than the dataset's current
            // extent. Row reads still decompress that entire chunk, so cap its
            // allocation independently of the logical shape before reading data.
            if let Some(chunk_shape) = dataset
                .chunk_shape()
                .map_err(|_| SofaError::InvalidContainer("HDF5 chunk dimensions"))?
            {
                let width = type_width(ty).expect("validated floating-point type") as u64;
                let chunk_bytes = chunk_shape.iter().try_fold(width, |bytes, dimension| {
                    bytes
                        .checked_mul(*dimension)
                        .ok_or(SofaError::ResourceLimitExceeded("HDF5 chunk bytes"))
                })?;
                let coefficient_bytes =
                    (limits.max_total_coefficients as u64).saturating_mul(width);
                let chunk_limit = limits
                    .max_file_bytes
                    .min(coefficient_bytes)
                    .min(16 * 1024 * 1024);
                if chunk_bytes > chunk_limit {
                    return Err(SofaError::ResourceLimitExceeded("HDF5 chunk bytes"));
                }
            }
            let attrs = read_hdf5_attributes(
                dataset
                    .attrs()
                    .map_err(|_| SofaError::InvalidContainer("HDF5 variable attributes"))?,
                limits,
            )?;
            let mut dims = Vec::with_capacity(shape.len());
            for length in shape {
                dims.push(dimensions.len());
                dimensions.push(Dimension {
                    len: usize::try_from(length)
                        .map_err(|_| SofaError::ResourceLimitExceeded("dimension product"))?,
                });
            }
            let bytes = elements
                .checked_mul(
                    type_width(ty)
                        .ok_or_else(|| SofaError::UnsupportedAttributeType(name.to_owned()))?,
                )
                .ok_or(SofaError::ResourceLimitExceeded("variable bytes"))?;
            variables.push(Variable {
                name: name.to_owned(),
                dims,
                attrs,
                ty,
                begin: 0,
                elements,
                bytes,
                hdf5_dataset: Some(dataset),
            });
        }
        Ok(Self {
            data,
            dimensions,
            globals,
            variables,
        })
    }

    fn variable(&self, name: &'static str) -> Result<&Variable, SofaError> {
        self.variables
            .iter()
            .find(|v| v.name == name)
            .ok_or(SofaError::MissingVariable(name))
    }

    fn global_text(&self, name: &'static str) -> Result<String, SofaError> {
        self.globals
            .iter()
            .find(|a| a.name == name)
            .and_then(|a| match &a.value {
                AttributeValue::Text(value) => Some(value.clone()),
                AttributeValue::Numbers => None,
            })
            .ok_or(SofaError::MissingAttribute(name))
    }

    fn attr_text(attrs: &[Attribute], name: &'static str) -> Option<String> {
        attrs
            .iter()
            .find(|a| a.name == name)
            .and_then(|a| match &a.value {
                AttributeValue::Text(value) => Some(value.clone()),
                AttributeValue::Numbers => None,
            })
    }

    fn values(&self, variable: &Variable) -> Result<Vec<f64>, SofaError> {
        if let Some(dataset) = &variable.hdf5_dataset {
            return dataset
                .read_f64()
                .map_err(|_| SofaError::InvalidContainer("HDF5 variable data"));
        }
        let bytes = &self.data[variable.begin..variable.begin + variable.bytes];
        let width = type_width(variable.ty)
            .ok_or(SofaError::UnsupportedAttributeType(variable.name.clone()))?;
        let expected = variable
            .elements
            .checked_mul(width)
            .ok_or(SofaError::ResourceLimitExceeded("variable values"))?;
        if bytes.len() < expected {
            return Err(SofaError::TruncatedContainer);
        }
        let mut values = Vec::with_capacity(variable.elements);
        for chunk in bytes[..expected].chunks_exact(width) {
            values.push(netcdf_number(variable.ty, chunk, &variable.name)?);
        }
        Ok(values)
    }

    fn copy_values_into(
        &self,
        variable: &Variable,
        first_value: usize,
        output: &mut [f64],
    ) -> Result<(), SofaError> {
        if let Some(dataset) = &variable.hdf5_dataset {
            let shape = self.shape(variable);
            if shape.len() != 3 || output.is_empty() {
                return Err(SofaError::InvalidDimension(
                    "Data.IR must have shape [M,R,N]".to_string(),
                ));
            }
            let row_len = shape[1]
                .checked_mul(shape[2])
                .ok_or(SofaError::ResourceLimitExceeded("variable values"))?;
            let row = first_value / row_len;
            let within_row = first_value % row_len;
            let end = within_row
                .checked_add(output.len())
                .ok_or(SofaError::ResourceLimitExceeded("variable values"))?;
            if end > row_len {
                return Err(SofaError::TruncatedContainer);
            }
            let row_values = dataset
                .read_f64_rows(
                    u64::try_from(row)
                        .map_err(|_| SofaError::ResourceLimitExceeded("HDF5 row index"))?,
                    1,
                )
                .map_err(|_| SofaError::InvalidContainer("HDF5 variable data"))?;
            output.copy_from_slice(
                row_values
                    .get(within_row..end)
                    .ok_or(SofaError::TruncatedContainer)?,
            );
            return Ok(());
        }
        let width = type_width(variable.ty)
            .ok_or(SofaError::UnsupportedAttributeType(variable.name.clone()))?;
        let byte_start = first_value
            .checked_mul(width)
            .ok_or(SofaError::ResourceLimitExceeded("variable values"))?;
        let byte_count = output
            .len()
            .checked_mul(width)
            .ok_or(SofaError::ResourceLimitExceeded("variable values"))?;
        let byte_end = byte_start
            .checked_add(byte_count)
            .ok_or(SofaError::ResourceLimitExceeded("variable values"))?;
        if byte_end > variable.bytes {
            return Err(SofaError::TruncatedContainer);
        }
        let begin = variable
            .begin
            .checked_add(byte_start)
            .ok_or(SofaError::TruncatedContainer)?;
        let end = begin
            .checked_add(byte_count)
            .ok_or(SofaError::TruncatedContainer)?;
        let bytes = self
            .data
            .get(begin..end)
            .ok_or(SofaError::TruncatedContainer)?;
        for (value, chunk) in output.iter_mut().zip(bytes.chunks_exact(width)) {
            *value = netcdf_number(variable.ty, chunk, &variable.name)?;
        }
        Ok(())
    }

    fn shape(&self, variable: &Variable) -> Vec<usize> {
        variable
            .dims
            .iter()
            .map(|index| self.dimensions[*index].len)
            .collect()
    }
}

fn read_hdf5_attributes(
    attrs: HashMap<String, hdf5_pure::AttrValue>,
    limits: SofaLoadLimits,
) -> Result<Vec<Attribute>, SofaError> {
    if attrs.len() > 65_536 {
        return Err(SofaError::ResourceLimitExceeded("HDF5 attributes"));
    }
    attrs
        .into_iter()
        .map(|(name, value)| {
            if name.len() > limits.max_metadata_bytes {
                return Err(SofaError::ResourceLimitExceeded("metadata bytes"));
            }
            let value = if let Some(text) = value.as_str() {
                if text.len() > limits.max_metadata_bytes {
                    return Err(SofaError::ResourceLimitExceeded("metadata bytes"));
                }
                AttributeValue::Text(text.to_owned())
            } else {
                AttributeValue::Numbers
            };
            Ok(Attribute { name, value })
        })
        .collect()
}

fn parse_dimensions(cursor: &mut Cursor<'_>) -> Result<Vec<Dimension>, SofaError> {
    let tag = cursor.u32()?;
    if tag == 0 {
        return Ok(Vec::new());
    }
    if tag != NC_DIMENSION_TAG {
        return Err(SofaError::InvalidContainer("dimension tag"));
    }
    let count = cursor.count()?;
    let mut dimensions = Vec::with_capacity(count);
    for _ in 0..count {
        let name = cursor.string(1 << 20)?;
        let len = cursor.u32()? as usize;
        if len == 0 {
            return Err(SofaError::InvalidDimension(name));
        }
        let _ = name;
        dimensions.push(Dimension { len });
    }
    Ok(dimensions)
}

fn parse_attributes(cursor: &mut Cursor<'_>, max_text: usize) -> Result<Vec<Attribute>, SofaError> {
    let tag = cursor.u32()?;
    if tag == 0 {
        return Ok(Vec::new());
    }
    if tag != NC_ATTRIBUTE_TAG {
        return Err(SofaError::InvalidContainer("attribute tag"));
    }
    let count = cursor.count()?;
    let mut attributes = Vec::with_capacity(count);
    for _ in 0..count {
        let name = cursor.string(max_text)?;
        let ty = cursor.u32()?;
        let count = cursor.count()?;
        let value = if ty == NC_CHAR {
            let bytes = cursor.bytes(count)?;
            if bytes.len() > max_text {
                return Err(SofaError::ResourceLimitExceeded("metadata bytes"));
            }
            let text = String::from_utf8(bytes.to_vec())
                .map_err(|_| SofaError::InvalidContainer("attribute text"))?;
            cursor.align4()?;
            AttributeValue::Text(text.trim_end_matches('\0').trim().to_string())
        } else {
            let width =
                type_width(ty).ok_or_else(|| SofaError::UnsupportedAttributeType(name.clone()))?;
            let total = count
                .checked_mul(width)
                .ok_or(SofaError::ResourceLimitExceeded("attribute values"))?;
            let bytes = cursor.bytes(total)?;
            let mut values = Vec::with_capacity(count);
            for chunk in bytes.chunks_exact(width) {
                values.push(match ty {
                    NC_BYTE => f64::from(i8::from_be_bytes([chunk[0]])),
                    NC_SHORT => f64::from(i16::from_be_bytes([chunk[0], chunk[1]])),
                    NC_INT => f64::from(i32::from_be_bytes(
                        chunk
                            .try_into()
                            .map_err(|_| SofaError::TruncatedContainer)?,
                    )),
                    NC_FLOAT => f64::from(f32::from_bits(u32::from_be_bytes(
                        chunk
                            .try_into()
                            .map_err(|_| SofaError::TruncatedContainer)?,
                    ))),
                    NC_DOUBLE => f64::from_bits(u64::from_be_bytes(
                        chunk
                            .try_into()
                            .map_err(|_| SofaError::TruncatedContainer)?,
                    )),
                    _ => return Err(SofaError::UnsupportedAttributeType(name.clone())),
                });
            }
            cursor.align4()?;
            let _ = values;
            AttributeValue::Numbers
        };
        attributes.push(Attribute { name, value });
    }
    Ok(attributes)
}

fn parse_variables(
    cursor: &mut Cursor<'_>,
    dimensions: &[Dimension],
    limits: SofaLoadLimits,
) -> Result<Vec<Variable>, SofaError> {
    let tag = cursor.u32()?;
    if tag == 0 {
        return Ok(Vec::new());
    }
    if tag != NC_VARIABLE_TAG {
        return Err(SofaError::InvalidContainer("variable tag"));
    }
    let count = cursor.count()?;
    let mut variables = Vec::with_capacity(count);
    for _ in 0..count {
        let name = cursor.string(limits.max_metadata_bytes)?;
        let rank = cursor.count()?;
        let mut dims = Vec::with_capacity(rank);
        let mut elements = 1usize;
        for _ in 0..rank {
            let dim = cursor.u32()? as usize;
            let dimension = dimensions
                .get(dim)
                .ok_or_else(|| SofaError::InvalidDimension(name.clone()))?;
            dims.push(dim);
            elements = elements
                .checked_mul(dimension.len)
                .ok_or(SofaError::ResourceLimitExceeded("dimension product"))?;
        }
        let attrs = parse_attributes(cursor, limits.max_metadata_bytes)?;
        let ty = cursor.u32()?;
        let width =
            type_width(ty).ok_or_else(|| SofaError::UnsupportedAttributeType(name.clone()))?;
        let vsize = cursor.u32()? as usize;
        let begin = cursor.u32()? as usize;
        let bytes = elements
            .checked_mul(width)
            .ok_or(SofaError::ResourceLimitExceeded("variable bytes"))?;
        if vsize < bytes {
            return Err(SofaError::TruncatedContainer);
        }
        variables.push(Variable {
            name,
            dims,
            attrs,
            ty,
            begin,
            elements,
            bytes: vsize,
            hdf5_dataset: None,
        });
    }
    Ok(variables)
}

fn type_width(ty: u32) -> Option<usize> {
    match ty {
        NC_BYTE | NC_CHAR => Some(1),
        NC_SHORT => Some(2),
        NC_INT | NC_FLOAT => Some(4),
        NC_DOUBLE => Some(8),
        _ => None,
    }
}

fn netcdf_number(ty: u32, bytes: &[u8], name: &str) -> Result<f64, SofaError> {
    match ty {
        NC_BYTE => Ok(f64::from(i8::from_be_bytes([bytes[0]]))),
        NC_SHORT => Ok(f64::from(i16::from_be_bytes([bytes[0], bytes[1]]))),
        NC_INT => Ok(f64::from(i32::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3],
        ]))),
        NC_FLOAT => Ok(f64::from(f32::from_bits(u32::from_be_bytes(
            bytes
                .try_into()
                .map_err(|_| SofaError::TruncatedContainer)?,
        )))),
        NC_DOUBLE => Ok(f64::from_bits(u64::from_be_bytes(
            bytes
                .try_into()
                .map_err(|_| SofaError::TruncatedContainer)?,
        ))),
        _ => Err(SofaError::UnsupportedAttributeType(name.to_owned())),
    }
}

fn validate_and_build(
    file: &NetcdfFile<'_>,
    limits: SofaLoadLimits,
) -> Result<LoadedSofaHrirBank, SofaError> {
    let conventions = file.global_text("Conventions")?;
    if !conventions
        .split(',')
        .any(|part| part.trim().eq_ignore_ascii_case("SOFA"))
    {
        return Err(SofaError::UnsupportedSofaConvention(conventions));
    }
    let sofa_convention = file.global_text("SOFAConventions")?;
    if !sofa_convention.eq_ignore_ascii_case("SimpleFreeFieldHRIR") {
        return Err(SofaError::UnsupportedSofaConvention(sofa_convention));
    }
    let version = file.global_text("SOFAConventionsVersion")?;
    if !matches!(version.trim(), "1.0" | "1.1" | "1.2") {
        return Err(SofaError::UnsupportedSofaConventionVersion(version));
    }
    let data_type = file.global_text("DataType")?;
    if !data_type.eq_ignore_ascii_case("FIR") {
        return Err(SofaError::UnsupportedSofaConvention(data_type));
    }
    let room_type = file.global_text("RoomType")?;
    if !room_type.to_ascii_lowercase().contains("free field") {
        return Err(SofaError::UnsupportedSofaConvention(room_type));
    }

    let ir = file.variable("Data.IR")?;
    require_float_variable(ir)?;
    let ir_shape = file.shape(ir);
    if ir_shape.len() != 3 {
        return Err(SofaError::InvalidDimension(
            "Data.IR must have rank 3 [M,R,N]".to_string(),
        ));
    }
    let (measurements, receivers, taps) = (ir_shape[0], ir_shape[1], ir_shape[2]);
    if measurements == 0 || measurements > limits.max_measurements {
        return Err(SofaError::ResourceLimitExceeded("measurements"));
    }
    if receivers != 2 {
        return Err(SofaError::InvalidDimension(
            "Data.IR receiver dimension must be exactly 2".to_string(),
        ));
    }
    if taps == 0 || taps > limits.max_fir_samples {
        return Err(SofaError::ResourceLimitExceeded("FIR samples"));
    }
    if ir.elements != measurements * receivers * taps {
        return Err(SofaError::InvalidImpulseResponse(
            "Data.IR element count".to_string(),
        ));
    }

    let sample_rate_var = file.variable("Data.SamplingRate")?;
    require_float_variable(sample_rate_var)?;
    let sample_rates = file.values(sample_rate_var)?;
    let units = NetcdfFile::attr_text(&sample_rate_var.attrs, "Units")
        .ok_or(SofaError::MissingAttribute("Data.SamplingRate:Units"))?;
    if !units_hertz(&units) {
        return Err(SofaError::InvalidSamplingRate(
            "unsupported units".to_string(),
        ));
    }
    if sample_rates.is_empty() {
        return Err(SofaError::InvalidSamplingRate("empty".to_string()));
    }
    let rate = sample_rates[0];
    if !rate.is_finite() || rate <= 0.0 || rate.fract() != 0.0 || rate > f64::from(u32::MAX) {
        return Err(SofaError::InvalidSamplingRate(rate.to_string()));
    }
    if sample_rates.iter().any(|value| *value != rate) {
        return Err(SofaError::InvalidSamplingRate(
            "measurement-varying rate".to_string(),
        ));
    }
    #[allow(clippy::cast_sign_loss)]
    let sample_rate = rate as u32;

    let listener_position = read_fixed_vec3(file, "ListenerPosition", "metre")?;
    let listener_view = read_fixed_vec3(file, "ListenerView", "metre")?;
    let listener_up = read_fixed_vec3(file, "ListenerUp", "metre")?;
    let basis = listener_basis(listener_view, listener_up)?;
    let receiver_var = file.variable("ReceiverPosition")?;
    let receiver_positions = read_fixed_matrix(file, receiver_var, receivers, 3, "metre")?;
    // SOFA receivers are already listener-local: +X is forward and +Y is
    // left, regardless of the listener's world position or orientation.
    // Convert only the axis convention to OpenJOC's +X right / +Y forward.
    let receiver_local = receiver_positions
        .iter()
        .map(|position| CartesianPosition::new(-position.y, position.x, position.z))
        .collect::<Vec<_>>();
    let (left_receiver, right_receiver) = receiver_ears(&receiver_local)?;

    let source_var = file.variable("SourcePosition")?;
    require_float_variable(source_var)?;
    let source_shape = file.shape(source_var);
    if source_shape != vec![measurements, 3] {
        return Err(SofaError::InvalidDimension(
            "SourcePosition must be [M,3]".to_string(),
        ));
    }
    let source_units = NetcdfFile::attr_text(&source_var.attrs, "Units")
        .ok_or(SofaError::MissingAttribute("SourcePosition:Units"))?;
    let source_type = NetcdfFile::attr_text(&source_var.attrs, "Type")
        .ok_or(SofaError::MissingAttribute("SourcePosition:Type"))?;
    if !source_type.eq_ignore_ascii_case("spherical") || !units_spherical(&source_units) {
        return Err(SofaError::InvalidCoordinate(
            "SourcePosition requires spherical degree, degree, metre".to_string(),
        ));
    }
    let source_values = file.values(source_var)?;

    if let Ok(emitter_var) = file.variable("EmitterPosition") {
        require_float_variable(emitter_var)?;
        let values = file.values(emitter_var)?;
        if values
            .iter()
            .any(|value| !value.is_finite() || value.abs() > MAX_COORDINATE_TOLERANCE)
        {
            return Err(SofaError::InvalidCoordinate(
                "non-default EmitterPosition".to_string(),
            ));
        }
    }

    let mut directions = Vec::with_capacity(measurements);
    for measurement in 0..measurements {
        let azimuth = source_values[measurement * 3].to_radians();
        let elevation = source_values[measurement * 3 + 1].to_radians();
        let distance = source_values[measurement * 3 + 2];
        if !azimuth.is_finite()
            || !elevation.is_finite()
            || !distance.is_finite()
            || distance <= 0.0
        {
            return Err(SofaError::InvalidCoordinate(format!(
                "source measurement {measurement}"
            )));
        }
        let world = CartesianPosition::new(
            distance * elevation.cos() * azimuth.cos(),
            distance * elevation.cos() * azimuth.sin(),
            distance * elevation.sin(),
        );
        let local = transform(sub(world, listener_position), basis);
        let direction = normalize(local).ok_or_else(|| {
            SofaError::InvalidCoordinate(format!("zero source direction at {measurement}"))
        })?;
        directions.push(direction);
    }
    HrirBank::validate_unique_canonical_hrir_directions(directions.len(), |index| {
        let direction = directions[index];
        [direction.x, direction.y, direction.z]
    })
    .map_err(|error| match error {
        openjoc_render::RenderError::DuplicateHrirDirection { first, second } => {
            SofaError::DuplicateDirection { first, second }
        }
        _ => SofaError::InvalidCoordinate("canonical source direction".to_string()),
    })?;

    let delays = read_delays(file, measurements, receivers, limits)?;
    let mut entries: Vec<HrirEntry> = Vec::with_capacity(measurements);
    let mut max_expanded_taps = 0usize;
    let mut expanded_total = 0usize;
    for measurement in 0..measurements {
        let direction = directions[measurement];
        let left_delay = delays[measurement * receivers + left_receiver];
        let right_delay = delays[measurement * receivers + right_receiver];
        let left_len = left_delay
            .checked_add(taps)
            .ok_or(SofaError::ResourceLimitExceeded("expanded taps"))?;
        let right_len = right_delay
            .checked_add(taps)
            .ok_or(SofaError::ResourceLimitExceeded("expanded taps"))?;
        let pair_len = left_len.max(right_len);
        max_expanded_taps = max_expanded_taps.max(pair_len);
        let total = pair_len
            .checked_mul(2)
            .ok_or(SofaError::ResourceLimitExceeded("expanded taps"))?;
        expanded_total =
            expanded_total
                .checked_add(total)
                .ok_or(SofaError::ResourceLimitExceeded(
                    "expanded FIR coefficients",
                ))?;
        if expanded_total > limits.max_total_coefficients {
            return Err(SofaError::ResourceLimitExceeded(
                "expanded FIR coefficients",
            ));
        }
        let mut left = vec![0.0; pair_len];
        let mut right = vec![0.0; pair_len];
        file.copy_values_into(
            ir,
            (measurement * receivers + left_receiver) * taps,
            &mut left[left_delay..left_len],
        )?;
        file.copy_values_into(
            ir,
            (measurement * receivers + right_receiver) * taps,
            &mut right[right_delay..right_len],
        )?;
        let pair = HrirPair::new_with_delays(sample_rate, left, right, [left_delay, right_delay])
            .map_err(|_| {
            SofaError::InvalidImpulseResponse("HrirPair validation".to_string())
        })?;
        entries.push(
            HrirEntry::new(HrirEntryId::new(measurement as u64), direction, pair)
                .map_err(|_| SofaError::InvalidCoordinate(format!("direction {measurement}")))?,
        );
    }
    if expanded_total > limits.max_total_coefficients {
        return Err(SofaError::ResourceLimitExceeded("total FIR coefficients"));
    }
    let bank = HrirBank::new(sample_rate, entries).map_err(|error| match error {
        openjoc_render::RenderError::DuplicateHrirDirection { first, second } => {
            SofaError::DuplicateDirection { first, second }
        }
        _ => SofaError::InvalidImpulseResponse("HrirBank validation".to_string()),
    })?;
    let metadata = SofaHrirMetadata {
        convention_version: version,
        title: file
            .globals
            .iter()
            .find(|a| a.name == "Title")
            .and_then(|a| match &a.value {
                AttributeValue::Text(v) => Some(v.clone()),
                AttributeValue::Numbers => None,
            }),
        database_name: file
            .globals
            .iter()
            .find(|a| a.name == "DatabaseName")
            .and_then(|a| match &a.value {
                AttributeValue::Text(v) => Some(v.clone()),
                AttributeValue::Numbers => None,
            }),
        listener_short_name: file
            .globals
            .iter()
            .find(|a| a.name == "ListenerShortName")
            .and_then(|a| match &a.value {
                AttributeValue::Text(v) => Some(v.clone()),
                AttributeValue::Numbers => None,
            }),
        license: file
            .globals
            .iter()
            .find(|a| a.name == "License")
            .and_then(|a| match &a.value {
                AttributeValue::Text(v) => Some(v.clone()),
                AttributeValue::Numbers => None,
            }),
        measurement_count: measurements,
        original_fir_length: taps,
        expanded_max_tap_length: max_expanded_taps,
        sample_rate_hz: sample_rate,
    };
    Ok(LoadedSofaHrirBank { bank, metadata })
}

/// Resolves a virtual-speaker direction using exact lookup first and a
/// bounded, deterministic spherical-local interpolation otherwise.
///
/// The interpolation uses the nearest valid two-point great-circle segment or
/// three-point spherical neighborhood.  The requested direction must lie on
/// the selected segment or inside the selected local spherical triangle;
/// extrapolation is rejected.  Both ears use the same spatial weights.  HRIR
/// onset delays are interpolated separately from the delay-aligned FIR shape.
pub fn resolve_hrir(
    bank: &HrirBank,
    direction: CartesianPosition,
) -> Result<ResolvedHrir, SofaError> {
    let target = normalize(direction).ok_or_else(|| {
        SofaError::InvalidCoordinate("binaural direction must be finite and nonzero".to_string())
    })?;
    let target_position = CartesianPosition::new(target.x, target.y, target.z);
    let target = [target.x, target.y, target.z];
    if let Ok(entry) = bank.resolve_exact(target_position) {
        return Ok(ResolvedHrir {
            pair: entry.pair().clone(),
            exact_entry: Some(entry.id()),
            neighbor_count: 1,
        });
    }
    let neighborhood = find_interpolation_neighborhood(target, bank.entries().len(), |index| {
        bank.entries()[index].direction()
    })?;
    build_resolved_interpolation(bank, &neighborhood.indices, &neighborhood.weights)
}

/// Resolves one orientation-dependent direction using the same exact lookup
/// and local spherical interpolation as [`resolve_hrir`], with a deterministic
/// bounded expansion of the nearest-measurement window when the default local
/// search finds no containing segment or triangle. It still rejects uncovered
/// directions; the static resolver remains unchanged for compatibility.
pub fn resolve_hrir_for_listener_orientation(
    bank: &HrirBank,
    direction: CartesianPosition,
) -> Result<ResolvedHrir, SofaError> {
    let target = normalize(direction).ok_or_else(|| {
        SofaError::InvalidCoordinate("binaural direction must be finite and nonzero".to_string())
    })?;
    let target_position = CartesianPosition::new(target.x, target.y, target.z);
    let target = [target.x, target.y, target.z];
    if let Ok(entry) = bank.resolve_exact(target_position) {
        return Ok(ResolvedHrir {
            pair: entry.pair().clone(),
            exact_entry: Some(entry.id()),
            neighbor_count: 1,
        });
    }
    let neighborhood = find_interpolation_neighborhood_for_listener_orientation(
        target,
        bank.entries().len(),
        |index| bank.entries()[index].direction(),
    )?;
    build_resolved_interpolation(bank, &neighborhood.indices, &neighborhood.weights)
}

/// Resolves a built-in f32-resident bank. Only the selected measurement taps
/// or the small interpolated kernel are widened to the renderer's f64 format.
pub fn resolve_hrir_f32(
    bank: &BuiltinHrirF32Bank,
    direction: CartesianPosition,
) -> Result<ResolvedHrir, SofaError> {
    let target = normalize(direction).ok_or_else(|| {
        SofaError::InvalidCoordinate("binaural direction must be finite and nonzero".to_string())
    })?;
    let target = [target.x, target.y, target.z];
    if let Some((index, record)) = bank
        .records()
        .iter()
        .enumerate()
        .find(|(_, record)| directions_match(target, record.direction))
    {
        let left = bank
            .ear_taps(index, 0)
            .ok_or(SofaError::InvalidImpulseResponse(
                "left tap range".to_owned(),
            ))?
            .iter()
            .copied()
            .map(f64::from)
            .collect();
        let right = bank
            .ear_taps(index, 1)
            .ok_or(SofaError::InvalidImpulseResponse(
                "right tap range".to_owned(),
            ))?
            .iter()
            .copied()
            .map(f64::from)
            .collect();
        let delays = [
            usize::try_from(record.delays[0]).map_err(|_| {
                SofaError::InvalidImpulseResponse("packed delay overflow".to_owned())
            })?,
            usize::try_from(record.delays[1]).map_err(|_| {
                SofaError::InvalidImpulseResponse("packed delay overflow".to_owned())
            })?,
        ];
        let pair = HrirPair::new_with_delays(bank.sample_rate_hz(), left, right, delays)
            .map_err(|error| SofaError::InvalidImpulseResponse(error.to_string()))?;
        return Ok(ResolvedHrir {
            pair,
            exact_entry: Some(HrirEntryId::new(index as u64)),
            neighbor_count: 1,
        });
    }

    let neighborhood = find_interpolation_neighborhood(target, bank.direction_count(), |index| {
        bank.records()[index].direction
    })?;
    let pair = interpolate_f32_pair(bank, &neighborhood.indices, &neighborhood.weights)?;
    Ok(ResolvedHrir {
        pair,
        exact_entry: None,
        neighbor_count: neighborhood.indices.len(),
    })
}

/// Built-in f32-bank counterpart to
/// [`resolve_hrir_for_listener_orientation`]. It preserves f32 resident-bank
/// storage and only widens the selected or interpolated kernel.
pub fn resolve_hrir_f32_for_listener_orientation(
    bank: &BuiltinHrirF32Bank,
    direction: CartesianPosition,
) -> Result<ResolvedHrir, SofaError> {
    resolve_orientation_f32(bank, direction, None)
}

/// Immutable built-in orientation query resources, constructed off the render path.
/// The index is bound to its bank; it cannot be reused with a different resource.
#[derive(Clone, Debug)]
pub struct ListenerOrientationHrirBank {
    bank: std::sync::Arc<BuiltinHrirF32Bank>,
    index: OrientationDirectionIndex,
}

impl ListenerOrientationHrirBank {
    pub fn new(bank: std::sync::Arc<BuiltinHrirF32Bank>) -> Self {
        let index = OrientationDirectionIndex::new(&bank);
        Self { bank, index }
    }

    pub fn resolve(&self, direction: CartesianPosition) -> Result<ResolvedHrir, SofaError> {
        resolve_orientation_f32(&self.bank, direction, Some(&self.index))
    }

    pub fn tap_storage_bytes(&self) -> usize {
        self.bank.tap_storage_bytes()
    }
}

fn resolve_orientation_f32(
    bank: &BuiltinHrirF32Bank,
    direction: CartesianPosition,
    index: Option<&OrientationDirectionIndex>,
) -> Result<ResolvedHrir, SofaError> {
    #[cfg(feature = "orientation-profile")]
    let lookup_span = orientation_profile::Span::new(1);
    let target = normalize(direction).ok_or_else(|| {
        SofaError::InvalidCoordinate("binaural direction must be finite and nonzero".to_string())
    })?;
    let target = [target.x, target.y, target.z];
    let exact = if let Some(index) = index {
        index.exact(bank, target)
    } else {
        bank.records()
            .iter()
            .position(|record| directions_match(target, record.direction))
    };
    if let Some(index) = exact {
        let record = &bank.records()[index];
        #[cfg(feature = "orientation-profile")]
        drop(lookup_span);
        #[cfg(feature = "orientation-profile")]
        let _taps_span = orientation_profile::Span::new(4);
        let left = bank
            .ear_taps(index, 0)
            .ok_or(SofaError::InvalidImpulseResponse(
                "left tap range".to_owned(),
            ))?
            .iter()
            .copied()
            .map(f64::from)
            .collect();
        let right = bank
            .ear_taps(index, 1)
            .ok_or(SofaError::InvalidImpulseResponse(
                "right tap range".to_owned(),
            ))?
            .iter()
            .copied()
            .map(f64::from)
            .collect();
        let delays = [
            usize::try_from(record.delays[0]).map_err(|_| {
                SofaError::InvalidImpulseResponse("packed delay overflow".to_owned())
            })?,
            usize::try_from(record.delays[1]).map_err(|_| {
                SofaError::InvalidImpulseResponse("packed delay overflow".to_owned())
            })?,
        ];
        let pair = HrirPair::new_with_delays(bank.sample_rate_hz(), left, right, delays)
            .map_err(|error| SofaError::InvalidImpulseResponse(error.to_string()))?;
        return Ok(ResolvedHrir {
            pair,
            exact_entry: Some(HrirEntryId::new(index as u64)),
            neighbor_count: 1,
        });
    }

    #[cfg(feature = "orientation-profile")]
    drop(lookup_span);
    let neighborhood = if let Some(index) = index {
        if bank.direction_count() < 2 {
            return Err(SofaError::InsufficientInterpolationData {
                required: 2,
                available: bank.direction_count(),
            });
        }
        #[cfg(feature = "orientation-profile")]
        let ranking_span = orientation_profile::Span::new(2);
        let candidates = index.nearest(bank, target, 128);
        #[cfg(feature = "orientation-profile")]
        drop(ranking_span);
        orientation_neighborhood_from_candidates(
            target,
            bank.direction_count(),
            &candidates,
            |i| bank.records()[i].direction,
        )?
    } else {
        find_interpolation_neighborhood_for_listener_orientation(
            target,
            bank.direction_count(),
            |i| bank.records()[i].direction,
        )?
    };
    #[cfg(feature = "orientation-profile")]
    let _taps_span = orientation_profile::Span::new(4);
    let pair = interpolate_f32_pair(bank, &neighborhood.indices, &neighborhood.weights)?;
    Ok(ResolvedHrir {
        pair,
        exact_entry: None,
        neighbor_count: neighborhood.indices.len(),
    })
}

#[derive(Debug)]
struct InterpolationNeighborhood {
    indices: Vec<usize>,
    weights: Vec<f64>,
}

fn find_interpolation_neighborhood(
    target: [f64; 3],
    direction_count: usize,
    direction_at: impl Fn(usize) -> [f64; 3],
) -> Result<InterpolationNeighborhood, SofaError> {
    find_interpolation_neighborhood_with_candidate_limit(
        target,
        direction_count,
        direction_at,
        MAX_LOCAL_INTERPOLATION_CANDIDATES,
    )
}

fn find_interpolation_neighborhood_with_candidate_limit(
    target: [f64; 3],
    direction_count: usize,
    direction_at: impl Fn(usize) -> [f64; 3],
    candidate_limit: usize,
) -> Result<InterpolationNeighborhood, SofaError> {
    find_interpolation_neighborhood_with_options(
        target,
        direction_count,
        direction_at,
        candidate_limit,
    )
}

fn find_interpolation_neighborhood_for_listener_orientation(
    target: [f64; 3],
    direction_count: usize,
    direction_at: impl Fn(usize) -> [f64; 3],
) -> Result<InterpolationNeighborhood, SofaError> {
    const CANDIDATE_LIMITS: [usize; 7] = [8, 16, 32, 48, 64, 96, 128];
    if direction_count < 2 {
        return Err(SofaError::InsufficientInterpolationData {
            required: 2,
            available: direction_count,
        });
    }

    // Rank once, then grow nested prefixes. The old implementation reranked
    // the entire measurement bank at every tier, despite each tier being a
    // strict prefix of the next one. Preserve deterministic dot/index ordering
    // so a successful 8-candidate query still selects the same triangle.
    #[cfg(feature = "orientation-profile")]
    let ranking_span = orientation_profile::Span::new(2);
    let candidates = nearest_candidates_with_limit(
        target,
        (0..direction_count).map(|index| (index, direction_at(index))),
        *CANDIDATE_LIMITS.last().expect("non-empty candidate limits"),
    );
    #[cfg(feature = "orientation-profile")]
    drop(ranking_span);
    orientation_neighborhood_from_candidates(target, direction_count, &candidates, direction_at)
}

fn orientation_neighborhood_from_candidates(
    target: [f64; 3],
    direction_count: usize,
    candidates: &[Candidate],
    direction_at: impl Fn(usize) -> [f64; 3],
) -> Result<InterpolationNeighborhood, SofaError> {
    const CANDIDATE_LIMITS: [usize; 7] = [8, 16, 32, 48, 64, 96, 128];
    #[cfg(feature = "orientation-profile")]
    let _geometry_span = orientation_profile::Span::new(3);
    let candidate_directions = candidates
        .iter()
        .map(|candidate| direction_at(candidate.index))
        .collect::<Vec<_>>();
    let projection = TangentProjection::new(target);
    let candidate_projections = projection.map(|projection| {
        candidate_directions
            .iter()
            .copied()
            .map(|direction| projection.project(target, direction))
            .collect::<Vec<_>>()
    });
    let mut previous_limit = 0;
    for candidate_limit in CANDIDATE_LIMITS {
        let candidate_limit = candidate_limit.min(candidates.len());
        if candidate_limit == previous_limit {
            continue;
        }
        match find_interpolation_neighborhood_for_candidate_expansion(
            target,
            candidates,
            &candidate_directions,
            candidate_projections.as_deref(),
            previous_limit,
            candidate_limit,
        ) {
            Ok(neighborhood) => return Ok(neighborhood),
            Err(error @ SofaError::InterpolationOutsideCoverage(_)) => {
                previous_limit = candidate_limit;
                if candidate_limit == candidates.len() {
                    return Err(error);
                }
            }
            Err(error) => return Err(error),
        }
    }
    Err(SofaError::InsufficientInterpolationData {
        required: 2,
        available: direction_count,
    })
}

fn find_interpolation_neighborhood_for_candidate_expansion(
    _target: [f64; 3],
    candidates: &[Candidate],
    candidate_directions: &[[f64; 3]],
    candidate_projections: Option<&[[f64; 2]]>,
    previous_limit: usize,
    candidate_limit: usize,
) -> Result<InterpolationNeighborhood, SofaError> {
    // All triples with their largest candidate index below previous_limit
    // were already rejected at the preceding tier. Visit only newly possible
    // triples, while retaining the same nested lexicographic order as a full
    // scan of the current prefix.
    // Reuse first/third terms across the second-vertex loop at larger tiers.
    // Fixed bounded stack storage adds no query allocations or render work.
    let mut first_terms = [TriangleFirstTerms::ZERO; 128];
    for first in 0..candidate_limit {
        if candidate_limit >= 16 {
            if let Some(projected) = candidate_projections {
                for third in first + 2..candidate_limit {
                    first_terms[third] =
                        TriangleFirstTerms::new(projected[first], projected[third]);
                }
            }
        }
        for second in first + 1..candidate_limit {
            for third in (second + 1).max(previous_limit)..candidate_limit {
                let selected = [candidates[first], candidates[second], candidates[third]];
                if selected
                    .iter()
                    .any(|candidate| candidate.angle > MAX_INTERPOLATION_ANGLE_RADIANS)
                {
                    continue;
                }
                if let Some(weights) = candidate_projections.and_then(|projected| {
                    if candidate_limit >= 16 {
                        first_terms[third].weights(projected[second], projected[third])
                    } else {
                        spherical_triangle_weights_from_projected([
                            projected[first],
                            projected[second],
                            projected[third],
                        ])
                    }
                }) {
                    return Ok(InterpolationNeighborhood {
                        indices: selected.map(|candidate| candidate.index).to_vec(),
                        weights: weights.to_vec(),
                    });
                }
            }
        }
    }
    for first in 0..candidate_limit {
        for second in (first + 1).max(previous_limit)..candidate_limit {
            let left = candidates[first];
            let right = candidates[second];
            if let Some(weights) = great_circle_segment_weights_with_angles(
                candidate_directions[first],
                candidate_directions[second],
                left.angle,
                right.angle,
            ) {
                return Ok(InterpolationNeighborhood {
                    indices: vec![left.index, right.index],
                    weights: weights.to_vec(),
                });
            }
        }
    }
    Err(SofaError::InterpolationOutsideCoverage(format!(
        "nearest measurement is {:.2} degrees away and no local spherical segment/triangle contains the request",
        candidates[0].angle.to_degrees()
    )))
}

#[derive(Clone, Copy)]
struct TangentProjection {
    basis_x: [f64; 3],
    basis_y: [f64; 3],
}

impl TangentProjection {
    fn new(target: [f64; 3]) -> Option<Self> {
        let reference = if target[2].abs() < 0.9 {
            [0.0, 0.0, 1.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let basis_x = normalize_array(cross_array(reference, target))?;
        let basis_y = cross_array(target, basis_x);
        Some(Self { basis_x, basis_y })
    }

    fn project(self, target: [f64; 3], vertex: [f64; 3]) -> [f64; 2] {
        [
            dot_array(
                sub_array(vertex, scale_array(target, dot_array(vertex, target))),
                self.basis_x,
            ),
            dot_array(
                sub_array(vertex, scale_array(target, dot_array(vertex, target))),
                self.basis_y,
            ),
        ]
    }
}

#[derive(Clone, Copy)]
struct TriangleFirstTerms {
    a: f64,
    c: f64,
    beta_numerator: f64,
}

impl TriangleFirstTerms {
    const ZERO: Self = Self {
        a: 0.0,
        c: 0.0,
        beta_numerator: 0.0,
    };
    fn new(first: [f64; 2], third: [f64; 2]) -> Self {
        let a = first[0] - third[0];
        let c = first[1] - third[1];
        Self {
            a,
            c,
            beta_numerator: a.mul_add(-third[1], third[0] * c),
        }
    }
    fn weights(self, second: [f64; 2], third: [f64; 2]) -> Option<[f64; 3]> {
        let b = second[0] - third[0];
        let d = second[1] - third[1];
        let determinant = self.a.mul_add(d, -b * self.c);
        if !determinant.is_finite() || determinant.abs() <= INTERPOLATION_GEOMETRY_TOLERANCE {
            return None;
        }
        let alpha = ((-third[0]).mul_add(d, -b * -third[1])) / determinant;
        if !alpha.is_finite() || alpha < -INTERPOLATION_WEIGHT_TOLERANCE {
            return None;
        }
        let beta = self.beta_numerator / determinant;
        if !beta.is_finite() || beta < -INTERPOLATION_WEIGHT_TOLERANCE {
            return None;
        }
        let gamma = 1.0 - alpha - beta;
        let weights = [alpha, beta, gamma];
        weights
            .iter()
            .all(|weight| weight.is_finite() && *weight >= -INTERPOLATION_WEIGHT_TOLERANCE)
            .then_some(weights.map(|weight| weight.max(0.0)))
    }
}

fn spherical_triangle_weights_from_projected(projected: [[f64; 2]; 3]) -> Option<[f64; 3]> {
    TriangleFirstTerms::new(projected[0], projected[2]).weights(projected[1], projected[2])
}

fn find_interpolation_neighborhood_with_options(
    target: [f64; 3],
    direction_count: usize,
    direction_at: impl Fn(usize) -> [f64; 3],
    candidate_limit: usize,
) -> Result<InterpolationNeighborhood, SofaError> {
    if direction_count < 2 {
        return Err(SofaError::InsufficientInterpolationData {
            required: 2,
            available: direction_count,
        });
    }
    if candidate_limit < 2 {
        return Err(SofaError::InsufficientInterpolationData {
            required: 2,
            available: candidate_limit,
        });
    }
    let candidates = nearest_candidates_with_limit(
        target,
        (0..direction_count).map(|index| (index, direction_at(index))),
        candidate_limit,
    );
    for first in 0..candidates.len() {
        for second in first + 1..candidates.len() {
            for third in second + 1..candidates.len() {
                let selected = [candidates[first], candidates[second], candidates[third]];
                if selected
                    .iter()
                    .any(|candidate| candidate.angle > MAX_INTERPOLATION_ANGLE_RADIANS)
                {
                    continue;
                }
                let ordered = selected.map(|candidate| direction_at(candidate.index));
                if let Some(weights) = spherical_triangle_weights(target, ordered) {
                    return Ok(InterpolationNeighborhood {
                        indices: selected.map(|candidate| candidate.index).to_vec(),
                        weights: weights.to_vec(),
                    });
                }
            }
        }
    }
    for first in 0..candidates.len() {
        for second in first + 1..candidates.len() {
            let left = candidates[first];
            let right = candidates[second];
            let first_direction = direction_at(left.index);
            let second_direction = direction_at(right.index);
            if let Some(weights) =
                great_circle_segment_weights(target, first_direction, second_direction)
            {
                return Ok(InterpolationNeighborhood {
                    indices: vec![left.index, right.index],
                    weights: weights.to_vec(),
                });
            }
        }
    }
    Err(SofaError::InterpolationOutsideCoverage(format!(
        "nearest measurement is {:.2} degrees away and no local spherical segment/triangle contains the request",
        candidates[0].angle.to_degrees()
    )))
}

fn directions_match(first: [f64; 3], second: [f64; 3]) -> bool {
    (1.0 - dot_array(first, second)).abs() <= 1.0e-12
}

const MAX_LOCAL_INTERPOLATION_CANDIDATES: usize = 8;
const MAX_INTERPOLATION_ANGLE_RADIANS: f64 = 2.0 * std::f64::consts::PI / 3.0;
const INTERPOLATION_GEOMETRY_TOLERANCE: f64 = 1.0e-10;
const INTERPOLATION_WEIGHT_TOLERANCE: f64 = 1.0e-8;

#[derive(Clone, Copy, Debug)]
struct Candidate {
    index: usize,
    angle: f64,
    dot: f64,
}

#[cfg(test)]
fn nearest_candidates(
    target: [f64; 3],
    directions: impl Iterator<Item = (usize, [f64; 3])>,
) -> Vec<Candidate> {
    nearest_candidates_with_limit(target, directions, MAX_LOCAL_INTERPOLATION_CANDIDATES)
}

fn nearest_candidates_with_limit(
    target: [f64; 3],
    directions: impl Iterator<Item = (usize, [f64; 3])>,
    candidate_limit: usize,
) -> Vec<Candidate> {
    if candidate_limit == 0 {
        return Vec::new();
    }
    let mut candidates = BinaryHeap::with_capacity(candidate_limit);
    for (index, direction) in directions {
        let dot = dot_array(target, direction).clamp(-1.0, 1.0);
        let candidate = CandidateHeapEntry(Candidate {
            index,
            angle: 0.0,
            dot,
        });
        if candidates.len() < candidate_limit {
            candidates.push(candidate);
        } else if candidates
            .peek()
            .is_some_and(|worst| is_better_candidate(&candidate.0, &worst.0))
        {
            candidates.pop();
            candidates.push(candidate);
        }
    }
    sorted_candidates(candidates)
}

fn sorted_candidates(candidates: BinaryHeap<CandidateHeapEntry>) -> Vec<Candidate> {
    let mut candidates = candidates
        .into_iter()
        .map(|entry| entry.0)
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        if left.dot == right.dot {
            left.index.cmp(&right.index)
        } else {
            right.dot.partial_cmp(&left.dot).unwrap_or(Ordering::Equal)
        }
    });
    for candidate in &mut candidates {
        candidate.angle = candidate.dot.acos();
    }
    candidates
}

fn is_better_candidate(candidate: &Candidate, other: &Candidate) -> bool {
    candidate.dot > other.dot || (candidate.dot == other.dot && candidate.index < other.index)
}

/// A bounded max-heap whose head is the worst retained candidate. The public
/// resolver still returns the same dot-descending, index-ascending ordering.
#[derive(Clone, Copy, Debug)]
struct CandidateHeapEntry(Candidate);

impl PartialEq for CandidateHeapEntry {
    fn eq(&self, other: &Self) -> bool {
        self.0.index == other.0.index && self.0.dot == other.0.dot
    }
}

impl Eq for CandidateHeapEntry {}

impl PartialOrd for CandidateHeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CandidateHeapEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.0.dot == other.0.dot {
            self.0.index.cmp(&other.0.index)
        } else {
            other
                .0
                .dot
                .partial_cmp(&self.0.dot)
                .unwrap_or(Ordering::Equal)
        }
    }
}

fn build_resolved_interpolation(
    bank: &HrirBank,
    indices: &[usize],
    weights: &[f64],
) -> Result<ResolvedHrir, SofaError> {
    if indices.len() != weights.len() || indices.len() < 2 {
        return Err(SofaError::InvalidInterpolationResult(
            "weight/index cardinality".to_string(),
        ));
    }
    let normalized_weights = normalize_interpolation_weights(weights)?;
    let pair = interpolate_pair(bank, indices, &normalized_weights)?;
    Ok(ResolvedHrir {
        pair,
        exact_entry: None,
        neighbor_count: indices.len(),
    })
}

fn normalize_interpolation_weights(weights: &[f64]) -> Result<Vec<f64>, SofaError> {
    let mut weight_sum = 0.0;
    for weight in weights {
        if !weight.is_finite() || *weight < -INTERPOLATION_WEIGHT_TOLERANCE {
            return Err(SofaError::InvalidInterpolationResult(
                "non-finite or negative interpolation weight".to_string(),
            ));
        }
        weight_sum += *weight;
    }
    if !weight_sum.is_finite() || weight_sum <= 0.0 {
        return Err(SofaError::InvalidInterpolationResult(
            "invalid interpolation weight sum".to_string(),
        ));
    }
    Ok(weights
        .iter()
        .map(|weight| (*weight).max(0.0) / weight_sum)
        .collect::<Vec<_>>())
}

fn interpolate_f32_pair(
    bank: &BuiltinHrirF32Bank,
    indices: &[usize],
    weights: &[f64],
) -> Result<HrirPair, SofaError> {
    if indices.len() != weights.len() || indices.len() < 2 {
        return Err(SofaError::InvalidInterpolationResult(
            "weight/index cardinality".to_string(),
        ));
    }
    let weights = normalize_interpolation_weights(weights)?;
    let mut max_aligned_taps = 0usize;
    let mut delay_values = [0.0; 2];
    for (&index, &weight) in indices.iter().zip(&weights) {
        let record = bank
            .records()
            .get(index)
            .ok_or_else(|| SofaError::InvalidInterpolationResult("neighbor index".to_string()))?;
        for (ear_index, compact_delay) in record.delays.into_iter().enumerate() {
            let delay = usize::try_from(compact_delay).map_err(|_| {
                SofaError::InvalidInterpolationResult("packed delay overflow".to_string())
            })?;
            let taps = bank.ear_taps(index, ear_index).ok_or_else(|| {
                SofaError::InvalidInterpolationResult("neighbor tap range".to_string())
            })?;
            if delay > taps.len() {
                return Err(SofaError::InvalidInterpolationResult(format!(
                    "ear {ear_index} delay exceeds tap count"
                )));
            }
            max_aligned_taps = max_aligned_taps.max(taps.len() - delay);
            delay_values[ear_index] += weight * delay as f64;
        }
    }
    if max_aligned_taps == 0 {
        return Err(SofaError::InvalidInterpolationResult(
            "empty delay-aligned HRIR".to_string(),
        ));
    }
    let delays = [
        rounded_delay(delay_values[0])?,
        rounded_delay(delay_values[1])?,
    ];
    let output_len = delays
        .iter()
        .copied()
        .max()
        .and_then(|delay| delay.checked_add(max_aligned_taps))
        .ok_or_else(|| SofaError::InvalidInterpolationResult("tap length overflow".to_string()))?;
    let mut output = [vec![0.0; output_len], vec![0.0; output_len]];
    for (&index, &weight) in indices.iter().zip(&weights) {
        let record = bank
            .records()
            .get(index)
            .ok_or_else(|| SofaError::InvalidInterpolationResult("neighbor index".to_string()))?;
        for ear_index in 0..2 {
            let taps = bank.ear_taps(index, ear_index).ok_or_else(|| {
                SofaError::InvalidInterpolationResult("neighbor tap range".to_string())
            })?;
            let source_delay = usize::try_from(record.delays[ear_index]).map_err(|_| {
                SofaError::InvalidInterpolationResult("packed delay overflow".to_string())
            })?;
            for (tap_index, tap) in taps[source_delay..].iter().enumerate() {
                output[ear_index][delays[ear_index] + tap_index] += weight * f64::from(*tap);
            }
        }
    }
    if output
        .iter()
        .flat_map(|taps| taps.iter())
        .any(|tap| !tap.is_finite())
    {
        return Err(SofaError::InvalidInterpolationResult(
            "non-finite interpolated tap".to_string(),
        ));
    }
    let [left, right] = output;
    HrirPair::new_with_delays(bank.sample_rate_hz(), left, right, delays)
        .map_err(|error| SofaError::InvalidInterpolationResult(error.to_string()))
}

fn interpolate_pair(
    bank: &HrirBank,
    indices: &[usize],
    weights: &[f64],
) -> Result<HrirPair, SofaError> {
    let mut max_aligned_taps = 0usize;
    let mut delay_values = [0.0; 2];
    for (index, weight) in indices.iter().zip(weights) {
        let entry = bank
            .entries()
            .get(*index)
            .ok_or_else(|| SofaError::InvalidInterpolationResult("neighbor index".to_string()))?;
        for (ear_index, ear) in [HrirEar::Left, HrirEar::Right].into_iter().enumerate() {
            let taps = match ear {
                HrirEar::Left => entry.pair().left_taps(),
                HrirEar::Right => entry.pair().right_taps(),
            };
            let delay = entry.pair().delay_samples(ear);
            if delay > taps.len() {
                return Err(SofaError::InvalidInterpolationResult(format!(
                    "{ear:?} delay exceeds tap count"
                )));
            }
            max_aligned_taps = max_aligned_taps.max(taps.len() - delay);
            delay_values[ear_index] += *weight * delay as f64;
        }
    }
    if max_aligned_taps == 0 {
        return Err(SofaError::InvalidInterpolationResult(
            "empty delay-aligned HRIR".to_string(),
        ));
    }
    let delays = [
        rounded_delay(delay_values[0])?,
        rounded_delay(delay_values[1])?,
    ];
    let output_len = delays
        .iter()
        .copied()
        .max()
        .and_then(|delay| delay.checked_add(max_aligned_taps))
        .ok_or_else(|| SofaError::InvalidInterpolationResult("tap length overflow".to_string()))?;
    let mut output = [vec![0.0; output_len], vec![0.0; output_len]];

    for (index, weight) in indices.iter().zip(weights) {
        let entry = bank
            .entries()
            .get(*index)
            .ok_or_else(|| SofaError::InvalidInterpolationResult("neighbor index".to_string()))?;
        for (ear_index, ear) in [HrirEar::Left, HrirEar::Right].into_iter().enumerate() {
            let taps = match ear {
                HrirEar::Left => entry.pair().left_taps(),
                HrirEar::Right => entry.pair().right_taps(),
            };
            let source_delay = entry.pair().delay_samples(ear);
            for (tap_index, tap) in taps[source_delay..].iter().enumerate() {
                output[ear_index][delays[ear_index] + tap_index] += *weight * *tap;
            }
        }
    }
    if output
        .iter()
        .flat_map(|taps| taps.iter())
        .any(|tap| !tap.is_finite())
    {
        return Err(SofaError::InvalidInterpolationResult(
            "non-finite interpolated tap".to_string(),
        ));
    }
    HrirPair::new_with_delays(
        bank.sample_rate_hz(),
        output[0].clone(),
        output[1].clone(),
        delays,
    )
    .map_err(|error| SofaError::InvalidInterpolationResult(error.to_string()))
}

fn rounded_delay(value: f64) -> Result<usize, SofaError> {
    if !value.is_finite() || value < 0.0 || value > usize::MAX as f64 {
        return Err(SofaError::InvalidInterpolationResult(
            "non-finite interpolated delay".to_string(),
        ));
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(value.round() as usize)
}

fn spherical_triangle_weights(target: [f64; 3], vertices: [[f64; 3]; 3]) -> Option<[f64; 3]> {
    let reference = if target[2].abs() < 0.9 {
        [0.0, 0.0, 1.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let basis_x = normalize_array(cross_array(reference, target))?;
    let basis_y = cross_array(target, basis_x);
    let projected = vertices.map(|vertex| {
        [
            dot_array(
                sub_array(vertex, scale_array(target, dot_array(vertex, target))),
                basis_x,
            ),
            dot_array(
                sub_array(vertex, scale_array(target, dot_array(vertex, target))),
                basis_y,
            ),
        ]
    });
    let a = projected[0][0] - projected[2][0];
    let b = projected[1][0] - projected[2][0];
    let c = projected[0][1] - projected[2][1];
    let d = projected[1][1] - projected[2][1];
    let determinant = a.mul_add(d, -b * c);
    if !determinant.is_finite() || determinant.abs() <= INTERPOLATION_GEOMETRY_TOLERANCE {
        return None;
    }
    let alpha = ((-projected[2][0]).mul_add(d, -b * -projected[2][1])) / determinant;
    let beta = (a.mul_add(-projected[2][1], projected[2][0] * c)) / determinant;
    let gamma = 1.0 - alpha - beta;
    let weights = [alpha, beta, gamma];
    weights
        .iter()
        .all(|weight| weight.is_finite() && *weight >= -INTERPOLATION_WEIGHT_TOLERANCE)
        .then_some(weights.map(|weight| weight.max(0.0)))
}

fn great_circle_segment_weights(
    target: [f64; 3],
    first: [f64; 3],
    second: [f64; 3],
) -> Option<[f64; 2]> {
    let whole = angular_distance(first, second);
    if !whole.is_finite()
        || whole <= INTERPOLATION_GEOMETRY_TOLERANCE
        || whole >= std::f64::consts::PI
    {
        return None;
    }
    let first_part = angular_distance(first, target);
    let second_part = angular_distance(target, second);
    if first_part + second_part > whole + INTERPOLATION_WEIGHT_TOLERANCE {
        return None;
    }
    Some([second_part / whole, first_part / whole])
}

// Candidate angles use the same clamped dot and acos as the scalar reference.
// Exchanging multiplication operands in dot_array does not change its FMA order.
fn great_circle_segment_weights_with_angles(
    first: [f64; 3],
    second: [f64; 3],
    first_part: f64,
    second_part: f64,
) -> Option<[f64; 2]> {
    let whole = angular_distance(first, second);
    if !whole.is_finite()
        || whole <= INTERPOLATION_GEOMETRY_TOLERANCE
        || whole >= std::f64::consts::PI
    {
        return None;
    }
    if first_part + second_part > whole + INTERPOLATION_WEIGHT_TOLERANCE {
        return None;
    }
    Some([second_part / whole, first_part / whole])
}

fn angular_distance(first: [f64; 3], second: [f64; 3]) -> f64 {
    dot_array(first, second).clamp(-1.0, 1.0).acos()
}

fn dot_array(first: [f64; 3], second: [f64; 3]) -> f64 {
    first[0].mul_add(second[0], first[1].mul_add(second[1], first[2] * second[2]))
}

fn cross_array(first: [f64; 3], second: [f64; 3]) -> [f64; 3] {
    [
        first[1] * second[2] - first[2] * second[1],
        first[2] * second[0] - first[0] * second[2],
        first[0] * second[1] - first[1] * second[0],
    ]
}

fn sub_array(first: [f64; 3], second: [f64; 3]) -> [f64; 3] {
    [
        first[0] - second[0],
        first[1] - second[1],
        first[2] - second[2],
    ]
}

fn scale_array(value: [f64; 3], factor: f64) -> [f64; 3] {
    [value[0] * factor, value[1] * factor, value[2] * factor]
}

fn normalize_array(value: [f64; 3]) -> Option<[f64; 3]> {
    let norm = dot_array(value, value).sqrt();
    (norm.is_finite() && norm > 0.0).then_some(scale_array(value, 1.0 / norm))
}

fn read_delays(
    file: &NetcdfFile<'_>,
    measurements: usize,
    receivers: usize,
    limits: SofaLoadLimits,
) -> Result<Vec<usize>, SofaError> {
    let variable = file.variable("Data.Delay")?;
    require_float_variable(variable)?;
    let shape = file.shape(variable);
    let values = file.values(variable)?;
    let values = if shape == vec![receivers] || shape == vec![1, receivers] {
        if values.len() != receivers {
            return Err(SofaError::InvalidDimension(
                "Data.Delay element count".to_string(),
            ));
        }
        values
            .into_iter()
            .cycle()
            .take(measurements * receivers)
            .collect::<Vec<_>>()
    } else if shape == vec![measurements, receivers] {
        if values.len() != measurements * receivers {
            return Err(SofaError::InvalidDimension(
                "Data.Delay element count".to_string(),
            ));
        }
        values
    } else {
        return Err(SofaError::InvalidDimension(
            "Data.Delay must be [R], [1,R], or [M,R]".to_string(),
        ));
    };
    // FIR Data.Delay is defined in samples by the convention itself. Some
    // writers add Units; validate that extension when present, but do not
    // require it in otherwise standard SOFA files.
    if let Some(units) = optional_coordinate_attribute(variable, "Units")? {
        if !matches!(
            units.trim().to_ascii_lowercase().as_str(),
            "sample" | "samples"
        ) {
            return Err(SofaError::InvalidCoordinate(
                "Data.Delay units must be samples".to_string(),
            ));
        }
    }
    values
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            if !value.is_finite() || value < 0.0 {
                return Err(SofaError::InvalidCoordinate(format!(
                    "invalid delay {index}"
                )));
            }
            if value.fract() != 0.0 {
                return Err(SofaError::UnsupportedFractionalSofaDelay {
                    measurement: index / receivers,
                    receiver: index % receivers,
                    value,
                });
            }
            if value > limits.max_delay_samples as f64 || value > usize::MAX as f64 {
                return Err(SofaError::ResourceLimitExceeded("delay samples"));
            }
            #[allow(clippy::cast_sign_loss)]
            Ok(value as usize)
        })
        .collect()
}

fn optional_coordinate_attribute(
    variable: &Variable,
    name: &'static str,
) -> Result<Option<String>, SofaError> {
    if !variable
        .attrs
        .iter()
        .any(|attribute| attribute.name == name)
    {
        return Ok(None);
    }
    NetcdfFile::attr_text(&variable.attrs, name)
        .map(Some)
        .ok_or_else(|| {
            SofaError::InvalidCoordinate(format!("{}:{name} must be text", variable.name))
        })
}

fn read_fixed_vec3(
    file: &NetcdfFile<'_>,
    name: &'static str,
    required_units: &str,
) -> Result<CartesianPosition, SofaError> {
    let variable = file.variable(name)?;
    require_float_variable(variable)?;
    let shape = file.shape(variable);
    let values = file.values(variable)?;
    if !(shape == vec![3] || shape == vec![1, 3]) || values.len() != 3 {
        return Err(SofaError::InvalidDimension(format!(
            "{name} must be [3] or [1,3]"
        )));
    }
    // ListenerView's Type and Units describe both orientation vectors in
    // SOFA. Accept the standard omission on ListenerUp, while validating any
    // explicit ListenerUp attributes instead of silently discarding them.
    let inherited = if name == "ListenerUp" {
        Some(file.variable("ListenerView")?)
    } else {
        None
    };
    let attribute = |key| -> Result<Option<String>, SofaError> {
        if let Some(value) = optional_coordinate_attribute(variable, key)? {
            return Ok(Some(value));
        }
        inherited.map_or(Ok(None), |view| optional_coordinate_attribute(view, key))
    };
    if let Some(kind) = attribute("Type")? {
        if !kind.eq_ignore_ascii_case("cartesian") {
            return Err(SofaError::InvalidCoordinate(format!(
                "{name} type must be cartesian"
            )));
        }
    }
    if !required_units.is_empty() {
        let units = attribute("Units")?.ok_or(SofaError::MissingAttribute("listener units"))?;
        if !units_metre(&units) {
            return Err(SofaError::InvalidCoordinate(format!("{name} units")));
        }
    }
    if values.iter().any(|value| !value.is_finite()) {
        return Err(SofaError::InvalidCoordinate(name.to_string()));
    }
    Ok(CartesianPosition::new(values[0], values[1], values[2]))
}

fn read_fixed_matrix(
    file: &NetcdfFile<'_>,
    variable: &Variable,
    rows: usize,
    cols: usize,
    required_units: &str,
) -> Result<Vec<CartesianPosition>, SofaError> {
    require_float_variable(variable)?;
    let shape = file.shape(variable);
    if shape != vec![rows, cols] && shape != vec![rows, cols, 1] {
        return Err(SofaError::InvalidDimension(format!(
            "{} must be [{rows},3] or [{rows},3,1]",
            variable.name
        )));
    }
    let units = NetcdfFile::attr_text(&variable.attrs, "Units")
        .ok_or(SofaError::MissingAttribute("receiver units"))?;
    if !units_metre(&units) || required_units != "metre" {
        return Err(SofaError::InvalidCoordinate("receiver units".to_string()));
    }
    let values = file.values(variable)?;
    if values.iter().any(|value| !value.is_finite()) {
        return Err(SofaError::InvalidCoordinate(variable.name.clone()));
    }
    Ok(values
        .chunks_exact(3)
        .map(|chunk| CartesianPosition::new(chunk[0], chunk[1], chunk[2]))
        .collect())
}

fn listener_basis(
    view: CartesianPosition,
    up: CartesianPosition,
) -> Result<[CartesianPosition; 3], SofaError> {
    let forward =
        normalize(view).ok_or_else(|| SofaError::InvalidCoordinate("ListenerView".to_string()))?;
    let projected = sub(up, scale(forward, dot(up, forward)));
    let up = normalize(projected).ok_or_else(|| {
        SofaError::InvalidCoordinate("ListenerUp collinear with view".to_string())
    })?;
    let right = normalize(cross(forward, up))
        .ok_or_else(|| SofaError::InvalidCoordinate("listener basis".to_string()))?;
    Ok([right, forward, up])
}

fn require_float_variable(variable: &Variable) -> Result<(), SofaError> {
    if matches!(variable.ty, NC_FLOAT | NC_DOUBLE) {
        Ok(())
    } else {
        Err(SofaError::InvalidImpulseResponse(format!(
            "{} must use floating-point storage",
            variable.name
        )))
    }
}

fn receiver_ears(local: &[CartesianPosition]) -> Result<(usize, usize), SofaError> {
    if local.len() != 2 {
        return Err(SofaError::InvalidReceiverGeometry);
    }
    let first = local[0].x;
    let second = local[1].x;
    if first < -MAX_COORDINATE_TOLERANCE && second > MAX_COORDINATE_TOLERANCE {
        Ok((0, 1))
    } else if second < -MAX_COORDINATE_TOLERANCE && first > MAX_COORDINATE_TOLERANCE {
        Ok((1, 0))
    } else {
        Err(SofaError::InvalidReceiverGeometry)
    }
}

fn units_metre(value: &str) -> bool {
    value
        .to_ascii_lowercase()
        .replace(' ', "")
        .contains("metre")
        || value.to_ascii_lowercase().replace(' ', "") == "m"
}
fn units_hertz(value: &str) -> bool {
    matches!(value.trim().to_ascii_lowercase().as_str(), "hertz" | "hz")
}
fn units_spherical(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase().replace(' ', "");
    normalized == "degree,degree,metre" || normalized == "degree,degree,m"
}

fn dot(a: CartesianPosition, b: CartesianPosition) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}
fn cross(a: CartesianPosition, b: CartesianPosition) -> CartesianPosition {
    CartesianPosition::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}
fn normalize(value: CartesianPosition) -> Option<CartesianPosition> {
    let length = dot(value, value).sqrt();
    if length.is_finite() && length > 0.0 {
        Some(CartesianPosition::new(
            value.x / length,
            value.y / length,
            value.z / length,
        ))
    } else {
        None
    }
}
fn sub(a: CartesianPosition, b: CartesianPosition) -> CartesianPosition {
    CartesianPosition::new(a.x - b.x, a.y - b.y, a.z - b.z)
}
fn scale(value: CartesianPosition, factor: f64) -> CartesianPosition {
    CartesianPosition::new(value.x * factor, value.y * factor, value.z * factor)
}
fn transform(value: CartesianPosition, basis: [CartesianPosition; 3]) -> CartesianPosition {
    CartesianPosition::new(
        dot(value, basis[0]),
        dot(value, basis[1]),
        dot(value, basis[2]),
    )
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod builtin_tests {
    use super::{
        BUILTIN_GENERIC_HRTF_SAMPLE_RATE_HZ, BuiltinHrtf, load_builtin_generic_hrir,
        load_builtin_hrir, resolve_hrir,
    };
    use openjoc_render::{
        BinauralRenderer, BinauralSourceBlock, CartesianPosition, HrirBank, HrirEntry, HrirEntryId,
        SourceId, StaticBinauralSource,
    };

    #[test]
    fn bundled_generic_resource_round_trips_through_the_strict_sofa_path() {
        let loaded = load_builtin_generic_hrir().expect("bundled SADIE II resource");
        assert_eq!(
            loaded.metadata.sample_rate_hz,
            BUILTIN_GENERIC_HRTF_SAMPLE_RATE_HZ
        );
        assert_eq!(loaded.metadata.measurement_count, 8_817);
        assert_eq!(loaded.metadata.original_fir_length, 256);
        assert_eq!(loaded.metadata.expanded_max_tap_length, 256);
        assert_eq!(loaded.bank.entries().len(), 8_817);
    }

    #[test]
    fn bundled_generic_resource_covers_the_admitted_virtual_directions() {
        let loaded = load_builtin_generic_hrir().expect("bundled SADIE II resource");
        for (name, direction) in [
            ("front", CartesianPosition::new(0.0, 1.0, 0.0)),
            ("30-left", CartesianPosition::new(-0.5, 0.8660254, 0.0)),
            ("left", CartesianPosition::new(-1.0, 0.0, 0.0)),
            (
                "rear-left",
                CartesianPosition::new(
                    -std::f64::consts::FRAC_1_SQRT_2,
                    -std::f64::consts::FRAC_1_SQRT_2,
                    0.0,
                ),
            ),
            ("rear", CartesianPosition::new(0.0, -1.0, 0.0)),
            (
                "upper-front",
                CartesianPosition::new(
                    0.0,
                    std::f64::consts::FRAC_1_SQRT_2,
                    std::f64::consts::FRAC_1_SQRT_2,
                ),
            ),
            ("zenith", CartesianPosition::new(0.0, 0.0, 1.0)),
            (
                "lower-front",
                CartesianPosition::new(
                    0.0,
                    std::f64::consts::FRAC_1_SQRT_2,
                    -std::f64::consts::FRAC_1_SQRT_2,
                ),
            ),
            ("api-top-front", CartesianPosition::new(-1.0, 0.0, 1.0)),
            ("api-top-rear", CartesianPosition::new(-1.0, 1.0, 1.0)),
            ("api-top-middle", CartesianPosition::new(-1.0, 0.0, 1.0)),
            ("api-wide", CartesianPosition::new(-1.0, -0.2, 0.0)),
        ] {
            let resolved = resolve_hrir(&loaded.bank, direction)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert!(resolved.pair.left_taps().iter().all(|tap| tap.is_finite()));
            assert!(resolved.pair.right_taps().iter().all(|tap| tap.is_finite()));
            assert!(resolved.neighbor_count >= 1);
        }
    }

    #[test]
    fn built_in_registry_has_stable_ids_and_all_resources_load() {
        let ids = BuiltinHrtf::all()
            .iter()
            .map(|preset| preset.id())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["sadie-ii-d1-ku100", "sadie-ii-d2-kemar"]);
        for preset in BuiltinHrtf::all() {
            let metadata = preset.metadata();
            let loaded = load_builtin_hrir(*preset).expect("built-in HRTF resource");
            assert_eq!(loaded.metadata.sample_rate_hz, metadata.sample_rate_hz);
            assert_eq!(loaded.metadata.original_fir_length, metadata.ir_length);
            let expected_entries = metadata.measurement_count + 15;
            assert_eq!(loaded.metadata.measurement_count, expected_entries);
            assert!(
                loaded
                    .bank
                    .entries()
                    .iter()
                    .flat_map(|entry| entry.pair().left_taps())
                    .all(|sample| sample.is_finite())
            );
        }
    }

    #[test]
    #[ignore = "manual release performance harness"]
    fn builtin_hrtf_performance_harness() {
        use std::{hint::black_box, mem::size_of, time::Instant};

        let directions = [
            CartesianPosition::new(0.0, 1.0, 0.0),
            CartesianPosition::new(-1.0, 1.0, 0.0),
            CartesianPosition::new(-1.0, 0.0, 0.0),
            CartesianPosition::new(-1.0, -1.0, 0.0),
            CartesianPosition::new(-1.0, 1.0, 1.0),
            CartesianPosition::new(-1.0, 0.0, 1.0),
            CartesianPosition::new(-1.0, -1.0, 1.0),
            CartesianPosition::new(0.0, 0.0, 1.0),
        ];
        for preset in BuiltinHrtf::all() {
            let load_started = Instant::now();
            let loaded = load_builtin_hrir(*preset).expect("built-in HRTF");
            let load_ms = load_started.elapsed().as_secs_f64() * 1_000.0;
            let resolve_started = Instant::now();
            for direction in directions {
                black_box(resolve_hrir(&loaded.bank, direction).expect("built-in coverage"));
            }
            let resolve_ms = resolve_started.elapsed().as_secs_f64() * 1_000.0;
            let mut entries = Vec::new();
            let mut sources = Vec::new();
            for (index, direction) in directions.into_iter().enumerate() {
                let resolved = resolve_hrir(&loaded.bank, direction).expect("render coverage");
                let entry = HrirEntry::new(
                    HrirEntryId::new(index as u64 + 1_000),
                    direction,
                    resolved.pair,
                )
                .expect("render entry");
                let source = StaticBinauralSource::new(
                    SourceId::new(index as u64 + 1),
                    direction,
                    1.0,
                    entry.id(),
                )
                .expect("render source");
                entries.push(entry);
                sources.push(source);
            }
            let bank = HrirBank::new(loaded.bank.sample_rate_hz(), entries).expect("render bank");
            let mut renderer = BinauralRenderer::new(48_000, bank, sources).expect("renderer");
            let input = [0.0; 256];
            let mut left = [0.0; 256];
            let mut right = [0.0; 256];
            let blocks = directions
                .into_iter()
                .enumerate()
                .map(|(index, _)| BinauralSourceBlock::new(SourceId::new(index as u64 + 1), &input))
                .collect::<Vec<_>>();
            let render_started = Instant::now();
            for _ in 0..200 {
                renderer
                    .render_block(&blocks, &mut left, &mut right)
                    .expect("render block");
            }
            let render_ms = render_started.elapsed().as_secs_f64() * 1_000.0;
            let estimated_tap_bytes = loaded
                .bank
                .entries()
                .iter()
                .map(|entry| entry.pair().tap_count() * 2 * size_of::<f64>())
                .sum::<usize>();
            println!(
                "preset={} load_ms={load_ms:.3} resolve_8_directions_ms={resolve_ms:.3} render_51200_samples_8_sources_ms={render_ms:.3} entries={} estimated_hrir_tap_bytes={estimated_tap_bytes}",
                preset.id(),
                loaded.bank.entries().len(),
            );
        }
    }
}
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

#[cfg(test)]
mod nearest_candidate_tests {
    use super::{
        BuiltinHrtf, Candidate, CartesianPosition, HrirBank, HrirEntry, HrirEntryId, HrirPair,
        Ordering, TangentProjection, angular_distance, dot_array,
        find_interpolation_neighborhood_for_candidate_expansion,
        find_interpolation_neighborhood_for_listener_orientation,
        find_interpolation_neighborhood_with_candidate_limit,
        find_interpolation_neighborhood_with_options, interpolate_f32_pair, load_builtin_hrir_f32,
        nearest_candidates, nearest_candidates_with_limit, normalize, resolve_hrir_f32,
        resolve_hrir_f32_for_listener_orientation, resolve_hrir_for_listener_orientation,
    };

    #[test]
    fn nearest_candidate_selection_orders_by_dot_and_stable_index() {
        let directions = vec![
            (9, [0.0, 1.0, 0.0]),
            (2, [0.0, 1.0, 0.0]),
            (3, [0.0, 0.8, 0.6]),
            (4, [0.0, 0.0, 1.0]),
            (5, [0.0, -1.0, 0.0]),
            (6, [1.0, 0.0, 0.0]),
            (7, [-1.0, 0.0, 0.0]),
            (8, [0.0, -0.8, -0.6]),
            (10, [0.0, 0.6, 0.8]),
        ];
        let selected = nearest_candidates([0.0, 1.0, 0.0], directions.into_iter());
        let indices = selected
            .iter()
            .map(|candidate| candidate.index)
            .collect::<Vec<_>>();
        assert_eq!(indices, [2, 9, 3, 10, 4, 6, 7, 8]);
    }

    #[test]
    fn bounded_heap_candidates_match_full_stable_sort() {
        let target = [0.0, 1.0, 0.0];
        let directions = vec![
            (0, [0.0, 1.0, 0.0]),
            (1, [0.0, -1.0, 0.0]),
            (2, [1.0, 0.0, 0.0]),
            (3, [-1.0, 0.0, 0.0]),
            (4, [0.0, 0.6, 0.8]),
            (5, [0.0, 0.6, -0.8]),
            (6, [0.0, 0.6, 0.8]),
            (7, [0.0, 0.0, 1.0]),
            (8, [0.0, 0.0, -1.0]),
            (9, [0.0, 0.8, 0.6]),
        ];
        for limit in [0, 1, 2, 3, 8, 16] {
            let mut expected = directions
                .iter()
                .map(|(index, direction)| Candidate {
                    index: *index,
                    angle: 0.0,
                    dot: dot_array(target, *direction).clamp(-1.0, 1.0),
                })
                .collect::<Vec<_>>();
            expected.sort_by(|left, right| {
                if left.dot == right.dot {
                    left.index.cmp(&right.index)
                } else {
                    right.dot.partial_cmp(&left.dot).unwrap_or(Ordering::Equal)
                }
            });
            expected.truncate(limit);
            let actual = nearest_candidates_with_limit(target, directions.iter().copied(), limit);
            assert_eq!(
                actual
                    .iter()
                    .map(|candidate| candidate.index)
                    .collect::<Vec<_>>(),
                expected
                    .iter()
                    .map(|candidate| candidate.index)
                    .collect::<Vec<_>>(),
                "candidate limit {limit}"
            );
            assert_eq!(
                actual
                    .iter()
                    .map(|candidate| candidate.dot)
                    .collect::<Vec<_>>(),
                expected
                    .iter()
                    .map(|candidate| candidate.dot)
                    .collect::<Vec<_>>(),
                "candidate limit {limit}"
            );
        }
    }

    #[test]
    fn orientation_resolver_expands_the_search_without_changing_static_lookup() {
        let targets = [
            CartesianPosition::new(-1.017_300_101_593_674_7, 0.982_395_288_719_107_8, 1.0),
            CartesianPosition::new(-1.0, 0.965_925_826_289_068_3, 0.258_819_045_102_520_74),
        ];
        for preset in BuiltinHrtf::all() {
            let bank = load_builtin_hrir_f32(*preset)
                .expect("built-in f32 HRTF")
                .bank;
            for target in targets {
                assert!(resolve_hrir_f32(&bank, target).is_err());
                let resolved = resolve_hrir_f32_for_listener_orientation(&bank, target)
                    .expect("bounded orientation-only candidate expansion");
                assert_eq!(resolved.neighbor_count, 3);
                assert_eq!(resolved.pair.sample_rate_hz(), 48_000);
                assert!(resolved.pair.tap_count() <= 512);
                assert!(
                    resolved
                        .pair
                        .left_taps()
                        .iter()
                        .chain(resolved.pair.right_taps())
                        .all(|tap| tap.is_finite())
                );
            }
        }
    }

    #[test]
    fn orientation_expansion_matches_the_rescan_reference_bit_for_bit() {
        const CANDIDATE_LIMITS: [usize; 7] = [8, 16, 32, 48, 64, 96, 128];
        let mut targets = vec![
            normalize(CartesianPosition::new(
                -1.017_300_101_593_674_7,
                0.982_395_288_719_107_8,
                1.0,
            ))
            .unwrap(),
            normalize(CartesianPosition::new(
                -1.0,
                0.965_925_826_289_068_3,
                0.258_819_045_102_520_74,
            ))
            .unwrap(),
        ];
        // Deterministic spread over the sphere supplements the boundary cases
        // above with continuous off-grid directions from many orientations.
        let golden_angle = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
        for sample in 0..96 {
            let z = 1.0 - 2.0 * (sample as f64 + 0.5) / 96.0;
            let radius = (1.0 - z * z).sqrt();
            let azimuth = sample as f64 * golden_angle;
            targets.push(CartesianPosition::new(
                radius * azimuth.cos(),
                radius * azimuth.sin(),
                z,
            ));
        }

        for preset in BuiltinHrtf::all() {
            let bank = load_builtin_hrir_f32(*preset)
                .expect("built-in f32 HRTF")
                .bank;
            for target in targets.iter().copied() {
                let target = normalize(target).unwrap();
                let target = [target.x, target.y, target.z];
                let optimized = find_interpolation_neighborhood_for_listener_orientation(
                    target,
                    bank.direction_count(),
                    |index| bank.records()[index].direction,
                );
                let mut reference = None;
                for limit in CANDIDATE_LIMITS {
                    match find_interpolation_neighborhood_with_options(
                        target,
                        bank.direction_count(),
                        |index| bank.records()[index].direction,
                        limit,
                    ) {
                        Ok(neighborhood) => {
                            reference = Some(Ok(neighborhood));
                            break;
                        }
                        Err(error @ super::SofaError::InterpolationOutsideCoverage(_)) => {
                            reference = Some(Err(error));
                        }
                        Err(error) => {
                            reference = Some(Err(error));
                            break;
                        }
                    }
                }
                let reference = reference.expect("candidate tiers are non-empty");
                match (optimized, reference) {
                    (Ok(optimized), Ok(reference)) => {
                        assert_eq!(optimized.indices, reference.indices);
                        assert_eq!(optimized.weights, reference.weights);
                        let optimized_pair =
                            interpolate_f32_pair(&bank, &optimized.indices, &optimized.weights)
                                .unwrap();
                        let reference_pair =
                            interpolate_f32_pair(&bank, &reference.indices, &reference.weights)
                                .unwrap();
                        assert_eq!(optimized_pair, reference_pair);
                    }
                    (Err(optimized), Err(reference)) => {
                        assert_eq!(optimized.to_string(), reference.to_string());
                    }
                    (optimized, reference) => panic!(
                        "optimized/reference coverage differs for {}: {optimized:?} vs {reference:?}",
                        preset.id(),
                    ),
                }
            }
        }
    }

    #[test]
    fn orientation_resolver_still_rejects_uncovered_directions() {
        let bank = HrirBank::new(
            48_000,
            vec![
                HrirEntry::new(
                    HrirEntryId::new(1),
                    CartesianPosition::new(0.0, 1.0, 0.0),
                    HrirPair::new(48_000, vec![1.0], vec![1.0]).unwrap(),
                )
                .unwrap(),
                HrirEntry::new(
                    HrirEntryId::new(2),
                    CartesianPosition::new(1.0, 0.0, 0.0),
                    HrirPair::new(48_000, vec![0.5], vec![0.5]).unwrap(),
                )
                .unwrap(),
            ],
        )
        .unwrap();
        let error =
            resolve_hrir_for_listener_orientation(&bank, CartesianPosition::new(-1.0, 0.0, 0.0))
                .unwrap_err();
        assert!(matches!(
            error,
            super::SofaError::InterpolationOutsideCoverage(_)
        ));
    }

    #[test]
    #[ignore = "manual diagnostic comparing bounded spherical-search candidate windows"]
    fn builtin_orientation_candidate_window_probe() {
        let targets = [
            (
                "top-front-left yaw -1 degree",
                [-1.017_300_101_593_674_7, 0.982_395_288_719_107_8, 1.0],
            ),
            (
                "top-front-left yaw +1 degree",
                [-0.982_395_288_719_107_8, 1.017_300_101_593_674_7, 1.0],
            ),
            (
                "front-left pitch +15 degrees",
                [-1.0, 0.965_925_826_289_068_3, 0.258_819_045_102_520_74],
            ),
            (
                "composite yaw30 pitch20 roll-15 source-1",
                [
                    -0.474_475_771_917_261_3,
                    1.283_643_991_742_328,
                    -0.356_554_125_382_586_8,
                ],
            ),
        ];
        for preset in BuiltinHrtf::all() {
            let bank = load_builtin_hrir_f32(*preset)
                .expect("built-in f32 HRTF")
                .bank;
            for (label, direction) in targets {
                let direction = normalize(CartesianPosition::new(
                    direction[0],
                    direction[1],
                    direction[2],
                ))
                .expect("finite nonzero direction");
                let target = [direction.x, direction.y, direction.z];
                for candidate_limit in [8, 16, 32, 48, 64, 96, 128, 192, 256] {
                    let result = find_interpolation_neighborhood_with_candidate_limit(
                        target,
                        bank.direction_count(),
                        |index| bank.records()[index].direction,
                        candidate_limit,
                    );
                    match result {
                        Ok(neighborhood) => {
                            let vertices = neighborhood
                                .indices
                                .iter()
                                .map(|&index| bank.records()[index].direction)
                                .collect::<Vec<_>>();
                            let max_edge_degrees = vertices
                                .iter()
                                .enumerate()
                                .flat_map(|(first, direction)| {
                                    vertices[first + 1..]
                                        .iter()
                                        .map(move |other| angular_distance(*direction, *other))
                                })
                                .fold(0.0_f64, f64::max)
                                .to_degrees();
                            let max_vertex_degrees = vertices
                                .iter()
                                .map(|vertex| angular_distance(target, *vertex))
                                .fold(0.0_f64, f64::max)
                                .to_degrees();
                            let pair = interpolate_f32_pair(
                                &bank,
                                &neighborhood.indices,
                                &neighborhood.weights,
                            );
                            match pair {
                                Ok(pair) => eprintln!(
                                    "candidate-window preset={} query={label} limit={candidate_limit} selected={:?} weights={:?} resolved_taps={} max_edge={:.2}deg max_vertex={:.2}deg",
                                    preset.id(),
                                    neighborhood.indices,
                                    neighborhood.weights,
                                    pair.tap_count(),
                                    max_edge_degrees,
                                    max_vertex_degrees,
                                ),
                                Err(error) => eprintln!(
                                    "candidate-window preset={} query={label} limit={candidate_limit} geometry=found kernel=reject {error}",
                                    preset.id(),
                                ),
                            }
                        }
                        Err(error) => eprintln!(
                            "candidate-window preset={} query={label} limit={candidate_limit} reject {error}",
                            preset.id(),
                        ),
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "manual diagnostic separating orientation candidate ranking and geometry costs"]
    fn profile_orientation_search_components() {
        const CANDIDATE_LIMITS: [usize; 7] = [8, 16, 32, 48, 64, 96, 128];
        let golden_angle = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
        let targets = (0..48)
            .map(|sample| {
                let z = 1.0 - 2.0 * (sample as f64 + 0.5) / 48.0;
                let radius = (1.0 - z * z).sqrt();
                let azimuth = sample as f64 * golden_angle;
                CartesianPosition::new(radius * azimuth.cos(), radius * azimuth.sin(), z)
            })
            .collect::<Vec<_>>();
        for preset in BuiltinHrtf::all() {
            let bank = load_builtin_hrir_f32(*preset)
                .expect("built-in f32 HRTF")
                .bank;
            let mut ranking_ns = Vec::new();
            let mut projection_ns = Vec::new();
            let mut geometry_ns = Vec::new();
            let mut deepest_window = 0;
            for target in targets.iter().copied() {
                let target = normalize(target).unwrap();
                let target = [target.x, target.y, target.z];
                let started = std::time::Instant::now();
                let candidates = nearest_candidates_with_limit(
                    target,
                    (0..bank.direction_count())
                        .map(|index| (index, bank.records()[index].direction)),
                    128,
                );
                ranking_ns.push(started.elapsed().as_nanos());

                let started = std::time::Instant::now();
                let candidate_directions = candidates
                    .iter()
                    .map(|candidate| bank.records()[candidate.index].direction)
                    .collect::<Vec<_>>();
                let candidate_projections = TangentProjection::new(target).map(|projection| {
                    candidate_directions
                        .iter()
                        .copied()
                        .map(|direction| projection.project(target, direction))
                        .collect::<Vec<_>>()
                });
                projection_ns.push(started.elapsed().as_nanos());

                let started = std::time::Instant::now();
                let mut previous_limit = 0;
                for limit in CANDIDATE_LIMITS {
                    let limit = limit.min(candidates.len());
                    if limit == previous_limit {
                        continue;
                    }
                    match find_interpolation_neighborhood_for_candidate_expansion(
                        target,
                        &candidates,
                        &candidate_directions,
                        candidate_projections.as_deref(),
                        previous_limit,
                        limit,
                    ) {
                        Ok(_) => {
                            deepest_window = deepest_window.max(limit);
                            break;
                        }
                        Err(_) => previous_limit = limit,
                    }
                }
                geometry_ns.push(started.elapsed().as_nanos());
            }
            for (name, timings) in [
                ("rank", &mut ranking_ns),
                ("project", &mut projection_ns),
                ("geometry", &mut geometry_ns),
            ] {
                timings.sort_unstable();
                let sum = timings.iter().sum::<u128>();
                eprintln!(
                    "orientation profile {} {name}: n={} avg={:.3} us p50={:.3} us p95={:.3} us max={:.3} us",
                    preset.id(),
                    timings.len(),
                    sum as f64 / timings.len() as f64 / 1_000.0,
                    timings[timings.len() / 2] as f64 / 1_000.0,
                    timings[(timings.len() * 95).div_ceil(100).saturating_sub(1)] as f64 / 1_000.0,
                    timings[timings.len() - 1] as f64 / 1_000.0,
                );
            }
            eprintln!(
                "orientation profile {} candidate window maximum used={} of 128 across {} query directions",
                preset.id(),
                deepest_window,
                targets.len(),
            );
        }
    }
}
impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    fn skip(&mut self, count: usize) -> Result<(), SofaError> {
        self.pos = self
            .pos
            .checked_add(count)
            .ok_or(SofaError::TruncatedContainer)?;
        if self.pos > self.data.len() {
            return Err(SofaError::TruncatedContainer);
        }
        Ok(())
    }
    fn bytes(&mut self, count: usize) -> Result<&'a [u8], SofaError> {
        let start = self.pos;
        self.skip(count)?;
        Ok(&self.data[start..self.pos])
    }
    fn u32(&mut self) -> Result<u32, SofaError> {
        Ok(u32::from_be_bytes(
            self.bytes(4)?
                .try_into()
                .map_err(|_| SofaError::TruncatedContainer)?,
        ))
    }
    fn count(&mut self) -> Result<usize, SofaError> {
        let value = self.u32()? as usize;
        if value > 1_000_000 {
            return Err(SofaError::ResourceLimitExceeded("container count"));
        }
        Ok(value)
    }
    fn align4(&mut self) -> Result<(), SofaError> {
        let aligned = (self.pos + 3) & !3;
        self.skip(aligned - self.pos)
    }
    fn string(&mut self, max: usize) -> Result<String, SofaError> {
        let len = self.count()?;
        if len > max {
            return Err(SofaError::ResourceLimitExceeded("metadata bytes"));
        }
        let bytes = self.bytes(len)?;
        let text = String::from_utf8(bytes.to_vec())
            .map_err(|_| SofaError::InvalidContainer("UTF-8 string"))?;
        self.align4()?;
        Ok(text)
    }
}

/// Opt-in diagnostic timing, excluded from ordinary builds and render paths.
#[cfg(feature = "orientation-profile")]
pub mod orientation_profile {
    use std::{cell::Cell, time::Instant};
    thread_local! { static TOTALS: Cell<[u64; 6]> = const { Cell::new([0; 6]) }; }
    pub struct Span {
        stage: usize,
        start: Instant,
    }
    impl Span {
        pub fn new(stage: usize) -> Self {
            Self {
                stage,
                start: Instant::now(),
            }
        }
    }
    impl Drop for Span {
        fn drop(&mut self) {
            TOTALS.with(|totals| {
                let mut values = totals.get();
                values[self.stage] += self.start.elapsed().as_nanos() as u64;
                totals.set(values);
            });
        }
    }
    /// Transform, exact lookup, ranking, geometry, taps, update construction (ns).
    pub fn take() -> [u64; 6] {
        TOTALS.with(|totals| totals.replace([0; 6]))
    }
}

#[derive(Clone, Debug)]
struct OrientationDirectionIndex {
    order: Vec<usize>,
    nodes: Vec<DirectionNode>,
}

#[derive(Clone, Debug)]
struct DirectionNode {
    lower: [f64; 3],
    upper: [f64; 3],
    range: std::ops::Range<usize>,
    children: Option<[usize; 2]>,
}

impl OrientationDirectionIndex {
    fn new(bank: &BuiltinHrirF32Bank) -> Self {
        let mut index = Self {
            order: (0..bank.direction_count()).collect(),
            nodes: Vec::new(),
        };
        index.build(bank, 0..bank.direction_count());
        index
    }

    fn build(&mut self, bank: &BuiltinHrirF32Bank, range: std::ops::Range<usize>) -> usize {
        let mut lower = [f64::INFINITY; 3];
        let mut upper = [f64::NEG_INFINITY; 3];
        for &i in &self.order[range.clone()] {
            for axis in 0..3 {
                lower[axis] = lower[axis].min(bank.records()[i].direction[axis]);
                upper[axis] = upper[axis].max(bank.records()[i].direction[axis]);
            }
        }
        let node = self.nodes.len();
        self.nodes.push(DirectionNode {
            lower,
            upper,
            range: range.clone(),
            children: None,
        });
        if range.len() > 24 {
            let axis = (0..3)
                .max_by(|&a, &b| (upper[a] - lower[a]).total_cmp(&(upper[b] - lower[b])))
                .unwrap();
            self.order[range.clone()].sort_unstable_by(|&a, &b| {
                bank.records()[a].direction[axis]
                    .total_cmp(&bank.records()[b].direction[axis])
                    .then(a.cmp(&b))
            });
            let middle = range.start + range.len() / 2;
            let left = self.build(bank, range.start..middle);
            let right = self.build(bank, middle..range.end);
            self.nodes[node].children = Some([left, right]);
        }
        node
    }

    fn upper_dot(&self, node: usize, target: [f64; 3]) -> f64 {
        let node = &self.nodes[node];
        let products = std::array::from_fn::<_, 3, _>(|axis| {
            target[axis]
                * if target[axis] >= 0.0 {
                    node.upper[axis]
                } else {
                    node.lower[axis]
                }
        });
        // Round outward conservatively for three products and two additions.
        // Prune only on strict inequality; equal dots must retain lowest index.
        products.iter().sum::<f64>()
            + 16.0 * f64::EPSILON * (1.0 + products.iter().map(|x| x.abs()).sum::<f64>())
    }

    fn exact(&self, bank: &BuiltinHrirF32Bank, target: [f64; 3]) -> Option<usize> {
        let mut found = None;
        self.visit_exact(0, bank, target, &mut found);
        found
    }

    fn visit_exact(
        &self,
        node: usize,
        bank: &BuiltinHrirF32Bank,
        target: [f64; 3],
        found: &mut Option<usize>,
    ) {
        if self.upper_dot(node, target) < 1.0 - 1.0e-12 {
            return;
        }
        if let Some([left, right]) = self.nodes[node].children {
            self.visit_exact(left, bank, target, found);
            self.visit_exact(right, bank, target, found);
        } else {
            for &i in &self.order[self.nodes[node].range.clone()] {
                if found.is_none_or(|current| i < current)
                    && directions_match(target, bank.records()[i].direction)
                {
                    *found = Some(i);
                }
            }
        }
    }

    fn nearest(&self, bank: &BuiltinHrirF32Bank, target: [f64; 3], limit: usize) -> Vec<Candidate> {
        let mut heap = BinaryHeap::with_capacity(limit);
        self.visit(0, bank, target, limit, &mut heap);
        sorted_candidates(heap)
    }

    fn visit(
        &self,
        node: usize,
        bank: &BuiltinHrirF32Bank,
        target: [f64; 3],
        limit: usize,
        heap: &mut BinaryHeap<CandidateHeapEntry>,
    ) {
        if heap.len() == limit
            && heap
                .peek()
                .is_some_and(|worst| self.upper_dot(node, target) < worst.0.dot)
        {
            return;
        }
        if let Some([left, right]) = self.nodes[node].children {
            let (first, second) = if self.upper_dot(left, target) >= self.upper_dot(right, target) {
                (left, right)
            } else {
                (right, left)
            };
            self.visit(first, bank, target, limit, heap);
            self.visit(second, bank, target, limit, heap);
        } else {
            for &i in &self.order[self.nodes[node].range.clone()] {
                let candidate = CandidateHeapEntry(Candidate {
                    index: i,
                    dot: dot_array(target, bank.records()[i].direction).clamp(-1.0, 1.0),
                    angle: 0.0,
                });
                if heap.len() < limit {
                    heap.push(candidate);
                } else if heap
                    .peek()
                    .is_some_and(|worst| is_better_candidate(&candidate.0, &worst.0))
                {
                    heap.pop();
                    heap.push(candidate);
                }
            }
        }
    }
}

#[cfg(test)]
mod orientation_index_tests {
    use super::*;

    #[test]
    fn indexed_exact_lookup_preserves_first_match_for_every_builtin_record() {
        for preset in BuiltinHrtf::all() {
            let bank = load_builtin_hrir_f32(*preset).unwrap().bank;
            let index = OrientationDirectionIndex::new(&bank);
            for record in bank.records() {
                let direction = normalize(CartesianPosition::new(
                    record.direction[0],
                    record.direction[1],
                    record.direction[2],
                ))
                .unwrap();
                let target = [direction.x, direction.y, direction.z];
                assert_eq!(
                    index.exact(&bank, target),
                    bank.records()
                        .iter()
                        .position(|r| directions_match(target, r.direction))
                );
            }
        }
    }

    #[test]
    fn indexed_ranking_and_complete_hrirs_match_scan_for_rapid_and_nearby_queries() {
        for preset in BuiltinHrtf::all() {
            let bank = std::sync::Arc::new(load_builtin_hrir_f32(*preset).unwrap().bank);
            let indexed = ListenerOrientationHrirBank::new(bank.clone());
            let mut targets = vec![
                [1.0, 0.0, 0.0],
                [-1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, -1.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, -1.0],
            ];
            // Existing exact directions include aliases: first matching index must survive.
            targets.extend(bank.records().iter().step_by(97).map(|r| r.direction));
            for i in 0..720 {
                let z = 1.0 - 2.0 * (f64::from(i) + 0.5) / 720.0;
                let angle = f64::from(i) * 2.399_963_229_728_653;
                let radius = (1.0 - z * z).sqrt();
                targets.push([radius * angle.cos(), radius * angle.sin(), z]);
            }
            // Repeated queries interleaved with nearby queries cannot depend on cache history.
            for i in 0..40 {
                let angle = 0.3 + f64::from(i % 4) * 0.00001;
                targets.push([angle.cos(), angle.sin(), 0.0]);
            }
            for target in targets {
                let expected = nearest_candidates_with_limit(
                    target,
                    bank.records()
                        .iter()
                        .enumerate()
                        .map(|(i, r)| (i, r.direction)),
                    128,
                );
                let actual = indexed.index.nearest(&bank, target, 128);
                assert_eq!(actual.len(), expected.len());
                for (a, b) in actual.iter().zip(&expected) {
                    assert_eq!(
                        (a.index, a.dot.to_bits(), a.angle.to_bits()),
                        (b.index, b.dot.to_bits(), b.angle.to_bits())
                    );
                }
                let direction = CartesianPosition::new(target[0], target[1], target[2]);
                let expected = resolve_hrir_f32_for_listener_orientation(&bank, direction);
                let actual = indexed.resolve(direction);
                match (actual, expected) {
                    (Ok(a), Ok(b)) => {
                        assert_eq!(a.exact_entry, b.exact_entry);
                        assert_eq!(a.neighbor_count, b.neighbor_count);
                        assert_eq!(a.pair, b.pair);
                        for (a, b) in a
                            .pair
                            .left_taps()
                            .iter()
                            .chain(a.pair.right_taps())
                            .zip(b.pair.left_taps().iter().chain(b.pair.right_taps()))
                        {
                            assert_eq!(a.to_bits(), b.to_bits());
                        }
                    }
                    (Err(a), Err(b)) => assert_eq!(a.to_string(), b.to_string()),
                    (a, b) => panic!("result mismatch {a:?} {b:?}"),
                }
            }
            for direction in [
                CartesianPosition::new(0.0, 0.0, 0.0),
                CartesianPosition::new(f64::NAN, 1.0, 0.0),
            ] {
                assert_eq!(
                    indexed.resolve(direction).unwrap_err().to_string(),
                    resolve_hrir_f32_for_listener_orientation(&bank, direction)
                        .unwrap_err()
                        .to_string()
                );
            }
            eprintln!(
                "{preset:?} index capacity bytes={}",
                indexed.index.order.capacity() * std::mem::size_of::<usize>()
                    + indexed.index.nodes.capacity() * std::mem::size_of::<DirectionNode>()
            );
        }
    }
}
