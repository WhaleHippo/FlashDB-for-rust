use embedded_storage::nor_flash::NorFlash;

use crate::error::Result;
use crate::layout::kv::{KV_ERR_HDR, KvLayout};
use crate::storage::NorFlashRegion;

pub(crate) fn mark_record_invalid<F>(
    storage: &mut NorFlashRegion<F>,
    layout: &KvLayout,
    record_offset: u32,
) -> Result<(), F::Error>
where
    F: NorFlash,
{
    advance_status(storage, layout, record_offset, KV_ERR_HDR)
}

pub(crate) fn commit_record_status<F>(
    storage: &mut NorFlashRegion<F>,
    layout: &KvLayout,
    record_offset: u32,
    status: usize,
) -> Result<(), F::Error>
where
    F: NorFlash,
{
    advance_status(storage, layout, record_offset, status)
}

fn advance_status<F>(
    storage: &mut NorFlashRegion<F>,
    layout: &KvLayout,
    record_offset: u32,
    target_status: usize,
) -> Result<(), F::Error>
where
    F: NorFlash,
{
    let mut scratch = [0xFF; super::db::HEADER_BUF_CAP];
    let write_unit = layout.write_unit_bytes();
    for state in 2..=target_status {
        layout.kv_status_scheme().write_transition(
            storage,
            record_offset,
            state,
            &mut scratch[..write_unit],
        )?;
    }
    Ok(())
}
