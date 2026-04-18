use embedded_storage::nor_flash::{
    ErrorType, NorFlash, NorFlashError, NorFlashErrorKind, ReadNorFlash,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MockFlashError {
    OutOfBounds,
    NotAligned,
    RequiresErase,
}

impl NorFlashError for MockFlashError {
    fn kind(&self) -> NorFlashErrorKind {
        match self {
            Self::OutOfBounds => NorFlashErrorKind::OutOfBounds,
            Self::NotAligned => NorFlashErrorKind::NotAligned,
            Self::RequiresErase => NorFlashErrorKind::Other,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MockFlash<const N: usize, const WRITE_SIZE: usize, const ERASE_SIZE: usize> {
    bytes: [u8; N],
}

impl<const N: usize, const WRITE_SIZE: usize, const ERASE_SIZE: usize>
    MockFlash<N, WRITE_SIZE, ERASE_SIZE>
{
    pub const fn new() -> Self {
        Self { bytes: [0xFF; N] }
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }
}

impl<const N: usize, const WRITE_SIZE: usize, const ERASE_SIZE: usize> Default
    for MockFlash<N, WRITE_SIZE, ERASE_SIZE>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize, const WRITE_SIZE: usize, const ERASE_SIZE: usize> ErrorType
    for MockFlash<N, WRITE_SIZE, ERASE_SIZE>
{
    type Error = MockFlashError;
}

impl<const N: usize, const WRITE_SIZE: usize, const ERASE_SIZE: usize> ReadNorFlash
    for MockFlash<N, WRITE_SIZE, ERASE_SIZE>
{
    const READ_SIZE: usize = 1;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        let offset = offset as usize;
        let end = offset.saturating_add(bytes.len());
        if end > N {
            return Err(MockFlashError::OutOfBounds);
        }
        bytes.copy_from_slice(&self.bytes[offset..end]);
        Ok(())
    }

    fn capacity(&self) -> usize {
        N
    }
}

impl<const N: usize, const WRITE_SIZE: usize, const ERASE_SIZE: usize> NorFlash
    for MockFlash<N, WRITE_SIZE, ERASE_SIZE>
{
    const WRITE_SIZE: usize = WRITE_SIZE;
    const ERASE_SIZE: usize = ERASE_SIZE;

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        let offset = offset as usize;
        if offset % WRITE_SIZE != 0 || bytes.len() % WRITE_SIZE != 0 {
            return Err(MockFlashError::NotAligned);
        }
        let end = offset.saturating_add(bytes.len());
        if end > N {
            return Err(MockFlashError::OutOfBounds);
        }
        for (dst, src) in self.bytes[offset..end]
            .iter_mut()
            .zip(bytes.iter().copied())
        {
            if (src | *dst) != *dst {
                return Err(MockFlashError::RequiresErase);
            }
            *dst &= src;
        }
        Ok(())
    }

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        let from = from as usize;
        let to = to as usize;
        if from % ERASE_SIZE != 0 || to % ERASE_SIZE != 0 || from > to {
            return Err(MockFlashError::NotAligned);
        }
        if to > N {
            return Err(MockFlashError::OutOfBounds);
        }
        for byte in &mut self.bytes[from..to] {
            *byte = 0xFF;
        }
        Ok(())
    }
}
