// pattern: Functional Core

//! Original-syntax AC-3 primitives used by the Annex-J mixed JOC frontend.

use crate::{Eac3Error, StreamType, parse_syncframe_header};

const CRC_POLYNOMIAL: u16 = 0x8005;

/// Validates both original-syntax AC-3 CRC coverage regions.
pub(crate) fn validate_ac3_crc(bytes: &[u8]) -> Result<(), Eac3Error> {
    let header = parse_syncframe_header(bytes)?;
    if header.stream_type != StreamType::LegacyIndependent {
        return Err(Eac3Error::InvalidAc3Crc {
            region: "non-ac3-syncframe",
        });
    }
    let frame = bytes
        .get(..header.frame_size)
        .ok_or(Eac3Error::TruncatedFrame {
            offset: 0,
            declared: header.frame_size,
            available: bytes.len(),
        })?;
    let frame_words = header.frame_size / 2;
    let five_eighth_words = (frame_words >> 1)
        .checked_add(frame_words >> 3)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    let five_eighth_bytes = five_eighth_words
        .checked_mul(2)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    if crc16(&frame[2..five_eighth_bytes]) != 0 {
        return Err(Eac3Error::InvalidAc3Crc { region: "crc1" });
    }
    if crc16(&frame[2..]) != 0 {
        return Err(Eac3Error::InvalidAc3Crc { region: "crc2" });
    }
    Ok(())
}

fn crc16(bytes: &[u8]) -> u16 {
    let mut register = 0_u16;
    for byte in bytes {
        for shift in (0..8).rev() {
            let input = u16::from((byte >> shift) & 1);
            let feedback = ((register >> 15) & 1) ^ input;
            register <<= 1;
            if feedback != 0 {
                register ^= CRC_POLYNOMIAL;
            }
        }
    }
    register
}
