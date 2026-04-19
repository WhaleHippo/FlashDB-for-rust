use embedded_storage::nor_flash::NorFlash;
use heapless::Vec;

use crate::config::{
    BlobMode, MAX_RUNTIME_WRITE_SIZE, MAX_TS_HEADER_LEN, MAX_TS_INDEX_LEN, MAX_TS_PAYLOAD_LEN,
    MAX_TS_RECORDS, MAX_TS_SECTORS, TimestampPolicy, TsdbConfig,
};
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
    sectors: Vec<TsSectorRuntime, MAX_TS_SECTORS>,
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
        let oldest_sector = select_oldest_sector(&sectors);
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

        let payload_storage_len = self.payload_storage_len(payload)?;
        let index_len = self
            .layout
            .index_header_len(self.mode)
            .map_err(map_core_error::<F::Error>)? as u32;
        let needed = index_len
            .checked_add(payload_storage_len)
            .ok_or(Error::OutOfBounds)?;
        let sector_index = self.ensure_writable_sector(needed)?;
        if self.total_record_count() >= MAX_TS_RECORDS {
            return Err(Error::InvariantViolation(
                "TSDB record count exceeds bounded no_alloc snapshot capacity",
            ));
        }
        let sector_base = self
            .region()
            .sector_start(sector_index)
            .map_err(map_core_error::<F::Error>)?;
        let write_size = self.region().write_size() as usize;
        let erase_size = self.region().erase_size();

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
            let data_offset = data_offset_for_entry(
                &self.layout,
                self.mode,
                erase_size,
                sector_base,
                sector.entry_count,
                sector.empty_data_offset,
                payload_storage_len,
            )
            .map_err(map_core_error::<F::Error>)?;
            let mut index_buf = [0u8; MAX_TS_INDEX_LEN];
            let index_buf = &mut index_buf[..index_len as usize];
            let index_header = match self.mode {
                TsBlobMode::Variable => {
                    TsIndexHeader::variable(timestamp, data_offset, payload.len() as u32)
                }
                TsBlobMode::Fixed(_) => TsIndexHeader::new(timestamp),
            };
            index_header
                .encode(&self.layout, self.mode, index_buf)
                .map_err(map_core_error::<F::Error>)?;
            self.storage.write(index_offset, index_buf)?;

            let mut write_scratch = [0u8; MAX_RUNTIME_WRITE_SIZE];
            self.storage
                .write_aligned(data_offset, payload, &mut write_scratch[..write_size])?;

            let mut status_scratch = [0u8; MAX_RUNTIME_WRITE_SIZE];
            self.layout.tsl_status_scheme().write_transition(
                &mut self.storage,
                index_offset,
                TSL_WRITE,
                &mut status_scratch[..write_size],
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

        self.oldest_sector = select_oldest_sector(&self.sectors);
        self.current_sector = Some(sector_index);
        self.last_timestamp = Some(timestamp);
        Ok(())
    }

    pub fn iter(&mut self) -> Result<TsIterator<MAX_TS_RECORDS>, F::Error> {
        Ok(TsIterator::new(self.snapshot_records()?))
    }

    pub fn iter_reverse(&mut self) -> Result<TsIterator<MAX_TS_RECORDS>, F::Error> {
        let mut records = self.snapshot_records()?;
        records.reverse();
        Ok(TsIterator::new(records))
    }

    pub fn iter_by_time(
        &mut self,
        from: u64,
        to: u64,
    ) -> Result<TsIterator<MAX_TS_RECORDS>, F::Error> {
        let mut records = self.snapshot_records()?;
        let (lower, upper) = if from <= to { (from, to) } else { (to, from) };
        records.retain(|record| record.timestamp >= lower && record.timestamp <= upper);
        if from > to {
            records.reverse();
        }
        Ok(TsIterator::new(records))
    }

    pub fn query_count(&mut self, from: u64, to: u64, status: usize) -> Result<usize, F::Error> {
        let records = self.snapshot_records()?;
        let (lower, upper) = if from <= to { (from, to) } else { (to, from) };
        Ok(records
            .into_iter()
            .filter(|record| {
                record.timestamp >= lower && record.timestamp <= upper && record.status == status
            })
            .count())
    }

    pub fn set_status(&mut self, timestamp: u64, status: usize) -> Result<bool, F::Error> {
        self.validate_status_target(status)?;
        let Some((index_offset, current_status)) =
            self.find_index_offset_for_timestamp(timestamp)?
        else {
            return Ok(false);
        };
        if status < current_status {
            return Err(Error::InvariantViolation(
                "TSDB status transitions must be monotonic",
            ));
        }
        if status == current_status {
            return Ok(true);
        }
        let write_size = self.region().write_size() as usize;
        let mut scratch = [0u8; MAX_RUNTIME_WRITE_SIZE];
        self.layout.tsl_status_scheme().write_transition(
            &mut self.storage,
            index_offset,
            status,
            &mut scratch[..write_size],
        )?;
        Ok(true)
    }

    pub fn clean(&mut self) -> Result<(), F::Error> {
        self.format()
    }

    fn snapshot_records(&mut self) -> Result<Vec<TsOwnedRecord, MAX_TS_RECORDS>, F::Error> {
        let mut records = Vec::new();
        for sector_index in sector_iteration_order(self.region().sector_count(), self.oldest_sector)
        {
            if self.sectors[sector_index as usize].entry_count == 0 {
                continue;
            }
            self.collect_sector_records(sector_index, &mut records)?;
        }
        Ok(records)
    }

    fn collect_sector_records(
        &mut self,
        sector_index: u32,
        out: &mut Vec<TsOwnedRecord, MAX_TS_RECORDS>,
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
        let mut index_buf = [0u8; MAX_TS_INDEX_LEN];
        let index_buf = &mut index_buf[..index_len as usize];
        let mut entry_index = 0_u32;

        while index_offset < empty_index {
            self.storage.read(index_offset, index_buf)?;
            let header = TsIndexHeader::decode(&self.layout, self.mode, index_buf)
                .map_err(map_core_error::<F::Error>)?;
            if header.status == TSL_PRE_WRITE || header.status == 0 {
                break;
            }
            let (log_addr, log_len) = record_location(
                &self.layout,
                self.mode,
                self.region().erase_size(),
                sector_base,
                entry_index,
                &header,
            )
            .map_err(map_core_error::<F::Error>)?;
            let log_end = log_addr.checked_add(log_len).ok_or(Error::OutOfBounds)?;
            if log_addr < sector_base || log_end > sector_end {
                return Err(Error::CorruptedHeader);
            }
            if log_len as usize > MAX_TS_PAYLOAD_LEN {
                return Err(Error::InvariantViolation(
                    "TSDB payload exceeds bounded no_alloc capacity",
                ));
            }
            let mut payload = Vec::new();
            let mut payload_buf = [0u8; MAX_TS_PAYLOAD_LEN];
            self.storage
                .read(log_addr, &mut payload_buf[..log_len as usize])?;
            payload
                .extend_from_slice(&payload_buf[..log_len as usize])
                .map_err(|_| {
                    Error::InvariantViolation("TSDB payload exceeded bounded no_alloc capacity")
                })?;
            out.push(TsOwnedRecord {
                status: header.status,
                timestamp: header.timestamp,
                payload,
            })
            .map_err(|_| {
                Error::InvariantViolation("TSDB snapshot exceeded bounded no_alloc capacity")
            })?;
            index_offset = index_offset
                .checked_add(index_len)
                .ok_or(Error::OutOfBounds)?;
            entry_index = entry_index.checked_add(1).ok_or(Error::OutOfBounds)?;
        }

        Ok(())
    }

    fn find_index_offset_for_timestamp(
        &mut self,
        timestamp: u64,
    ) -> Result<Option<(u32, usize)>, F::Error> {
        let index_len = self
            .layout
            .index_header_len(self.mode)
            .map_err(map_core_error::<F::Error>)? as u32;
        let mut index_buf = [0u8; MAX_TS_INDEX_LEN];
        let index_buf = &mut index_buf[..index_len as usize];

        for sector_index in sector_iteration_order(self.region().sector_count(), self.oldest_sector)
        {
            let sector = self.sectors[sector_index as usize];
            if sector.entry_count == 0 {
                continue;
            }
            let sector_base = self
                .region()
                .sector_start(sector_index)
                .map_err(map_core_error::<F::Error>)?;
            let mut index_offset = sector_base
                .checked_add(
                    self.layout
                        .sector_header_len()
                        .map_err(map_core_error::<F::Error>)? as u32,
                )
                .ok_or(Error::OutOfBounds)?;
            while index_offset < sector.empty_index_offset {
                self.storage.read(index_offset, index_buf)?;
                let header = TsIndexHeader::decode(&self.layout, self.mode, &index_buf)
                    .map_err(map_core_error::<F::Error>)?;
                if header.status == 0 || header.status == TSL_PRE_WRITE {
                    break;
                }
                if header.timestamp == timestamp {
                    return Ok(Some((index_offset, header.status)));
                }
                index_offset = index_offset
                    .checked_add(index_len)
                    .ok_or(Error::OutOfBounds)?;
            }
        }

        Ok(None)
    }

    fn validate_status_target(&self, status: usize) -> Result<(), F::Error> {
        if status == 0
            || status == TSL_PRE_WRITE
            || status >= self.layout.tsl_status_scheme().state_count()
        {
            return Err(Error::InvariantViolation("invalid TSDB target status"));
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

    fn payload_storage_len(&self, payload: &[u8]) -> Result<u32, F::Error> {
        match self.mode {
            TsBlobMode::Variable => {
                if payload.len() > MAX_TS_PAYLOAD_LEN {
                    return Err(Error::InvariantViolation(
                        "TSDB variable payload exceeds bounded no_alloc capacity",
                    ));
                }
                align_up(payload.len(), self.region().write_size() as usize)
                    .map(|len| len as u32)
                    .map_err(map_core_error::<F::Error>)
            }
            TsBlobMode::Fixed(len) => {
                if payload.len() != len as usize {
                    return Err(Error::InvariantViolation(
                        "TSDB fixed blob mode requires exact payload length",
                    ));
                }
                self.layout
                    .fixed_blob_len(self.mode)
                    .map(|len| len as u32)
                    .map_err(map_core_error::<F::Error>)
            }
        }
    }

    fn ensure_writable_sector(&mut self, needed: u32) -> Result<u32, F::Error> {
        let sector_count = self.region().sector_count();
        let mut sector_index = self.current_sector.unwrap_or(0);
        let header_len = self
            .layout
            .sector_header_len()
            .map_err(map_core_error::<F::Error>)? as u32;
        if needed
            > self
                .region()
                .erase_size()
                .checked_sub(header_len)
                .ok_or(Error::OutOfBounds)?
        {
            return Err(Error::NoSpace);
        }

        for _ in 0..sector_count {
            let remaining = self.sector_remaining(sector_index)?;
            if remaining >= needed {
                return Ok(sector_index);
            }

            if self.sectors[sector_index as usize].entry_count == 0 {
                return Err(Error::NoSpace);
            }

            self.mark_sector_full(sector_index)?;
            let Some(next_sector) =
                next_sector_index(sector_index, sector_count, self.config.rollover)
            else {
                self.current_sector = None;
                return Err(Error::NoSpace);
            };
            if self.sectors[next_sector as usize].entry_count > 0
                || self.sectors[next_sector as usize].store_status == SECTOR_STORE_FULL
            {
                if !self.config.rollover {
                    self.current_sector = None;
                    return Err(Error::NoSpace);
                }
                self.reset_sector(next_sector)?;
            }
            self.current_sector = Some(next_sector);
            self.oldest_sector = select_oldest_sector(&self.sectors);
            sector_index = next_sector;
        }

        Err(Error::NoSpace)
    }

    fn sector_remaining(&self, sector_index: u32) -> Result<u32, F::Error> {
        let sector = self.sectors[sector_index as usize];
        sector
            .empty_data_offset
            .checked_sub(sector.empty_index_offset)
            .ok_or(Error::OutOfBounds)
    }

    fn total_record_count(&self) -> usize {
        self.sectors
            .iter()
            .map(|sector| sector.entry_count as usize)
            .sum()
    }

    fn reset_sector(&mut self, sector_index: u32) -> Result<(), F::Error> {
        self.storage.erase_sector(sector_index)?;
        let sector_base = self
            .region()
            .sector_start(sector_index)
            .map_err(map_core_error::<F::Error>)?;
        self.sectors[sector_index as usize] =
            empty_sector_runtime(self.region(), &self.layout, sector_base)
                .map_err(map_core_error::<F::Error>)?;
        self.oldest_sector = select_oldest_sector(&self.sectors);
        Ok(())
    }

    fn mark_sector_full(&mut self, sector_index: u32) -> Result<(), F::Error> {
        if self.sectors[sector_index as usize].store_status == SECTOR_STORE_FULL {
            return Ok(());
        }
        let sector_base = self
            .region()
            .sector_start(sector_index)
            .map_err(map_core_error::<F::Error>)?;
        let write_size = self.region().write_size() as usize;
        let mut scratch = [0u8; MAX_RUNTIME_WRITE_SIZE];
        self.layout.sector_status_scheme().write_transition(
            &mut self.storage,
            sector_base,
            SECTOR_STORE_FULL,
            &mut scratch[..write_size],
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
        BlobMode::Fixed(len) => Ok(TsBlobMode::Fixed(len as u32)),
    }
}

fn empty_sector_table(
    region: &StorageRegion,
    layout: &TsLayout,
    mode: TsBlobMode,
) -> Result<Vec<TsSectorRuntime, MAX_TS_SECTORS>> {
    let _sector_count = region.sector_count() as usize;
    let sector_len = region.erase_size();
    let index_len = layout.index_header_len(mode)? as u32;
    let header_len = layout.sector_header_len()? as u32;
    if header_len >= sector_len || index_len == 0 {
        return Err(Error::InvariantViolation(
            "TS layout does not fit inside the configured sector",
        ));
    }
    let mut sectors = Vec::new();
    for sector_index in 0..region.sector_count() {
        let base = region.sector_start(sector_index)?;
        sectors
            .push(empty_sector_runtime(region, layout, base)?)
            .map_err(|_| {
                Error::InvariantViolation("TSDB sector table exceeded bounded no_alloc capacity")
            })?;
    }
    Ok(sectors)
}

fn empty_sector_runtime(
    region: &StorageRegion,
    layout: &TsLayout,
    sector_base: u32,
) -> Result<TsSectorRuntime> {
    Ok(TsSectorRuntime {
        store_status: SECTOR_STORE_EMPTY,
        start_time: None,
        end_time: None,
        empty_index_offset: sector_base + layout.sector_header_len()? as u32,
        empty_data_offset: sector_base + region.erase_size(),
        entry_count: 0,
    })
}

fn scan_all_sectors<F>(
    storage: &mut NorFlashRegion<F>,
    layout: &TsLayout,
    mode: TsBlobMode,
) -> Result<Vec<TsSectorRuntime, MAX_TS_SECTORS>, F::Error>
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
    let mut header_buf = [0u8; MAX_TS_HEADER_LEN];
    let mut index_buf = [0u8; MAX_TS_INDEX_LEN];

    for sector_index in 0..region.sector_count() {
        let base = region
            .sector_start(sector_index)
            .map_err(map_core_error::<F::Error>)?;
        storage.read(base, &mut header_buf[..header_len])?;
        if is_erased(&header_buf[..header_len]) {
            continue;
        }
        let header = TsSectorHeader::decode(layout, &header_buf[..header_len])
            .map_err(map_core_error::<F::Error>)?;
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
        let mut entry_index = 0_u32;
        while index_offset
            .checked_add(index_len)
            .is_some_and(|end| end <= sector.empty_data_offset)
        {
            storage.read(index_offset, &mut index_buf[..index_len as usize])?;
            if is_erased(&index_buf[..index_len as usize]) {
                break;
            }
            let entry = TsIndexHeader::decode(layout, mode, &index_buf[..index_len as usize])
                .map_err(map_core_error::<F::Error>)?;
            if entry.status == 0 || entry.status == TSL_PRE_WRITE {
                break;
            }
            let (log_addr, log_len) =
                record_location(layout, mode, region.erase_size(), base, entry_index, &entry)
                    .map_err(map_core_error::<F::Error>)?;
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
            entry_index = entry_index.checked_add(1).ok_or(Error::OutOfBounds)?;
        }
        sector.end_time = last_time;
    }

    Ok(sectors)
}

fn select_oldest_sector(sectors: &[TsSectorRuntime]) -> Option<u32> {
    sectors
        .iter()
        .enumerate()
        .filter(|(_, sector)| sector.entry_count > 0)
        .min_by_key(|(_, sector)| sector.start_time.unwrap_or(u64::MAX))
        .map(|(index, _)| index as u32)
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

fn sector_iteration_order(
    sector_count: u32,
    oldest_sector: Option<u32>,
) -> Vec<u32, MAX_TS_SECTORS> {
    if sector_count == 0 {
        return Vec::new();
    }
    let start = oldest_sector.unwrap_or(0) % sector_count;
    let mut order = Vec::new();
    for offset in 0..sector_count {
        order
            .push((start + offset) % sector_count)
            .expect("sector_iteration_order bounded by validated MAX_TS_SECTORS");
    }
    order
}

fn next_sector_index(sector_index: u32, sector_count: u32, rollover: bool) -> Option<u32> {
    if sector_index + 1 < sector_count {
        Some(sector_index + 1)
    } else if rollover {
        Some(0)
    } else {
        None
    }
}

fn data_offset_for_entry(
    layout: &TsLayout,
    mode: TsBlobMode,
    sector_len: u32,
    sector_base: u32,
    entry_index: u32,
    empty_data_offset: u32,
    payload_storage_len: u32,
) -> Result<u32> {
    match mode {
        TsBlobMode::Variable => empty_data_offset
            .checked_sub(payload_storage_len)
            .ok_or(Error::OutOfBounds),
        TsBlobMode::Fixed(_) => {
            let relative =
                layout.fixed_blob_data_offset(sector_len as usize, mode, entry_index as usize)?
                    as u32;
            sector_base.checked_add(relative).ok_or(Error::OutOfBounds)
        }
    }
}

fn record_location(
    layout: &TsLayout,
    mode: TsBlobMode,
    sector_len: u32,
    sector_base: u32,
    entry_index: u32,
    header: &TsIndexHeader,
) -> Result<(u32, u32)> {
    match mode {
        TsBlobMode::Variable => Ok((
            header.log_addr.ok_or(Error::CorruptedHeader)?,
            header.log_len.ok_or(Error::CorruptedHeader)?,
        )),
        TsBlobMode::Fixed(_) => {
            let relative =
                layout.fixed_blob_data_offset(sector_len as usize, mode, entry_index as usize)?
                    as u32;
            let log_len = layout.fixed_blob_len(mode)? as u32;
            Ok((
                sector_base
                    .checked_add(relative)
                    .ok_or(Error::OutOfBounds)?,
                log_len,
            ))
        }
    }
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
    let header_len = layout
        .sector_header_len()
        .map_err(map_core_error::<F::Error>)?;
    let mut header_buf = [0u8; MAX_TS_HEADER_LEN];
    header
        .encode(layout, &mut header_buf[..header_len])
        .map_err(map_core_error::<F::Error>)?;
    let mut scratch = [0u8; MAX_RUNTIME_WRITE_SIZE];
    storage.write_aligned(
        sector_base,
        &header_buf[..header_len],
        &mut scratch[..write_size],
    )
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
