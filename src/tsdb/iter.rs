use alloc::vec;
use alloc::vec::Vec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TsOwnedRecord {
    pub status: usize,
    pub timestamp: u64,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct TsIterator {
    inner: vec::IntoIter<TsOwnedRecord>,
}

impl TsIterator {
    pub(crate) fn new(records: Vec<TsOwnedRecord>) -> Self {
        Self {
            inner: records.into_iter(),
        }
    }
}

impl Iterator for TsIterator {
    type Item = TsOwnedRecord;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}
