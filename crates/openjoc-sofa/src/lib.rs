//! Strict, read-only ingestion for the `SimpleFreeFieldHRIR` SOFA subset.
//!
//! The reader intentionally implements the portable NetCDF classic CDF-1
//! container subset used by the project-owned fixture.  It has no native
//! dependency and rejects HDF5/NetCDF-4 containers explicitly.  This keeps
//! the normal OpenJOC build portable while making the supported input contract
//! honest and inspectable.  It is not a generic SOFA or NetCDF API.

use std::{fmt, fs, path::Path};

use openjoc_render::{CartesianPosition, HrirBank, HrirEntry, HrirEntryId, HrirPair};

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
            Self::UnsupportedContainerOrEncoding => {
                f.write_str("unsupported SOFA container: only NetCDF classic CDF-1 is supported")
            }
            Self::TruncatedContainer => f.write_str("truncated NetCDF container"),
            Self::InvalidContainer(message) => write!(f, "invalid NetCDF container: {message}"),
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

/// Loads a local, seekable CDF-1 `SimpleFreeFieldHRIR` file.
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

/// Parses a complete in-memory CDF-1 buffer.  This is public for callers that
/// already own a bounded file buffer; no global cache or file handle is kept.
pub fn parse_simple_free_field_hrir(
    data: &[u8],
    limits: SofaLoadLimits,
) -> Result<LoadedSofaHrirBank, SofaError> {
    if data.len() as u64 > limits.max_file_bytes {
        return Err(SofaError::ResourceLimitExceeded("file bytes"));
    }
    let file = NetcdfFile::parse(data, limits)?;
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
        if &data[..3] != b"CDF" {
            return Err(SofaError::UnsupportedContainerOrEncoding);
        }
        if data[3] != 1 {
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
            let value = match variable.ty {
                NC_BYTE => f64::from(i8::from_be_bytes([chunk[0]])),
                NC_SHORT => f64::from(i16::from_be_bytes([chunk[0], chunk[1]])),
                NC_INT => f64::from(i32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])),
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
                _ => return Err(SofaError::UnsupportedAttributeType(variable.name.clone())),
            };
            values.push(value);
        }
        Ok(values)
    }

    fn shape(&self, variable: &Variable) -> Vec<usize> {
        variable
            .dims
            .iter()
            .map(|index| self.dimensions[*index].len)
            .collect()
    }
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
    let ir_values = file.values(ir)?;
    if ir_values.len() != measurements * receivers * taps {
        return Err(SofaError::InvalidImpulseResponse(
            "Data.IR element count".to_string(),
        ));
    }
    if let Some(index) = ir_values.iter().position(|value| !value.is_finite()) {
        return Err(SofaError::InvalidImpulseResponse(format!(
            "non-finite tap at {index}"
        )));
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
    let receiver_local = receiver_positions
        .iter()
        .map(|position| transform(sub(*position, listener_position), basis))
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

    let delays = read_delays(file, measurements, receivers, limits)?;
    let mut entries: Vec<HrirEntry> = Vec::with_capacity(measurements);
    let mut max_expanded_taps = 0usize;
    let mut expanded_total = 0usize;
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
        if measurement > 0 {
            for (first, entry) in entries.iter().enumerate() {
                if same_direction(entry.direction(), direction) {
                    return Err(SofaError::DuplicateDirection {
                        first,
                        second: measurement,
                    });
                }
            }
        }
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
        left[left_delay..left_len].copy_from_slice(
            &ir_values[(measurement * receivers + left_receiver) * taps
                ..(measurement * receivers + left_receiver + 1) * taps],
        );
        right[right_delay..right_len].copy_from_slice(
            &ir_values[(measurement * receivers + right_receiver) * taps
                ..(measurement * receivers + right_receiver + 1) * taps],
        );
        let pair = HrirPair::new(sample_rate, left, right)
            .map_err(|_| SofaError::InvalidImpulseResponse("HrirPair validation".to_string()))?;
        entries.push(
            HrirEntry::new(HrirEntryId::new(measurement as u64), direction, pair)
                .map_err(|_| SofaError::InvalidCoordinate(format!("direction {measurement}")))?,
        );
    }
    if expanded_total > limits.max_total_coefficients {
        return Err(SofaError::ResourceLimitExceeded("total FIR coefficients"));
    }
    let bank = HrirBank::new(sample_rate, entries)
        .map_err(|_| SofaError::InvalidImpulseResponse("HrirBank validation".to_string()))?;
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
    let values = if shape == vec![receivers] {
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
            "Data.Delay must be [R] or [M,R]".to_string(),
        ));
    };
    let units = NetcdfFile::attr_text(&variable.attrs, "Units")
        .ok_or(SofaError::MissingAttribute("Data.Delay:Units"))?;
    if !units.to_ascii_lowercase().contains("sample") {
        return Err(SofaError::InvalidCoordinate(
            "Data.Delay units must be samples".to_string(),
        ));
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
    if !required_units.is_empty() {
        let units = NetcdfFile::attr_text(&variable.attrs, "Units")
            .ok_or(SofaError::MissingAttribute("listener units"))?;
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
    if shape != vec![rows, cols] {
        return Err(SofaError::InvalidDimension(format!(
            "{} must be [{rows},3]",
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
fn same_direction(current: [f64; 3], direction: CartesianPosition) -> bool {
    (current[0] * direction.x + current[1] * direction.y + current[2] * direction.z - 1.0).abs()
        <= 1.0e-12
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
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
