use flashdb_for_rust::{
    storage::{MockFlash, NorFlashRegion, StorageRegion},
    StorageRegionConfig,
};

#[test]
fn aligned_write_passes_through_full_chunks() {
    let flash = MockFlash::<4096, 8, 2048>::new();
    let region = StorageRegion::new(StorageRegionConfig::new(0, 4096, 2048, 8)).unwrap();
    let mut storage = NorFlashRegion::new(flash, region).unwrap();
    let mut scratch = [0u8; 8];

    storage
        .write_aligned(16, &[1, 2, 3, 4, 5, 6, 7, 8], &mut scratch)
        .unwrap();

    let flash = storage.into_inner();
    assert_eq!(&flash.as_slice()[16..24], &[1, 2, 3, 4, 5, 6, 7, 8]);
}

#[test]
fn aligned_write_pads_tail_with_erased_bytes() {
    let flash = MockFlash::<4096, 8, 2048>::new();
    let region = StorageRegion::new(StorageRegionConfig::new(0, 4096, 2048, 8)).unwrap();
    let mut storage = NorFlashRegion::new(flash, region).unwrap();
    let mut scratch = [0u8; 8];

    storage
        .write_aligned(24, &[0x11, 0x22, 0x33], &mut scratch)
        .unwrap();

    let flash = storage.into_inner();
    assert_eq!(
        &flash.as_slice()[24..32],
        &[0x11, 0x22, 0x33, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]
    );
}

#[test]
fn aligned_write_preserves_nor_semantics() {
    let flash = MockFlash::<4096, 8, 2048>::new();
    let region = StorageRegion::new(StorageRegionConfig::new(0, 4096, 2048, 8)).unwrap();
    let mut storage = NorFlashRegion::new(flash, region).unwrap();
    let mut scratch = [0u8; 8];

    storage
        .write_aligned(0, &[0x00, 0x00, 0x00], &mut scratch)
        .unwrap();
    assert!(storage
        .write_aligned(0, &[0xFF, 0x00, 0x00], &mut scratch)
        .is_err());
}

#[test]
fn aligned_write_rejects_unaligned_offsets() {
    let flash = MockFlash::<4096, 8, 2048>::new();
    let region = StorageRegion::new(StorageRegionConfig::new(0, 4096, 2048, 8)).unwrap();
    let mut storage = NorFlashRegion::new(flash, region).unwrap();
    let mut scratch = [0u8; 8];

    assert!(storage
        .write_aligned(3, &[0x11, 0x22], &mut scratch)
        .is_err());
}
