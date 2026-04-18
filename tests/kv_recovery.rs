use flashdb_for_rust::crc::crc_chain;
use flashdb_for_rust::kv::KvDb;
use flashdb_for_rust::layout::kv::{KV_PRE_WRITE, KV_WRITE, KvLayout, KvRecordHeader};
use flashdb_for_rust::storage::{MockFlash, NorFlashRegion, StorageRegion};
use flashdb_for_rust::{KvConfig, StorageRegionConfig};

type TestFlash = MockFlash<2048, 4, 256>;

fn test_config() -> KvConfig {
    KvConfig {
        region: StorageRegionConfig::new(0, 1024, 256, 4),
        max_key_len: 32,
        max_value_len: 128,
    }
}

fn layout() -> KvLayout {
    KvLayout::new(32).unwrap()
}

fn append_raw_record(
    flash: TestFlash,
    config: KvConfig,
    cursor: u32,
    status: usize,
    key: &[u8],
    value: Option<&[u8]>,
    crc32: u32,
) -> TestFlash {
    let region = StorageRegion::new(config.region).unwrap();
    let mut storage = NorFlashRegion::new(flash, region).unwrap();
    let layout = layout();
    let value = value.unwrap_or_default();

    let mut header =
        KvRecordHeader::finalized(&layout, key.len() as u8, value.len() as u32, crc32).unwrap();
    header.status = status;

    let mut header_buf = vec![0xFF; layout.record_header_len().unwrap()];
    header.encode(&layout, &mut header_buf).unwrap();
    storage.write(cursor, &header_buf).unwrap();

    if status != KV_PRE_WRITE {
        let key_len = layout.aligned_key_len(header.key_len).unwrap();
        let value_len = layout.aligned_value_len(header.value_len).unwrap();
        let mut key_buf = vec![0xFF; key_len];
        key_buf[..key.len()].copy_from_slice(key);
        storage
            .write(cursor + header_buf.len() as u32, &key_buf)
            .unwrap();

        let mut value_buf = vec![0xFF; value_len];
        value_buf[..value.len()].copy_from_slice(value);
        storage
            .write(
                cursor + layout.value_offset(&header).unwrap() as u32,
                &value_buf,
            )
            .unwrap();
    }

    storage.into_inner()
}

#[test]
fn mount_discards_pre_write_tail_and_keeps_last_good_value() {
    let config = test_config();
    let mut db = KvDb::mount(TestFlash::new(), config).unwrap();
    db.format().unwrap();
    db.set("key", b"stable").unwrap();

    let cursor = db.write_cursor();
    let flash = db.into_flash();
    let flash = append_raw_record(
        flash,
        config,
        cursor,
        KV_PRE_WRITE,
        b"key",
        Some(b"broken"),
        0,
    );

    let mut rebooted = KvDb::mount(flash, config).unwrap();

    let mut buf = [0u8; 32];
    let len = rebooted.get_blob_into("key", &mut buf).unwrap().unwrap();
    assert_eq!(&buf[..len], b"stable");

    rebooted.set("fresh", b"after-recovery").unwrap();
    let len = rebooted.get_blob_into("fresh", &mut buf).unwrap().unwrap();
    assert_eq!(&buf[..len], b"after-recovery");
}

#[test]
fn mount_discards_crc_mismatched_tail_and_preserves_previous_record() {
    let config = test_config();
    let mut db = KvDb::mount(TestFlash::new(), config).unwrap();
    db.format().unwrap();
    db.set("answer", b"good").unwrap();

    let cursor = db.write_cursor();
    let flash = db.into_flash();

    let wrong_crc = crc_chain(&[b"definitely", b"wrong"]);
    let flash = append_raw_record(
        flash,
        config,
        cursor,
        KV_WRITE,
        b"answer",
        Some(b"bad-tail"),
        wrong_crc,
    );

    let mut rebooted = KvDb::mount(flash, config).unwrap();

    let mut buf = [0u8; 32];
    let len = rebooted.get_blob_into("answer", &mut buf).unwrap().unwrap();
    assert_eq!(&buf[..len], b"good");

    rebooted.set("fresh", b"after-crc-recovery").unwrap();
    let len = rebooted.get_blob_into("fresh", &mut buf).unwrap().unwrap();
    assert_eq!(&buf[..len], b"after-crc-recovery");
}
