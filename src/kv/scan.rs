use core::fmt::Debug;

use embedded_storage::nor_flash::NorFlash;

use crate::blob::locator::KvValueLocator;
use crate::crc::{CRC32_INIT, crc32_update};
use crate::error::{Error, Result};
use crate::layout::common::ERASED_BYTE;
use crate::layout::kv::{
    KV_DELETED, KV_ERR_HDR, KV_PRE_WRITE, KV_WRITE, KvLayout, KvRecordHeader, KvSectorHeader,
};
use crate::storage::NorFlashRegion;

use super::db::{HEADER_BUF_CAP, SCAN_CHUNK_CAP};
use super::recovery;
use super::write;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LookupRecord {
    pub(crate) value_locator: KvValueLocator,
    pub(crate) deleted: bool,
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
            return Ok(sector_base);
        }
        if read_sector_header(storage, layout, sector_base).is_err() {
            return Ok(sector_base);
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
                KV_WRITE | KV_DELETED => {
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

            if matches!(header.status, KV_WRITE | KV_DELETED)
                && record_crc_matches(storage, layout, record_offset, &header)?
                && key_matches(storage, layout, record_offset, &header, key)?
            {
                latest = Some(LookupRecord {
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

fn read_erased<F>(
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
