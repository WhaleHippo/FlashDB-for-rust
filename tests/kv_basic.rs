use flashdb_for_rust::kv::KvDb;
use flashdb_for_rust::storage::MockFlash;
use flashdb_for_rust::{KvConfig, StorageRegionConfig};

type TestFlash = MockFlash<2048, 4, 256>;

fn test_config() -> KvConfig {
    KvConfig {
        region: StorageRegionConfig::new(0, 1024, 256, 4),
        max_key_len: 32,
        max_value_len: 128,
    }
}

fn mount_db() -> KvDb<TestFlash> {
    KvDb::mount(TestFlash::new(), test_config()).unwrap()
}

#[test]
fn kv_roundtrip_delete_and_format_flow() {
    let mut db = mount_db();
    db.format().unwrap();

    let mut buf = [0u8; 32];
    assert!(!db.contains_key("alpha").unwrap());
    assert_eq!(db.get_blob_into("alpha", &mut buf).unwrap(), None);

    db.set("alpha", b"one").unwrap();
    let locator = db.get_locator("alpha").unwrap().unwrap();
    assert_eq!(locator.len(), 3);
    assert!(db.contains_key("alpha").unwrap());

    let len = db.get_blob_into("alpha", &mut buf).unwrap().unwrap();
    assert_eq!(&buf[..len], b"one");

    assert!(db.delete("alpha").unwrap());
    assert!(!db.contains_key("alpha").unwrap());
    assert_eq!(db.get_blob_into("alpha", &mut buf).unwrap(), None);

    db.set("alpha", b"two").unwrap();
    let len = db.get_blob_into("alpha", &mut buf).unwrap().unwrap();
    assert_eq!(&buf[..len], b"two");

    db.format().unwrap();
    assert!(!db.contains_key("alpha").unwrap());
    assert_eq!(db.get_blob_into("alpha", &mut buf).unwrap(), None);
}

#[test]
fn kv_overwrite_latest_wins_across_reboot() {
    let config = test_config();
    let mut db = KvDb::mount(TestFlash::new(), config).unwrap();
    db.format().unwrap();

    db.set("mode", b"old").unwrap();
    db.set("mode", b"newer-value").unwrap();

    let mut buf = [0u8; 32];
    let len = db.get_blob_into("mode", &mut buf).unwrap().unwrap();
    assert_eq!(&buf[..len], b"newer-value");

    let flash = db.into_flash();
    let mut rebooted = KvDb::mount(flash, config).unwrap();
    let len = rebooted.get_blob_into("mode", &mut buf).unwrap().unwrap();
    assert_eq!(&buf[..len], b"newer-value");

    assert!(rebooted.delete("mode").unwrap());
    assert_eq!(rebooted.get_blob_into("mode", &mut buf).unwrap(), None);

    rebooted.set("mode", b"restored").unwrap();
    let len = rebooted.get_blob_into("mode", &mut buf).unwrap().unwrap();
    assert_eq!(&buf[..len], b"restored");
}
