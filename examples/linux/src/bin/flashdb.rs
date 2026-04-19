use flashdb_for_rust::kv::KvDb;
use flashdb_for_rust::storage::MockFlash;
use flashdb_for_rust::tsdb::TsDb;
use flashdb_for_rust::{BlobMode, KvConfig, StorageRegionConfig, TimestampPolicy, TsdbConfig};

const FLASH_BYTES: usize = 4096;
const WRITE_SIZE: usize = 4;
const ERASE_SIZE: usize = 1024;

type ExampleFlash = MockFlash<FLASH_BYTES, WRITE_SIZE, ERASE_SIZE>;

fn main() {
    println!("FlashDB Linux host example start");
    run_flashdb_smoke().expect("FlashDB Linux host example failed");
    println!("FlashDB Linux host example passed");
}

fn run_flashdb_smoke()
-> flashdb_for_rust::Result<(), flashdb_for_rust::storage::mock::MockFlashError> {
    let kv_config = kv_config();
    let mut kv = KvDb::mount(ExampleFlash::new(), kv_config)?;
    kv.format()?;
    kv.set("platform", b"linux")?;
    kv.set("mode", b"host-smoke")?;

    let mut kv_buf = [0u8; 32];
    let Some(kv_len) = kv.get_blob_into("platform", &mut kv_buf)? else {
        panic!("missing KV value after write");
    };
    assert_eq!(&kv_buf[..kv_len], b"linux");

    let kv_flash = kv.into_flash();
    let mut kv_rebooted = KvDb::mount(kv_flash, kv_config)?;
    let Some(kv_len) = kv_rebooted.get_blob_into("mode", &mut kv_buf)? else {
        panic!("missing KV value after reboot");
    };
    assert_eq!(&kv_buf[..kv_len], b"host-smoke");

    let ts_config = ts_config();
    let mut ts = TsDb::mount(ExampleFlash::new(), ts_config)?;
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
    let mut rebooted = TsDb::mount(ts_flash, ts_config)?;
    let by_time = rebooted.iter_by_time(3, 2)?.collect::<Vec<_>>();
    assert_eq!(by_time.len(), 2);
    assert_eq!(by_time[0].timestamp, 3);
    assert_eq!(by_time[1].timestamp, 2);
    assert_eq!(
        rebooted.query_count(1, 3, flashdb_for_rust::layout::ts::TSL_WRITE)?,
        3
    );

    println!("KV reboot round-trip ok: platform=linux, mode=host-smoke");
    println!(
        "TS query ok: latest_ts={}, window=[{}, {}]",
        reverse[0].timestamp, by_time[1].timestamp, by_time[0].timestamp
    );

    Ok(())
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
