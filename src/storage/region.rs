use crate::config::StorageRegionConfig;
use crate::error::{AlignmentError, Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageRegion {
    start: u32,
    len: u32,
    erase_size: u32,
    write_size: u32,
}

impl StorageRegion {
    pub fn new(config: StorageRegionConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            start: config.start,
            len: config.len,
            erase_size: config.erase_size,
            write_size: config.write_size,
        })
    }

    pub const fn start(&self) -> u32 {
        self.start
    }

    pub const fn len(&self) -> u32 {
        self.len
    }

    pub const fn erase_size(&self) -> u32 {
        self.erase_size
    }

    pub const fn write_size(&self) -> u32 {
        self.write_size
    }

    pub const fn sector_count(&self) -> u32 {
        self.len / self.erase_size
    }

    pub fn contains(&self, offset: u32, len: u32) -> bool {
        offset <= self.len && len <= self.len.saturating_sub(offset)
    }

    pub fn to_absolute(&self, offset: u32) -> Result<u32> {
        if offset > self.len {
            return Err(Error::OutOfBounds);
        }
        self.start.checked_add(offset).ok_or(Error::OutOfBounds)
    }

    pub fn sector_start(&self, sector_index: u32) -> Result<u32> {
        if sector_index >= self.sector_count() {
            return Err(Error::OutOfBounds);
        }
        self.to_absolute(sector_index.saturating_mul(self.erase_size))
    }

    pub fn sector_index_of(&self, offset: u32) -> Result<u32> {
        if offset >= self.len {
            return Err(Error::OutOfBounds);
        }
        Ok(offset / self.erase_size)
    }

    pub fn require_write_aligned(&self, value: u32) -> Result<u32> {
        if value % self.write_size != 0 {
            return Err(Error::Alignment(AlignmentError::UnalignedValue {
                value,
                align: self.write_size,
            }));
        }
        Ok(value)
    }
}
