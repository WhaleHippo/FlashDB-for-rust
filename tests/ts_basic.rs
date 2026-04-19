use flashdb_for_rust::error::Error;
use flashdb_for_rust::layout::ts::{TSL_DELETED, TSL_USER_STATUS1, TSL_WRITE};
use flashdb_for_rust::storage::MockFlash;
use flashdb_for_rust::tsdb::TsDb;
use flashdb_for_rust::{BlobMode, StorageRegionConfig, TimestampPolicy, TsdbConfig};

type TestFlash = MockFlash<2048, 4, 256>;

fn test_config() -> TsdbConfig {
    TsdbConfig {
        region: StorageRegionConfig::new(0, 512, 256, 4),
        blob_mode: BlobMode::Variable,
        timestamp_policy: TimestampPolicy::StrictMonotonic,
        rollover: false,
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
    assert_eq!(records[0].payload.as_slice(), b"one");
    assert_eq!(records[4].timestamp, 50);
    assert_eq!(records[4].payload.as_slice(), b"five");

    let flash = db.into_flash();
    let mut rebooted = TsDb::mount(flash, config).unwrap();
    let records = rebooted.iter().unwrap().collect::<Vec<_>>();
    assert_eq!(records.len(), 5);
    assert_eq!(records[1].timestamp, 20);
    assert_eq!(records[1].payload.as_slice(), b"two");
    assert_eq!(records[3].timestamp, 40);
    assert_eq!(records[3].payload.as_slice(), b"four");
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

#[test]
fn tsdb_reverse_iteration_returns_latest_first_across_sectors() {
    let mut db = TsDb::mount(TestFlash::new(), test_config()).unwrap();
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

    let reverse = db.iter_reverse().unwrap().collect::<Vec<_>>();
    let timestamps = reverse
        .iter()
        .map(|record| record.timestamp)
        .collect::<Vec<_>>();
    let payloads = reverse
        .iter()
        .map(|record| record.payload.clone())
        .collect::<Vec<_>>();

    assert_eq!(timestamps, vec![50, 40, 30, 20, 10]);
    assert_eq!(payloads[0].as_slice(), b"five");
    assert_eq!(payloads[4].as_slice(), b"one");
}

#[test]
fn tsdb_iter_by_time_and_query_count_follow_inclusive_bounds() {
    let mut db = TsDb::mount(TestFlash::new(), test_config()).unwrap();
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

    let forward = db.iter_by_time(20, 40).unwrap().collect::<Vec<_>>();
    assert_eq!(
        forward
            .iter()
            .map(|record| record.timestamp)
            .collect::<Vec<_>>(),
        vec![20, 30, 40]
    );
    assert_eq!(db.query_count(20, 40, TSL_WRITE).unwrap(), 3);
    assert_eq!(db.query_count(20, 40, TSL_DELETED).unwrap(), 0);

    let reverse = db.iter_by_time(40, 20).unwrap().collect::<Vec<_>>();
    assert_eq!(
        reverse
            .iter()
            .map(|record| record.timestamp)
            .collect::<Vec<_>>(),
        vec![40, 30, 20]
    );
}

#[test]
fn tsdb_set_status_updates_query_results_and_reverse_view() {
    let mut db = TsDb::mount(TestFlash::new(), test_config()).unwrap();
    db.format().unwrap();

    for (ts, payload) in [
        (10_u64, b"one".as_slice()),
        (20, b"two".as_slice()),
        (30, b"three".as_slice()),
    ] {
        db.append(ts, payload).unwrap();
    }

    assert!(db.set_status(20, TSL_USER_STATUS1).unwrap());
    assert!(!db.set_status(99, TSL_USER_STATUS1).unwrap());

    let range = db.iter_by_time(10, 30).unwrap().collect::<Vec<_>>();
    assert_eq!(range[0].status, TSL_WRITE);
    assert_eq!(range[1].status, TSL_USER_STATUS1);
    assert_eq!(range[2].status, TSL_WRITE);
    assert_eq!(db.query_count(10, 30, TSL_USER_STATUS1).unwrap(), 1);
    assert_eq!(db.query_count(10, 30, TSL_WRITE).unwrap(), 2);

    let reverse = db.iter_reverse().unwrap().collect::<Vec<_>>();
    assert_eq!(reverse[1].timestamp, 20);
    assert_eq!(reverse[1].status, TSL_USER_STATUS1);
}

#[test]
fn tsdb_clean_resets_all_records_and_allows_reuse_after_reboot() {
    let config = test_config();
    let mut db = TsDb::mount(TestFlash::new(), config).unwrap();
    db.format().unwrap();

    db.append(10, b"one").unwrap();
    db.append(20, b"two").unwrap();
    db.set_status(20, TSL_USER_STATUS1).unwrap();

    db.clean().unwrap();
    assert_eq!(db.iter().unwrap().count(), 0);
    assert_eq!(db.iter_reverse().unwrap().count(), 0);
    assert_eq!(db.query_count(0, 100, TSL_WRITE).unwrap(), 0);
    assert_eq!(db.oldest_sector_index(), None);
    assert_eq!(db.current_sector_index(), Some(0));
    assert_eq!(db.last_timestamp(), None);

    db.append(30, b"three").unwrap();
    let flash = db.into_flash();
    let mut rebooted = TsDb::mount(flash, config).unwrap();
    let records = rebooted.iter().unwrap().collect::<Vec<_>>();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].timestamp, 30);
    assert_eq!(records[0].payload.as_slice(), b"three");
    assert_eq!(records[0].status, TSL_WRITE);
}
