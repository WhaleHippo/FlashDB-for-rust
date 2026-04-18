use flashdb_for_rust::layout::common::{
    DATA_UNUSED_SENTINEL, ERASED_BYTE, FAILED_ADDR, KV_RECORD_MAGIC, KV_SECTOR_MAGIC,
    SECTOR_DIRTY_STATUS_COUNT, SECTOR_STORE_STATUS_COUNT, TSL_STATUS_COUNT, TS_SECTOR_MAGIC,
    WRITTEN_BYTE,
};

#[test]
fn common_constants_match_upstream_flashdb_values() {
    assert_eq!(ERASED_BYTE, 0xFF);
    assert_eq!(WRITTEN_BYTE, 0x00);
    assert_eq!(DATA_UNUSED_SENTINEL, 0xFFFF_FFFF);
    assert_eq!(FAILED_ADDR, 0xFFFF_FFFF);
    assert_eq!(KV_SECTOR_MAGIC, 0x3042_4446);
    assert_eq!(KV_RECORD_MAGIC, 0x3030_564B);
    assert_eq!(TS_SECTOR_MAGIC, 0x304C_5354);
}

#[test]
fn status_count_constants_match_plan_expectations() {
    assert_eq!(SECTOR_STORE_STATUS_COUNT, 4);
    assert_eq!(SECTOR_DIRTY_STATUS_COUNT, 4);
    assert_eq!(TSL_STATUS_COUNT, 6);
}
