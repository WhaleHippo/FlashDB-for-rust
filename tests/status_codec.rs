use flashdb_for_embassy::layout::common::{
    KV_STATUS_COUNT, SECTOR_DIRTY_STATUS_COUNT, SECTOR_STORE_STATUS_COUNT, TSL_STATUS_COUNT,
};
use flashdb_for_embassy::layout::status::{StatusScheme, StatusTableBuf};

#[test]
fn roundtrips_bit_granularity_status_table() {
    let scheme = StatusScheme::new(KV_STATUS_COUNT, 1).unwrap();
    let mut buf = [0xFF; 1];
    scheme.encode(3, &mut buf).unwrap();
    assert_eq!(buf, [0x1F]);
    assert_eq!(scheme.decode(&buf).unwrap(), 3);
}

#[test]
fn roundtrips_byte_granularity_status_table() {
    let scheme = StatusScheme::new(SECTOR_STORE_STATUS_COUNT, 8).unwrap();
    let mut buf = [0xFF; 3];
    scheme.encode(2, &mut buf).unwrap();
    assert_eq!(buf, [0x00, 0x00, 0xFF]);
    assert_eq!(scheme.decode(&buf).unwrap(), 2);
}

#[test]
fn roundtrips_word_granularity_status_table() {
    let scheme = StatusScheme::new(SECTOR_DIRTY_STATUS_COUNT, 32).unwrap();
    let mut buf = [0xFF; 12];
    let span = scheme.encode_transition(3, &mut buf).unwrap();
    assert_eq!(span, Some((8, 4)));
    assert_eq!(&buf[0..4], &[0x00; 4]);
    assert_eq!(&buf[4..8], &[0x00; 4]);
    assert_eq!(&buf[8..12], &[0x00; 4]);
    assert_eq!(scheme.decode(&buf).unwrap(), 3);
}

#[test]
fn roundtrips_eight_byte_granularity_status_table() {
    let scheme = StatusScheme::new(SECTOR_STORE_STATUS_COUNT, 64).unwrap();
    let mut buf = [0xFF; 24];
    let span = scheme.encode_transition(2, &mut buf).unwrap();
    assert_eq!(span, Some((8, 8)));
    assert_eq!(&buf[0..8], &[0x00; 8]);
    assert_eq!(&buf[8..16], &[0x00; 8]);
    assert_eq!(&buf[16..24], &[0xFF; 8]);
    assert_eq!(scheme.decode(&buf).unwrap(), 2);
}

#[test]
fn borrowed_status_table_buf_wraps_scheme() {
    let scheme = StatusScheme::new(TSL_STATUS_COUNT, 8).unwrap();
    let mut backing = [0xFF; 5];
    let mut table = StatusTableBuf::new(scheme, &mut backing).unwrap();
    table.encode(1).unwrap();
    assert_eq!(table.as_bytes(), &[0x00, 0xFF, 0xFF, 0xFF, 0xFF]);
    assert_eq!(table.decode().unwrap(), 1);
}

#[test]
fn rejects_partial_programming() {
    let scheme = StatusScheme::new(SECTOR_DIRTY_STATUS_COUNT, 32).unwrap();
    let mut buf = [0xFF; 12];
    buf[0] = 0x00;
    assert!(scheme.decode(&buf).is_err());
}
