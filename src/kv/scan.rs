use core::fmt::Debug;
use core::str;

use embedded_storage::nor_flash::NorFlash;

use crate::blob::locator::KvValueLocator;
use crate::crc::{CRC32_INIT, crc32_update};
use crate::error::{Error, Result};
use crate::layout::common::ERASED_BYTE;
use crate::layout::kv::{
    KV_DELETED, KV_ERR_HDR, KV_PRE_DELETE, KV_PRE_WRITE, KV_WRITE, KvLayout, KvRecordHeader,
    KvSectorHeader, SECTOR_DIRTY_FALSE, SECTOR_STORE_EMPTY, SECTOR_STORE_FULL, SECTOR_STORE_USING,
};
use crate::storage::NorFlashRegion;

use super::db::{HEADER_BUF_CAP, SCAN_CHUNK_CAP};
use super::recovery;
use super::write;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LookupRecord {
    pub(crate) record_offset: u32,
    pub(crate) value_locator: KvValueLocator,
    pub(crate) deleted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KvSectorMeta {
    pub sector_index: u32,
    pub store_status: usize,
    pub dirty_status: usize,
    pub next_record_offset: u32,
    pub remaining_bytes: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KvIntegrityReport {
    pub sector_issues: u32,
    pub record_issues: u32,
}

impl KvIntegrityReport {
    pub const fn is_clean(&self) -> bool {
        self.sector_issues == 0 && self.record_issues == 0
    }
}

pub(crate) fn ensure_runtime_caps<E>(layout: &KvLayout, write_size: usize) -> Result<(), E>
where
    E: Debug,
{
    let sector_header_len = layout.sector_header_len().map_err(map_core_error)?;
    let record_header_len = layout.record_header_len().map_err(map_core_error)?;
    if sector_header_len > HEADER_BUF_CAP {
        return Err(Error::InvariantViolation(
            "KV sector header exceeds internal scratch capacity",
        ));
    }
    if record_header_len > HEADER_BUF_CAP {
        return Err(Error::InvariantViolation(
            "KV record header exceeds internal scratch capacity",
        ));
    }
    if write_size > HEADER_BUF_CAP {
        return Err(Error::InvariantViolation(
            "KV write size exceeds internal scratch capacity",
        ));
    }
    Ok(())
}

pub(crate) fn boot_scan<F>(
    storage: &mut NorFlashRegion<F>,
    layout: &KvLayout,
) -> Result<u32, F::Error>
where
    F: NorFlash,
{
    let region = *storage.region();
    let sector_header_len = layout.sector_header_len().map_err(map_core_error)? as u32;
    let record_header_len = layout.record_header_len().map_err(map_core_error)? as u32;

    for sector_index in 0..region.sector_count() {
        let sector_base = sector_index
            .checked_mul(region.erase_size())
            .ok_or(Error::OutOfBounds)?;
        if read_erased(storage, sector_base, sector_header_len as usize)? {
            return Ok(sector_base + sector_header_len);
        }
        if read_sector_header(storage, layout, sector_base).is_err() {
            return Ok(sector_base + sector_header_len);
        }

        let sector_end = sector_base
            .checked_add(region.erase_size())
            .ok_or(Error::OutOfBounds)?;
        let mut record_offset = sector_base
            .checked_add(sector_header_len)
            .ok_or(Error::OutOfBounds)?;

        while record_offset
            .checked_add(record_header_len)
            .is_some_and(|end| end <= sector_end)
        {
            if read_erased(storage, record_offset, record_header_len as usize)? {
                return Ok(record_offset);
            }

            let header = match read_record_header(storage, layout, record_offset) {
                Ok(header) => header,
                Err(_) => return Ok(record_offset),
            };
            let next_offset = record_offset
                .checked_add(header.total_len)
                .ok_or(Error::OutOfBounds)?;
            if next_offset > sector_end {
                return Ok(record_offset);
            }

            match header.status {
                KV_WRITE | KV_DELETED | KV_PRE_DELETE => {
                    if !record_crc_matches(storage, layout, record_offset, &header)? {
                        recovery::mark_record_invalid(storage, layout, record_offset)?;
                    }
                }
                KV_PRE_WRITE => {
                    recovery::mark_record_invalid(storage, layout, record_offset)?;
                }
                KV_ERR_HDR => {}
                _ => {
                    recovery::mark_record_invalid(storage, layout, record_offset)?;
                }
            }

            record_offset = next_offset;
        }

        if record_offset < sector_end {
            return Ok(record_offset);
        }
    }

    Ok(region.len())
}

pub(crate) fn lookup_key<F>(
    storage: &mut NorFlashRegion<F>,
    layout: &KvLayout,
    key: &[u8],
) -> Result<Option<LookupRecord>, F::Error>
where
    F: NorFlash,
{
    let region = *storage.region();
    let sector_header_len = layout.sector_header_len().map_err(map_core_error)? as u32;
    let record_header_len = layout.record_header_len().map_err(map_core_error)? as u32;
    let mut latest = None;

    for sector_index in 0..region.sector_count() {
        let sector_base = sector_index
            .checked_mul(region.erase_size())
            .ok_or(Error::OutOfBounds)?;
        if read_erased(storage, sector_base, sector_header_len as usize)? {
            break;
        }
        if read_sector_header(storage, layout, sector_base).is_err() {
            break;
        }

        let sector_end = sector_base
            .checked_add(region.erase_size())
            .ok_or(Error::OutOfBounds)?;
        let mut record_offset = sector_base
            .checked_add(sector_header_len)
            .ok_or(Error::OutOfBounds)?;

        while record_offset
            .checked_add(record_header_len)
            .is_some_and(|end| end <= sector_end)
        {
            if read_erased(storage, record_offset, record_header_len as usize)? {
                break;
            }

            let header = match read_record_header(storage, layout, record_offset) {
                Ok(header) => header,
                Err(_) => break,
            };
            let next_offset = record_offset
                .checked_add(header.total_len)
                .ok_or(Error::OutOfBounds)?;
            if next_offset > sector_end {
                break;
            }

            if matches!(header.status, KV_WRITE | KV_DELETED | KV_PRE_DELETE)
                && record_crc_matches(storage, layout, record_offset, &header)?
                && key_matches(storage, layout, record_offset, &header, key)?
            {
                latest = Some(LookupRecord {
                    record_offset,
                    value_locator: value_locator(storage, layout, record_offset, &header)?,
                    deleted: header.status == KV_DELETED,
                });
            }

            record_offset = next_offset;
        }
    }

    Ok(latest)
}

pub(crate) fn read_sector_header<F>(
    storage: &mut NorFlashRegion<F>,
    layout: &KvLayout,
    sector_base: u32,
) -> Result<KvSectorHeader, F::Error>
where
    F: NorFlash,
{
    let header_len = layout.sector_header_len().map_err(map_core_error)?;
    let mut buf = [ERASED_BYTE; HEADER_BUF_CAP];
    storage.read(sector_base, &mut buf[..header_len])?;
    KvSectorHeader::decode(layout, &buf[..header_len]).map_err(map_core_error)
}

pub(crate) fn read_record_header<F>(
    storage: &mut NorFlashRegion<F>,
    layout: &KvLayout,
    record_offset: u32,
) -> Result<KvRecordHeader, F::Error>
where
    F: NorFlash,
{
    let header_len = layout.record_header_len().map_err(map_core_error)?;
    let mut buf = [ERASED_BYTE; HEADER_BUF_CAP];
    storage.read(record_offset, &mut buf[..header_len])?;
    KvRecordHeader::decode(layout, &buf[..header_len]).map_err(map_core_error)
}

pub(crate) fn is_all_erased(bytes: &[u8]) -> bool {
    bytes.iter().all(|&byte| byte == ERASED_BYTE)
}

pub(crate) fn map_core_error<E>(err: Error) -> Error<E>
where
    E: Debug,
{
    match err {
        Error::Storage(_) => {
            Error::InvariantViolation("unexpected storage error remapping core KV error")
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

pub(crate) fn read_erased<F>(
    storage: &mut NorFlashRegion<F>,
    offset: u32,
    len: usize,
) -> Result<bool, F::Error>
where
    F: NorFlash,
{
    let mut buf = [ERASED_BYTE; HEADER_BUF_CAP];
    storage.read(offset, &mut buf[..len])?;
    Ok(is_all_erased(&buf[..len]))
}

pub(crate) fn read_key_into<F>(
    storage: &mut NorFlashRegion<F>,
    layout: &KvLayout,
    record_offset: u32,
    header: &KvRecordHeader,
    out: &mut [u8],
) -> Result<usize, F::Error>
where
    F: NorFlash,
{
    let key_len = header.key_len as usize;
    if out.len() < key_len {
        return Err(Error::BufferTooSmall {
            needed: key_len,
            actual: out.len(),
        });
    }

    let header_len = layout.record_header_len().map_err(map_core_error)? as u32;
    let key_offset = record_offset
        .checked_add(header_len)
        .ok_or(Error::OutOfBounds)?;
    storage.read(key_offset, &mut out[..key_len])?;
    Ok(key_len)
}

pub(crate) fn read_value_into<F>(
    storage: &mut NorFlashRegion<F>,
    layout: &KvLayout,
    record_offset: u32,
    header: &KvRecordHeader,
    out: &mut [u8],
) -> Result<usize, F::Error>
where
    F: NorFlash,
{
    let value_len = header.value_len as usize;
    if out.len() < value_len {
        return Err(Error::BufferTooSmall {
            needed: value_len,
            actual: out.len(),
        });
    }

    let value_offset = record_offset
        .checked_add(layout.value_offset(header).map_err(map_core_error)? as u32)
        .ok_or(Error::OutOfBounds)?;
    storage.read(value_offset, &mut out[..value_len])?;
    Ok(value_len)
}

pub(crate) fn sector_meta<F>(
    storage: &mut NorFlashRegion<F>,
    layout: &KvLayout,
    sector_index: u32,
) -> Result<KvSectorMeta, F::Error>
where
    F: NorFlash,
{
    let region = *storage.region();
    if sector_index >= region.sector_count() {
        return Err(Error::OutOfBounds);
    }

    let sector_base = sector_index
        .checked_mul(region.erase_size())
        .ok_or(Error::OutOfBounds)?;
    let sector_end = sector_base
        .checked_add(region.erase_size())
        .ok_or(Error::OutOfBounds)?;
    let sector_header_len = layout.sector_header_len().map_err(map_core_error)? as u32;
    let record_header_len = layout.record_header_len().map_err(map_core_error)? as u32;

    if read_erased(storage, sector_base, sector_header_len as usize)? {
        let next_record_offset = sector_base + sector_header_len;
        return Ok(KvSectorMeta {
            sector_index,
            store_status: SECTOR_STORE_EMPTY,
            dirty_status: SECTOR_DIRTY_FALSE,
            next_record_offset,
            remaining_bytes: sector_end - next_record_offset,
        });
    }

    let header = read_sector_header(storage, layout, sector_base)?;
    let mut next_record_offset = sector_base + sector_header_len;
    while next_record_offset
        .checked_add(record_header_len)
        .is_some_and(|end| end <= sector_end)
    {
        if read_erased(storage, next_record_offset, record_header_len as usize)? {
            break;
        }
        let record = match read_record_header(storage, layout, next_record_offset) {
            Ok(record) => record,
            Err(_) => break,
        };
        let candidate = next_record_offset
            .checked_add(record.total_len)
            .ok_or(Error::OutOfBounds)?;
        if candidate > sector_end {
            break;
        }
        next_record_offset = candidate;
    }

    let remaining_bytes = sector_end - next_record_offset;
    let store_status = if next_record_offset == sector_base + sector_header_len {
        SECTOR_STORE_EMPTY
    } else if remaining_bytes < record_header_len {
        SECTOR_STORE_FULL
    } else {
        SECTOR_STORE_USING
    };

    Ok(KvSectorMeta {
        sector_index,
        store_status,
        dirty_status: header.dirty_status,
        next_record_offset,
        remaining_bytes,
    })
}

pub(crate) fn integrity_check<F>(
    storage: &mut NorFlashRegion<F>,
    layout: &KvLayout,
) -> Result<KvIntegrityReport, F::Error>
where
    F: NorFlash,
{
    let region = *storage.region();
    let sector_header_len = layout.sector_header_len().map_err(map_core_error)? as u32;
    let record_header_len = layout.record_header_len().map_err(map_core_error)? as u32;
    let mut report = KvIntegrityReport {
        sector_issues: 0,
        record_issues: 0,
    };

    for sector_index in 0..region.sector_count() {
        let sector_base = sector_index
            .checked_mul(region.erase_size())
            .ok_or(Error::OutOfBounds)?;
        let sector_end = sector_base
            .checked_add(region.erase_size())
            .ok_or(Error::OutOfBounds)?;
        if read_erased(storage, sector_base, sector_header_len as usize)? {
            continue;
        }
        if read_sector_header(storage, layout, sector_base).is_err() {
            report.sector_issues += 1;
            continue;
        }

        let mut record_offset = sector_base + sector_header_len;
        while record_offset
            .checked_add(record_header_len)
            .is_some_and(|end| end <= sector_end)
        {
            if read_erased(storage, record_offset, record_header_len as usize)? {
                break;
            }
            let header = match read_record_header(storage, layout, record_offset) {
                Ok(header) => header,
                Err(_) => {
                    report.record_issues += 1;
                    break;
                }
            };
            let next_offset = match record_offset.checked_add(header.total_len) {
                Some(next_offset) if next_offset <= sector_end => next_offset,
                _ => {
                    report.record_issues += 1;
                    break;
                }
            };
            if matches!(header.status, KV_WRITE | KV_DELETED | KV_PRE_DELETE)
                && !record_crc_matches(storage, layout, record_offset, &header)?
            {
                report.record_issues += 1;
            }
            record_offset = next_offset;
        }
    }

    Ok(report)
}

pub(crate) fn utf8_key(bytes: &[u8]) -> Result<&str> {
    str::from_utf8(bytes)
        .map_err(|_| Error::InvariantViolation("KV key bytes must remain valid UTF-8"))
}

fn value_locator<F>(
    storage: &NorFlashRegion<F>,
    layout: &KvLayout,
    record_offset: u32,
    header: &KvRecordHeader,
) -> Result<KvValueLocator, F::Error>
where
    F: NorFlash,
{
    let value_offset = layout.value_offset(header).map_err(map_core_error)? as u32;
    let data_offset = record_offset
        .checked_add(value_offset)
        .ok_or(Error::OutOfBounds)?;
    KvValueLocator::new(
        storage.region(),
        record_offset,
        data_offset,
        header.value_len,
    )
    .map_err(map_core_error)
}

fn key_matches<F>(
    storage: &mut NorFlashRegion<F>,
    layout: &KvLayout,
    record_offset: u32,
    header: &KvRecordHeader,
    key: &[u8],
) -> Result<bool, F::Error>
where
    F: NorFlash,
{
    if header.key_len as usize != key.len() {
        return Ok(false);
    }

    let mut remaining = key.len();
    let mut compared = 0usize;
    let mut buf = [0u8; SCAN_CHUNK_CAP];
    let header_len = layout.record_header_len().map_err(map_core_error)? as u32;
    let mut flash_offset = record_offset
        .checked_add(header_len)
        .ok_or(Error::OutOfBounds)?;

    while remaining > 0 {
        let chunk = remaining.min(buf.len());
        storage.read(flash_offset, &mut buf[..chunk])?;
        if buf[..chunk] != key[compared..compared + chunk] {
            return Ok(false);
        }
        flash_offset = flash_offset
            .checked_add(chunk as u32)
            .ok_or(Error::OutOfBounds)?;
        compared += chunk;
        remaining -= chunk;
    }

    Ok(true)
}

pub(crate) fn record_crc_matches<F>(
    storage: &mut NorFlashRegion<F>,
    layout: &KvLayout,
    record_offset: u32,
    header: &KvRecordHeader,
) -> Result<bool, F::Error>
where
    F: NorFlash,
{
    let seed = layout.crc_seed_bytes(header);
    let mut state = crc32_update(CRC32_INIT, &seed);
    let mut buf = [0u8; SCAN_CHUNK_CAP];

    let header_len = layout.record_header_len().map_err(map_core_error)? as u32;
    let key_offset = record_offset
        .checked_add(header_len)
        .ok_or(Error::OutOfBounds)?;
    state = crc_region(
        storage,
        key_offset,
        header.key_len as usize,
        &mut buf,
        state,
    )?;
    let key_padding = layout
        .aligned_key_len(header.key_len)
        .map_err(map_core_error)?
        - header.key_len as usize;
    state = write::crc32_pad_ff(state, key_padding);

    let value_rel = layout.value_offset(header).map_err(map_core_error)? as u32;
    let value_offset = record_offset
        .checked_add(value_rel)
        .ok_or(Error::OutOfBounds)?;
    state = crc_region(
        storage,
        value_offset,
        header.value_len as usize,
        &mut buf,
        state,
    )?;
    let value_padding = layout
        .aligned_value_len(header.value_len)
        .map_err(map_core_error)?
        - header.value_len as usize;
    state = write::crc32_pad_ff(state, value_padding);

    Ok((!state) == header.crc32)
}

fn crc_region<F>(
    storage: &mut NorFlashRegion<F>,
    mut offset: u32,
    len: usize,
    buf: &mut [u8; SCAN_CHUNK_CAP],
    mut state: u32,
) -> Result<u32, F::Error>
where
    F: NorFlash,
{
    let mut remaining = len;
    while remaining > 0 {
        let chunk = remaining.min(buf.len());
        storage.read(offset, &mut buf[..chunk])?;
        state = crc32_update(state, &buf[..chunk]);
        offset = offset.checked_add(chunk as u32).ok_or(Error::OutOfBounds)?;
        remaining -= chunk;
    }
    Ok(state)
}
