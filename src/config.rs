use crate::error::{AlignmentError, Error, Result};

pub const MAX_KV_KEY_LEN: usize = 64;
pub const MAX_KV_VALUE_LEN: usize = 256;
pub const MAX_KV_RECORDS: usize = 64;
pub const MAX_TS_PAYLOAD_LEN: usize = 256;
pub const MAX_TS_RECORDS: usize = 64;
pub const MAX_TS_SECTORS: usize = 64;
pub const MAX_RUNTIME_WRITE_SIZE: usize = 32;
pub const MAX_TS_HEADER_LEN: usize = 128;
pub const MAX_TS_INDEX_LEN: usize = 128;

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
        if self.write_size as usize > MAX_RUNTIME_WRITE_SIZE {
            return Err(Error::InvariantViolation(
                "write_size exceeds bounded no_alloc scratch capacity",
            ));
        }
        if self.sector_count() as usize > MAX_TS_SECTORS {
            return Err(Error::InvariantViolation(
                "sector count exceeds bounded no_alloc runtime capacity",
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
        if self.max_key_len > MAX_KV_KEY_LEN {
            return Err(Error::InvariantViolation(
                "KV max_key_len exceeds bounded no_alloc capacity",
            ));
        }
        if self.max_value_len > MAX_KV_VALUE_LEN {
            return Err(Error::InvariantViolation(
                "KV max_value_len exceeds bounded no_alloc capacity",
            ));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TsdbConfig {
    pub region: StorageRegionConfig,
    pub blob_mode: BlobMode,
    pub timestamp_policy: TimestampPolicy,
    pub rollover: bool,
}

impl TsdbConfig {
    pub fn validate(self) -> Result<Self> {
        self.region.validate()?;
        if matches!(self.blob_mode, BlobMode::Fixed(0)) {
            return Err(Error::InvariantViolation(
                "fixed blob mode must be non-zero",
            ));
        }
        if matches!(self.blob_mode, BlobMode::Fixed(len) if len > MAX_TS_PAYLOAD_LEN) {
            return Err(Error::InvariantViolation(
                "TSDB fixed blob length exceeds bounded no_alloc payload capacity",
            ));
        }
        Ok(self)
    }
}
