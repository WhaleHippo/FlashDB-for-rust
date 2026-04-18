use flashdb_for_rust::{storage::StorageRegion, StorageRegionConfig};

#[test]
fn rejects_invalid_region() {
    let config = StorageRegionConfig::new(1, 4096, 2048, 8);
    assert!(StorageRegion::new(config).is_err());
}

#[test]
fn computes_sector_geometry() {
    let config = StorageRegionConfig::new(0, 4096, 2048, 8);
    let region = StorageRegion::new(config).unwrap();
    assert_eq!(region.sector_count(), 2);
    assert_eq!(region.to_absolute(128).unwrap(), 128);
    assert_eq!(region.sector_start(1).unwrap(), 2048);
    assert_eq!(region.sector_index_of(2050).unwrap(), 1);
}
