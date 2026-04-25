use std::time::{SystemTime, UNIX_EPOCH};

use flashdb_for_rust::kv::KvDb;
use flashdb_for_rust::storage::{FileFlashError, FileFlashSimulator};
use flashdb_for_rust::tsdb::TsDb;
use flashdb_for_rust::{BlobMode, KvConfig, StorageRegionConfig, TimestampPolicy, TsdbConfig};

const FLASH_BYTES: usize = 4096;
const WRITE_SIZE: usize = 4;
const ERASE_SIZE: usize = 1024;

type ExampleFlash = FileFlashSimulator<WRITE_SIZE, ERASE_SIZE>;

fn main() {
    println!("FlashDB Linux host example start");
    run_flashdb_smoke().expect("FlashDB Linux host example failed");
    println!("FlashDB Linux host example passed");
}

fn run_flashdb_smoke() -> flashdb_for_rust::Result<(), FileFlashError> {
    let kv_path = temp_flash_path("kv");
    let ts_path = temp_flash_path("ts");

    let kv_config = kv_config();
    let mut kv = KvDb::mount(
        ExampleFlash::new(&kv_path, FLASH_BYTES)
            .map_err(io_to_flash_error)
            .map_err(flashdb_for_rust::Error::Storage)?,
        kv_config,
    )?;
    kv.format()?;
    kv.set("platform", b"linux")?;
    kv.set("mode", b"host-smoke")?;

    let mut kv_buf = [0u8; 32];
    let Some(kv_len) = kv.get_blob_into("platform", &mut kv_buf)? else {
        panic!("missing KV value after write");
    };
    assert_eq!(&kv_buf[..kv_len], b"linux");

    let kv_flash = kv.into_flash();
    let mut kv_rebooted = KvDb::mount(
        kv_flash
            .reopen()
            .map_err(io_to_flash_error)
            .map_err(flashdb_for_rust::Error::Storage)?,
        kv_config,
    )?;
    let Some(kv_len) = kv_rebooted.get_blob_into("mode", &mut kv_buf)? else {
        panic!("missing KV value after reboot");
    };
    assert_eq!(&kv_buf[..kv_len], b"host-smoke");

    let ts_config = ts_config();
    let mut ts = TsDb::mount(
        ExampleFlash::new(&ts_path, FLASH_BYTES)
            .map_err(io_to_flash_error)
            .map_err(flashdb_for_rust::Error::Storage)?,
        ts_config,
    )?;
    ts.format()?;
    ts.append(1, b"cold")?;
    ts.append(2, b"warm")?;
    ts.append(3, b"hot")?;

    let forward = ts.iter()?.collect::<Vec<_>>();
    assert_eq!(forward.len(), 3);
    assert_eq!(forward[0].timestamp, 1);
    assert_eq!(forward[2].payload.as_slice(), b"hot");

    let reverse = ts.iter_reverse()?.collect::<Vec<_>>();
    assert_eq!(reverse[0].timestamp, 3);
    assert_eq!(reverse[0].payload.as_slice(), b"hot");

    let ts_flash = ts.into_flash();
    let mut rebooted = TsDb::mount(
        ts_flash
            .reopen()
            .map_err(io_to_flash_error)
            .map_err(flashdb_for_rust::Error::Storage)?,
        ts_config,
    )?;
    let by_time = rebooted.iter_by_time(3, 2)?.collect::<Vec<_>>();
    assert_eq!(by_time.len(), 2);
    assert_eq!(by_time[0].timestamp, 3);
    assert_eq!(by_time[1].timestamp, 2);
    assert_eq!(
        rebooted.query_count(1, 3, flashdb_for_rust::layout::ts::TSL_WRITE)?,
        3
    );

    println!("KV file-backed reboot round-trip ok: platform=linux, mode=host-smoke");
    println!(
        "TS file-backed query ok: latest_ts={}, window=[{}, {}]",
        reverse[0].timestamp, by_time[1].timestamp, by_time[0].timestamp
    );

    std::fs::remove_file(kv_path)
        .map_err(io_to_flash_error)
        .map_err(flashdb_for_rust::Error::Storage)?;
    std::fs::remove_file(ts_path)
        .map_err(io_to_flash_error)
        .map_err(flashdb_for_rust::Error::Storage)?;
    Ok(())
}

fn temp_flash_path(kind: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("flashdb-linux-example-{kind}-{nanos}.bin"))
}

fn io_to_flash_error(error: std::io::Error) -> FileFlashError {
    FileFlashError::Io(error.kind())
}

fn kv_config() -> KvConfig {
    KvConfig {
        region: StorageRegionConfig::new(0, 2048, ERASE_SIZE as u32, WRITE_SIZE as u32),
        max_key_len: 32,
        max_value_len: 64,
    }
}

fn ts_config() -> TsdbConfig {
    TsdbConfig {
        region: StorageRegionConfig::new(0, 2048, ERASE_SIZE as u32, WRITE_SIZE as u32),
        blob_mode: BlobMode::Variable,
        timestamp_policy: TimestampPolicy::StrictMonotonic,
        rollover: false,
    }
}
