use flashdb_for_rust::error::Error;
use flashdb_for_rust::storage::MockFlash;
use flashdb_for_rust::tsdb::TsDb;
use flashdb_for_rust::{BlobMode, StorageRegionConfig, TimestampPolicy, TsdbConfig};

type TestFlash = MockFlash<2048, 4, 256>;

fn test_config() -> TsdbConfig {
    TsdbConfig {
        region: StorageRegionConfig::new(0, 512, 256, 4),
        blob_mode: BlobMode::Variable,
        timestamp_policy: TimestampPolicy::StrictMonotonic,
    }
}

#[test]
fn tsdb_variable_append_iterates_in_timestamp_order_across_reboot() {
    let config = test_config();
    let mut db = TsDb::mount(TestFlash::new(), config).unwrap();
    db.format().unwrap();

    for (ts, payload) in [
        (10_u64, b"one".as_slice()),
        (20, b"two".as_slice()),
        (30, b"three".as_slice()),
        (40, b"four".as_slice()),
        (50, b"five".as_slice()),
    ] {
        db.append(ts, payload).unwrap();
    }

    assert_eq!(db.oldest_sector_index(), Some(0));
    assert_eq!(db.current_sector_index(), Some(1));
    assert_eq!(db.last_timestamp(), Some(50));

    let records = db.iter().unwrap().collect::<Vec<_>>();
    assert_eq!(records.len(), 5);
    assert_eq!(records[0].timestamp, 10);
    assert_eq!(records[0].payload, b"one");
    assert_eq!(records[4].timestamp, 50);
    assert_eq!(records[4].payload, b"five");

    let flash = db.into_flash();
    let mut rebooted = TsDb::mount(flash, config).unwrap();
    let records = rebooted.iter().unwrap().collect::<Vec<_>>();
    assert_eq!(records.len(), 5);
    assert_eq!(records[1].timestamp, 20);
    assert_eq!(records[1].payload, b"two");
    assert_eq!(records[3].timestamp, 40);
    assert_eq!(records[3].payload, b"four");
    assert_eq!(rebooted.oldest_sector_index(), Some(0));
    assert_eq!(rebooted.current_sector_index(), Some(1));
    assert_eq!(rebooted.last_timestamp(), Some(50));
}

#[test]
fn tsdb_strict_monotonic_policy_rejects_equal_or_older_timestamps() {
    let mut db = TsDb::mount(TestFlash::new(), test_config()).unwrap();
    db.format().unwrap();

    db.append(100, b"first").unwrap();

    let equal_err = db.append(100, b"equal").unwrap_err();
    assert!(matches!(
        equal_err,
        Error::TimestampNotMonotonic {
            last: 100,
            next: 100
        }
    ));

    let older_err = db.append(99, b"older").unwrap_err();
    assert!(matches!(
        older_err,
        Error::TimestampNotMonotonic {
            last: 100,
            next: 99
        }
    ));
}
