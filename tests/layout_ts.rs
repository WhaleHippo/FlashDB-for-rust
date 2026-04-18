use flashdb_for_embassy::layout::common::{FORMAT_VERSION, TS_SECTOR_MAGIC};
use flashdb_for_embassy::layout::ts::{
    fixed_index_span, sector_remaining, TsIndexHeader, TsSectorHeader,
};

#[test]
fn ts_sector_header_roundtrip() {
    let header = TsSectorHeader::new(2);
    let mut buf = [0u8; 16];
    header.encode(&mut buf).unwrap();
    assert_eq!(&buf[..4], &TS_SECTOR_MAGIC.to_le_bytes());
    assert_eq!(
        TsSectorHeader::decode(&buf).unwrap().format_version,
        FORMAT_VERSION
    );
}

#[test]
fn ts_index_and_capacity_helpers_work() {
    let header = TsIndexHeader::new(123, 512, 32);
    let mut buf = [0u8; 20];
    header.encode(&mut buf).unwrap();
    let decoded = TsIndexHeader::decode(&buf).unwrap();
    assert_eq!(decoded.data_offset, 512);
    assert_eq!(fixed_index_span(3), 60);
    assert_eq!(sector_remaining(4096, 16, 60, 128), 3892);
}
