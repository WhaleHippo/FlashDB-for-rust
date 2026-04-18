use flashdb_for_embassy::{
    storage::{MockFlash, NorFlashRegion, StorageRegion},
    StorageRegionConfig,
};

#[test]
fn rejects_region_geometry_that_disagrees_with_backend() {
    let flash = MockFlash::<4096, 8, 2048>::new();
    let region = StorageRegion::new(StorageRegionConfig::new(0, 4096, 2048, 4)).unwrap();
    assert!(NorFlashRegion::new(flash, region).is_err());
}
