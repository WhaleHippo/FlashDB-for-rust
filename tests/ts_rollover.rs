use flashdb_for_rust::error::Error;
use flashdb_for_rust::layout::ts::TSL_WRITE;
use flashdb_for_rust::storage::MockFlash;
use flashdb_for_rust::tsdb::TsDb;
use flashdb_for_rust::{BlobMode, StorageRegionConfig, TimestampPolicy, TsdbConfig};

type TestFlash = MockFlash<2048, 4, 256>;

fn variable_config(rollover: bool) -> TsdbConfig {
    TsdbConfig {
        region: StorageRegionConfig::new(0, 512, 256, 4),
        blob_mode: BlobMode::Variable,
        timestamp_policy: TimestampPolicy::StrictMonotonic,
        rollover,
    }
}

fn fixed_config(rollover: bool) -> TsdbConfig {
    TsdbConfig {
        region: StorageRegionConfig::new(0, 512, 256, 4),
        blob_mode: BlobMode::Fixed(16),
        timestamp_policy: TimestampPolicy::StrictMonotonic,
        rollover,
    }
}

#[test]
fn tsdb_non_rollover_returns_no_space_after_last_sector_fills() {
    let mut db = TsDb::mount(TestFlash::new(), variable_config(false)).unwrap();
    db.format().unwrap();

    let payload = vec![0xAB; 56];
    db.append(10, &payload).unwrap();
    db.append(20, &payload).unwrap();
    db.append(30, &payload).unwrap();
    db.append(40, &payload).unwrap();

    let err = db.append(50, &payload).unwrap_err();
    assert!(matches!(err, Error::NoSpace));
}

#[test]
fn tsdb_rollover_wraps_and_recovers_oldest_current_and_live_records() {
    let config = variable_config(true);
    let mut db = TsDb::mount(TestFlash::new(), config).unwrap();
    db.format().unwrap();

    let payload = vec![0xCD; 56];
    for ts in [10_u64, 20, 30, 40, 50] {
        db.append(ts, &payload).unwrap();
    }

    let records = db.iter().unwrap().collect::<Vec<_>>();
    assert_eq!(
        records
            .iter()
            .map(|record| record.timestamp)
            .collect::<Vec<_>>(),
        vec![30, 40, 50]
    );
    assert!(records.iter().all(|record| record.status == TSL_WRITE));
    assert_eq!(db.oldest_sector_index(), Some(1));
    assert_eq!(db.current_sector_index(), Some(0));
    assert_eq!(db.last_timestamp(), Some(50));

    let flash = db.into_flash();
    let mut rebooted = TsDb::mount(flash, config).unwrap();
    let rebooted_records = rebooted.iter().unwrap().collect::<Vec<_>>();
    assert_eq!(
        rebooted_records
            .iter()
            .map(|record| record.timestamp)
            .collect::<Vec<_>>(),
        vec![30, 40, 50]
    );
    assert_eq!(rebooted.oldest_sector_index(), Some(1));
    assert_eq!(rebooted.current_sector_index(), Some(0));
    assert_eq!(rebooted.last_timestamp(), Some(50));
}

#[test]
fn tsdb_fixed_blob_mode_appends_iterates_and_reboots() {
    let config = fixed_config(false);
    let mut db = TsDb::mount(TestFlash::new(), config).unwrap();
    db.format().unwrap();

    let first = [0x11; 16];
    let second = [0x22; 16];
    let third = [0x33; 16];
    db.append(10, &first).unwrap();
    db.append(20, &second).unwrap();
    db.append(30, &third).unwrap();

    let records = db.iter().unwrap().collect::<Vec<_>>();
    assert_eq!(records.len(), 3);
    assert_eq!(records[0].timestamp, 10);
    assert_eq!(records[0].payload, first);
    assert_eq!(records[1].timestamp, 20);
    assert_eq!(records[1].payload, second);
    assert_eq!(records[2].timestamp, 30);
    assert_eq!(records[2].payload, third);

    let err = db.append(40, &[0x44; 15]).unwrap_err();
    assert!(matches!(err, Error::InvariantViolation(_)));

    let flash = db.into_flash();
    let mut rebooted = TsDb::mount(flash, config).unwrap();
    let rebooted_records = rebooted.iter_reverse().unwrap().collect::<Vec<_>>();
    assert_eq!(
        rebooted_records
            .iter()
            .map(|record| record.timestamp)
            .collect::<Vec<_>>(),
        vec![30, 20, 10]
    );
    assert_eq!(rebooted_records[0].payload, third);
    assert_eq!(rebooted_records[2].payload, first);
}
