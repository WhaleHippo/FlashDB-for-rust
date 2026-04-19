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
    use flashdb_for_rust::layout::common::ERASED_BYTE;
    use flashdb_for_rust::layout::kv::{KV_PRE_WRITE, KV_WRITE, KvLayout, KvRecordHeader};
    use flashdb_for_rust::layout::ts::{
        SECTOR_STORE_EMPTY, SECTOR_STORE_FULL, SECTOR_STORE_USING, TSL_PRE_WRITE, TSL_USER_STATUS1,
        TSL_WRITE, TsIndexHeader, TsLayout, TsSectorHeader,
    };
    use flashdb_for_rust::storage::{FileFlashSimulator, NorFlashRegion, StorageRegion};
    use flashdb_for_rust::tsdb::TsDb;
    use flashdb_for_rust::{BlobMode, KvConfig, StorageRegionConfig, TimestampPolicy, TsdbConfig};

    type CrashFlash = FileFlashSimulator<4, 256>;

    #[derive(Clone, Copy, Debug)]
    struct TsSectorCursor {
        store_status: usize,
        entry_count: u32,
        empty_index_offset: u32,
        empty_data_offset: u32,
    }

    fn kv_config() -> KvConfig {
        KvConfig {
            region: StorageRegionConfig::new(0, 1024, 256, 4),
            max_key_len: 32,
            max_value_len: 128,
        }
    }

    fn ts_config() -> TsdbConfig {
        TsdbConfig {
            region: StorageRegionConfig::new(0, 512, 256, 4),
            blob_mode: BlobMode::Variable,
            timestamp_policy: TimestampPolicy::StrictMonotonic,
            rollover: false,
        }
    }

    fn kv_layout() -> KvLayout {
        KvLayout::new(32).expect("KV layout should be constructible")
    }

    fn ts_layout() -> TsLayout {
        TsLayout::new(32, 4).expect("TS layout should be constructible")
    }

    fn open_flash(path: &str) -> Result<CrashFlash, String> {
        CrashFlash::new(path, 2048).map_err(|err| format!("open flash: {err}"))
    }

    fn append_raw_kv_record(
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
        let layout = kv_layout();
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

    fn ts_payload_storage_len(payload_len: usize, write_size: usize) -> u32 {
        payload_len.div_ceil(write_size) as u32 * write_size as u32
    }

    fn is_erased(bytes: &[u8]) -> bool {
        bytes.iter().all(|&byte| byte == ERASED_BYTE)
    }

    fn scan_ts_sector_cursors(
        flash: &mut CrashFlash,
        config: TsdbConfig,
    ) -> Result<Vec<TsSectorCursor>, String> {
        let region = StorageRegion::new(config.region).map_err(|err| format!("region: {err:?}"))?;
        let mut storage = NorFlashRegion::new(flash.clone(), region)
            .map_err(|err| format!("storage: {err:?}"))?;
        let layout = ts_layout();
        let header_len = layout
            .sector_header_len()
            .map_err(|err| format!("sector_header_len: {err:?}"))?;
        let index_len = layout
            .index_header_len(flashdb_for_rust::layout::ts::TsBlobMode::Variable)
            .map_err(|err| format!("index_header_len: {err:?}"))? as u32;

        let mut sectors = Vec::new();
        for sector_index in 0..storage.region().sector_count() {
            let base = storage
                .region()
                .sector_start(sector_index)
                .map_err(|err| format!("sector_start: {err:?}"))?;
            let mut cursor = TsSectorCursor {
                store_status: SECTOR_STORE_EMPTY,
                entry_count: 0,
                empty_index_offset: base + header_len as u32,
                empty_data_offset: base + storage.region().erase_size(),
            };

            let mut header_buf = vec![0u8; header_len];
            storage
                .read(base, &mut header_buf)
                .map_err(|err| format!("read sector header: {err:?}"))?;
            if is_erased(&header_buf) {
                sectors.push(cursor);
                continue;
            }

            let header = TsSectorHeader::decode(&layout, &header_buf)
                .map_err(|err| format!("decode sector header: {err:?}"))?;
            cursor.store_status = header.store_status;

            let mut index_offset = base + header_len as u32;
            while index_offset
                .checked_add(index_len)
                .is_some_and(|end| end <= cursor.empty_data_offset)
            {
                let mut index_buf = vec![0u8; index_len as usize];
                storage
                    .read(index_offset, &mut index_buf)
                    .map_err(|err| format!("read index: {err:?}"))?;
                if is_erased(&index_buf) {
                    break;
                }
                let index = TsIndexHeader::decode(
                    &layout,
                    flashdb_for_rust::layout::ts::TsBlobMode::Variable,
                    &index_buf,
                )
                .map_err(|err| format!("decode index: {err:?}"))?;
                if index.status == 0 || index.status == TSL_PRE_WRITE {
                    break;
                }
                let log_addr = index
                    .log_addr
                    .ok_or_else(|| "missing log_addr".to_string())?;
                cursor.empty_index_offset = index_offset + index_len;
                cursor.empty_data_offset = log_addr;
                cursor.entry_count += 1;
                index_offset = cursor.empty_index_offset;
            }

            sectors.push(cursor);
        }

        Ok(sectors)
    }

    fn select_ts_current_sector(cursors: &[TsSectorCursor]) -> Option<u32> {
        if let Some((index, _)) = cursors
            .iter()
            .enumerate()
            .rev()
            .find(|(_, sector)| sector.store_status == SECTOR_STORE_USING)
        {
            return Some(index as u32);
        }
        cursors
            .iter()
            .enumerate()
            .find(|(_, sector)| sector.entry_count == 0 && sector.store_status != SECTOR_STORE_FULL)
            .map(|(index, _)| index as u32)
    }

    fn inject_ts_prewrite_tail(
        flash: CrashFlash,
        config: TsdbConfig,
        timestamp: u64,
        payload: &[u8],
    ) -> Result<CrashFlash, String> {
        let mut flash_for_scan = flash.clone();
        let cursors = scan_ts_sector_cursors(&mut flash_for_scan, config)?;
        let current_sector = select_ts_current_sector(&cursors)
            .ok_or_else(|| "no writable TS sector found".to_string())?;
        let cursor = cursors[current_sector as usize];
        let payload_storage_len = ts_payload_storage_len(payload.len(), 4);
        let data_offset = cursor
            .empty_data_offset
            .checked_sub(payload_storage_len)
            .ok_or_else(|| "payload does not fit in current sector".to_string())?;

        let region = StorageRegion::new(config.region).map_err(|err| format!("region: {err:?}"))?;
        let mut storage =
            NorFlashRegion::new(flash, region).map_err(|err| format!("storage: {err:?}"))?;
        let layout = ts_layout();
        let index_len = layout
            .index_header_len(flashdb_for_rust::layout::ts::TsBlobMode::Variable)
            .map_err(|err| format!("index_header_len: {err:?}"))?;
        let header = TsIndexHeader::variable(timestamp, data_offset, payload.len() as u32);
        let mut index_buf = vec![0xFF; index_len];
        header
            .encode(
                &layout,
                flashdb_for_rust::layout::ts::TsBlobMode::Variable,
                &mut index_buf,
            )
            .map_err(|err| format!("encode ts index: {err:?}"))?;
        storage
            .write(cursor.empty_index_offset, &index_buf)
            .map_err(|err| format!("write ts prewrite index: {err:?}"))?;
        Ok(storage.into_inner())
    }

    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        return Err("usage: flashdb-crash-harness <command> <path>".into());
    };
    let Some(path) = args.next() else {
        return Err("missing backing-file path".into());
    };

    match command.as_str() {
        "kv-init-stable" => {
            let mut db = KvDb::mount(open_flash(&path)?, kv_config())
                .map_err(|err| format!("mount: {err:?}"))?;
            db.format().map_err(|err| format!("format: {err:?}"))?;
            db.set("key", b"stable")
                .map_err(|err| format!("set stable: {err:?}"))?;
        }
        "kv-inject-prewrite-tail" => {
            let db = KvDb::mount(open_flash(&path)?, kv_config())
                .map_err(|err| format!("mount: {err:?}"))?;
            let cursor = db.write_cursor();
            let flash = db.into_flash();
            let _flash = append_raw_kv_record(
                flash,
                kv_config(),
                cursor,
                KV_PRE_WRITE,
                b"key",
                Some(b"broken"),
                0,
            )?;
        }
        "kv-check-stable-and-write-fresh" => {
            let mut db = KvDb::mount(open_flash(&path)?, kv_config())
                .map_err(|err| format!("mount: {err:?}"))?;
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
            let mut db = KvDb::mount(open_flash(&path)?, kv_config())
                .map_err(|err| format!("mount: {err:?}"))?;
            db.format().map_err(|err| format!("format: {err:?}"))?;
            db.set("answer", b"good")
                .map_err(|err| format!("set answer: {err:?}"))?;
        }
        "kv-inject-crc-tail" => {
            let db = KvDb::mount(open_flash(&path)?, kv_config())
                .map_err(|err| format!("mount: {err:?}"))?;
            let cursor = db.write_cursor();
            let flash = db.into_flash();
            let wrong_crc = crc_chain(&[b"definitely", b"wrong"]);
            let _flash = append_raw_kv_record(
                flash,
                kv_config(),
                cursor,
                KV_WRITE,
                b"answer",
                Some(b"bad-tail"),
                wrong_crc,
            )?;
        }
        "kv-check-answer-and-write-fresh" => {
            let mut db = KvDb::mount(open_flash(&path)?, kv_config())
                .map_err(|err| format!("mount: {err:?}"))?;
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
        "ts-init-seed" => {
            let mut db = TsDb::mount(open_flash(&path)?, ts_config())
                .map_err(|err| format!("mount ts: {err:?}"))?;
            db.format().map_err(|err| format!("format ts: {err:?}"))?;
            db.append(10, b"one")
                .map_err(|err| format!("append 10: {err:?}"))?;
            db.append(20, b"two")
                .map_err(|err| format!("append 20: {err:?}"))?;
        }
        "ts-inject-prewrite-tail" => {
            let flash = open_flash(&path)?;
            let _flash = inject_ts_prewrite_tail(flash, ts_config(), 30, b"broken-tail")?;
        }
        "ts-check-seed-and-append-fresh" => {
            let mut db = TsDb::mount(open_flash(&path)?, ts_config())
                .map_err(|err| format!("mount ts: {err:?}"))?;
            let records = db
                .iter()
                .map_err(|err| format!("iter: {err:?}"))?
                .collect::<Vec<_>>();
            let timestamps = records
                .iter()
                .map(|record| record.timestamp)
                .collect::<Vec<_>>();
            if timestamps != vec![10, 20] {
                return Err(format!("expected [10, 20], got {timestamps:?}"));
            }
            db.append(30, b"fresh")
                .map_err(|err| format!("append fresh: {err:?}"))?;
            let count = db
                .query_count(0, 100, TSL_WRITE)
                .map_err(|err| format!("query_count: {err:?}"))?;
            if count != 3 {
                return Err(format!(
                    "expected 3 live records after recovery, got {count}"
                ));
            }
        }
        "ts-init-window" => {
            let mut db = TsDb::mount(open_flash(&path)?, ts_config())
                .map_err(|err| format!("mount ts: {err:?}"))?;
            db.format().map_err(|err| format!("format ts: {err:?}"))?;
            for (timestamp, payload) in [
                (10_u64, b"one".as_slice()),
                (20, b"two".as_slice()),
                (30, b"three".as_slice()),
            ] {
                db.append(timestamp, payload)
                    .map_err(|err| format!("append window {timestamp}: {err:?}"))?;
            }
        }
        "ts-check-window-query" => {
            let mut db = TsDb::mount(open_flash(&path)?, ts_config())
                .map_err(|err| format!("mount ts: {err:?}"))?;
            let reverse = db
                .iter_reverse()
                .map_err(|err| format!("iter_reverse: {err:?}"))?
                .collect::<Vec<_>>();
            let reverse_timestamps = reverse
                .iter()
                .map(|record| record.timestamp)
                .collect::<Vec<_>>();
            if reverse_timestamps != vec![30, 20, 10] {
                return Err(format!(
                    "expected reverse [30, 20, 10], got {reverse_timestamps:?}"
                ));
            }
            let query_count = db
                .query_count(15, 30, TSL_WRITE)
                .map_err(|err| format!("query_count: {err:?}"))?;
            if query_count != 2 {
                return Err(format!("expected query_count 2, got {query_count}"));
            }
            let by_time = db
                .iter_by_time(30, 20)
                .map_err(|err| format!("iter_by_time: {err:?}"))?
                .collect::<Vec<_>>();
            let by_time_timestamps = by_time
                .iter()
                .map(|record| record.timestamp)
                .collect::<Vec<_>>();
            if by_time_timestamps != vec![30, 20] {
                return Err(format!(
                    "expected range [30, 20], got {by_time_timestamps:?}"
                ));
            }
            db.append(40, b"four")
                .map_err(|err| format!("append 40: {err:?}"))?;
        }
        "ts-init-status-window" => {
            let mut db = TsDb::mount(open_flash(&path)?, ts_config())
                .map_err(|err| format!("mount ts: {err:?}"))?;
            db.format().map_err(|err| format!("format ts: {err:?}"))?;
            for (timestamp, payload) in [
                (10_u64, b"one".as_slice()),
                (20, b"two".as_slice()),
                (30, b"three".as_slice()),
            ] {
                db.append(timestamp, payload)
                    .map_err(|err| format!("append status window {timestamp}: {err:?}"))?;
            }
        }
        "ts-set-status-and-reboot-check" => {
            let mut db = TsDb::mount(open_flash(&path)?, ts_config())
                .map_err(|err| format!("mount ts: {err:?}"))?;
            db.set_status(20, TSL_USER_STATUS1)
                .map_err(|err| format!("set_status user1: {err:?}"))?;
            let flash = db.into_flash();
            let mut rebooted =
                TsDb::mount(flash, ts_config()).map_err(|err| format!("remount ts: {err:?}"))?;
            let records = rebooted
                .iter_by_time(10, 30)
                .map_err(|err| format!("iter_by_time after status: {err:?}"))?
                .collect::<Vec<_>>();
            let statuses = records
                .iter()
                .map(|record| record.status)
                .collect::<Vec<_>>();
            if statuses != vec![TSL_WRITE, TSL_USER_STATUS1, TSL_WRITE] {
                return Err(format!(
                    "expected statuses [WRITE, USER1, WRITE], got {statuses:?}"
                ));
            }
            let user1_count = rebooted
                .query_count(10, 30, TSL_USER_STATUS1)
                .map_err(|err| format!("query_count user1: {err:?}"))?;
            let write_count = rebooted
                .query_count(10, 30, TSL_WRITE)
                .map_err(|err| format!("query_count write: {err:?}"))?;
            if user1_count != 1 || write_count != 2 {
                return Err(format!(
                    "expected user1=1 and write=2, got user1={user1_count}, write={write_count}"
                ));
            }
        }
        "ts-init-clean-window" => {
            let mut db = TsDb::mount(open_flash(&path)?, ts_config())
                .map_err(|err| format!("mount ts: {err:?}"))?;
            db.format().map_err(|err| format!("format ts: {err:?}"))?;
            db.append(10, b"one")
                .map_err(|err| format!("append clean 10: {err:?}"))?;
            db.append(20, b"two")
                .map_err(|err| format!("append clean 20: {err:?}"))?;
            db.set_status(20, TSL_USER_STATUS1)
                .map_err(|err| format!("set_status clean: {err:?}"))?;
        }
        "ts-clean-and-reboot-check" => {
            let mut db = TsDb::mount(open_flash(&path)?, ts_config())
                .map_err(|err| format!("mount ts: {err:?}"))?;
            db.clean().map_err(|err| format!("clean ts: {err:?}"))?;
            let flash = db.into_flash();
            let mut rebooted =
                TsDb::mount(flash, ts_config()).map_err(|err| format!("remount ts: {err:?}"))?;
            let count = rebooted
                .iter()
                .map_err(|err| format!("iter after clean: {err:?}"))?
                .count();
            if count != 0 {
                return Err(format!(
                    "expected 0 records after clean reboot, got {count}"
                ));
            }
            let query_count = rebooted
                .query_count(0, 100, TSL_WRITE)
                .map_err(|err| format!("query_count after clean: {err:?}"))?;
            if query_count != 0 {
                return Err(format!(
                    "expected 0 live records after clean reboot, got {query_count}"
                ));
            }
            rebooted
                .append(30, b"three")
                .map_err(|err| format!("append after clean reboot: {err:?}"))?;
            let records = rebooted
                .iter()
                .map_err(|err| format!("iter after clean append: {err:?}"))?
                .collect::<Vec<_>>();
            if records.len() != 1 || records[0].timestamp != 30 {
                return Err(format!(
                    "expected single timestamp 30 after clean append, got {:?}",
                    records
                        .iter()
                        .map(|record| record.timestamp)
                        .collect::<Vec<_>>()
                ));
            }
        }
        other => return Err(format!("unknown command: {other}")),
    }

    Ok(())
}
