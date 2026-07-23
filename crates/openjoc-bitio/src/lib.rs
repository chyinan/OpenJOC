// pattern: Functional Core

//! Checked, MSB-first bitstream reading for `OpenJOC` parsers.

use core::fmt;

/// Structured failures returned while reading a bounded bit slice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BitError {
    /// The requested integer width cannot be represented by `u64`.
    InvalidWidth { width: u8 },
    /// The input does not contain the complete requested field.
    EndOfInput { requested: usize, remaining: usize },
    /// Input length or cursor arithmetic exceeded the platform `usize` range.
    LengthOverflow,
}

impl fmt::Display for BitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidWidth { width } => {
                write!(formatter, "invalid bit width {width}; expected 0..=64")
            }
            Self::EndOfInput {
                requested,
                remaining,
            } => write!(
                formatter,
                "truncated bitstream: requested {requested} bits with {remaining} remaining"
            ),
            Self::LengthOverflow => formatter.write_str("bitstream length arithmetic overflow"),
        }
    }
}

impl std::error::Error for BitError {}

/// Minimal interface shared by the normative `OpenJOC` syntax parsers.
pub trait BitRead {
    /// Reads one bit in MSB-first order.
    ///
    /// # Errors
    ///
    /// Returns [`BitError`] when no complete bit remains or length arithmetic failed.
    fn read_bit(&mut self) -> Result<bool, BitError>;

    /// Reads up to 64 bits in MSB-first order into the low bits of a `u64`.
    ///
    /// # Errors
    ///
    /// Returns [`BitError`] for widths above 64, truncated input, or arithmetic failure.
    fn read_bits(&mut self, n: u8) -> Result<u64, BitError>;

    /// Returns the exact number of unread bits, or zero after length overflow.
    fn bits_remaining(&self) -> usize;

    /// Advances to the next byte boundary without reading beyond the input.
    ///
    /// # Errors
    ///
    /// Returns [`BitError`] if the remaining partial byte is truncated or its
    /// cursor arithmetic cannot be represented.
    fn byte_align(&mut self) -> Result<(), BitError>;
}

/// A non-owning, bounded MSB-first bit reader.
#[derive(Clone, Debug)]
pub struct BitReader<'a> {
    bytes: &'a [u8],
    bit_position: usize,
    bit_len: Option<usize>,
}

impl<'a> BitReader<'a> {
    /// Creates a reader positioned at the first (most significant) input bit.
    #[must_use]
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            bit_position: 0,
            bit_len: bytes.len().checked_mul(8),
        }
    }

    fn checked_remaining(&self) -> Result<usize, BitError> {
        let bit_len = self.bit_len.ok_or(BitError::LengthOverflow)?;
        bit_len
            .checked_sub(self.bit_position)
            .ok_or(BitError::LengthOverflow)
    }
}

impl BitRead for BitReader<'_> {
    fn read_bit(&mut self) -> Result<bool, BitError> {
        Ok(self.read_bits(1)? != 0)
    }

    fn read_bits(&mut self, n: u8) -> Result<u64, BitError> {
        if n > 64 {
            return Err(BitError::InvalidWidth { width: n });
        }

        let requested = usize::from(n);
        let remaining = self.checked_remaining()?;
        if requested > remaining {
            return Err(BitError::EndOfInput {
                requested,
                remaining,
            });
        }

        let end = self
            .bit_position
            .checked_add(requested)
            .ok_or(BitError::LengthOverflow)?;
        let mut value = 0_u64;
        while self.bit_position < end {
            let byte_index = self.bit_position / 8;
            let bit_index = self.bit_position % 8;
            let bit = (self.bytes[byte_index] >> (7 - bit_index)) & 1;
            value = (value << 1) | u64::from(bit);
            self.bit_position = self
                .bit_position
                .checked_add(1)
                .ok_or(BitError::LengthOverflow)?;
        }
        Ok(value)
    }

    fn bits_remaining(&self) -> usize {
        self.checked_remaining().unwrap_or(0)
    }

    fn byte_align(&mut self) -> Result<(), BitError> {
        let misalignment = self.bit_position % 8;
        if misalignment == 0 {
            return Ok(());
        }

        let discarded = 8 - misalignment;
        let remaining = self.checked_remaining()?;
        if discarded > remaining {
            return Err(BitError::EndOfInput {
                requested: discarded,
                remaining,
            });
        }
        self.bit_position = self
            .bit_position
            .checked_add(discarded)
            .ok_or(BitError::LengthOverflow)?;
        Ok(())
    }
}
