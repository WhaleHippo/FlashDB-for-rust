use flashdb_for_rust::{
    storage::{MockFlash, NorFlashRegion, StorageRegion},
    StorageRegionConfig,
};

#[test]
fn region_helpers_map_logical_offsets_to_absolute_addresses() {
    let region = StorageRegion::new(StorageRegionConfig::new(2048, 4096, 2048, 8)).unwrap();
    assert!(region.contains(0, 0));
    assert!(region.contains(128, 64));
    assert!(!region.contains(4096, 1));
    assert_eq!(region.to_absolute(64).unwrap(), 2112);
    assert_eq!(region.sector_start(1).unwrap(), 4096);
    assert_eq!(region.sector_index_of(2050).unwrap(), 1);
}

#[test]
fn nor_flash_region_reads_and_writes_region_relative_offsets() {
    let flash = MockFlash::<8192, 8, 2048>::new();
    let region = StorageRegion::new(StorageRegionConfig::new(2048, 4096, 2048, 8)).unwrap();
    let mut storage = NorFlashRegion::new(flash, region).unwrap();

    storage.write(8, &[0xAA; 8]).unwrap();
    let mut readback = [0u8; 8];
    storage.read(8, &mut readback).unwrap();
    assert_eq!(readback, [0xAA; 8]);

    let flash = storage.into_inner();
    assert_eq!(&flash.as_slice()[2056..2064], &[0xAA; 8]);
}
