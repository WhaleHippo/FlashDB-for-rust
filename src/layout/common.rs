pub const ERASED_BYTE: u8 = 0xFF;
pub const WRITTEN_BYTE: u8 = 0x00;
pub const FORMAT_VERSION: u16 = 1;

pub const KV_SECTOR_MAGIC: u32 = 0x3042_4446; // "FDB0"
pub const KV_RECORD_MAGIC: u32 = 0x3030_564B; // "KV00"
pub const TS_SECTOR_MAGIC: u32 = 0x304C_5354; // "TSL0"
pub const DATA_UNUSED_SENTINEL: u32 = 0xFFFF_FFFF; // overlaps with valid u32 max; keep sentinel use explicit in higher layers
pub const FAILED_ADDR: u32 = 0xFFFF_FFFF;

pub const KV_STATUS_COUNT: usize = 6;
pub const TSL_STATUS_COUNT: usize = 6;
pub const SECTOR_STORE_STATUS_COUNT: usize = 4;
pub const SECTOR_DIRTY_STATUS_COUNT: usize = 4;
