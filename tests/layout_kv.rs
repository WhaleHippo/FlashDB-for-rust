use flashdb_for_rust::crc::crc_chain;
use flashdb_for_rust::layout::common::{DATA_UNUSED_SENTINEL, KV_RECORD_MAGIC, KV_SECTOR_MAGIC};
use flashdb_for_rust::layout::kv::{
    KvLayout, KvRecordHeader, KvSectorHeader, KV_PRE_WRITE, SECTOR_DIRTY_FALSE, SECTOR_STORE_EMPTY,
};

#[test]
fn kv_layout_matches_upstream_lengths_across_granularities() {
    let g1 = KvLayout::new(1).unwrap();
    let g8 = KvLayout::new(8).unwrap();
    let g32 = KvLayout::new(32).unwrap();
    let g64 = KvLayout::new(64).unwrap();

    assert_eq!(g1.sector_header_len().unwrap(), 16);
    assert_eq!(g8.sector_header_len().unwrap(), 20);
    assert_eq!(g32.sector_header_len().unwrap(), 36);
    assert_eq!(g64.sector_header_len().unwrap(), 64);

    assert_eq!(g1.record_header_len().unwrap(), 24);
    assert_eq!(g8.record_header_len().unwrap(), 28);
    assert_eq!(g32.record_header_len().unwrap(), 40);
    assert_eq!(g64.record_header_len().unwrap(), 64);
}

#[test]
fn kv_sector_header_roundtrip_preserves_status_tables_and_fields() {
    let layout = KvLayout::new(32).unwrap();
    let header = KvSectorHeader::new_empty();
    let mut buf = vec![0u8; layout.sector_header_len().unwrap()];
    header.encode(&layout, &mut buf).unwrap();

    assert_eq!(
        &buf[layout.sector_magic_offset().unwrap()..layout.sector_magic_offset().unwrap() + 4],
        &KV_SECTOR_MAGIC.to_le_bytes()
    );

    let decoded = KvSectorHeader::decode(&layout, &buf).unwrap();
    assert_eq!(decoded.store_status, SECTOR_STORE_EMPTY);
    assert_eq!(decoded.dirty_status, SECTOR_DIRTY_FALSE);
    assert_eq!(decoded.combined, DATA_UNUSED_SENTINEL);
    assert_eq!(decoded.reserved, DATA_UNUSED_SENTINEL);
}

#[test]
fn kv_record_layout_helpers_follow_upstream_padding_rules() {
    let layout = KvLayout::new(32).unwrap();
    let mut header = KvRecordHeader::new(5, 19);
    header.status = KV_PRE_WRITE;
    header.crc32 = 0x1122_3344;
    header.total_len = layout.record_total_len(&header).unwrap() as u32;

    let mut buf = vec![0u8; layout.record_header_len().unwrap()];
    header.encode(&layout, &mut buf).unwrap();
    assert_eq!(
        &buf[layout.record_magic_offset().unwrap()..layout.record_magic_offset().unwrap() + 4],
        &KV_RECORD_MAGIC.to_le_bytes()
    );

    let decoded = KvRecordHeader::decode(&layout, &buf).unwrap();
    assert_eq!(decoded.status, KV_PRE_WRITE);
    assert_eq!(decoded.total_len as usize, 68);
    assert_eq!(layout.value_offset(&decoded).unwrap(), 48);
}

#[test]
fn kv_crc_seed_matches_flashdb_compatible_prefix() {
    let layout = KvLayout::new(8).unwrap();
    let header = KvRecordHeader::finalized(&layout, 3, 7, 0).unwrap();
    let seed = layout.crc_seed_bytes(&header);

    assert_eq!(seed[0], 3);
    assert_eq!(&seed[1..4], &[0xFF; 3]);
    assert_eq!(&seed[4..8], &7u32.to_le_bytes());

    let digest = crc_chain(&[&seed, b"key", b"value!!"]);
    assert_ne!(digest, 0);
}

#[test]
fn kv_record_decode_rejects_too_small_total_len() {
    let layout = KvLayout::new(8).unwrap();
    let mut header = KvRecordHeader::new(4, 4);
    header.total_len = layout.record_header_len().unwrap() as u32;

    let mut buf = vec![0u8; layout.record_header_len().unwrap()];
    header.encode(&layout, &mut buf).unwrap();
    assert!(KvRecordHeader::decode(&layout, &buf).is_err());
}
