use crate::error::{DecodeError, Error, Result};
use crate::layout::common::{FORMAT_VERSION, TS_SECTOR_MAGIC};

const TS_INDEX_MAGIC: u32 = 0x5453_4944;

pub const TS_SECTOR_HEADER_LEN: usize = 16;
pub const TS_INDEX_HEADER_LEN: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TsBlobMode {
    Variable,
    Fixed(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TsSectorHeader {
    pub magic: u32,
    pub format_version: u16,
    pub sector_index: u16,
    pub oldest_timestamp: u32,
    pub newest_timestamp: u32,
}

impl TsSectorHeader {
    pub const fn new(sector_index: u16) -> Self {
        Self {
            magic: TS_SECTOR_MAGIC,
            format_version: FORMAT_VERSION,
            sector_index,
            oldest_timestamp: 0,
            newest_timestamp: 0,
        }
    }

    pub fn encode(&self, out: &mut [u8]) -> Result<()> {
        if out.len() < TS_SECTOR_HEADER_LEN {
            return Err(Error::Decode(DecodeError::BufferTooShort));
        }
        out[..4].copy_from_slice(&self.magic.to_le_bytes());
        out[4..6].copy_from_slice(&self.format_version.to_le_bytes());
        out[6..8].copy_from_slice(&self.sector_index.to_le_bytes());
        out[8..12].copy_from_slice(&self.oldest_timestamp.to_le_bytes());
        out[12..16].copy_from_slice(&self.newest_timestamp.to_le_bytes());
        Ok(())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < TS_SECTOR_HEADER_LEN {
            return Err(Error::Decode(DecodeError::BufferTooShort));
        }
        let header = Self {
            magic: u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            format_version: u16::from_le_bytes(bytes[4..6].try_into().unwrap()),
            sector_index: u16::from_le_bytes(bytes[6..8].try_into().unwrap()),
            oldest_timestamp: u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            newest_timestamp: u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
        };
        if header.magic != TS_SECTOR_MAGIC {
            return Err(Error::Decode(DecodeError::InvalidMagic));
        }
        if header.format_version != FORMAT_VERSION {
            return Err(Error::UnsupportedFormatVersion(header.format_version));
        }
        Ok(header)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TsIndexHeader {
    pub magic: u32,
    pub timestamp: u32,
    pub data_offset: u32,
    pub data_len: u16,
    pub flags: u16,
    pub crc32: u32,
}

impl TsIndexHeader {
    pub const fn new(timestamp: u32, data_offset: u32, data_len: u16) -> Self {
        Self {
            magic: TS_INDEX_MAGIC,
            timestamp,
            data_offset,
            data_len,
            flags: 0,
            crc32: 0,
        }
    }

    pub fn encode(&self, out: &mut [u8]) -> Result<()> {
        if out.len() < TS_INDEX_HEADER_LEN {
            return Err(Error::Decode(DecodeError::BufferTooShort));
        }
        out[..4].copy_from_slice(&self.magic.to_le_bytes());
        out[4..8].copy_from_slice(&self.timestamp.to_le_bytes());
        out[8..12].copy_from_slice(&self.data_offset.to_le_bytes());
        out[12..14].copy_from_slice(&self.data_len.to_le_bytes());
        out[14..16].copy_from_slice(&self.flags.to_le_bytes());
        out[16..20].copy_from_slice(&self.crc32.to_le_bytes());
        Ok(())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < TS_INDEX_HEADER_LEN {
            return Err(Error::Decode(DecodeError::BufferTooShort));
        }
        let header = Self {
            magic: u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            timestamp: u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            data_offset: u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            data_len: u16::from_le_bytes(bytes[12..14].try_into().unwrap()),
            flags: u16::from_le_bytes(bytes[14..16].try_into().unwrap()),
            crc32: u32::from_le_bytes(bytes[16..20].try_into().unwrap()),
        };
        if header.magic != TS_INDEX_MAGIC {
            return Err(Error::Decode(DecodeError::InvalidMagic));
        }
        Ok(header)
    }
}

pub fn fixed_index_span(entries: usize) -> usize {
    entries * TS_INDEX_HEADER_LEN
}

pub fn sector_remaining(
    total: usize,
    header_len: usize,
    used_front: usize,
    used_back: usize,
) -> usize {
    total.saturating_sub(header_len + used_front + used_back)
}
