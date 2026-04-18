use flashdb_for_embassy::layout::status::StatusScheme;

#[test]
fn roundtrips_monotonic_status_table() {
    let scheme = StatusScheme::new(4, 1).unwrap();
    let mut buf = [0xFF; 4];
    scheme.encode(2, &mut buf).unwrap();
    assert_eq!(buf, [0x00, 0x00, 0xFF, 0xFF]);
    assert_eq!(scheme.decode(&buf).unwrap(), 2);
}

#[test]
fn rejects_partial_programming() {
    let scheme = StatusScheme::new(3, 2).unwrap();
    let buf = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
    assert!(scheme.decode(&buf).is_err());
}
