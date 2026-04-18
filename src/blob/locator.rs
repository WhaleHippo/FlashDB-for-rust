use crate::error::{Error, Result};
use crate::storage::region::StorageRegion;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlobLocator {
    meta_offset: u32,
    data_offset: u32,
    len: u32,
}

impl BlobLocator {
    pub fn new(
        region: &StorageRegion,
        meta_offset: u32,
        data_offset: u32,
        len: u32,
    ) -> Result<Self> {
        if meta_offset >= region.len() {
            return Err(Error::OutOfBounds);
        }
        if !region.contains(data_offset, len) {
            return Err(Error::OutOfBounds);
        }

        Ok(Self {
            meta_offset,
            data_offset,
            len,
        })
    }

    pub const fn meta_offset(&self) -> u32 {
        self.meta_offset
    }

    pub const fn data_offset(&self) -> u32 {
        self.data_offset
    }

    pub const fn len(&self) -> u32 {
        self.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn end_offset(&self) -> Result<u32> {
        self.data_offset
            .checked_add(self.len)
            .ok_or(Error::OutOfBounds)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KvValueLocator(BlobLocator);

impl KvValueLocator {
    pub fn new(
        region: &StorageRegion,
        meta_offset: u32,
        data_offset: u32,
        len: u32,
    ) -> Result<Self> {
        BlobLocator::new(region, meta_offset, data_offset, len).map(Self)
    }

    pub const fn into_inner(self) -> BlobLocator {
        self.0
    }

    pub const fn meta_offset(&self) -> u32 {
        self.0.meta_offset()
    }

    pub const fn data_offset(&self) -> u32 {
        self.0.data_offset()
    }

    pub const fn len(&self) -> u32 {
        self.0.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<KvValueLocator> for BlobLocator {
    fn from(value: KvValueLocator) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TsPayloadLocator(BlobLocator);

impl TsPayloadLocator {
    pub fn new(
        region: &StorageRegion,
        meta_offset: u32,
        data_offset: u32,
        len: u32,
    ) -> Result<Self> {
        BlobLocator::new(region, meta_offset, data_offset, len).map(Self)
    }

    pub const fn into_inner(self) -> BlobLocator {
        self.0
    }

    pub const fn meta_offset(&self) -> u32 {
        self.0.meta_offset()
    }

    pub const fn data_offset(&self) -> u32 {
        self.0.data_offset()
    }

    pub const fn len(&self) -> u32 {
        self.0.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<TsPayloadLocator> for BlobLocator {
    fn from(value: TsPayloadLocator) -> Self {
        value.0
    }
}
