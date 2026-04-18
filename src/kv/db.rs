use embedded_storage::nor_flash::NorFlash;

use crate::blob::locator::KvValueLocator;
use crate::config::KvConfig;
use crate::error::{Error, Result};
use crate::layout::kv::{KV_DELETED, KV_WRITE, KvLayout};
use crate::storage::{NorFlashRegion, StorageRegion};

use super::scan;
use super::write;

pub(crate) const HEADER_BUF_CAP: usize = 256;
pub(crate) const SCAN_CHUNK_CAP: usize = 64;

#[derive(Debug)]
pub struct KvDb<F>
where
    F: NorFlash,
{
    config: KvConfig,
    layout: KvLayout,
    storage: NorFlashRegion<F>,
    write_cursor: u32,
}

impl<F> KvDb<F>
where
    F: NorFlash,
{
    pub fn mount(flash: F, config: KvConfig) -> Result<Self, F::Error> {
        let config = config
            .validate()
            .map_err(scan::map_core_error::<F::Error>)?;
        if config.max_key_len > u8::MAX as usize {
            return Err(Error::InvariantViolation("KV max_key_len must fit in u8"));
        }

        let region = StorageRegion::new(config.region).map_err(scan::map_core_error)?;
        let layout =
            KvLayout::new((region.write_size() as usize) * 8).map_err(scan::map_core_error)?;
        scan::ensure_runtime_caps(&layout, region.write_size() as usize)?;

        let mut storage = NorFlashRegion::new(flash, region)?;
        let write_cursor = scan::boot_scan(&mut storage, &layout)?;

        Ok(Self {
            config,
            layout,
            storage,
            write_cursor,
        })
    }

    pub const fn config(&self) -> KvConfig {
        self.config
    }

    pub const fn layout(&self) -> KvLayout {
        self.layout
    }

    pub fn region(&self) -> &StorageRegion {
        self.storage.region()
    }

    pub const fn write_cursor(&self) -> u32 {
        self.write_cursor
    }

    pub fn into_flash(self) -> F {
        self.storage.into_inner()
    }

    pub fn format(&mut self) -> Result<(), F::Error> {
        self.write_cursor = write::format_storage(&mut self.storage, &self.layout)?;
        Ok(())
    }

    pub fn set(&mut self, key: &str, value: &[u8]) -> Result<(), F::Error> {
        self.validate_key_value(key.as_bytes(), value)?;
        self.write_cursor = write::append_record(
            &mut self.storage,
            &self.layout,
            self.write_cursor,
            key.as_bytes(),
            value,
            KV_WRITE,
        )?;
        Ok(())
    }

    pub fn delete(&mut self, key: &str) -> Result<bool, F::Error> {
        let key_bytes = key.as_bytes();
        self.validate_key(key_bytes)?;
        if self.get_locator(key)?.is_none() {
            return Ok(false);
        }

        self.write_cursor = write::append_record(
            &mut self.storage,
            &self.layout,
            self.write_cursor,
            key_bytes,
            &[],
            KV_DELETED,
        )?;
        Ok(true)
    }

    pub fn contains_key(&mut self, key: &str) -> Result<bool, F::Error> {
        Ok(self.get_locator(key)?.is_some())
    }

    pub fn get_locator(&mut self, key: &str) -> Result<Option<KvValueLocator>, F::Error> {
        let key_bytes = key.as_bytes();
        self.validate_key(key_bytes)?;
        Ok(
            scan::lookup_key(&mut self.storage, &self.layout, key_bytes)?
                .and_then(|record| (!record.deleted).then_some(record.value_locator)),
        )
    }

    pub fn get_blob_into(&mut self, key: &str, out: &mut [u8]) -> Result<Option<usize>, F::Error> {
        let Some(locator) = self.get_locator(key)? else {
            return Ok(None);
        };

        let needed = locator.len() as usize;
        if out.len() < needed {
            return Err(Error::BufferTooSmall {
                needed,
                actual: out.len(),
            });
        }

        self.storage
            .read(locator.data_offset(), &mut out[..needed])?;
        Ok(Some(needed))
    }

    fn validate_key_value(&self, key: &[u8], value: &[u8]) -> Result<(), F::Error> {
        self.validate_key(key)?;
        if value.len() > self.config.max_value_len {
            return Err(Error::InvariantViolation(
                "KV value exceeds configured max_value_len",
            ));
        }
        Ok(())
    }

    fn validate_key(&self, key: &[u8]) -> Result<(), F::Error> {
        if key.is_empty() {
            return Err(Error::InvariantViolation("KV key must be non-empty"));
        }
        if key.len() > self.config.max_key_len {
            return Err(Error::InvariantViolation(
                "KV key exceeds configured max_key_len",
            ));
        }
        if key.len() > u8::MAX as usize {
            return Err(Error::InvariantViolation("KV key length must fit in u8"));
        }
        Ok(())
    }
}
