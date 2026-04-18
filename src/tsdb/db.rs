use alloc::vec;
use alloc::vec::Vec;

use embedded_storage::nor_flash::NorFlash;

use crate::config::{BlobMode, TimestampPolicy, TsdbConfig};
use crate::error::{Error, Result};
use crate::layout::align::align_up;
use crate::layout::common::ERASED_BYTE;
use crate::layout::ts::{
    SECTOR_STORE_EMPTY, SECTOR_STORE_FULL, SECTOR_STORE_USING, TSL_PRE_WRITE, TSL_WRITE,
    TsBlobMode, TsIndexHeader, TsLayout, TsSectorHeader,
};
use crate::storage::{NorFlashRegion, StorageRegion};

use super::iter::{TsIterator, TsOwnedRecord};

#[derive(Debug)]
pub struct TsDb<F>
where
    F: NorFlash,
{
    config: TsdbConfig,
    layout: TsLayout,
    storage: NorFlashRegion<F>,
    mode: TsBlobMode,
    sectors: Vec<TsSectorRuntime>,
    current_sector: Option<u32>,
    oldest_sector: Option<u32>,
    last_timestamp: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
struct TsSectorRuntime {
    store_status: usize,
    start_time: Option<u64>,
    end_time: Option<u64>,
    empty_index_offset: u32,
    empty_data_offset: u32,
    entry_count: u32,
}

impl<F> TsDb<F>
where
    F: NorFlash,
{
    pub fn mount(flash: F, config: TsdbConfig) -> Result<Self, F::Error> {
        let config = config.validate().map_err(map_core_error::<F::Error>)?;
        let region = StorageRegion::new(config.region).map_err(map_core_error::<F::Error>)?;
        let layout = TsLayout::new((region.write_size() as usize) * 8, 4)
            .map_err(map_core_error::<F::Error>)?;
        let mode = blob_mode_from_config(config)?;
        let mut storage = NorFlashRegion::new(flash, region)?;
        let sectors = scan_all_sectors(&mut storage, &layout, mode)?;
        let oldest_sector = sectors
            .iter()
            .enumerate()
            .find(|(_, sector)| sector.entry_count > 0)
            .map(|(index, _)| index as u32);
        let last_timestamp = sectors.iter().filter_map(|sector| sector.end_time).max();
        let current_sector = select_current_sector(&sectors);

        Ok(Self {
            config,
            layout,
            storage,
            mode,
            sectors,
            current_sector,
            oldest_sector,
            last_timestamp,
        })
    }

    pub const fn config(&self) -> TsdbConfig {
        self.config
    }

    pub const fn layout(&self) -> TsLayout {
        self.layout
    }

    pub fn region(&self) -> &StorageRegion {
        self.storage.region()
    }

    pub fn format(&mut self) -> Result<(), F::Error> {
        for sector_index in 0..self.region().sector_count() {
            self.storage.erase_sector(sector_index)?;
        }
        self.sectors = empty_sector_table(self.region(), &self.layout, self.mode)
            .map_err(map_core_error::<F::Error>)?;
        self.current_sector = Some(0);
        self.oldest_sector = None;
        self.last_timestamp = None;
        Ok(())
    }

    pub const fn oldest_sector_index(&self) -> Option<u32> {
        self.oldest_sector
    }

    pub const fn current_sector_index(&self) -> Option<u32> {
        self.current_sector
    }

    pub const fn last_timestamp(&self) -> Option<u64> {
        self.last_timestamp
    }

    pub fn into_flash(self) -> F {
        self.storage.into_inner()
    }

    pub fn append(&mut self, timestamp: u64, payload: &[u8]) -> Result<(), F::Error> {
        self.validate_timestamp(timestamp)?;

        let aligned_payload_len = align_up(payload.len(), self.region().write_size() as usize)
            .map_err(map_core_error::<F::Error>)?;
        let index_len = self
            .layout
            .index_header_len(self.mode)
            .map_err(map_core_error::<F::Error>)? as u32;
        let needed = index_len
            .checked_add(aligned_payload_len as u32)
            .ok_or(Error::OutOfBounds)?;
        let sector_index = self.ensure_writable_sector(needed)?;
        let sector_base = self
            .region()
            .sector_start(sector_index)
            .map_err(map_core_error::<F::Error>)?;
        let write_size = self.region().write_size() as usize;

        {
            let sector = &mut self.sectors[sector_index as usize];
            if sector.entry_count == 0 {
                initialize_sector(
                    &mut self.storage,
                    &self.layout,
                    sector_base,
                    timestamp,
                    write_size,
                )?;
                sector.store_status = SECTOR_STORE_USING;
                sector.start_time = Some(timestamp);
            }

            let index_offset = sector.empty_index_offset;
            let data_offset = sector
                .empty_data_offset
                .checked_sub(aligned_payload_len as u32)
                .ok_or(Error::OutOfBounds)?;
            let mut index_buf = vec![0u8; index_len as usize];
            TsIndexHeader::variable(timestamp, data_offset, payload.len() as u32)
                .encode(&self.layout, self.mode, &mut index_buf)
                .map_err(map_core_error::<F::Error>)?;
            self.storage.write(index_offset, &index_buf)?;

            let mut write_scratch = vec![0u8; write_size];
            self.storage
                .write_aligned(data_offset, payload, &mut write_scratch)?;

            let mut status_scratch = vec![0u8; write_size];
            self.layout.tsl_status_scheme().write_transition(
                &mut self.storage,
                index_offset,
                TSL_WRITE,
                &mut status_scratch,
            )?;

            sector.empty_index_offset = sector
                .empty_index_offset
                .checked_add(index_len)
                .ok_or(Error::OutOfBounds)?;
            sector.empty_data_offset = data_offset;
            sector.end_time = Some(timestamp);
            sector.entry_count += 1;
            sector.store_status = SECTOR_STORE_USING;
        }

        if self.oldest_sector.is_none() {
            self.oldest_sector = Some(sector_index);
        }
        self.current_sector = Some(sector_index);
        self.last_timestamp = Some(timestamp);
        Ok(())
    }

    pub fn iter(&mut self) -> Result<TsIterator, F::Error> {
        let mut records = Vec::new();
        for sector_index in 0..self.region().sector_count() {
            if self.sectors[sector_index as usize].entry_count == 0 {
                continue;
            }
            self.collect_sector_records(sector_index, &mut records)?;
        }
        Ok(TsIterator::new(records))
    }

    fn collect_sector_records(
        &mut self,
        sector_index: u32,
        out: &mut Vec<TsOwnedRecord>,
    ) -> Result<(), F::Error> {
        let sector_base = self
            .region()
            .sector_start(sector_index)
            .map_err(map_core_error::<F::Error>)?;
        let sector_end = sector_base
            .checked_add(self.region().erase_size())
            .ok_or(Error::OutOfBounds)?;
        let index_len = self
            .layout
            .index_header_len(self.mode)
            .map_err(map_core_error::<F::Error>)? as u32;
        let mut index_offset = sector_base
            .checked_add(
                self.layout
                    .sector_header_len()
                    .map_err(map_core_error::<F::Error>)? as u32,
            )
            .ok_or(Error::OutOfBounds)?;
        let empty_index = self.sectors[sector_index as usize].empty_index_offset;
        let mut index_buf = vec![0u8; index_len as usize];

        while index_offset < empty_index {
            self.storage.read(index_offset, &mut index_buf)?;
            let header = TsIndexHeader::decode(&self.layout, self.mode, &index_buf)
                .map_err(map_core_error::<F::Error>)?;
            if header.status == TSL_PRE_WRITE || header.status == 0 {
                break;
            }
            if header.status == TSL_WRITE {
                let log_addr = header.log_addr.ok_or(Error::CorruptedHeader)?;
                let log_len = header.log_len.ok_or(Error::CorruptedHeader)?;
                let log_end = log_addr.checked_add(log_len).ok_or(Error::OutOfBounds)?;
                if log_addr < sector_base || log_end > sector_end {
                    return Err(Error::CorruptedHeader);
                }
                let mut payload = vec![0u8; log_len as usize];
                self.storage.read(log_addr, &mut payload)?;
                out.push(TsOwnedRecord {
                    timestamp: header.timestamp,
                    payload,
                });
            }
            index_offset = index_offset
                .checked_add(index_len)
                .ok_or(Error::OutOfBounds)?;
        }

        Ok(())
    }

    fn validate_timestamp(&self, timestamp: u64) -> Result<(), F::Error> {
        let Some(last) = self.last_timestamp else {
            return Ok(());
        };
        let ok = match self.config.timestamp_policy {
            TimestampPolicy::StrictMonotonic => timestamp > last,
            TimestampPolicy::AllowEqual => timestamp >= last,
        };
        if ok {
            Ok(())
        } else {
            Err(Error::TimestampNotMonotonic {
                last,
                next: timestamp,
            })
        }
    }

    fn ensure_writable_sector(&mut self, needed: u32) -> Result<u32, F::Error> {
        let sector_count = self.region().sector_count();
        let mut sector_index = self.current_sector.unwrap_or(0);

        loop {
            if sector_index >= sector_count {
                return Err(Error::NoSpace);
            }
            let remaining = self.sector_remaining(sector_index)?;
            if remaining >= needed {
                return Ok(sector_index);
            }

            if self.sectors[sector_index as usize].entry_count == 0 {
                return Err(Error::NoSpace);
            }

            self.mark_sector_full(sector_index)?;
            sector_index = sector_index.checked_add(1).ok_or(Error::OutOfBounds)?;
            self.current_sector = Some(sector_index);
        }
    }

    fn sector_remaining(&self, sector_index: u32) -> Result<u32, F::Error> {
        let sector = self.sectors[sector_index as usize];
        sector
            .empty_data_offset
            .checked_sub(sector.empty_index_offset)
            .ok_or(Error::OutOfBounds)
    }

    fn mark_sector_full(&mut self, sector_index: u32) -> Result<(), F::Error> {
        if self.sectors[sector_index as usize].store_status == SECTOR_STORE_FULL {
            return Ok(());
        }
        let sector_base = self
            .region()
            .sector_start(sector_index)
            .map_err(map_core_error::<F::Error>)?;
        let mut scratch = vec![0u8; self.region().write_size() as usize];
        self.layout.sector_status_scheme().write_transition(
            &mut self.storage,
            sector_base,
            SECTOR_STORE_FULL,
            &mut scratch,
        )?;
        self.sectors[sector_index as usize].store_status = SECTOR_STORE_FULL;
        Ok(())
    }
}

fn blob_mode_from_config<F>(config: TsdbConfig) -> Result<TsBlobMode, F>
where
    F: core::fmt::Debug,
{
    match config.blob_mode {
        BlobMode::Variable => Ok(TsBlobMode::Variable),
        BlobMode::Fixed(_) => Err(Error::InvariantViolation(
            "TSDB fixed blob mode is not implemented yet",
        )),
    }
}

fn empty_sector_table(
    region: &StorageRegion,
    layout: &TsLayout,
    mode: TsBlobMode,
) -> Result<Vec<TsSectorRuntime>> {
    let sector_count = region.sector_count() as usize;
    let header_len = layout.sector_header_len()? as u32;
    let sector_len = region.erase_size();
    let index_len = layout.index_header_len(mode)? as u32;
    if header_len >= sector_len || index_len == 0 {
        return Err(Error::InvariantViolation(
            "TS layout does not fit inside the configured sector",
        ));
    }
    let mut sectors = Vec::with_capacity(sector_count);
    for sector_index in 0..region.sector_count() {
        let base = region.sector_start(sector_index)?;
        sectors.push(TsSectorRuntime {
            store_status: SECTOR_STORE_EMPTY,
            start_time: None,
            end_time: None,
            empty_index_offset: base + header_len,
            empty_data_offset: base + sector_len,
            entry_count: 0,
        });
    }
    Ok(sectors)
}

fn scan_all_sectors<F>(
    storage: &mut NorFlashRegion<F>,
    layout: &TsLayout,
    mode: TsBlobMode,
) -> Result<Vec<TsSectorRuntime>, F::Error>
where
    F: NorFlash,
{
    let region = *storage.region();
    let mut sectors =
        empty_sector_table(&region, layout, mode).map_err(map_core_error::<F::Error>)?;
    let header_len = layout
        .sector_header_len()
        .map_err(map_core_error::<F::Error>)?;
    let index_len = layout
        .index_header_len(mode)
        .map_err(map_core_error::<F::Error>)? as u32;
    let mut header_buf = vec![0u8; header_len];
    let mut index_buf = vec![0u8; index_len as usize];

    for sector_index in 0..region.sector_count() {
        let base = region
            .sector_start(sector_index)
            .map_err(map_core_error::<F::Error>)?;
        storage.read(base, &mut header_buf)?;
        if is_erased(&header_buf) {
            continue;
        }
        let header =
            TsSectorHeader::decode(layout, &header_buf).map_err(map_core_error::<F::Error>)?;
        let sector = &mut sectors[sector_index as usize];
        sector.store_status = header.store_status;
        sector.start_time = sentinel_to_option(header.start_time);

        let sector_end = base
            .checked_add(region.erase_size())
            .ok_or(Error::OutOfBounds)?;
        let mut index_offset = base
            .checked_add(header_len as u32)
            .ok_or(Error::OutOfBounds)?;
        let mut last_time = None;
        while index_offset
            .checked_add(index_len)
            .is_some_and(|end| end <= sector_end)
        {
            storage.read(index_offset, &mut index_buf)?;
            if is_erased(&index_buf) {
                break;
            }
            let entry = TsIndexHeader::decode(layout, mode, &index_buf)
                .map_err(map_core_error::<F::Error>)?;
            if entry.status == 0 || entry.status == TSL_PRE_WRITE {
                break;
            }
            let log_addr = entry.log_addr.ok_or(Error::CorruptedHeader)?;
            let log_len = entry.log_len.ok_or(Error::CorruptedHeader)?;
            let _aligned_len = align_up(log_len as usize, region.write_size() as usize)
                .map_err(map_core_error::<F::Error>)? as u32;
            let log_end = log_addr.checked_add(log_len).ok_or(Error::OutOfBounds)?;
            if log_addr < base || log_end > sector_end {
                return Err(Error::CorruptedHeader);
            }
            sector.empty_index_offset = index_offset
                .checked_add(index_len)
                .ok_or(Error::OutOfBounds)?;
            sector.empty_data_offset = log_addr;
            sector.entry_count += 1;
            last_time = Some(entry.timestamp);
            index_offset = sector.empty_index_offset;
        }
        sector.end_time = last_time;
    }

    Ok(sectors)
}

fn select_current_sector(sectors: &[TsSectorRuntime]) -> Option<u32> {
    if let Some((index, _)) = sectors
        .iter()
        .enumerate()
        .rev()
        .find(|(_, sector)| sector.store_status == SECTOR_STORE_USING)
    {
        return Some(index as u32);
    }
    if let Some((index, _)) = sectors
        .iter()
        .enumerate()
        .find(|(_, sector)| sector.entry_count == 0 && sector.store_status != SECTOR_STORE_FULL)
    {
        return Some(index as u32);
    }
    None
}

fn initialize_sector<F>(
    storage: &mut NorFlashRegion<F>,
    layout: &TsLayout,
    sector_base: u32,
    timestamp: u64,
    write_size: usize,
) -> Result<(), F::Error>
where
    F: NorFlash,
{
    let mut header = TsSectorHeader::new_empty();
    header.store_status = SECTOR_STORE_USING;
    header.start_time = timestamp;
    let mut header_buf = vec![
        0u8;
        layout
            .sector_header_len()
            .map_err(map_core_error::<F::Error>)?
    ];
    header
        .encode(layout, &mut header_buf)
        .map_err(map_core_error::<F::Error>)?;
    let mut scratch = vec![0u8; write_size];
    storage.write_aligned(sector_base, &header_buf, &mut scratch)
}

fn sentinel_to_option(value: u64) -> Option<u64> {
    (value != u32::MAX as u64).then_some(value)
}

fn is_erased(bytes: &[u8]) -> bool {
    bytes.iter().all(|&byte| byte == ERASED_BYTE)
}

fn map_core_error<E>(err: Error) -> Error<E>
where
    E: core::fmt::Debug,
{
    match err {
        Error::Storage(_) => {
            Error::InvariantViolation("unexpected storage error during TSDB core operation")
        }
        Error::Decode(decode) => Error::Decode(decode),
        Error::Alignment(alignment) => Error::Alignment(alignment),
        Error::OutOfBounds => Error::OutOfBounds,
        Error::CorruptedHeader => Error::CorruptedHeader,
        Error::CrcMismatch => Error::CrcMismatch,
        Error::NoSpace => Error::NoSpace,
        Error::BufferTooSmall { needed, actual } => Error::BufferTooSmall { needed, actual },
        Error::InvalidBlobOffset { offset, len } => Error::InvalidBlobOffset { offset, len },
        Error::UnsupportedFormatVersion(version) => Error::UnsupportedFormatVersion(version),
        Error::InvariantViolation(msg) => Error::InvariantViolation(msg),
        Error::TimestampNotMonotonic { last, next } => Error::TimestampNotMonotonic { last, next },
    }
}
