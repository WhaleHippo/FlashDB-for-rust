use flashdb_for_embassy::layout::common::{FORMAT_VERSION, KV_RECORD_MAGIC, KV_SECTOR_MAGIC};
use flashdb_for_embassy::layout::kv::{KvRecordHeader, KvSectorHeader};

#[test]
fn kv_sector_header_roundtrip() {
    let header = KvSectorHeader::new(3);
    let mut buf = [0u8; 12];
    header.encode(&mut buf).unwrap();
    assert_eq!(&buf[..4], &KV_SECTOR_MAGIC.to_le_bytes());
    assert_eq!(
        KvSectorHeader::decode(&buf).unwrap().format_version,
        FORMAT_VERSION
    );
}

#[test]
fn kv_record_layout_helpers_work() {
    let header = KvRecordHeader::new(5, 19);
    let mut buf = [0u8; 20];
    header.encode(&mut buf).unwrap();
    assert_eq!(&buf[..4], &KV_RECORD_MAGIC.to_le_bytes());
    let decoded = KvRecordHeader::decode(&buf).unwrap();
    assert_eq!(decoded.value_offset(8).unwrap(), 32);
    assert_eq!(decoded.total_len(8).unwrap(), 48);
}
