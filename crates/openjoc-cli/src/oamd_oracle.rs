// pattern: Functional Core

use serde::Serialize;
use std::fmt;

#[derive(Clone, Debug, Serialize)]
pub struct OracleField {
    pub name: String,
    pub start_bit: usize,
    pub end_bit: usize,
    pub raw_bits: String,
    pub integer_value: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct OracleElement {
    pub index: usize,
    pub id: u8,
    pub header_start_bit: usize,
    pub header_end_bit: usize,
    pub body_start_bit: usize,
    pub body_end_bit: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct OracleTrace {
    pub payload_bits: usize,
    pub fields: Vec<OracleField>,
    pub elements: Vec<OracleElement>,
    pub warp_start_bit: usize,
    pub warp_end_bit: usize,
    pub warp_raw: u8,
    pub direct_byte_mask_raw: u8,
    pub payload_end_bit: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OracleError {
    Truncated {
        start: usize,
        end: usize,
        total: usize,
    },
    UnsupportedPrefix {
        field: String,
        value: u64,
    },
    InvalidElementCount {
        value: u64,
    },
    InvalidElementId {
        index: usize,
        value: u64,
    },
    InvalidElementSize {
        index: usize,
        value: u64,
    },
    NonzeroTrailingBits,
    MissingWarp,
}

impl fmt::Display for OracleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { start, end, total } => {
                write!(
                    formatter,
                    "oracle bit range [{start},{end}) exceeds {total} bits"
                )
            }
            Self::UnsupportedPrefix { field, value } => {
                write!(
                    formatter,
                    "oracle only supports observed prefix {field} value  {value}"
                )
            }
            Self::InvalidElementCount { value } => {
                write!(
                    formatter,
                    "oracle element count {value} is outside observed form"
                )
            }
            Self::InvalidElementId { index, value } => {
                write!(
                    formatter,
                    "oracle element {index} id {value} is outside 1/2"
                )
            }
            Self::InvalidElementSize { index, value } => {
                write!(formatter, "oracle element {index} size {value} is invalid")
            }
            Self::NonzeroTrailingBits => formatter.write_str("oracle found nonzero trailing bits"),
            Self::MissingWarp => formatter.write_str("oracle did not encounter element 2 warp"),
        }
    }
}

impl std::error::Error for OracleError {}

struct Reader<'a> {
    bits: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn new(bits: &'a [u8]) -> Result<Self, OracleError> {
        bits.len().checked_mul(8).ok_or(OracleError::Truncated {
            start: 0,
            end: usize::MAX,
            total: 0,
        })?;
        Ok(Self { bits, position: 0 })
    }

    fn total(&self) -> usize {
        self.bits.len() * 8
    }

    fn read(&mut self, width: usize, name: impl Into<String>) -> Result<OracleField, OracleError> {
        let start = self.position;
        let end = start.saturating_add(width);
        if end > self.total() {
            return Err(OracleError::Truncated {
                start,
                end,
                total: self.total(),
            });
        }
        let mut value = 0_u64;
        for bit in start..end {
            value = (value << 1) | u64::from((self.bits[bit / 8] >> (7 - bit % 8)) & 1);
        }
        self.position = end;
        Ok(OracleField {
            name: name.into(),
            start_bit: start,
            end_bit: end,
            raw_bits: raw_bits(self.bits, start, end),
            integer_value: value,
        })
    }

    fn variable(
        &mut self,
        width: usize,
        max_groups: usize,
        name: &str,
    ) -> Result<(u64, Vec<OracleField>), OracleError> {
        let mut value = 0_u64;
        let mut fields = Vec::new();
        for group in 0..max_groups {
            let group_value = self.read(width, format!("{name}.group{group}.value"))?;
            let more = self.read(1, format!("{name}.group{group}.more"))?;
            value = if group == 0 {
                group_value.integer_value
            } else {
                value.checked_add(group_value.integer_value).ok_or(
                    OracleError::InvalidElementSize {
                        index: group,
                        value,
                    },
                )?
            };
            fields.push(group_value);
            fields.push(more.clone());
            if more.integer_value == 0 {
                return Ok((value, fields));
            }
            value = value
                .checked_shl(width as u32)
                .and_then(|shifted| shifted.checked_add(1_u64 << width))
                .ok_or(OracleError::InvalidElementSize {
                    index: group,
                    value,
                })?;
        }
        Err(OracleError::InvalidElementSize { index: 0, value })
    }

    fn position(&self) -> usize {
        self.position
    }
}

pub fn trace_observed_payload(payload: &[u8]) -> Result<OracleTrace, OracleError> {
    let mut reader = Reader::new(payload)?;
    let mut fields = Vec::new();
    let syntax = reader.read(2, "syntax_version")?;
    fields.push(syntax.clone());
    if syntax.integer_value != 0 {
        return Err(OracleError::UnsupportedPrefix {
            field: syntax.name,
            value: syntax.integer_value,
        });
    }
    let object_code = reader.read(5, "object_count_code")?;
    fields.push(object_code.clone());
    if object_code.integer_value != 15 {
        return Err(OracleError::UnsupportedPrefix {
            field: object_code.name,
            value: object_code.integer_value,
        });
    }
    let assignment = reader.read(1, "program_assignment_dynamic_only")?;
    fields.push(assignment.clone());
    if assignment.integer_value != 1 {
        return Err(OracleError::UnsupportedPrefix {
            field: assignment.name,
            value: assignment.integer_value,
        });
    }
    let lfe = reader.read(1, "dynamic_lfe_present")?;
    fields.push(lfe.clone());
    let alternate = reader.read(1, "alternate_object_data_present")?;
    fields.push(alternate.clone());
    if alternate.integer_value != 0 {
        return Err(OracleError::UnsupportedPrefix {
            field: alternate.name,
            value: alternate.integer_value,
        });
    }
    let element_count = reader.read(4, "element_count")?;
    fields.push(element_count.clone());
    if element_count.integer_value != 2 {
        return Err(OracleError::InvalidElementCount {
            value: element_count.integer_value,
        });
    }

    let mut elements = Vec::new();
    let mut warp = None;
    for index in 0..2 {
        let header_start_bit = reader.position();
        let id = reader.read(4, format!("element{index}.id"))?;
        fields.push(id.clone());
        if !matches!(id.integer_value, 1 | 2) {
            return Err(OracleError::InvalidElementId {
                index,
                value: id.integer_value,
            });
        }
        let (size_minus_one, size_fields) =
            reader.variable(4, 4, &format!("element{index}.size"))?;
        fields.extend(size_fields);
        let size_bytes = size_minus_one + 1;
        if size_bytes == 0 || size_bytes > 4096 {
            return Err(OracleError::InvalidElementSize {
                index,
                value: size_bytes,
            });
        }
        let header_end_bit = reader.position();
        let body_start_bit = header_end_bit;
        let body_bits = size_bytes as usize * 8;
        let body_end_bit =
            body_start_bit
                .checked_add(body_bits)
                .ok_or(OracleError::InvalidElementSize {
                    index,
                    value: size_bytes,
                })?;
        if body_end_bit > reader.total() {
            return Err(OracleError::Truncated {
                start: body_start_bit,
                end: body_end_bit,
                total: reader.total(),
            });
        }
        if id.integer_value == 2 {
            let discard = reader.read(1, "element1.discard_unknown")?;
            fields.push(discard.clone());
            let warp_field = reader.read(2, "element1.warp_mode")?;
            fields.push(warp_field.clone());
            let reserved = reader.read(2, "element1.reserved_after_warp")?;
            fields.push(reserved.clone());
            let global = reader.read(2, "element1.global_trim_mode")?;
            fields.push(global.clone());
            let disable = reader.read(1, "element1.disable_trim_per_object_present")?;
            fields.push(disable.clone());
            warp = Some((
                warp_field.start_bit,
                warp_field.end_bit,
                warp_field.integer_value as u8,
            ));
        }
        reader.position = body_end_bit;
        elements.push(OracleElement {
            index,
            id: id.integer_value as u8,
            header_start_bit,
            header_end_bit,
            body_start_bit,
            body_end_bit,
        });
    }
    if reader.position() < reader.total() {
        let trailing = reader.read(reader.total() - reader.position(), "payload_trailing_bits")?;
        fields.push(trailing.clone());
        if trailing.integer_value != 0 {
            return Err(OracleError::NonzeroTrailingBits);
        }
    }
    let (warp_start_bit, warp_end_bit, warp_raw) = warp.ok_or(OracleError::MissingWarp)?;
    let byte_index = warp_start_bit / 8;
    let bit_offset = warp_start_bit % 8;
    let direct_byte_mask_raw = if bit_offset <= 6 {
        let shift = 6 - bit_offset;
        (payload[byte_index] >> shift) & 0b11
    } else {
        let first = (payload[byte_index] & 1) << 1;
        let second = payload.get(byte_index + 1).copied().unwrap_or_default() >> 7;
        first | second
    };
    Ok(OracleTrace {
        payload_bits: reader.total(),
        fields,
        elements,
        warp_start_bit,
        warp_end_bit,
        warp_raw,
        direct_byte_mask_raw,
        payload_end_bit: reader.total(),
    })
}

fn raw_bits(bytes: &[u8], start: usize, end: usize) -> String {
    (start..end)
        .map(|bit| char::from(b'0' + ((bytes[bit / 8] >> (7 - bit % 8)) & 1)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::trace_observed_payload;

    fn bits_to_bytes(bits: &str) -> Vec<u8> {
        assert_eq!(bits.len() % 8, 0);
        bits.as_bytes()
            .chunks(8)
            .map(|chunk| {
                chunk
                    .iter()
                    .fold(0_u8, |value, bit| (value << 1) | u8::from(*bit == b'1'))
            })
            .collect()
    }

    #[test]
    fn independent_oracle_closes_observed_two_element_shape() {
        let payload = bits_to_bytes(concat!(
            "00", "01111", "1", "1", "0", "0010", // prefix, 14 bits
            "0001", "00000", "00000000", // element 1: id, size, body
            "0010", "00000",
            "01100000", // element 2: id, size, discard/warp/reserved/global/disable
        ));
        let trace = trace_observed_payload(&payload).expect("oracle trace");
        assert_eq!(trace.payload_bits, 48);
        assert_eq!(trace.elements[0].body_start_bit, 23);
        assert_eq!(trace.elements[0].body_end_bit, 31);
        assert_eq!(trace.elements[1].body_start_bit, 40);
        assert_eq!(trace.warp_start_bit, 41);
        assert_eq!(trace.warp_end_bit, 43);
        assert_eq!(trace.warp_raw, 0b11);
        assert_eq!(trace.direct_byte_mask_raw, 0b11);
        assert_eq!(trace.payload_end_bit, 48);
    }
}
