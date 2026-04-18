use crate::error::{DecodeError, Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusScheme {
    state_count: usize,
    granularity_bytes: usize,
}

impl StatusScheme {
    pub fn new(state_count: usize, granularity_bytes: usize) -> Result<Self> {
        if state_count == 0 || granularity_bytes == 0 {
            return Err(Error::InvariantViolation("status scheme parameters must be non-zero"));
        }
        Ok(Self {
            state_count,
            granularity_bytes,
        })
    }

    pub const fn state_count(&self) -> usize {
        self.state_count
    }

    pub const fn granularity_bytes(&self) -> usize {
        self.granularity_bytes
    }

    pub const fn table_len(&self) -> usize {
        self.state_count * self.granularity_bytes
    }

    pub fn encode(&self, state_index: usize, out: &mut [u8]) -> Result<()> {
        if out.len() < self.table_len() {
            return Err(Error::Decode(DecodeError::BufferTooShort));
        }
        if state_index >= self.state_count {
            return Err(Error::Decode(DecodeError::InvalidState));
        }
        out[..self.table_len()].fill(0xFF);
        let programmed = state_index * self.granularity_bytes;
        out[..programmed].fill(0x00);
        Ok(())
    }

    pub fn decode(&self, bytes: &[u8]) -> Result<usize> {
        if bytes.len() < self.table_len() {
            return Err(Error::Decode(DecodeError::BufferTooShort));
        }
        let mut state = 0;
        while state < self.state_count {
            let start = state * self.granularity_bytes;
            let end = start + self.granularity_bytes;
            let chunk = &bytes[start..end];
            let is_programmed = chunk.iter().all(|&b| b == 0x00);
            if is_programmed {
                state += 1;
                continue;
            }
            let is_erased = chunk.iter().all(|&b| b == 0xFF);
            if is_erased {
                return Ok(state);
            }
            return Err(Error::Decode(DecodeError::InvalidState));
        }
        Ok(self.state_count.saturating_sub(1))
    }
}
