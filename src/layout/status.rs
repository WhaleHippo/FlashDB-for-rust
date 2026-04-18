use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};

use crate::error::{DecodeError, Error, Result};
use crate::layout::common::{ERASED_BYTE, WRITTEN_BYTE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusScheme {
    state_count: usize,
    write_granularity_bits: usize,
}

impl StatusScheme {
    pub fn new(state_count: usize, write_granularity_bits: usize) -> Result<Self> {
        if state_count == 0 || write_granularity_bits == 0 {
            return Err(Error::InvariantViolation(
                "status scheme parameters must be non-zero",
            ));
        }
        if write_granularity_bits != 1 && !write_granularity_bits.is_multiple_of(8) {
            return Err(Error::InvariantViolation(
                "write granularity must be 1 bit or a whole number of bytes",
            ));
        }
        Ok(Self {
            state_count,
            write_granularity_bits,
        })
    }

    pub const fn state_count(&self) -> usize {
        self.state_count
    }

    pub const fn write_granularity_bits(&self) -> usize {
        self.write_granularity_bits
    }

    pub const fn write_granularity_bytes(&self) -> usize {
        if self.write_granularity_bits == 1 {
            0
        } else {
            self.write_granularity_bits / 8
        }
    }

    pub const fn write_granularity_storage_bytes(&self) -> usize {
        if self.write_granularity_bits == 1 {
            1
        } else {
            self.write_granularity_bits / 8
        }
    }

    pub const fn table_len(&self) -> usize {
        if self.write_granularity_bits == 1 {
            self.state_count.div_ceil(8)
        } else {
            ((self.state_count - 1) * self.write_granularity_bits).div_ceil(8)
        }
    }

    pub fn encode(&self, state_index: usize, out: &mut [u8]) -> Result<()> {
        self.encode_transition(state_index, out).map(|_| ())
    }

    pub fn encode_transition(
        &self,
        state_index: usize,
        out: &mut [u8],
    ) -> Result<Option<(usize, usize)>> {
        if out.len() < self.table_len() {
            return Err(Error::Decode(DecodeError::BufferTooShort));
        }
        if state_index >= self.state_count {
            return Err(Error::Decode(DecodeError::InvalidState));
        }

        let out = &mut out[..self.table_len()];
        out.fill(ERASED_BYTE);

        if state_index == 0 {
            return Ok(None);
        }

        if self.write_granularity_bits == 1 {
            let full_bytes = state_index / 8;
            let partial_bits = state_index % 8;

            for byte in out.iter_mut().take(full_bytes) {
                *byte = WRITTEN_BYTE;
            }
            if partial_bits != 0 {
                out[full_bytes] = 0xFFu8 >> partial_bits;
            }
        } else {
            let chunk_len = self.write_granularity_bytes();
            for chunk in out.chunks_mut(chunk_len).take(state_index) {
                chunk.fill(WRITTEN_BYTE);
            }
        }

        Ok(self.transition_write_span(state_index))
    }

    pub fn transition_write_span(&self, state_index: usize) -> Option<(usize, usize)> {
        if state_index == 0 || state_index >= self.state_count {
            return None;
        }

        if self.write_granularity_bits == 1 {
            Some(((state_index - 1) / 8, 1))
        } else {
            let chunk_len = self.write_granularity_bytes();
            Some(((state_index - 1) * chunk_len, chunk_len))
        }
    }

    pub fn transition_write_bytes(
        &self,
        state_index: usize,
        out: &mut [u8],
    ) -> Result<Option<(usize, usize)>> {
        let Some((offset, len)) = self.transition_write_span(state_index) else {
            if state_index == 0 {
                return Ok(None);
            }
            return Err(Error::Decode(DecodeError::InvalidState));
        };
        if out.len() < len {
            return Err(Error::Decode(DecodeError::BufferTooShort));
        }
        if self.write_granularity_bits == 1 {
            out[0] = if state_index.is_multiple_of(8) {
                WRITTEN_BYTE
            } else {
                0xFFu8 >> (state_index % 8)
            };
        } else {
            out[..len].fill(WRITTEN_BYTE);
        }
        Ok(Some((offset, len)))
    }

    pub fn write_transition<F>(
        &self,
        flash: &mut F,
        offset: u32,
        state_index: usize,
        scratch: &mut [u8],
    ) -> Result<(), F::Error>
    where
        F: NorFlash,
    {
        if F::WRITE_SIZE != self.write_granularity_storage_bytes() {
            return Err(Error::InvariantViolation(
                "status transition write size must match backend WRITE_SIZE",
            ));
        }
        let Some((span_offset, span_len)) = self
            .transition_write_bytes(state_index, scratch)
            .map_err(map_scheme_error)?
        else {
            return Ok(());
        };
        flash
            .write(offset + span_offset as u32, &scratch[..span_len])
            .map_err(Error::Storage)
    }

    pub fn read_status<F>(
        &self,
        flash: &mut F,
        offset: u32,
        scratch: &mut [u8],
    ) -> Result<usize, F::Error>
    where
        F: ReadNorFlash,
    {
        let table_len = self.table_len();
        if scratch.len() < table_len {
            return Err(Error::Decode(DecodeError::BufferTooShort));
        }
        flash
            .read(offset, &mut scratch[..table_len])
            .map_err(Error::Storage)?;
        self.decode(&scratch[..table_len]).map_err(map_scheme_error)
    }

    pub fn decode(&self, bytes: &[u8]) -> Result<usize> {
        if bytes.len() < self.table_len() {
            return Err(Error::Decode(DecodeError::BufferTooShort));
        }

        let bytes = &bytes[..self.table_len()];

        if self.write_granularity_bits == 1 {
            let mut state_index = 0;
            for bit_index in 0..(self.state_count - 1) {
                let byte_index = bit_index / 8;
                let mask = 0x80u8 >> (bit_index % 8);
                let programmed = bytes[byte_index] & mask == 0;
                if programmed {
                    state_index += 1;
                } else {
                    for remaining_bit in (bit_index + 1)..(self.state_count - 1) {
                        let remaining_byte = remaining_bit / 8;
                        let remaining_mask = 0x80u8 >> (remaining_bit % 8);
                        if bytes[remaining_byte] & remaining_mask == 0 {
                            return Err(Error::Decode(DecodeError::InvalidState));
                        }
                    }
                    return Ok(state_index);
                }
            }
            return Ok(state_index);
        }

        let chunk_len = self.write_granularity_bytes();
        let mut state_index = 0;
        let mut saw_erased = false;

        for chunk in bytes.chunks(chunk_len).take(self.state_count - 1) {
            let all_written = chunk.iter().all(|&byte| byte == WRITTEN_BYTE);
            let all_erased = chunk.iter().all(|&byte| byte == ERASED_BYTE);

            if all_written {
                if saw_erased {
                    return Err(Error::Decode(DecodeError::InvalidState));
                }
                state_index += 1;
            } else if all_erased {
                saw_erased = true;
            } else {
                return Err(Error::Decode(DecodeError::InvalidState));
            }
        }

        Ok(state_index)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct StatusTableBuf<'a> {
    scheme: StatusScheme,
    bytes: &'a mut [u8],
}

impl<'a> StatusTableBuf<'a> {
    pub fn new(scheme: StatusScheme, bytes: &'a mut [u8]) -> Result<Self> {
        if bytes.len() < scheme.table_len() {
            return Err(Error::Decode(DecodeError::BufferTooShort));
        }
        Ok(Self { scheme, bytes })
    }

    pub fn encode(&mut self, state_index: usize) -> Result<()> {
        self.scheme.encode(state_index, self.bytes)
    }

    pub fn decode(&self) -> Result<usize> {
        self.scheme.decode(self.bytes)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.scheme.table_len()]
    }
}

fn map_scheme_error<E>(err: Error) -> Error<E>
where
    E: core::fmt::Debug,
{
    match err {
        Error::Storage(_) => {
            Error::InvariantViolation("unexpected storage error during status-table operation")
        }
        Error::Decode(decode) => Error::Decode(decode),
        Error::Alignment(alignment) => Error::Alignment(alignment),
        Error::OutOfBounds => Error::OutOfBounds,
        Error::CorruptedHeader => Error::CorruptedHeader,
        Error::CrcMismatch => Error::CrcMismatch,
        Error::NoSpace => Error::NoSpace,
        Error::BufferTooSmall { needed, actual } => Error::BufferTooSmall { needed, actual },
        Error::InvalidBlobOffset { offset, len } => Error::InvalidBlobOffset { offset, len },
        Error::UnsupportedFormatVersion(version) => Error::UnsupportedFormatVersion(version),
        Error::InvariantViolation(msg) => Error::InvariantViolation(msg),
        Error::TimestampNotMonotonic { last, next } => Error::TimestampNotMonotonic { last, next },
    }
}
