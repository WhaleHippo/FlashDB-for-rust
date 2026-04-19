use heapless::Vec;

use crate::config::MAX_TS_PAYLOAD_LEN;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TsOwnedRecord {
    pub status: usize,
    pub timestamp: u64,
    pub payload: Vec<u8, MAX_TS_PAYLOAD_LEN>,
}

#[derive(Clone)]
pub struct TsIterator<const N: usize> {
    inner: <Vec<TsOwnedRecord, N> as IntoIterator>::IntoIter,
}

impl<const N: usize> TsIterator<N> {
    pub(crate) fn new(records: Vec<TsOwnedRecord, N>) -> Self {
        Self {
            inner: records.into_iter(),
        }
    }
}

impl<const N: usize> Iterator for TsIterator<N> {
    type Item = TsOwnedRecord;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}
