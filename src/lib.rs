#![cfg_attr(not(any(test, feature = "std")), no_std)]

pub mod blob;
pub mod config;
pub mod crc;
pub mod error;
pub mod kv;
pub mod layout;
pub mod storage;
pub mod tsdb;

pub use config::{
    BlobMode, KvConfig, MAX_KV_KEY_LEN, MAX_KV_RECORDS, MAX_KV_VALUE_LEN, MAX_RUNTIME_WRITE_SIZE,
    MAX_TS_HEADER_LEN, MAX_TS_INDEX_LEN, MAX_TS_PAYLOAD_LEN, MAX_TS_RECORDS, MAX_TS_SECTORS,
    StorageRegionConfig, TimestampPolicy, TsdbConfig,
};
pub use error::{AlignmentError, DecodeError, Error, Result};
