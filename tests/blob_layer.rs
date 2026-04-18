use flashdb_for_embassy::blob::{BlobBuf, BlobLocator, BlobRef};

#[test]
fn blob_ref_reports_locator_and_payload_shape() {
    let locator = BlobLocator::new(128, 5);
    let payload = *b"hello";
    let blob = BlobRef::new(locator, &payload);

    assert_eq!(blob.locator(), locator);
    assert_eq!(blob.len(), 5);
    assert!(!blob.is_empty());
    assert_eq!(blob.as_bytes(), b"hello");
}

#[test]
fn blob_buf_exposes_mutable_slice_metadata() {
    let locator = BlobLocator::new(256, 4);
    let mut storage = *b"rust";
    let mut blob = BlobBuf::new(locator, &mut storage);

    assert_eq!(blob.locator(), locator);
    assert_eq!(blob.len(), 4);
    assert!(!blob.is_empty());
    blob.as_mut_bytes()[0] = b'R';
    assert_eq!(blob.as_bytes(), b"Rust");
}
