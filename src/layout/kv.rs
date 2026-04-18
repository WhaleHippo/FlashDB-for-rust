use crate::error::{DecodeError, Error, Result};
use crate::layout::align::align_up;
use crate::layout::common::{FORMAT_VERSION, KV_RECORD_MAGIC, KV_SECTOR_MAGIC};

pub const KV_SECTOR_HEADER_LEN: usize = 12;
pub const KV_RECORD_HEADER_LEN: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KvSectorHeader {
    pub magic: u32,
    pub format_version: u16,
    pub sector_index: u16,
    pub reserved: u32,
}

impl KvSectorHeader {
    pub const fn new(sector_index: u16) -> Self {
        Self {
            magic: KV_SECTOR_MAGIC,
            format_version: FORMAT_VERSION,
            sector_index,
            reserved: 0,
        }
    }

    pub fn encode(&self, out: &mut [u8]) -> Result<()> {
        if out.len() < KV_SECTOR_HEADER_LEN {
            return Err(Error::Decode(DecodeError::BufferTooShort));
        }
        out[..4].copy_from_slice(&self.magic.to_le_bytes());
        out[4..6].copy_from_slice(&self.format_version.to_le_bytes());
        out[6..8].copy_from_slice(&self.sector_index.to_le_bytes());
        out[8..12].copy_from_slice(&self.reserved.to_le_bytes());
        Ok(())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < KV_SECTOR_HEADER_LEN {
            return Err(Error::Decode(DecodeError::BufferTooShort));
        }
        let header = Self {
            magic: u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            format_version: u16::from_le_bytes(bytes[4..6].try_into().unwrap()),
            sector_index: u16::from_le_bytes(bytes[6..8].try_into().unwrap()),
            reserved: u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
        };
        if header.magic != KV_SECTOR_MAGIC {
            return Err(Error::Decode(DecodeError::InvalidMagic));
        }
        if header.format_version != FORMAT_VERSION {
            return Err(Error::UnsupportedFormatVersion(header.format_version));
        }
        Ok(header)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KvRecordHeader {
    pub magic: u32,
    pub key_len: u16,
    pub value_len: u32,
    pub flags: u16,
    pub crc32: u32,
    pub name_crc32: u32,
}

impl KvRecordHeader {
    pub const fn new(key_len: u16, value_len: u32) -> Self {
        Self {
            magic: KV_RECORD_MAGIC,
            key_len,
            value_len,
            flags: 0,
            crc32: 0,
            name_crc32: 0,
        }
    }

    pub fn encode(&self, out: &mut [u8]) -> Result<()> {
        if out.len() < KV_RECORD_HEADER_LEN {
            return Err(Error::Decode(DecodeError::BufferTooShort));
        }
        out[..4].copy_from_slice(&self.magic.to_le_bytes());
        out[4..6].copy_from_slice(&self.key_len.to_le_bytes());
        out[6..10].copy_from_slice(&self.value_len.to_le_bytes());
        out[10..12].copy_from_slice(&self.flags.to_le_bytes());
        out[12..16].copy_from_slice(&self.crc32.to_le_bytes());
        out[16..20].copy_from_slice(&self.name_crc32.to_le_bytes());
        Ok(())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < KV_RECORD_HEADER_LEN {
            return Err(Error::Decode(DecodeError::BufferTooShort));
        }
        let header = Self {
            magic: u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            key_len: u16::from_le_bytes(bytes[4..6].try_into().unwrap()),
            value_len: u32::from_le_bytes(bytes[6..10].try_into().unwrap()),
            flags: u16::from_le_bytes(bytes[10..12].try_into().unwrap()),
            crc32: u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
            name_crc32: u32::from_le_bytes(bytes[16..20].try_into().unwrap()),
        };
        if header.magic != KV_RECORD_MAGIC {
            return Err(Error::Decode(DecodeError::InvalidMagic));
        }
        if header.key_len == 0 {
            return Err(Error::Decode(DecodeError::InvalidLength));
        }
        Ok(header)
    }

    pub fn total_len(&self, write_size: usize) -> Result<usize> {
        let base = KV_RECORD_HEADER_LEN
            .checked_add(self.key_len as usize)
            .and_then(|v| v.checked_add(self.value_len as usize))
            .ok_or(Error::InvariantViolation("KV record length overflow"))?;
        align_up(base, write_size)
    }

    pub fn value_offset(&self, write_size: usize) -> Result<usize> {
        align_up(KV_RECORD_HEADER_LEN + self.key_len as usize, write_size)
    }
}
