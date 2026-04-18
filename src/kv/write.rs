use embedded_storage::nor_flash::NorFlash;

use crate::crc::{CRC32_INIT, crc32_update};
use crate::error::{Error, Result};
use crate::layout::common::ERASED_BYTE;
use crate::layout::kv::{KV_PRE_WRITE, KvLayout, KvRecordHeader, KvSectorHeader};
use crate::storage::NorFlashRegion;

use super::db::{HEADER_BUF_CAP, SCAN_CHUNK_CAP};
use super::recovery;
use super::scan;

pub(crate) fn format_storage<F>(
    storage: &mut NorFlashRegion<F>,
    layout: &KvLayout,
) -> Result<u32, F::Error>
where
    F: NorFlash,
{
    for sector_index in 0..storage.region().sector_count() {
        storage.erase_sector(sector_index)?;
    }

    let header_len = layout.sector_header_len().map_err(scan::map_core_error)?;
    let mut header_buf = [ERASED_BYTE; HEADER_BUF_CAP];
    let header = KvSectorHeader::new_empty();
    header
        .encode(layout, &mut header_buf[..header_len])
        .map_err(scan::map_core_error)?;

    for sector_index in 0..storage.region().sector_count() {
        let sector_base = sector_index
            .checked_mul(storage.region().erase_size())
            .ok_or(Error::OutOfBounds)?;
        storage.write(sector_base, &header_buf[..header_len])?;
    }

    Ok(layout.sector_header_len().map_err(scan::map_core_error)? as u32)
}

pub(crate) fn append_record<F>(
    storage: &mut NorFlashRegion<F>,
    layout: &KvLayout,
    write_cursor: u32,
    key: &[u8],
    value: &[u8],
    final_status: usize,
) -> Result<u32, F::Error>
where
    F: NorFlash,
{
    let crc32 = compute_record_crc(layout, key, value).map_err(scan::map_core_error)?;
    let mut header = KvRecordHeader::finalized(layout, key.len() as u8, value.len() as u32, crc32)
        .map_err(scan::map_core_error)?;
    let total_len = header.total_len;
    let record_offset = find_record_slot(storage, layout, write_cursor, total_len)?;
    ensure_sector_header(storage, layout, record_offset)?;

    header.status = KV_PRE_WRITE;
    let header_len = layout.record_header_len().map_err(scan::map_core_error)?;
    let mut header_buf = [ERASED_BYTE; HEADER_BUF_CAP];
    header
        .encode(layout, &mut header_buf[..header_len])
        .map_err(scan::map_core_error)?;
    storage.write(record_offset, &header_buf[..header_len])?;

    let mut scratch = [ERASED_BYTE; HEADER_BUF_CAP];
    let payload_offset = record_offset + header_len as u32;
    write_payload(storage, payload_offset, key, &mut scratch)?;

    let value_offset =
        record_offset + layout.value_offset(&header).map_err(scan::map_core_error)? as u32;
    write_payload(storage, value_offset, value, &mut scratch)?;

    recovery::commit_record_status(storage, layout, record_offset, final_status)?;
    record_offset
        .checked_add(total_len)
        .ok_or(Error::InvariantViolation("KV write cursor overflow"))
}

fn find_record_slot<F>(
    storage: &NorFlashRegion<F>,
    layout: &KvLayout,
    cursor: u32,
    total_len: u32,
) -> Result<u32, F::Error>
where
    F: NorFlash,
{
    let region = storage.region();
    let sector_header_len = layout.sector_header_len().map_err(scan::map_core_error)? as u32;
    let mut candidate = cursor;

    loop {
        if candidate >= region.len() {
            return Err(Error::NoSpace);
        }

        let sector_index = candidate / region.erase_size();
        let sector_base = sector_index
            .checked_mul(region.erase_size())
            .ok_or(Error::OutOfBounds)?;
        let sector_end = sector_base
            .checked_add(region.erase_size())
            .ok_or(Error::OutOfBounds)?;
        let min_offset = sector_base
            .checked_add(sector_header_len)
            .ok_or(Error::OutOfBounds)?;
        let record_offset = candidate.max(min_offset);

        if record_offset
            .checked_add(total_len)
            .is_some_and(|end| end <= sector_end)
        {
            return Ok(record_offset);
        }

        candidate = sector_end;
    }
}

fn ensure_sector_header<F>(
    storage: &mut NorFlashRegion<F>,
    layout: &KvLayout,
    record_offset: u32,
) -> Result<(), F::Error>
where
    F: NorFlash,
{
    let sector_base =
        (record_offset / storage.region().erase_size()) * storage.region().erase_size();
    let header_len = layout.sector_header_len().map_err(scan::map_core_error)?;
    let mut buf = [ERASED_BYTE; HEADER_BUF_CAP];
    storage.read(sector_base, &mut buf[..header_len])?;

    if scan::is_all_erased(&buf[..header_len]) {
        let header = KvSectorHeader::new_empty();
        header
            .encode(layout, &mut buf[..header_len])
            .map_err(scan::map_core_error)?;
        storage.write(sector_base, &buf[..header_len])?;
        return Ok(());
    }

    KvSectorHeader::decode(layout, &buf[..header_len])
        .map(|_| ())
        .map_err(scan::map_core_error)
}

fn write_payload<F>(
    storage: &mut NorFlashRegion<F>,
    offset: u32,
    bytes: &[u8],
    scratch: &mut [u8; HEADER_BUF_CAP],
) -> Result<(), F::Error>
where
    F: NorFlash,
{
    if bytes.is_empty() {
        return Ok(());
    }
    let write_size = storage.region().write_size() as usize;
    storage.write_aligned(offset, bytes, &mut scratch[..write_size])
}

pub(crate) fn compute_record_crc(layout: &KvLayout, key: &[u8], value: &[u8]) -> Result<u32> {
    let header = KvRecordHeader::new(key.len() as u8, value.len() as u32);
    let seed = layout.crc_seed_bytes(&header);
    let mut state = crc32_update(CRC32_INIT, &seed);
    state = crc32_update(state, key);
    state = crc32_pad_ff(state, layout.aligned_key_len(header.key_len)? - key.len());
    state = crc32_update(state, value);
    state = crc32_pad_ff(
        state,
        layout.aligned_value_len(header.value_len)? - value.len(),
    );
    Ok(!state)
}

pub(crate) fn crc32_pad_ff(mut state: u32, padding_len: usize) -> u32 {
    let ff = [ERASED_BYTE; SCAN_CHUNK_CAP];
    let mut remaining = padding_len;
    while remaining > 0 {
        let chunk = remaining.min(ff.len());
        state = crc32_update(state, &ff[..chunk]);
        remaining -= chunk;
    }
    state
}
