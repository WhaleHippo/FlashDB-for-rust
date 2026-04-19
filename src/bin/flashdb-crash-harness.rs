#[cfg(not(feature = "std"))]
fn main() {
    panic!("flashdb-crash-harness requires --features std");
}

#[cfg(feature = "std")]
fn main() {
    if let Err(message) = real_main() {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

#[cfg(feature = "std")]
fn real_main() -> Result<(), String> {
    use flashdb_for_rust::crc::crc_chain;
    use flashdb_for_rust::kv::KvDb;
    use flashdb_for_rust::layout::kv::{KV_PRE_WRITE, KV_WRITE, KvLayout, KvRecordHeader};
    use flashdb_for_rust::storage::{FileFlashSimulator, NorFlashRegion, StorageRegion};
    use flashdb_for_rust::{KvConfig, StorageRegionConfig};

    type CrashFlash = FileFlashSimulator<4, 256>;

    fn config() -> KvConfig {
        KvConfig {
            region: StorageRegionConfig::new(0, 1024, 256, 4),
            max_key_len: 32,
            max_value_len: 128,
        }
    }

    fn layout() -> KvLayout {
        KvLayout::new(32).expect("layout should be constructible")
    }

    fn open_flash(path: &str) -> Result<CrashFlash, String> {
        CrashFlash::new(path, 2048).map_err(|err| format!("open flash: {err}"))
    }

    fn append_raw_record(
        flash: CrashFlash,
        config: KvConfig,
        cursor: u32,
        status: usize,
        key: &[u8],
        value: Option<&[u8]>,
        crc32: u32,
    ) -> Result<CrashFlash, String> {
        let region = StorageRegion::new(config.region).map_err(|err| format!("region: {err:?}"))?;
        let mut storage =
            NorFlashRegion::new(flash, region).map_err(|err| format!("storage: {err:?}"))?;
        let layout = layout();
        let value = value.unwrap_or_default();

        let mut header =
            KvRecordHeader::finalized(&layout, key.len() as u8, value.len() as u32, crc32)
                .map_err(|err| format!("header: {err:?}"))?;
        header.status = status;

        let mut header_buf = vec![
            0xFF;
            layout
                .record_header_len()
                .map_err(|err| format!("record_header_len: {err:?}"))?
        ];
        header
            .encode(&layout, &mut header_buf)
            .map_err(|err| format!("encode header: {err:?}"))?;
        storage
            .write(cursor, &header_buf)
            .map_err(|err| format!("write header: {err:?}"))?;

        if status != KV_PRE_WRITE {
            let key_len = layout
                .aligned_key_len(header.key_len)
                .map_err(|err| format!("aligned_key_len: {err:?}"))?;
            let value_len = layout
                .aligned_value_len(header.value_len)
                .map_err(|err| format!("aligned_value_len: {err:?}"))?;

            let mut key_buf = vec![0xFF; key_len];
            key_buf[..key.len()].copy_from_slice(key);
            storage
                .write(cursor + header_buf.len() as u32, &key_buf)
                .map_err(|err| format!("write key: {err:?}"))?;

            let mut value_buf = vec![0xFF; value_len];
            value_buf[..value.len()].copy_from_slice(value);
            storage
                .write(
                    cursor
                        + layout
                            .value_offset(&header)
                            .map_err(|err| format!("value_offset: {err:?}"))?
                            as u32,
                    &value_buf,
                )
                .map_err(|err| format!("write value: {err:?}"))?;
        }

        Ok(storage.into_inner())
    }

    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        return Err("usage: flashdb-crash-harness <command> <path>".into());
    };
    let Some(path) = args.next() else {
        return Err("missing backing-file path".into());
    };
    let config = config();

    match command.as_str() {
        "kv-init-stable" => {
            let mut db =
                KvDb::mount(open_flash(&path)?, config).map_err(|err| format!("mount: {err:?}"))?;
            db.format().map_err(|err| format!("format: {err:?}"))?;
            db.set("key", b"stable")
                .map_err(|err| format!("set stable: {err:?}"))?;
        }
        "kv-inject-prewrite-tail" => {
            let db =
                KvDb::mount(open_flash(&path)?, config).map_err(|err| format!("mount: {err:?}"))?;
            let cursor = db.write_cursor();
            let flash = db.into_flash();
            let _flash = append_raw_record(
                flash,
                config,
                cursor,
                KV_PRE_WRITE,
                b"key",
                Some(b"broken"),
                0,
            )?;
        }
        "kv-check-stable-and-write-fresh" => {
            let mut db =
                KvDb::mount(open_flash(&path)?, config).map_err(|err| format!("mount: {err:?}"))?;
            let mut buf = [0u8; 32];
            let len = db
                .get_blob_into("key", &mut buf)
                .map_err(|err| format!("get key: {err:?}"))?
                .ok_or_else(|| "missing key after recovery".to_string())?;
            if &buf[..len] != b"stable" {
                return Err(format!("expected stable, got {:?}", &buf[..len]));
            }
            db.set("fresh", b"after-recovery")
                .map_err(|err| format!("set fresh: {err:?}"))?;
        }
        "kv-init-answer" => {
            let mut db =
                KvDb::mount(open_flash(&path)?, config).map_err(|err| format!("mount: {err:?}"))?;
            db.format().map_err(|err| format!("format: {err:?}"))?;
            db.set("answer", b"good")
                .map_err(|err| format!("set answer: {err:?}"))?;
        }
        "kv-inject-crc-tail" => {
            let db =
                KvDb::mount(open_flash(&path)?, config).map_err(|err| format!("mount: {err:?}"))?;
            let cursor = db.write_cursor();
            let flash = db.into_flash();
            let wrong_crc = crc_chain(&[b"definitely", b"wrong"]);
            let _flash = append_raw_record(
                flash,
                config,
                cursor,
                KV_WRITE,
                b"answer",
                Some(b"bad-tail"),
                wrong_crc,
            )?;
        }
        "kv-check-answer-and-write-fresh" => {
            let mut db =
                KvDb::mount(open_flash(&path)?, config).map_err(|err| format!("mount: {err:?}"))?;
            let mut buf = [0u8; 32];
            let len = db
                .get_blob_into("answer", &mut buf)
                .map_err(|err| format!("get answer: {err:?}"))?
                .ok_or_else(|| "missing answer after recovery".to_string())?;
            if &buf[..len] != b"good" {
                return Err(format!("expected good, got {:?}", &buf[..len]));
            }
            db.set("fresh", b"after-crc-recovery")
                .map_err(|err| format!("set fresh: {err:?}"))?;
        }
        other => return Err(format!("unknown command: {other}")),
    }

    Ok(())
}
