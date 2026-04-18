use crate::error::{AlignmentError, Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobMode {
    Variable,
    Fixed(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimestampPolicy {
    StrictMonotonic,
    AllowEqual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageRegionConfig {
    pub start: u32,
    pub len: u32,
    pub erase_size: u32,
    pub write_size: u32,
}

impl StorageRegionConfig {
    pub const fn new(start: u32, len: u32, erase_size: u32, write_size: u32) -> Self {
        Self {
            start,
            len,
            erase_size,
            write_size,
        }
    }

    pub fn validate(self) -> Result<Self> {
        if self.erase_size == 0 || self.write_size == 0 {
            return Err(Error::Alignment(AlignmentError::ZeroAlignment));
        }
        if !self.start.is_multiple_of(self.erase_size) {
            return Err(Error::InvariantViolation("start must align to erase_size"));
        }
        if !self.start.is_multiple_of(self.write_size) {
            return Err(Error::InvariantViolation("start must align to write_size"));
        }
        if !self.len.is_multiple_of(self.erase_size) {
            return Err(Error::InvariantViolation("len must align to erase_size"));
        }
        if self.len < self.erase_size.saturating_mul(2) {
            return Err(Error::InvariantViolation(
                "region must span at least two erase blocks",
            ));
        }
        Ok(self)
    }

    pub const fn sector_count(&self) -> u32 {
        self.len / self.erase_size
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KvConfig {
    pub region: StorageRegionConfig,
    pub max_key_len: usize,
    pub max_value_len: usize,
}

impl KvConfig {
    pub fn validate(self) -> Result<Self> {
        self.region.validate()?;
        if self.max_key_len == 0 || self.max_value_len == 0 {
            return Err(Error::InvariantViolation("KV lengths must be non-zero"));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TsdbConfig {
    pub region: StorageRegionConfig,
    pub blob_mode: BlobMode,
    pub timestamp_policy: TimestampPolicy,
}

impl TsdbConfig {
    pub fn validate(self) -> Result<Self> {
        self.region.validate()?;
        if matches!(self.blob_mode, BlobMode::Fixed(0)) {
            return Err(Error::InvariantViolation(
                "fixed blob mode must be non-zero",
            ));
        }
        Ok(self)
    }
}
