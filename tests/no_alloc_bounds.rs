use flashdb_for_rust::error::Error;
use flashdb_for_rust::kv::KvDb;
use flashdb_for_rust::storage::MockFlash;
use flashdb_for_rust::tsdb::TsDb;
use flashdb_for_rust::{
    BlobMode, KvConfig, MAX_KV_RECORDS, MAX_TS_PAYLOAD_LEN, MAX_TS_RECORDS, StorageRegionConfig,
    TimestampPolicy, TsdbConfig,
};

type LargeFlash = MockFlash<32768, 4, 256>;

#[test]
fn kv_rejects_new_live_key_once_bounded_snapshot_capacity_is_reached() {
    let config = KvConfig {
        region: StorageRegionConfig::new(0, 16384, 256, 4),
        max_key_len: 16,
        max_value_len: 8,
    };
    let mut db = KvDb::mount(LargeFlash::new(), config).unwrap();
    db.format().unwrap();

    for i in 0..MAX_KV_RECORDS {
        db.set(&format!("k{i:02}"), b"v").unwrap();
    }

    let err = db.set("overflow", b"v").unwrap_err();
    assert!(matches!(err, Error::InvariantViolation(_)));
}

#[test]
fn tsdb_rejects_variable_payload_larger_than_bounded_no_alloc_capacity() {
    let config = TsdbConfig {
        region: StorageRegionConfig::new(0, 2048, 1024, 4),
        blob_mode: BlobMode::Variable,
        timestamp_policy: TimestampPolicy::StrictMonotonic,
        rollover: false,
    };
    let mut db = TsDb::mount(MockFlash::<4096, 4, 1024>::new(), config).unwrap();
    db.format().unwrap();

    let payload = vec![0xAB; MAX_TS_PAYLOAD_LEN + 1];
    let err = db.append(1, &payload).unwrap_err();
    assert!(matches!(err, Error::InvariantViolation(_)));
}

#[test]
fn tsdb_rejects_append_once_bounded_snapshot_capacity_is_reached() {
    let config = TsdbConfig {
        region: StorageRegionConfig::new(0, 16384, 256, 4),
        blob_mode: BlobMode::Variable,
        timestamp_policy: TimestampPolicy::StrictMonotonic,
        rollover: false,
    };
    let mut db = TsDb::mount(LargeFlash::new(), config).unwrap();
    db.format().unwrap();

    for ts in 0..(MAX_TS_RECORDS as u64) {
        db.append(ts + 1, b"v").unwrap();
    }

    let err = db.append((MAX_TS_RECORDS as u64) + 1, b"v").unwrap_err();
    assert!(matches!(err, Error::InvariantViolation(_)));
}
