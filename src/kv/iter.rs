use heapless::{String, Vec};

use embedded_storage::nor_flash::NorFlash;

use crate::config::{MAX_KV_KEY_LEN, MAX_KV_RECORDS, MAX_KV_VALUE_LEN};
use crate::error::{Error, Result};

use super::db::KvDb;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KvOwnedRecord {
    pub key: String<MAX_KV_KEY_LEN>,
    pub value: Vec<u8, MAX_KV_VALUE_LEN>,
}

pub struct KvIterator {
    inner: <Vec<KvOwnedRecord, MAX_KV_RECORDS> as IntoIterator>::IntoIter,
}

impl KvIterator {
    pub(crate) fn new(records: Vec<KvOwnedRecord, MAX_KV_RECORDS>) -> Self {
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

pub(crate) fn snapshot_live_records<F>(
    db: &mut KvDb<F>,
) -> Result<Vec<KvOwnedRecord, MAX_KV_RECORDS>, F::Error>
where
    F: NorFlash,
{
    let mut records = Vec::new();
    let mut key_buf = [0u8; MAX_KV_KEY_LEN];
    let mut value_buf = [0u8; MAX_KV_VALUE_LEN];
    let mut push_error = None;

    db.for_each_live_record(&mut key_buf, &mut value_buf, |key, value| {
        if push_error.is_some() {
            return;
        }

        let mut owned_key = String::new();
        if owned_key.push_str(key).is_err() {
            push_error = Some(Error::InvariantViolation(
                "KV key exceeded bounded no_alloc capacity",
            ));
            return;
        }

        let mut owned_value = Vec::new();
        if owned_value.extend_from_slice(value).is_err() {
            push_error = Some(Error::InvariantViolation(
                "KV value exceeded bounded no_alloc capacity",
            ));
            return;
        }

        if records
            .push(KvOwnedRecord {
                key: owned_key,
                value: owned_value,
            })
            .is_err()
        {
            push_error = Some(Error::InvariantViolation(
                "KV live-set exceeded bounded no_alloc capacity",
            ));
        }
    })?;

    if let Some(err) = push_error {
        return Err(err);
    }

    Ok(records)
}
