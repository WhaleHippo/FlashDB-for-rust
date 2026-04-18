use crate::error::{DecodeError, Error, Result};
use crate::layout::align::align_up;
use crate::layout::common::{
    DATA_UNUSED_SENTINEL, ERASED_BYTE, KV_RECORD_MAGIC, KV_SECTOR_MAGIC, KV_STATUS_COUNT,
    SECTOR_DIRTY_STATUS_COUNT, SECTOR_STORE_STATUS_COUNT,
};
use crate::layout::status::StatusScheme;

pub const SECTOR_STORE_EMPTY: usize = 1;
pub const SECTOR_STORE_USING: usize = 2;
pub const SECTOR_STORE_FULL: usize = 3;
pub const SECTOR_DIRTY_FALSE: usize = 1;
pub const SECTOR_DIRTY_TRUE: usize = 2;
pub const SECTOR_DIRTY_GC: usize = 3;
pub const KV_PRE_WRITE: usize = 1;
pub const KV_WRITE: usize = 2;
pub const KV_PRE_DELETE: usize = 3;
pub const KV_DELETED: usize = 4;
pub const KV_ERR_HDR: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KvLayout {
    write_granularity_bits: usize,
    store_status: StatusScheme,
    dirty_status: StatusScheme,
    kv_status: StatusScheme,
}

impl KvLayout {
    pub fn new(write_granularity_bits: usize) -> Result<Self> {
        Ok(Self {
            write_granularity_bits,
            store_status: StatusScheme::new(SECTOR_STORE_STATUS_COUNT, write_granularity_bits)?,
            dirty_status: StatusScheme::new(SECTOR_DIRTY_STATUS_COUNT, write_granularity_bits)?,
            kv_status: StatusScheme::new(KV_STATUS_COUNT, write_granularity_bits)?,
        })
    }

    pub const fn write_granularity_bits(&self) -> usize {
        self.write_granularity_bits
    }

    pub const fn write_unit_bytes(&self) -> usize {
        self.kv_status.write_granularity_storage_bytes()
    }

    pub const fn store_status_scheme(&self) -> StatusScheme {
        self.store_status
    }

    pub const fn dirty_status_scheme(&self) -> StatusScheme {
        self.dirty_status
    }

    pub const fn kv_status_scheme(&self) -> StatusScheme {
        self.kv_status
    }

    pub fn sector_store_offset(&self) -> usize {
        0
    }

    pub fn sector_dirty_offset(&self) -> usize {
        self.store_status.table_len()
    }

    pub fn sector_magic_offset(&self) -> Result<usize> {
        align_up(
            self.store_status.table_len() + self.dirty_status.table_len(),
            4,
        )
    }

    pub fn sector_combined_offset(&self) -> Result<usize> {
        Ok(self.sector_magic_offset()? + 4)
    }

    pub fn sector_reserved_offset(&self) -> Result<usize> {
        Ok(self.sector_combined_offset()? + 4)
    }

    pub fn sector_header_len(&self) -> Result<usize> {
        align_up(self.sector_reserved_offset()? + 4, self.write_unit_bytes())
    }

    pub fn record_magic_offset(&self) -> Result<usize> {
        align_up(self.kv_status.table_len(), 4)
    }

    pub fn record_len_offset(&self) -> Result<usize> {
        Ok(self.record_magic_offset()? + 4)
    }

    pub fn record_crc_offset(&self) -> Result<usize> {
        Ok(self.record_len_offset()? + 4)
    }

    pub fn record_key_len_offset(&self) -> Result<usize> {
        Ok(self.record_crc_offset()? + 4)
    }

    pub fn record_value_len_offset(&self) -> Result<usize> {
        align_up(self.record_key_len_offset()? + 1, 4)
    }

    pub fn record_header_len(&self) -> Result<usize> {
        align_up(self.record_value_len_offset()? + 4, self.write_unit_bytes())
    }

    pub fn aligned_key_len(&self, key_len: u8) -> Result<usize> {
        align_up(key_len as usize, self.write_unit_bytes())
    }

    pub fn aligned_value_len(&self, value_len: u32) -> Result<usize> {
        align_up(value_len as usize, self.write_unit_bytes())
    }

    pub fn record_total_len(&self, header: &KvRecordHeader) -> Result<usize> {
        let payload = self
            .aligned_key_len(header.key_len)?
            .checked_add(self.aligned_value_len(header.value_len)?)
            .ok_or(Error::InvariantViolation(
                "KV record payload length overflow",
            ))?;
        self.record_header_len()?
            .checked_add(payload)
            .ok_or(Error::InvariantViolation("KV record total length overflow"))
    }

    pub fn value_offset(&self, header: &KvRecordHeader) -> Result<usize> {
        self.record_header_len()?
            .checked_add(self.aligned_key_len(header.key_len)?)
            .ok_or(Error::InvariantViolation("KV value offset overflow"))
    }

    pub fn crc_seed_bytes(&self, header: &KvRecordHeader) -> [u8; 8] {
        let mut seed = [ERASED_BYTE; 8];
        seed[0] = header.key_len;
        seed[4..8].copy_from_slice(&header.value_len.to_le_bytes());
        seed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KvSectorHeader {
    pub store_status: usize,
    pub dirty_status: usize,
    pub magic: u32,
    pub combined: u32,
    pub reserved: u32,
}

impl KvSectorHeader {
    pub const fn new_empty() -> Self {
        Self {
            store_status: SECTOR_STORE_EMPTY,
            dirty_status: SECTOR_DIRTY_FALSE,
            magic: KV_SECTOR_MAGIC,
            combined: DATA_UNUSED_SENTINEL,
            reserved: DATA_UNUSED_SENTINEL,
        }
    }

    pub fn encode(&self, layout: &KvLayout, out: &mut [u8]) -> Result<()> {
        let total_len = layout.sector_header_len()?;
        if out.len() < total_len {
            return Err(Error::Decode(DecodeError::BufferTooShort));
        }
        let out = &mut out[..total_len];
        out.fill(ERASED_BYTE);

        layout
            .store_status_scheme()
            .encode(self.store_status, &mut out[layout.sector_store_offset()..])?;
        layout
            .dirty_status_scheme()
            .encode(self.dirty_status, &mut out[layout.sector_dirty_offset()..])?;

        let magic_offset = layout.sector_magic_offset()?;
        out[magic_offset..magic_offset + 4].copy_from_slice(&self.magic.to_le_bytes());
        out[magic_offset + 4..magic_offset + 8].copy_from_slice(&self.combined.to_le_bytes());
        out[magic_offset + 8..magic_offset + 12].copy_from_slice(&self.reserved.to_le_bytes());
        Ok(())
    }

    pub fn decode(layout: &KvLayout, bytes: &[u8]) -> Result<Self> {
        let total_len = layout.sector_header_len()?;
        if bytes.len() < total_len {
            return Err(Error::Decode(DecodeError::BufferTooShort));
        }
        let magic_offset = layout.sector_magic_offset()?;
        let header = Self {
            store_status: layout
                .store_status_scheme()
                .decode(&bytes[layout.sector_store_offset()..layout.sector_dirty_offset()])?,
            dirty_status: layout
                .dirty_status_scheme()
                .decode(&bytes[layout.sector_dirty_offset()..magic_offset])?,
            magic: u32::from_le_bytes(bytes[magic_offset..magic_offset + 4].try_into().unwrap()),
            combined: u32::from_le_bytes(
                bytes[magic_offset + 4..magic_offset + 8]
                    .try_into()
                    .unwrap(),
            ),
            reserved: u32::from_le_bytes(
                bytes[magic_offset + 8..magic_offset + 12]
                    .try_into()
                    .unwrap(),
            ),
        };
        if header.magic != KV_SECTOR_MAGIC {
            return Err(Error::Decode(DecodeError::InvalidMagic));
        }
        Ok(header)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KvRecordHeader {
    pub status: usize,
    pub magic: u32,
    pub total_len: u32,
    pub crc32: u32,
    pub key_len: u8,
    pub value_len: u32,
}

impl KvRecordHeader {
    pub const fn new(key_len: u8, value_len: u32) -> Self {
        Self {
            status: KV_PRE_WRITE,
            magic: KV_RECORD_MAGIC,
            total_len: 0,
            crc32: 0,
            key_len,
            value_len,
        }
    }

    pub fn finalized(layout: &KvLayout, key_len: u8, value_len: u32, crc32: u32) -> Result<Self> {
        let mut header = Self::new(key_len, value_len);
        header.crc32 = crc32;
        header.total_len = layout.record_total_len(&header)? as u32;
        Ok(header)
    }

    pub fn encode(&self, layout: &KvLayout, out: &mut [u8]) -> Result<()> {
        let total_len = layout.record_header_len()?;
        if out.len() < total_len {
            return Err(Error::Decode(DecodeError::BufferTooShort));
        }
        let out = &mut out[..total_len];
        out.fill(ERASED_BYTE);

        layout.kv_status_scheme().encode(self.status, out)?;

        let magic_offset = layout.record_magic_offset()?;
        let len_offset = layout.record_len_offset()?;
        let crc_offset = layout.record_crc_offset()?;
        let key_len_offset = layout.record_key_len_offset()?;
        let value_len_offset = layout.record_value_len_offset()?;

        out[magic_offset..magic_offset + 4].copy_from_slice(&self.magic.to_le_bytes());
        out[len_offset..len_offset + 4].copy_from_slice(&self.total_len.to_le_bytes());
        out[crc_offset..crc_offset + 4].copy_from_slice(&self.crc32.to_le_bytes());
        out[key_len_offset] = self.key_len;
        out[value_len_offset..value_len_offset + 4].copy_from_slice(&self.value_len.to_le_bytes());
        Ok(())
    }

    pub fn decode(layout: &KvLayout, bytes: &[u8]) -> Result<Self> {
        let total_len = layout.record_header_len()?;
        if bytes.len() < total_len {
            return Err(Error::Decode(DecodeError::BufferTooShort));
        }

        let magic_offset = layout.record_magic_offset()?;
        let len_offset = layout.record_len_offset()?;
        let crc_offset = layout.record_crc_offset()?;
        let key_len_offset = layout.record_key_len_offset()?;
        let value_len_offset = layout.record_value_len_offset()?;

        let header = Self {
            status: layout.kv_status_scheme().decode(bytes)?,
            magic: u32::from_le_bytes(bytes[magic_offset..magic_offset + 4].try_into().unwrap()),
            total_len: u32::from_le_bytes(bytes[len_offset..len_offset + 4].try_into().unwrap()),
            crc32: u32::from_le_bytes(bytes[crc_offset..crc_offset + 4].try_into().unwrap()),
            key_len: bytes[key_len_offset],
            value_len: u32::from_le_bytes(
                bytes[value_len_offset..value_len_offset + 4]
                    .try_into()
                    .unwrap(),
            ),
        };

        if header.magic != KV_RECORD_MAGIC {
            return Err(Error::Decode(DecodeError::InvalidMagic));
        }
        if header.key_len == 0 {
            return Err(Error::Decode(DecodeError::InvalidLength));
        }
        if header.total_len == u32::MAX {
            return Err(Error::Decode(DecodeError::InvalidLength));
        }

        let minimum_total_len = layout.record_total_len(&header)? as u32;
        if header.total_len < minimum_total_len {
            return Err(Error::Decode(DecodeError::InvalidLength));
        }
        if !(header.total_len as usize).is_multiple_of(layout.write_unit_bytes()) {
            return Err(Error::Decode(DecodeError::InvalidLength));
        }
        Ok(header)
    }
}
