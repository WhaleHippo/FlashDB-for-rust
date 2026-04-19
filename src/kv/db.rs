use embedded_storage::nor_flash::NorFlash;

use crate::blob::locator::KvValueLocator;
use crate::config::{KvConfig, MAX_KV_KEY_LEN, MAX_KV_RECORDS, MAX_KV_VALUE_LEN};
use crate::error::{Error, Result};
use crate::layout::kv::{KV_DELETED, KV_PRE_DELETE, KV_WRITE, KvLayout, SECTOR_DIRTY_TRUE};
use crate::storage::{NorFlashRegion, StorageRegion};

use super::gc;
use super::iter;
use super::scan;
use super::write;

pub(crate) const HEADER_BUF_CAP: usize = 256;
pub(crate) const SCAN_CHUNK_CAP: usize = 64;

#[derive(Debug)]
pub struct KvDb<F>
where
    F: NorFlash,
{
    pub(super) config: KvConfig,
    pub(super) layout: KvLayout,
    pub(super) storage: NorFlashRegion<F>,
    pub(super) write_cursor: u32,
}

pub use super::iter::{KvIterator, KvOwnedRecord};
pub use super::scan::{KvIntegrityReport, KvSectorMeta};

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

    pub fn collect_garbage(&mut self) -> Result<(), F::Error> {
        gc::collect_garbage(self)
    }

    pub fn iter(&mut self) -> Result<KvIterator, F::Error> {
        Ok(KvIterator::new(iter::snapshot_live_records(self)?))
    }

    pub fn set(&mut self, key: &str, value: &[u8]) -> Result<(), F::Error> {
        let key_bytes = key.as_bytes();
        self.validate_key_value(key_bytes, value)?;
        let existing = scan::lookup_key(&mut self.storage, &self.layout, key_bytes)?;
        if existing.is_none() && self.live_record_count()? >= MAX_KV_RECORDS {
            return Err(Error::InvariantViolation(
                "KV live record count exceeds bounded no_alloc snapshot capacity",
            ));
        }
        gc::ensure_space_for_record(self, key_bytes.len(), value.len())?;

        if let Some(existing) = existing {
            let sector_base = self.sector_base(existing.record_offset);
            super::recovery::commit_record_status(
                &mut self.storage,
                &self.layout,
                existing.record_offset,
                KV_PRE_DELETE,
            )?;
            super::recovery::commit_sector_dirty_status(
                &mut self.storage,
                &self.layout,
                sector_base,
                SECTOR_DIRTY_TRUE,
            )?;
            self.write_cursor = write::append_record(
                &mut self.storage,
                &self.layout,
                self.write_cursor,
                key_bytes,
                value,
                KV_WRITE,
            )?;
            super::recovery::commit_record_status(
                &mut self.storage,
                &self.layout,
                existing.record_offset,
                KV_DELETED,
            )?;
            return Ok(());
        }

        self.write_cursor = write::append_record(
            &mut self.storage,
            &self.layout,
            self.write_cursor,
            key_bytes,
            value,
            KV_WRITE,
        )?;
        Ok(())
    }

    pub fn delete(&mut self, key: &str) -> Result<bool, F::Error> {
        let key_bytes = key.as_bytes();
        self.validate_key(key_bytes)?;
        if scan::lookup_key(&mut self.storage, &self.layout, key_bytes)?.is_none() {
            return Ok(false);
        }

        gc::ensure_space_for_record(self, key_bytes.len(), 0)?;
        let existing = scan::lookup_key(&mut self.storage, &self.layout, key_bytes)?.ok_or(
            Error::InvariantViolation("live KV disappeared during delete"),
        )?;
        let sector_base = self.sector_base(existing.record_offset);
        super::recovery::commit_record_status(
            &mut self.storage,
            &self.layout,
            existing.record_offset,
            KV_PRE_DELETE,
        )?;
        super::recovery::commit_sector_dirty_status(
            &mut self.storage,
            &self.layout,
            sector_base,
            SECTOR_DIRTY_TRUE,
        )?;
        self.write_cursor = write::append_record(
            &mut self.storage,
            &self.layout,
            self.write_cursor,
            key_bytes,
            &[],
            KV_DELETED,
        )?;
        super::recovery::commit_record_status(
            &mut self.storage,
            &self.layout,
            existing.record_offset,
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

    pub fn sector_meta(&mut self, sector_index: u32) -> Result<KvSectorMeta, F::Error> {
        scan::sector_meta(&mut self.storage, &self.layout, sector_index)
    }

    pub fn check_integrity(&mut self) -> Result<KvIntegrityReport, F::Error> {
        scan::integrity_check(&mut self.storage, &self.layout)
    }

    pub fn for_each_live_record(
        &mut self,
        key_buf: &mut [u8],
        value_buf: &mut [u8],
        mut visit: impl FnMut(&str, &[u8]),
    ) -> Result<(), F::Error> {
        let region = *self.storage.region();
        let sector_header_len = self
            .layout
            .sector_header_len()
            .map_err(scan::map_core_error)? as u32;
        let record_header_len = self
            .layout
            .record_header_len()
            .map_err(scan::map_core_error)? as u32;

        for sector_index in 0..region.sector_count() {
            let sector_base = sector_index
                .checked_mul(region.erase_size())
                .ok_or(Error::OutOfBounds)?;
            if scan::read_erased(&mut self.storage, sector_base, sector_header_len as usize)? {
                break;
            }
            if scan::read_sector_header(&mut self.storage, &self.layout, sector_base).is_err() {
                break;
            }
            let sector_end = sector_base
                .checked_add(region.erase_size())
                .ok_or(Error::OutOfBounds)?;
            let mut record_offset = sector_base + sector_header_len;

            while record_offset
                .checked_add(record_header_len)
                .is_some_and(|end| end <= sector_end)
            {
                if scan::read_erased(&mut self.storage, record_offset, record_header_len as usize)?
                {
                    break;
                }
                let header = match scan::read_record_header(
                    &mut self.storage,
                    &self.layout,
                    record_offset,
                ) {
                    Ok(header) => header,
                    Err(_) => break,
                };
                let next_offset = record_offset
                    .checked_add(header.total_len)
                    .ok_or(Error::OutOfBounds)?;
                if next_offset > sector_end {
                    break;
                }

                if matches!(header.status, KV_WRITE | KV_PRE_DELETE)
                    && scan::record_crc_matches(
                        &mut self.storage,
                        &self.layout,
                        record_offset,
                        &header,
                    )?
                {
                    let key_len = scan::read_key_into(
                        &mut self.storage,
                        &self.layout,
                        record_offset,
                        &header,
                        key_buf,
                    )?;
                    if let Some(latest) =
                        scan::lookup_key(&mut self.storage, &self.layout, &key_buf[..key_len])?
                            .filter(|latest| {
                                !latest.deleted && latest.record_offset == record_offset
                            })
                    {
                        let value_len = scan::read_value_into(
                            &mut self.storage,
                            &self.layout,
                            latest.record_offset,
                            &header,
                            value_buf,
                        )?;
                        let key = scan::utf8_key(&key_buf[..key_len])
                            .map_err(scan::map_core_error::<F::Error>)?;
                        visit(key, &value_buf[..value_len]);
                    }
                }

                record_offset = next_offset;
            }
        }

        Ok(())
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

    fn sector_base(&self, offset: u32) -> u32 {
        (offset / self.storage.region().erase_size()) * self.storage.region().erase_size()
    }

    fn live_record_count(&mut self) -> Result<usize, F::Error> {
        let mut count = 0usize;
        let mut key_buf = [0u8; MAX_KV_KEY_LEN];
        let mut value_buf = [0u8; MAX_KV_VALUE_LEN];
        self.for_each_live_record(&mut key_buf, &mut value_buf, |_, _| {
            count += 1;
        })?;
        Ok(count)
    }
}
