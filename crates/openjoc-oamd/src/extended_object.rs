// pattern: Functional Core

use crate::{OamdError, ObjectClass, ObjectElement};
use openjoc_bitio::{BitRead, BitReader};

/// Decoded clause 5.5.13 extended object metadata in object/block order.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtendedObjectElement {
    pub divergence: Option<Vec<Vec<f64>>>,
    pub extended_precision: Option<Vec<Vec<[Option<u8>; 3]>>>,
    pub consumed_bits: usize,
}

impl ExtendedObjectElement {
    /// Applies corresponding high-precision position codewords to updates.
    ///
    /// # Errors
    /// Returns a shape error when dimensions disagree, or a position error.
    pub fn apply_positions(&self, objects: &mut ObjectElement) -> Result<(), OamdError> {
        let Some(precision) = &self.extended_precision else {
            return Ok(());
        };
        if precision.len() != objects.objects.len() {
            return Err(OamdError::ExtendedObjectShapeMismatch);
        }
        for (extensions, updates) in precision.iter().zip(&mut objects.objects) {
            if extensions.len() != updates.len() {
                return Err(OamdError::ExtendedObjectShapeMismatch);
            }
            for (extension, update) in extensions.iter().zip(updates) {
                update.render.position = update.render.position_coding.decode(*extension)?;
            }
        }
        Ok(())
    }
}

/// Decodes table 41.
///
/// # Errors
/// Returns an invalid-property error outside the two-bit domain.
pub fn decode_object_divergence_table(code: u8) -> Result<f64, OamdError> {
    [0.500_755, 0.608_529, 0.704_833, 1.0]
        .get(usize::from(code))
        .copied()
        .ok_or(OamdError::InvalidPropertyCode)
}

/// Decodes table 42.
///
/// # Errors
/// Returns a reserved-code error for zero and invalid-property outside six bits.
pub fn decode_object_divergence_code(code: u8) -> Result<f64, OamdError> {
    const VALUES: [f64; 63] = [
        0.0, 0.004_026, 0.007_16, 0.012_731, 0.020_173, 0.028_485, 0.040_21, 0.050_582, 0.063_601,
        0.079_914, 0.100_299, 0.125_666, 0.140_532, 0.157_027, 0.175_282, 0.195_417, 0.217_536,
        0.241_718, 0.268_002, 0.296_377, 0.326_766, 0.359_017, 0.392_895, 0.428_081, 0.464_184,
        0.500_755, 0.537_316, 0.573_389, 0.608_529, 0.642_346, 0.674_524, 0.704_833, 0.733_123,
        0.759_32, 0.783_416, 0.805_451, 0.825_506, 0.843_686, 0.860_112, 0.874_914, 0.888_222,
        0.900_168, 0.910_875, 0.920_461, 0.929_035, 0.936_698, 0.943_544, 0.949_656, 0.955_112,
        0.959_98, 0.964_322, 0.968_195, 0.974_729, 0.979_923, 0.984_05, 0.987_33, 0.989_935,
        0.992_874, 0.994_955, 0.996_817, 0.998_21, 0.998_993, 1.0,
    ];
    if code == 0 {
        return Err(OamdError::ReservedObjectDivergenceCode);
    }
    VALUES
        .get(usize::from(code - 1))
        .copied()
        .ok_or(OamdError::InvalidPropertyCode)
}

/// Parses clauses 5.5.13 through 5.5.15.
///
/// # Errors
/// Returns an OAMD error for shape mismatch, truncation, reserved syntax,
/// invalid table codes, or reuse without a previous block.
pub fn parse_extended_object_element(
    payload: &[u8],
    objects: &ObjectElement,
    object_classes: &[ObjectClass],
) -> Result<ExtendedObjectElement, OamdError> {
    let mut reader = BitReader::new(payload);
    parse_extended_object_element_reader(&mut reader, objects, object_classes)
}

pub(crate) fn parse_extended_object_element_reader(
    reader: &mut BitReader<'_>,
    objects: &ObjectElement,
    object_classes: &[ObjectClass],
) -> Result<ExtendedObjectElement, OamdError> {
    validate_dimensions(objects, object_classes)?;
    let initial_bits = reader.bits_remaining();
    let divergence = if reader.read_bit()? {
        Some(parse_divergence(reader, objects, object_classes)?)
    } else {
        None
    };
    let extended_precision = if reader.read_bit()? {
        Some(parse_precision(reader, objects, object_classes)?)
    } else {
        None
    };
    Ok(ExtendedObjectElement {
        divergence,
        extended_precision,
        consumed_bits: initial_bits - reader.bits_remaining(),
    })
}

fn validate_dimensions(
    objects: &ObjectElement,
    object_classes: &[ObjectClass],
) -> Result<(), OamdError> {
    if objects.objects.len() != object_classes.len()
        || objects
            .objects
            .iter()
            .any(|updates| updates.len() != objects.timing.blocks.len())
    {
        return Err(OamdError::ExtendedObjectShapeMismatch);
    }
    Ok(())
}

fn parse_divergence(
    reader: &mut impl BitRead,
    objects: &ObjectElement,
    object_classes: &[ObjectClass],
) -> Result<Vec<Vec<f64>>, OamdError> {
    let mut result = Vec::with_capacity(objects.objects.len());
    for (updates, class) in objects.objects.iter().zip(object_classes) {
        let mut values = Vec::with_capacity(updates.len());
        for update in updates {
            let value = if update.active && *class == ObjectClass::Dynamic && reader.read_bit()? {
                match read_u8(reader, 2)? {
                    0 => decode_object_divergence_table(read_u8(reader, 2)?)?,
                    1 => *values
                        .last()
                        .ok_or(OamdError::MissingPreviousObjectDivergence)?,
                    2 => decode_object_divergence_code(read_u8(reader, 6)?)?,
                    3 => return Err(OamdError::ReservedObjectDivergenceMode),
                    _ => unreachable!(),
                }
            } else {
                0.0
            };
            values.push(value);
        }
        result.push(values);
    }
    Ok(result)
}

fn parse_precision(
    reader: &mut impl BitRead,
    objects: &ObjectElement,
    object_classes: &[ObjectClass],
) -> Result<Vec<Vec<[Option<u8>; 3]>>, OamdError> {
    let mut result = Vec::with_capacity(objects.objects.len());
    for (updates, class) in objects.objects.iter().zip(object_classes) {
        let mut values = Vec::with_capacity(updates.len());
        for update in updates {
            let extension =
                if !update.active || *class != ObjectClass::Dynamic || !reader.read_bit()? {
                    [None; 3]
                } else {
                    let presence = read_u8(reader, 3)?;
                    let x = if presence & 0b100 != 0 {
                        Some(read_u8(reader, 2)?)
                    } else {
                        None
                    };
                    let y = if presence & 0b010 != 0 {
                        Some(read_u8(reader, 2)?)
                    } else {
                        None
                    };
                    let z = if presence & 0b001 != 0 {
                        Some(read_u8(reader, 2)?)
                    } else {
                        None
                    };
                    [x, y, z]
                };
            values.push(extension);
        }
        result.push(values);
    }
    Ok(result)
}

fn read_u8(reader: &mut impl BitRead, width: u8) -> Result<u8, OamdError> {
    Ok(u8::try_from(reader.read_bits(width)?)?)
}
