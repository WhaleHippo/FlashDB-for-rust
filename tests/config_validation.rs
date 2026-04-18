use flashdb_for_embassy::{BlobMode, KvConfig, StorageRegionConfig, TimestampPolicy, TsdbConfig};

fn region() -> StorageRegionConfig {
    StorageRegionConfig::new(0, 4096, 2048, 8)
}

#[test]
fn kv_config_rejects_zero_limits() {
    let config = KvConfig {
        region: region(),
        max_key_len: 0,
        max_value_len: 16,
    };

    assert!(config.validate().is_err());
}

#[test]
fn tsdb_fixed_blob_mode_must_be_non_zero() {
    let config = TsdbConfig {
        region: region(),
        blob_mode: BlobMode::Fixed(0),
        timestamp_policy: TimestampPolicy::StrictMonotonic,
    };

    assert!(config.validate().is_err());
}
