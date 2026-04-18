use embedded_storage::nor_flash::NorFlash;

use crate::error::{Error, Result};

use super::db::KvDb;
use super::iter::{KvOwnedRecord, snapshot_live_records};
use super::write;
use crate::layout::kv::KV_WRITE;

pub(crate) fn ensure_space_for_record<F>(
    db: &mut KvDb<F>,
    key_len: usize,
    value_len: usize,
) -> Result<(), F::Error>
where
    F: NorFlash,
{
    let total_len = write::record_total_len(&db.layout, key_len as u8, value_len as u32)
        .map_err(super::scan::map_core_error)?;
    if write::has_space_for_record(&db.storage, &db.layout, db.write_cursor, total_len)? {
        return Ok(());
    }

    collect_garbage(db)?;
    if write::has_space_for_record(&db.storage, &db.layout, db.write_cursor, total_len)? {
        Ok(())
    } else {
        Err(Error::NoSpace)
    }
}

pub(crate) fn collect_garbage<F>(db: &mut KvDb<F>) -> Result<(), F::Error>
where
    F: NorFlash,
{
    let live_records = snapshot_live_records(db)?;
    rewrite_from_live_set(db, &live_records)
}

fn rewrite_from_live_set<F>(
    db: &mut KvDb<F>,
    live_records: &[KvOwnedRecord],
) -> Result<(), F::Error>
where
    F: NorFlash,
{
    db.write_cursor = write::format_storage(&mut db.storage, &db.layout)?;
    for record in live_records {
        db.write_cursor = write::append_record(
            &mut db.storage,
            &db.layout,
            db.write_cursor,
            record.key.as_bytes(),
            &record.value,
            KV_WRITE,
        )?;
    }
    Ok(())
}
