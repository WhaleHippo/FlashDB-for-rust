use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec;
use alloc::vec::IntoIter;
use alloc::vec::Vec;

use embedded_storage::nor_flash::NorFlash;

use crate::error::Result;

use super::db::KvDb;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KvOwnedRecord {
    pub key: String,
    pub value: Vec<u8>,
}

#[derive(Debug)]
pub struct KvIterator {
    inner: IntoIter<KvOwnedRecord>,
}

impl KvIterator {
    pub(crate) fn new(records: Vec<KvOwnedRecord>) -> Self {
        Self {
            inner: records.into_iter(),
        }
    }
}

impl Iterator for KvIterator {
    type Item = KvOwnedRecord;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

pub(crate) fn snapshot_live_records<F>(db: &mut KvDb<F>) -> Result<Vec<KvOwnedRecord>, F::Error>
where
    F: NorFlash,
{
    let mut records = Vec::new();
    let mut key_buf = vec![0u8; db.config.max_key_len];
    let mut value_buf = vec![0u8; db.config.max_value_len];

    db.for_each_live_record(&mut key_buf, &mut value_buf, |key, value| {
        records.push(KvOwnedRecord {
            key: key.to_owned(),
            value: value.to_vec(),
        });
    })?;

    Ok(records)
}
