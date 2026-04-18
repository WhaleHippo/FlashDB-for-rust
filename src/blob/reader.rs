use core::fmt::Debug;

use embedded_storage::nor_flash::ReadNorFlash;

use crate::blob::locator::BlobLocator;
use crate::error::{Error, Result};
use crate::storage::{nor_flash::NorFlashRegion, region::StorageRegion};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlobRef<'a> {
    bytes: &'a [u8],
}

impl<'a> BlobRef<'a> {
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub const fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct BlobBuf<'a> {
    bytes: &'a mut [u8],
}

impl<'a> BlobBuf<'a> {
    pub fn new(bytes: &'a mut [u8]) -> Self {
        Self { bytes }
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.bytes
    }

    pub fn as_mut_bytes(&mut self) -> &mut [u8] {
        self.bytes
    }
}

pub trait BlobStorage {
    type Error: Debug;

    fn region(&self) -> &StorageRegion;
    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error>;
}

impl<F> BlobStorage for NorFlashRegion<F>
where
    F: ReadNorFlash,
    F::Error: Debug,
{
    type Error = F::Error;

    fn region(&self) -> &StorageRegion {
        self.region()
    }

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        NorFlashRegion::read(self, offset, bytes)
    }
}

#[derive(Debug)]
pub struct BlobReader<S> {
    storage: S,
}

impl<S> BlobReader<S> {
    pub const fn new(storage: S) -> Self {
        Self { storage }
    }

    pub const fn storage(&self) -> &S {
        &self.storage
    }

    pub fn storage_mut(&mut self) -> &mut S {
        &mut self.storage
    }

    pub fn into_inner(self) -> S {
        self.storage
    }
}

impl<S> BlobReader<S>
where
    S: BlobStorage,
{
    pub fn blob_len(&self, locator: BlobLocator) -> usize {
        locator.len() as usize
    }

    pub fn read_exact(&mut self, locator: BlobLocator, out: &mut [u8]) -> Result<(), S::Error> {
        let needed = self.blob_len(locator);
        if out.len() < needed {
            return Err(Error::BufferTooSmall {
                needed,
                actual: out.len(),
            });
        }

        let read_len = self.read_truncated(locator, out)?;
        if read_len != needed {
            return Err(Error::InvariantViolation(
                "blob exact read must fill the entire payload",
            ));
        }
        Ok(())
    }

    pub fn read_truncated(
        &mut self,
        locator: BlobLocator,
        out: &mut [u8],
    ) -> Result<usize, S::Error> {
        self.read_chunk(locator, 0, out)
    }

    pub fn read_chunk(
        &mut self,
        locator: BlobLocator,
        offset: u32,
        out: &mut [u8],
    ) -> Result<usize, S::Error> {
        if offset > locator.len() {
            return Err(Error::InvalidBlobOffset {
                offset,
                len: locator.len(),
            });
        }

        let available = (locator.len() - offset) as usize;
        let read_len = available.min(out.len());
        if read_len == 0 {
            return Ok(0);
        }

        let absolute_offset = locator
            .data_offset()
            .checked_add(offset)
            .ok_or(Error::OutOfBounds)?;
        self.storage.read(absolute_offset, &mut out[..read_len])?;
        Ok(read_len)
    }

    pub fn cursor(&mut self, locator: BlobLocator) -> BlobCursor<'_, S> {
        BlobCursor::new(self, locator)
    }
}

#[derive(Debug)]
pub struct BlobCursor<'a, S> {
    reader: &'a mut BlobReader<S>,
    locator: BlobLocator,
    offset: u32,
}

impl<'a, S> BlobCursor<'a, S>
where
    S: BlobStorage,
{
    pub fn new(reader: &'a mut BlobReader<S>, locator: BlobLocator) -> Self {
        Self {
            reader,
            locator,
            offset: 0,
        }
    }

    pub const fn position(&self) -> u32 {
        self.offset
    }

    pub const fn remaining(&self) -> u32 {
        self.locator.len() - self.offset
    }

    pub fn read_next(&mut self, out: &mut [u8]) -> Result<usize, S::Error> {
        let read_len = self.reader.read_chunk(self.locator, self.offset, out)?;
        self.offset += read_len as u32;
        Ok(read_len)
    }
}
