use embedded_storage::nor_flash::{ErrorType, NorFlash, ReadNorFlash};

use crate::error::{Error, Result};
use crate::storage::region::StorageRegion;

#[derive(Debug)]
pub struct NorFlashRegion<F> {
    flash: F,
    region: StorageRegion,
}

impl<F> NorFlashRegion<F> {
    pub const fn new(flash: F, region: StorageRegion) -> Self {
        Self { flash, region }
    }

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
        let absolute = self.region.to_absolute(offset).map_err(|_| Error::OutOfBounds)?;
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
        let absolute = self.region.to_absolute(offset).map_err(|_| Error::OutOfBounds)?;
        self.flash.write(absolute, bytes).map_err(Error::Storage)
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
        let absolute = self.region.to_absolute(offset).expect("region-relative read out of bounds");
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
        let absolute = self.region.to_absolute(offset).expect("region-relative write out of bounds");
        self.flash.write(absolute, bytes)
    }

    fn erase(&mut self, from: u32, to: u32) -> core::result::Result<(), Self::Error> {
        let absolute_from = self.region.to_absolute(from).expect("region-relative erase start out of bounds");
        let absolute_to = self.region.to_absolute(to).expect("region-relative erase end out of bounds");
        self.flash.erase(absolute_from, absolute_to)
    }
}
