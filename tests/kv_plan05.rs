use flashdb_for_rust::kv::KvDb;
use flashdb_for_rust::layout::kv::{
    KV_PRE_DELETE, KvLayout, SECTOR_DIRTY_FALSE, SECTOR_DIRTY_TRUE, SECTOR_STORE_EMPTY,
    SECTOR_STORE_USING,
};
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

fn gc_config() -> KvConfig {
    KvConfig {
        region: StorageRegionConfig::new(0, 1024, 256, 4),
        max_key_len: 32,
        max_value_len: 64,
    }
}

fn layout() -> KvLayout {
    KvLayout::new(32).unwrap()
}

fn mark_record_pre_delete(mut flash: TestFlash, config: KvConfig, record_offset: u32) -> TestFlash {
    let region = StorageRegion::new(config.region).unwrap();
    let mut storage = NorFlashRegion::new(flash, region).unwrap();
    let layout = layout();
    let mut scratch = [0xFF; 4];
    layout
        .kv_status_scheme()
        .write_transition(&mut storage, record_offset, KV_PRE_DELETE, &mut scratch)
        .unwrap();
    flash = storage.into_inner();
    flash
}

#[test]
fn sector_metadata_tracks_store_dirty_and_remaining_bytes() {
    let config = test_config();
    let mut db = KvDb::mount(TestFlash::new(), config).unwrap();
    db.format().unwrap();

    let empty = db.sector_meta(0).unwrap();
    assert_eq!(empty.store_status, SECTOR_STORE_EMPTY);
    assert_eq!(empty.dirty_status, SECTOR_DIRTY_FALSE);
    assert_eq!(
        empty.next_record_offset,
        layout().sector_header_len().unwrap() as u32
    );

    db.set("mode", b"v1").unwrap();
    let after_first_write = db.sector_meta(0).unwrap();
    assert_eq!(after_first_write.store_status, SECTOR_STORE_USING);
    assert_eq!(after_first_write.dirty_status, SECTOR_DIRTY_FALSE);
    assert_eq!(after_first_write.next_record_offset, db.write_cursor());

    db.set("mode", b"v2").unwrap();
    let after_overwrite = db.sector_meta(0).unwrap();
    assert_eq!(after_overwrite.store_status, SECTOR_STORE_USING);
    assert_eq!(after_overwrite.dirty_status, SECTOR_DIRTY_TRUE);
    assert!(after_overwrite.remaining_bytes < after_first_write.remaining_bytes);
}

#[test]
fn mount_treats_pre_delete_record_as_live_during_recovery() {
    let config = test_config();
    let mut db = KvDb::mount(TestFlash::new(), config).unwrap();
    db.format().unwrap();
    db.set("mode", b"stable").unwrap();

    let record_offset = layout().sector_header_len().unwrap() as u32;
    let flash = mark_record_pre_delete(db.into_flash(), config, record_offset);
    let mut rebooted = KvDb::mount(flash, config).unwrap();

    let mut value_buf = [0u8; 32];
    let len = rebooted
        .get_blob_into("mode", &mut value_buf)
        .unwrap()
        .expect("PRE_DELETE record should still be recoverable");
    assert_eq!(&value_buf[..len], b"stable");
}

#[test]
fn live_records_traversal_hides_stale_and_deleted_entries() {
    let config = test_config();
    let mut db = KvDb::mount(TestFlash::new(), config).unwrap();
    db.format().unwrap();

    db.set("alpha", b"one").unwrap();
    db.set("beta", b"two").unwrap();
    db.set("alpha", b"three").unwrap();
    db.delete("beta").unwrap();
    db.set("gamma", b"four").unwrap();

    let mut key_buf = [0u8; 32];
    let mut value_buf = [0u8; 32];
    let mut seen = Vec::new();
    db.for_each_live_record(&mut key_buf, &mut value_buf, |key, value| {
        seen.push((key.to_owned(), value.to_vec()));
    })
    .unwrap();

    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].0, "alpha");
    assert_eq!(seen[0].1, b"three");
    assert_eq!(seen[1].0, "gamma");
    assert_eq!(seen[1].1, b"four");
}

#[test]
fn integrity_check_reports_corrupted_record_headers() {
    let config = test_config();
    let mut db = KvDb::mount(TestFlash::new(), config).unwrap();
    db.format().unwrap();
    db.set("mode", b"stable").unwrap();

    let mut flash = db.into_flash();
    let record_offset = layout().sector_header_len().unwrap() as u32;
    let mut region =
        NorFlashRegion::new(flash, StorageRegion::new(config.region).unwrap()).unwrap();
    let magic_offset = record_offset + layout().record_magic_offset().unwrap() as u32;
    region
        .write(magic_offset, &[0x00, 0x00, 0x00, 0x00])
        .unwrap();
    flash = region.into_inner();

    let mut rebooted = KvDb::mount(flash, config).unwrap();
    let report = rebooted.check_integrity().unwrap();
    assert!(!report.is_clean());
    assert_eq!(report.record_issues, 1);
}

#[test]
fn repeated_overwrite_cycles_trigger_gc_and_keep_latest_value() {
    let config = gc_config();
    let mut db = KvDb::mount(TestFlash::new(), config).unwrap();
    db.format().unwrap();

    for generation in 0..12u8 {
        let value = [generation; 64];
        db.set("mode", &value).unwrap();
    }

    let mut buf = [0u8; 64];
    let len = db.get_blob_into("mode", &mut buf).unwrap().unwrap();
    assert_eq!(len, 64);
    assert_eq!(buf, [11u8; 64]);
}

#[test]
fn manual_gc_clears_dirty_sectors_and_preserves_live_records() {
    let config = gc_config();
    let mut db = KvDb::mount(TestFlash::new(), config).unwrap();
    db.format().unwrap();

    db.set("alpha", &[0x11; 64]).unwrap();
    db.set("alpha", &[0x22; 64]).unwrap();
    db.set("beta", &[0x33; 64]).unwrap();
    db.delete("beta").unwrap();

    db.collect_garbage().unwrap();

    let mut buf = [0u8; 64];
    let len = db.get_blob_into("alpha", &mut buf).unwrap().unwrap();
    assert_eq!(len, 64);
    assert_eq!(buf, [0x22; 64]);
    assert_eq!(db.get_blob_into("beta", &mut buf).unwrap(), None);

    for sector_index in 0..db.region().sector_count() {
        assert_eq!(
            db.sector_meta(sector_index).unwrap().dirty_status,
            SECTOR_DIRTY_FALSE
        );
    }
}

#[test]
fn iterator_snapshot_yields_only_live_records() {
    let config = gc_config();
    let mut db = KvDb::mount(TestFlash::new(), config).unwrap();
    db.format().unwrap();

    db.set("alpha", &[0x41; 8]).unwrap();
    db.set("beta", &[0x42; 8]).unwrap();
    db.set("alpha", &[0x43; 8]).unwrap();
    db.delete("beta").unwrap();
    db.collect_garbage().unwrap();

    let records: Vec<_> = db.iter().unwrap().collect();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].key, "alpha");
    assert_eq!(records[0].value.as_slice(), &[0x43; 8]);
}
