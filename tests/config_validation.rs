use flashdb_for_rust::{
    BlobMode, KvConfig, MAX_KV_KEY_LEN, MAX_KV_VALUE_LEN, MAX_RUNTIME_WRITE_SIZE,
    MAX_TS_PAYLOAD_LEN, MAX_TS_SECTORS, StorageRegionConfig, TimestampPolicy, TsdbConfig,
};

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
        rollover: false,
    };

    assert!(config.validate().is_err());
}

#[test]
fn kv_config_rejects_no_alloc_caps_exceeded() {
    let config = KvConfig {
        region: region(),
        max_key_len: MAX_KV_KEY_LEN + 1,
        max_value_len: MAX_KV_VALUE_LEN,
    };

    assert!(config.validate().is_err());
}

#[test]
fn region_rejects_write_size_and_sector_count_beyond_bounded_runtime_caps() {
    let too_wide_write =
        StorageRegionConfig::new(0, 4096, 2048, (MAX_RUNTIME_WRITE_SIZE + 4) as u32);
    assert!(too_wide_write.validate().is_err());

    let too_many_sectors = StorageRegionConfig::new(0, ((MAX_TS_SECTORS as u32) + 1) * 256, 256, 4);
    assert!(too_many_sectors.validate().is_err());
}

#[test]
fn tsdb_fixed_blob_mode_rejects_no_alloc_payload_cap_exceeded() {
    let config = TsdbConfig {
        region: region(),
        blob_mode: BlobMode::Fixed(MAX_TS_PAYLOAD_LEN + 1),
        timestamp_policy: TimestampPolicy::StrictMonotonic,
        rollover: false,
    };

    assert!(config.validate().is_err());
}
