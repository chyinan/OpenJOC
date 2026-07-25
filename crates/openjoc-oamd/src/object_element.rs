// pattern: Functional Core

use crate::{
    Extent3, Gain, MetadataTiming, OamdError, Position3, StandardPositionBits, ZoneConstraint,
    decode_absolute_position, decode_depth_factor, decode_differential_position,
    decode_distance_factor, decode_gain, decode_priority, decode_screen_factor, decode_size,
    decode_zone_constraints, timing::parse_metadata_timing_reader,
};
use openjoc_bitio::{BitRead, BitReader};

/// Object classification controlling render-info presence in clause 5.5.9.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectClass {
    BedOrIsf,
    Dynamic,
}

/// Room distance signalled by clauses 5.6.1.1.15 through 5.6.1.1.17.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Distance {
    InsideRoom,
    Finite(f64),
    Infinity,
}

/// Gain and priority after clause 5.6.4.7 update/reuse processing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectBasicInfo {
    pub gain: Gain,
    pub priority: f64,
}

impl ObjectBasicInfo {
    pub const DEFAULT: Self = Self {
        gain: Gain::NegativeInfinity,
        priority: 0.0,
    };
}

/// Render properties after clause 5.6.4.9 update/reuse processing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectRenderInfo {
    pub position: Position3,
    pub standard_position: StandardPositionBits,
    pub distance: Distance,
    pub zones: [ZoneConstraint; 6],
    pub size: Extent3,
    pub screen_anchor: bool,
    pub screen_factor: f64,
    pub depth_factor: f64,
    pub channel_lock: bool,
}

impl ObjectRenderInfo {
    pub const DEFAULT: Self = Self {
        position: Position3 {
            x: 0.5,
            y: 0.5,
            z: 0.0,
        },
        standard_position: StandardPositionBits { x: 31, y: 31, z: 0 },
        distance: Distance::InsideRoom,
        zones: [ZoneConstraint::Include; 6],
        size: Extent3::ZERO,
        screen_anchor: false,
        screen_factor: 0.0,
        depth_factor: 1.0,
        channel_lock: false,
    };
}

/// One fully resolved object property update.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectUpdate {
    pub active: bool,
    pub basic: ObjectBasicInfo,
    pub render: ObjectRenderInfo,
    /// Exact possibly unaligned byte-sized window reserved for additional data.
    pub additional_table_data: Option<Vec<u8>>,
}

/// Decoded clause 5.5.5 object element, organized object-major like the syntax.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectElement {
    pub timing: MetadataTiming,
    pub objects: Vec<Vec<ObjectUpdate>>,
    pub consumed_bits: usize,
}

/// Parses clauses 5.5.5 through 5.5.11 and resolves tables 28 through 31.
///
/// The caller supplies the content-description-derived object classes. Unknown
/// additional table data is retained losslessly inside its declared byte bound.
///
/// # Errors
/// Returns [`OamdError`] for truncation, reserved values, invalid property
/// coding, or size arithmetic overflow.
pub fn parse_object_element(
    payload: &[u8],
    object_classes: &[ObjectClass],
) -> Result<ObjectElement, OamdError> {
    let mut reader = BitReader::new(payload);
    parse_object_element_reader(&mut reader, object_classes)
}

pub(crate) fn parse_object_element_reader(
    reader: &mut BitReader<'_>,
    object_classes: &[ObjectClass],
) -> Result<ObjectElement, OamdError> {
    let initial_bits = reader.bits_remaining();
    let timing = parse_metadata_timing_reader(reader)?;
    if !reader.read_bit()? && reader.read_bits(5)? != 0 {
        return Err(OamdError::NonzeroReservedData);
    }

    let block_count = timing.blocks.len();
    let mut objects: Vec<Vec<ObjectUpdate>> = Vec::with_capacity(object_classes.len());
    for (object_index, class) in object_classes.iter().copied().enumerate() {
        let mut updates = Vec::with_capacity(block_count);
        for block_index in 0..block_count {
            let active = !reader.read_bit()?;
            let previous = updates.last();
            let basic_status = if !active {
                0
            } else if block_index == 0 {
                1
            } else {
                read_u8(reader, 2)?
            };
            let prior_gain = objects
                .get(object_index.wrapping_sub(1))
                .and_then(|object| object.get(block_index))
                .map(|update| update.basic.gain);
            let basic = parse_basic_info(reader, basic_status, previous, prior_gain)?;

            let render_status = if !active || class == ObjectClass::BedOrIsf {
                0
            } else if block_index == 0 {
                1
            } else {
                read_u8(reader, 2)?
            };
            let resolved_render = parse_render_info(reader, render_status, block_index, previous)?;
            let additional_table_data = if reader.read_bit()? {
                let byte_count = usize::from(read_u8(reader, 4)?) + 1;
                Some(read_byte_window(reader, byte_count)?)
            } else {
                None
            };
            updates.push(ObjectUpdate {
                active,
                basic,
                render: resolved_render,
                additional_table_data,
            });
        }
        objects.push(updates);
    }
    Ok(ObjectElement {
        timing,
        objects,
        consumed_bits: initial_bits - reader.bits_remaining(),
    })
}

fn parse_basic_info(
    reader: &mut impl BitRead,
    status: u8,
    previous: Option<&ObjectUpdate>,
    prior_gain: Option<Gain>,
) -> Result<ObjectBasicInfo, OamdError> {
    match status {
        0 => Ok(ObjectBasicInfo::DEFAULT),
        2 => previous
            .map(|update| update.basic)
            .ok_or(OamdError::MissingPreviousObjectUpdate),
        1 | 3 => {
            let mask = if status == 1 {
                0b11
            } else {
                read_u8(reader, 2)?
            };
            let base = previous.map_or(ObjectBasicInfo::DEFAULT, |update| update.basic);
            let gain = if mask & 0b10 != 0 {
                let index = read_u8(reader, 2)?;
                let bits = if index == 2 {
                    Some(read_u8(reader, 6)?)
                } else {
                    None
                };
                decode_gain(index, bits, prior_gain)?
            } else {
                base.gain
            };
            let priority = if mask & 1 != 0 {
                let default = reader.read_bit()?;
                decode_priority(
                    default,
                    if default {
                        None
                    } else {
                        Some(read_u8(reader, 5)?)
                    },
                )?
            } else {
                base.priority
            };
            Ok(ObjectBasicInfo { gain, priority })
        }
        _ => Err(OamdError::InvalidPropertyCode),
    }
}

fn parse_render_info(
    bits: &mut impl BitRead,
    status: u8,
    block_index: usize,
    previous: Option<&ObjectUpdate>,
) -> Result<ObjectRenderInfo, OamdError> {
    match status {
        0 => Ok(ObjectRenderInfo::DEFAULT),
        2 => previous
            .map(|update| update.render)
            .ok_or(OamdError::MissingPreviousObjectUpdate),
        1 | 3 => {
            let mask = if status == 1 {
                0b1111
            } else {
                read_u8(bits, 4)?
            };
            let mut render = previous.map_or(ObjectRenderInfo::DEFAULT, |update| update.render);
            if mask & 0b1000 != 0 {
                let differential = block_index != 0 && bits.read_bit()?;
                if differential {
                    let delta = [read_u8(bits, 3)?, read_u8(bits, 3)?, read_u8(bits, 3)?];
                    render.position =
                        decode_differential_position(render.standard_position, delta, [None; 3])?;
                    render.standard_position =
                        standard_after_delta(render.standard_position, delta)?;
                } else {
                    let x = read_u8(bits, 6)?;
                    let y = read_u8(bits, 6)?;
                    let positive = bits.read_bit()?;
                    let z_magnitude = read_u8(bits, 4)?;
                    render.position =
                        decode_absolute_position(x, y, positive, z_magnitude, [None; 3])?;
                    render.standard_position = StandardPositionBits {
                        x,
                        y,
                        z: if positive {
                            i8::try_from(z_magnitude)?
                        } else {
                            -i8::try_from(z_magnitude)?
                        },
                    };
                }
                render.distance = if bits.read_bit()? {
                    if bits.read_bit()? {
                        Distance::Infinity
                    } else {
                        Distance::Finite(decode_distance_factor(read_u8(bits, 4)?)?)
                    }
                } else {
                    Distance::InsideRoom
                };
            }
            if mask & 0b0100 != 0 {
                render.zones = decode_zone_constraints(read_u8(bits, 3)?, bits.read_bit()?)?;
            }
            if mask & 0b0010 != 0 {
                let index = read_u8(bits, 2)?;
                render.size = match index {
                    0 => decode_size(index, None, None)?,
                    1 => decode_size(index, Some(read_u8(bits, 5)?), None)?,
                    2 => decode_size(
                        index,
                        None,
                        Some([read_u8(bits, 5)?, read_u8(bits, 5)?, read_u8(bits, 5)?]),
                    )?,
                    3 => return Err(OamdError::ReservedSizeIndex),
                    _ => unreachable!(),
                };
            }
            if mask & 1 != 0 {
                render.screen_anchor = bits.read_bit()?;
                if render.screen_anchor {
                    render.screen_factor = decode_screen_factor(read_u8(bits, 3)?)?;
                    render.depth_factor = decode_depth_factor(read_u8(bits, 2)?)?;
                } else {
                    render.screen_factor = 0.0;
                    render.depth_factor = 1.0;
                }
            }
            render.channel_lock = bits.read_bit()?;
            Ok(render)
        }
        _ => Err(OamdError::InvalidPropertyCode),
    }
}

fn standard_after_delta(
    previous: StandardPositionBits,
    delta: [u8; 3],
) -> Result<StandardPositionBits, OamdError> {
    use crate::decode_signed_position_delta;
    let x = i16::from(previous.x) + i16::from(decode_signed_position_delta(delta[0])?);
    let y = i16::from(previous.y) + i16::from(decode_signed_position_delta(delta[1])?);
    let z = i16::from(previous.z) + i16::from(decode_signed_position_delta(delta[2])?);
    Ok(StandardPositionBits {
        x: u8::try_from(x.clamp(0, 62))?,
        y: u8::try_from(y.clamp(0, 62))?,
        z: i8::try_from(z.clamp(-15, 15))?,
    })
}

fn read_byte_window(reader: &mut impl BitRead, byte_count: usize) -> Result<Vec<u8>, OamdError> {
    let mut bytes = Vec::with_capacity(byte_count);
    for _ in 0..byte_count {
        bytes.push(read_u8(reader, 8)?);
    }
    Ok(bytes)
}

fn read_u8(reader: &mut impl BitRead, width: u8) -> Result<u8, OamdError> {
    Ok(u8::try_from(reader.read_bits(width)?)?)
}
