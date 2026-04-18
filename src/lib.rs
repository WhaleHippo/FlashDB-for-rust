#![cfg_attr(not(any(test, feature = "std")), no_std)]

extern crate alloc;

pub mod blob;
pub mod config;
pub mod crc;
pub mod error;
pub mod kv;
pub mod layout;
pub mod storage;
pub mod tsdb;

pub use config::{BlobMode, KvConfig, StorageRegionConfig, TimestampPolicy, TsdbConfig};
pub use error::{AlignmentError, DecodeError, Error, Result};
