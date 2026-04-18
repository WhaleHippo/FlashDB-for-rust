use flashdb_for_rust::layout::common::{DATA_UNUSED_SENTINEL, TS_SECTOR_MAGIC};
use flashdb_for_rust::layout::ts::{
    TsBlobMode, TsEndInfo, TsIndexHeader, TsLayout, TsSectorHeader, TSL_PRE_WRITE,
};

#[test]
fn ts_layout_matches_upstream_lengths_for_common_modes() {
    let layout8 = TsLayout::new(8, 4).unwrap();
    let layout32 = TsLayout::new(32, 4).unwrap();
    let layout64 = TsLayout::new(64, 4).unwrap();

    assert_eq!(layout8.sector_header_len().unwrap(), 48);
    assert_eq!(layout32.sector_header_len().unwrap(), 80);
    assert_eq!(layout64.sector_header_len().unwrap(), 160);

    assert_eq!(layout8.index_header_len(TsBlobMode::Variable).unwrap(), 20);
    assert_eq!(layout8.index_header_len(TsBlobMode::Fixed(32)).unwrap(), 12);
    assert_eq!(layout32.index_header_len(TsBlobMode::Variable).unwrap(), 32);
    assert_eq!(
        layout32.index_header_len(TsBlobMode::Fixed(32)).unwrap(),
        24
    );
}

#[test]
fn ts_sector_header_roundtrip_preserves_end_info() {
    let layout = TsLayout::new(32, 4).unwrap();
    let mut header = TsSectorHeader::new_empty();
    header.start_time = 123;
    header.end_info = [
        TsEndInfo {
            timestamp: 456,
            index: 0x100,
            status: TSL_PRE_WRITE,
        },
        TsEndInfo {
            timestamp: DATA_UNUSED_SENTINEL as u64,
            index: DATA_UNUSED_SENTINEL,
            status: 0,
        },
    ];

    let mut buf = vec![0u8; layout.sector_header_len().unwrap()];
    header.encode(&layout, &mut buf).unwrap();
    assert_eq!(
        &buf[layout.sector_magic_offset().unwrap()..layout.sector_magic_offset().unwrap() + 4],
        &TS_SECTOR_MAGIC.to_le_bytes()
    );

    let decoded = TsSectorHeader::decode(&layout, &buf).unwrap();
    assert_eq!(decoded.start_time, 123);
    assert_eq!(decoded.end_info[0].timestamp, 456);
    assert_eq!(decoded.end_info[0].index, 0x100);
    assert_eq!(decoded.end_info[0].status, TSL_PRE_WRITE);
}

#[test]
fn ts_index_roundtrip_supports_variable_and_fixed_blob_modes() {
    let layout = TsLayout::new(32, 4).unwrap();

    let variable = TsIndexHeader::variable(123, 512, 32);
    let mut variable_buf = vec![0u8; layout.index_header_len(TsBlobMode::Variable).unwrap()];
    variable
        .encode(&layout, TsBlobMode::Variable, &mut variable_buf)
        .unwrap();
    let decoded_variable =
        TsIndexHeader::decode(&layout, TsBlobMode::Variable, &variable_buf).unwrap();
    assert_eq!(decoded_variable.timestamp, 123);
    assert_eq!(decoded_variable.log_addr, Some(512));
    assert_eq!(decoded_variable.log_len, Some(32));

    let fixed = TsIndexHeader::new(456);
    let mut fixed_buf = vec![0u8; layout.index_header_len(TsBlobMode::Fixed(32)).unwrap()];
    fixed
        .encode(&layout, TsBlobMode::Fixed(32), &mut fixed_buf)
        .unwrap();
    let decoded_fixed = TsIndexHeader::decode(&layout, TsBlobMode::Fixed(32), &fixed_buf).unwrap();
    assert_eq!(decoded_fixed.timestamp, 456);
    assert_eq!(decoded_fixed.log_addr, None);
    assert_eq!(decoded_fixed.log_len, None);
}

#[test]
fn ts_capacity_helpers_match_dual_ended_layout() {
    let layout = TsLayout::new(32, 4).unwrap();
    assert_eq!(
        layout
            .fixed_entry_capacity(4096, TsBlobMode::Fixed(32))
            .unwrap(),
        71
    );
    assert_eq!(
        layout
            .fixed_blob_data_offset(4096, TsBlobMode::Fixed(32), 2)
            .unwrap(),
        4000
    );
    assert_eq!(
        layout
            .sector_remaining(4096, TsBlobMode::Variable, 3, 128)
            .unwrap(),
        3792
    );
}

#[test]
fn ts_layout_rejects_invalid_time_width() {
    assert!(TsLayout::new(32, 6).is_err());
}
