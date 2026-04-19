#![cfg(feature = "std")]

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use flashdb_for_rust::kv::KvDb;
use flashdb_for_rust::storage::file_sim::{FileFlashError, FileFlashSimulator};
use flashdb_for_rust::tsdb::TsDb;
use flashdb_for_rust::{BlobMode, KvConfig, StorageRegionConfig, TimestampPolicy, TsdbConfig};

const FLASH_BYTES: usize = 2048;
const WRITE_SIZE: usize = 4;
const ERASE_SIZE: usize = 256;

type TestFlash = FileFlashSimulator<WRITE_SIZE, ERASE_SIZE>;

fn unique_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("flashdb-for-rust-{name}-{nanos}.bin"))
}

fn kv_config() -> KvConfig {
    KvConfig {
        region: StorageRegionConfig::new(0, 1024, ERASE_SIZE as u32, WRITE_SIZE as u32),
        max_key_len: 32,
        max_value_len: 64,
    }
}

fn ts_config() -> TsdbConfig {
    TsdbConfig {
        region: StorageRegionConfig::new(0, 1024, ERASE_SIZE as u32, WRITE_SIZE as u32),
        blob_mode: BlobMode::Variable,
        timestamp_policy: TimestampPolicy::StrictMonotonic,
        rollover: false,
    }
}

#[test]
fn file_flash_simulator_persists_kv_and_tsdb_state_across_reopen() {
    let kv_path = unique_path("kv-persistence");
    let ts_path = unique_path("ts-persistence");

    let flash = TestFlash::new(&kv_path, FLASH_BYTES).unwrap();
    let mut kv = KvDb::mount(flash, kv_config()).unwrap();
    kv.format().unwrap();
    kv.set("mode", b"file-backed").unwrap();
    let flash = kv.into_flash();

    let mut kv_rebooted = KvDb::mount(flash.reopen().unwrap(), kv_config()).unwrap();
    let mut buf = [0u8; 32];
    let len = kv_rebooted
        .get_blob_into("mode", &mut buf)
        .unwrap()
        .unwrap();
    assert_eq!(&buf[..len], b"file-backed");

    let flash = TestFlash::new(&ts_path, FLASH_BYTES).unwrap();
    let mut ts = TsDb::mount(flash, ts_config()).unwrap();
    ts.format().unwrap();
    ts.append(1, b"cold").unwrap();
    ts.append(2, b"warm").unwrap();
    let flash = ts.into_flash();

    let mut ts_rebooted = TsDb::mount(flash.reopen().unwrap(), ts_config()).unwrap();
    let records = ts_rebooted.iter_reverse().unwrap().collect::<Vec<_>>();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].timestamp, 2);
    assert_eq!(records[0].payload.as_slice(), b"warm");
    assert_eq!(records[1].timestamp, 1);
    assert_eq!(records[1].payload.as_slice(), b"cold");

    std::fs::remove_file(kv_path).unwrap();
    std::fs::remove_file(ts_path).unwrap();
}

#[test]
fn file_flash_simulator_requires_erase_before_rewriting_bits_to_one() {
    let path = unique_path("erase");
    let mut flash = TestFlash::new(&path, FLASH_BYTES).unwrap();

    embedded_storage::nor_flash::NorFlash::write(&mut flash, 0, &[0x0F, 0x0F, 0x0F, 0x0F]).unwrap();
    let err =
        embedded_storage::nor_flash::NorFlash::write(&mut flash, 0, &[0xF0, 0xF0, 0xF0, 0xF0])
            .unwrap_err();
    assert_eq!(err, FileFlashError::RequiresErase);

    embedded_storage::nor_flash::NorFlash::erase(&mut flash, 0, ERASE_SIZE as u32).unwrap();
    embedded_storage::nor_flash::NorFlash::write(&mut flash, 0, &[0xF0, 0xF0, 0xF0, 0xF0]).unwrap();

    std::fs::remove_file(path).unwrap();
}
