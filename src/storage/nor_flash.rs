use embedded_storage::nor_flash::{ErrorType, NorFlash, ReadNorFlash};

use crate::error::{Error, Result};
use crate::layout::common::ERASED_BYTE;
use crate::storage::region::StorageRegion;

#[derive(Debug)]
pub struct NorFlashRegion<F> {
    flash: F,
    region: StorageRegion,
}

impl<F> NorFlashRegion<F>
where
    F: NorFlash,
{
    pub fn new(flash: F, region: StorageRegion) -> Result<Self, F::Error> {
        if region.write_size() as usize != F::WRITE_SIZE {
            return Err(Error::InvariantViolation(
                "storage region write_size must match backend WRITE_SIZE",
            ));
        }
        if region.erase_size() as usize != F::ERASE_SIZE {
            return Err(Error::InvariantViolation(
                "storage region erase_size must match backend ERASE_SIZE",
            ));
        }
        Ok(Self { flash, region })
    }
}

impl<F> NorFlashRegion<F> {
    pub const fn region(&self) -> &StorageRegion {
        &self.region
    }

    pub fn into_inner(self) -> F {
        self.flash
    }
}

impl<F> NorFlashRegion<F>
where
    F: ReadNorFlash,
{
    pub fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), F::Error> {
        if !self.region.contains(offset, bytes.len() as u32) {
            return Err(Error::OutOfBounds);
        }
        let absolute = self
            .region
            .to_absolute(offset)
            .map_err(|_| Error::OutOfBounds)?;
        self.flash.read(absolute, bytes).map_err(Error::Storage)
    }
}

impl<F> NorFlashRegion<F>
where
    F: NorFlash,
{
    pub fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), F::Error> {
        if !self.region.contains(offset, bytes.len() as u32) {
            return Err(Error::OutOfBounds);
        }
        let absolute = self
            .region
            .to_absolute(offset)
            .map_err(|_| Error::OutOfBounds)?;
        self.flash.write(absolute, bytes).map_err(Error::Storage)
    }

    pub fn write_aligned(
        &mut self,
        offset: u32,
        bytes: &[u8],
        scratch: &mut [u8],
    ) -> Result<(), F::Error> {
        if scratch.len() < F::WRITE_SIZE {
            return Err(Error::InvariantViolation(
                "aligned write scratch buffer must be at least one write unit",
            ));
        }
        self.region
            .require_write_aligned(offset)
            .map_err(|err| match err {
                Error::Alignment(alignment) => Error::Alignment(alignment),
                Error::OutOfBounds => Error::OutOfBounds,
                Error::InvariantViolation(msg) => Error::InvariantViolation(msg),
                _ => Error::InvariantViolation("unexpected alignment validation error"),
            })?;
        if !self.region.contains(offset, bytes.len() as u32) {
            return Err(Error::OutOfBounds);
        }

        let full_len = bytes.len() / F::WRITE_SIZE * F::WRITE_SIZE;
        if full_len > 0 {
            self.write(offset, &bytes[..full_len])?;
        }

        let tail = &bytes[full_len..];
        if tail.is_empty() {
            return Ok(());
        }

        scratch[..F::WRITE_SIZE].fill(ERASED_BYTE);
        scratch[..tail.len()].copy_from_slice(tail);
        self.write(offset + full_len as u32, &scratch[..F::WRITE_SIZE])
    }

    pub fn erase_sector(&mut self, sector_index: u32) -> Result<(), F::Error> {
        let from = self
            .region
            .sector_start(sector_index)
            .map_err(|_| Error::OutOfBounds)?;
        let to = from
            .checked_add(self.region.erase_size())
            .ok_or(Error::OutOfBounds)?;
        self.flash.erase(from, to).map_err(Error::Storage)
    }
}

impl<F> ErrorType for NorFlashRegion<F>
where
    F: ErrorType,
{
    type Error = F::Error;
}

impl<F> ReadNorFlash for NorFlashRegion<F>
where
    F: ReadNorFlash,
{
    const READ_SIZE: usize = F::READ_SIZE;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> core::result::Result<(), Self::Error> {
        let absolute = self
            .region
            .to_absolute(offset)
            .expect("region-relative read out of bounds");
        self.flash.read(absolute, bytes)
    }

    fn capacity(&self) -> usize {
        self.region.len() as usize
    }
}

impl<F> NorFlash for NorFlashRegion<F>
where
    F: NorFlash,
{
    const WRITE_SIZE: usize = F::WRITE_SIZE;
    const ERASE_SIZE: usize = F::ERASE_SIZE;

    fn write(&mut self, offset: u32, bytes: &[u8]) -> core::result::Result<(), Self::Error> {
        let absolute = self
            .region
            .to_absolute(offset)
            .expect("region-relative write out of bounds");
        self.flash.write(absolute, bytes)
    }

    fn erase(&mut self, from: u32, to: u32) -> core::result::Result<(), Self::Error> {
        let absolute_from = self
            .region
            .to_absolute(from)
            .expect("region-relative erase start out of bounds");
        let absolute_to = self
            .region
            .to_absolute(to)
            .expect("region-relative erase end out of bounds");
        self.flash.erase(absolute_from, absolute_to)
    }
}
