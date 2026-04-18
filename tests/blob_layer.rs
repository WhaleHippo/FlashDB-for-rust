use flashdb_for_embassy::blob::{
    BlobBuf, BlobLocator, BlobReader, BlobRef, KvValueLocator, TsPayloadLocator,
};
use flashdb_for_embassy::config::StorageRegionConfig;
use flashdb_for_embassy::error::Error;
use flashdb_for_embassy::storage::{MockFlash, NorFlashRegion, StorageRegion};

fn region() -> StorageRegion {
    StorageRegion::new(StorageRegionConfig::new(0, 4096, 1024, 4)).unwrap()
}

#[test]
fn blob_ref_and_blob_buf_are_borrowed_slices() {
    let payload = *b"hello";
    let blob = BlobRef::new(&payload);

    assert_eq!(blob.len(), 5);
    assert!(!blob.is_empty());
    assert_eq!(blob.as_bytes(), b"hello");

    let mut bytes = *b"rust";
    let mut buf = BlobBuf::new(&mut bytes);
    buf.as_mut_bytes()[0] = b'R';

    assert_eq!(buf.len(), 4);
    assert_eq!(buf.as_bytes(), b"Rust");
}

#[test]
fn locators_validate_region_and_preserve_type_information() {
    let region = region();
    let locator = BlobLocator::new(&region, 64, 128, 12).unwrap();
    let kv = KvValueLocator::new(&region, 64, 128, 12).unwrap();
    let ts = TsPayloadLocator::new(&region, 96, 256, 8).unwrap();

    assert_eq!(locator.meta_offset(), 64);
    assert_eq!(locator.data_offset(), 128);
    assert_eq!(locator.len(), 12);
    assert_eq!(locator.end_offset().unwrap(), 140);
    assert_eq!(BlobLocator::from(kv).data_offset(), 128);
    assert_eq!(BlobLocator::from(ts).meta_offset(), 96);
    assert!(BlobLocator::new(&region, 4096, 0, 1).is_err());
    assert!(BlobLocator::new(&region, 32, 4094, 4).is_err());
}

#[test]
fn blob_reader_supports_exact_truncated_chunk_and_cursor_reads() {
    let flash = MockFlash::<4096, 4, 1024>::new();
    let region = region();
    let mut storage = NorFlashRegion::new(flash, region).unwrap();
    let payload = b"hello rust!!";
    storage.write(128, payload).unwrap();

    let locator = BlobLocator::new(storage.region(), 64, 128, payload.len() as u32).unwrap();
    let mut reader = BlobReader::new(storage);

    let mut truncated = [0u8; 5];
    let read = reader.read_truncated(locator, &mut truncated).unwrap();
    assert_eq!(read, 5);
    assert_eq!(&truncated, b"hello");

    let mut chunk = [0u8; 4];
    let read = reader.read_chunk(locator, 6, &mut chunk).unwrap();
    assert_eq!(read, 4);
    assert_eq!(&chunk, b"rust");

    let mut exact = [0u8; 12];
    reader.read_exact(locator, &mut exact).unwrap();
    assert_eq!(&exact, payload);

    let mut cursor = reader.cursor(locator);
    let mut first = [0u8; 6];
    let mut second = [0u8; 6];
    assert_eq!(cursor.read_next(&mut first).unwrap(), 6);
    assert_eq!(cursor.read_next(&mut second).unwrap(), 6);
    assert_eq!(cursor.read_next(&mut second).unwrap(), 0);
    assert_eq!(&first, b"hello ");
    assert_eq!(&second, b"rust!!");
}

#[test]
fn blob_reader_reports_small_buffers_and_invalid_offsets() {
    let flash = MockFlash::<4096, 4, 1024>::new();
    let region = region();
    let mut storage = NorFlashRegion::new(flash, region).unwrap();
    storage.write(256, b"abcd").unwrap();

    let locator = BlobLocator::new(storage.region(), 32, 256, 4).unwrap();
    let mut reader = BlobReader::new(storage);

    let mut tiny = [0u8; 3];
    assert_eq!(
        reader.read_exact(locator, &mut tiny),
        Err(Error::BufferTooSmall {
            needed: 4,
            actual: 3,
        })
    );

    let mut buf = [0u8; 2];
    assert_eq!(
        reader.read_chunk(locator, 5, &mut buf),
        Err(Error::InvalidBlobOffset { offset: 5, len: 4 })
    );
}
