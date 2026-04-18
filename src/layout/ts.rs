use crate::error::{DecodeError, Error, Result};
use crate::layout::align::align_up;
use crate::layout::common::{DATA_UNUSED_SENTINEL, ERASED_BYTE, TS_SECTOR_MAGIC, TSL_STATUS_COUNT};
use crate::layout::status::StatusScheme;

pub const TSL_PRE_WRITE: usize = 1;
pub const TSL_WRITE: usize = 2;
pub const TSL_USER_STATUS1: usize = 3;
pub const TSL_DELETED: usize = 4;
pub const TSL_USER_STATUS2: usize = 5;
pub const SECTOR_STORE_EMPTY: usize = 1;
pub const SECTOR_STORE_USING: usize = 2;
pub const SECTOR_STORE_FULL: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TsBlobMode {
    Variable,
    Fixed(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TsLayout {
    write_granularity_bits: usize,
    time_bytes: usize,
    sector_status: StatusScheme,
    tsl_status: StatusScheme,
}

impl TsLayout {
    pub fn new(write_granularity_bits: usize, time_bytes: usize) -> Result<Self> {
        if time_bytes != 4 && time_bytes != 8 {
            return Err(Error::InvariantViolation(
                "TS timestamp field must be 4 or 8 bytes",
            ));
        }
        Ok(Self {
            write_granularity_bits,
            time_bytes,
            sector_status: StatusScheme::new(4, write_granularity_bits)?,
            tsl_status: StatusScheme::new(TSL_STATUS_COUNT, write_granularity_bits)?,
        })
    }

    pub const fn write_granularity_bits(&self) -> usize {
        self.write_granularity_bits
    }

    pub const fn write_unit_bytes(&self) -> usize {
        self.tsl_status.write_granularity_storage_bytes()
    }

    pub const fn time_bytes(&self) -> usize {
        self.time_bytes
    }

    fn native_time_align(&self) -> usize {
        if self.time_bytes >= 8 { 8 } else { 4 }
    }

    fn aligned_u32_field_len(&self) -> Result<usize> {
        align_up(4, self.write_unit_bytes())
    }

    fn aligned_time_field_len(&self) -> Result<usize> {
        align_up(self.time_bytes, self.write_unit_bytes())
    }

    pub const fn sector_status_scheme(&self) -> StatusScheme {
        self.sector_status
    }

    pub const fn tsl_status_scheme(&self) -> StatusScheme {
        self.tsl_status
    }

    pub fn sector_status_offset(&self) -> usize {
        0
    }

    pub fn sector_magic_offset(&self) -> Result<usize> {
        align_up(self.sector_status.table_len(), 4)
    }

    pub fn sector_start_time_offset(&self) -> Result<usize> {
        Ok(self.sector_magic_offset()? + self.aligned_u32_field_len()?)
    }

    pub fn sector_end0_time_offset(&self) -> Result<usize> {
        align_up(
            self.sector_start_time_offset()? + self.aligned_time_field_len()?,
            self.native_time_align(),
        )
    }

    pub fn sector_end0_index_offset(&self) -> Result<usize> {
        Ok(self.sector_end0_time_offset()? + self.aligned_time_field_len()?)
    }

    pub fn sector_end0_status_offset(&self) -> Result<usize> {
        Ok(self.sector_end0_index_offset()? + self.aligned_u32_field_len()?)
    }

    pub fn sector_end1_time_offset(&self) -> Result<usize> {
        align_up(
            self.sector_end0_status_offset()? + self.tsl_status.table_len(),
            self.native_time_align(),
        )
    }

    pub fn sector_end1_index_offset(&self) -> Result<usize> {
        Ok(self.sector_end1_time_offset()? + self.aligned_time_field_len()?)
    }

    pub fn sector_end1_status_offset(&self) -> Result<usize> {
        Ok(self.sector_end1_index_offset()? + self.aligned_u32_field_len()?)
    }

    pub fn sector_reserved_offset(&self) -> Result<usize> {
        align_up(
            self.sector_end1_status_offset()? + self.tsl_status.table_len(),
            4,
        )
    }

    pub fn sector_header_len(&self) -> Result<usize> {
        align_up(self.sector_reserved_offset()? + 4, self.write_unit_bytes())
    }

    pub fn index_time_offset(&self) -> Result<usize> {
        align_up(self.tsl_status.table_len(), self.native_time_align())
    }

    pub fn index_log_len_offset(&self) -> Result<usize> {
        align_up(self.index_time_offset()? + self.time_bytes, 4)
    }

    pub fn index_log_addr_offset(&self) -> Result<usize> {
        Ok(self.index_log_len_offset()? + 4)
    }

    pub fn index_header_len(&self, mode: TsBlobMode) -> Result<usize> {
        let base = match mode {
            TsBlobMode::Variable => self.index_log_addr_offset()? + 4,
            TsBlobMode::Fixed(_) => self.index_time_offset()? + self.time_bytes,
        };
        align_up(base, self.write_unit_bytes())
    }

    pub fn fixed_blob_len(&self, mode: TsBlobMode) -> Result<usize> {
        match mode {
            TsBlobMode::Variable => Err(Error::InvariantViolation(
                "fixed blob helpers require fixed blob mode",
            )),
            TsBlobMode::Fixed(0) => Err(Error::InvariantViolation(
                "fixed blob mode must be non-zero",
            )),
            TsBlobMode::Fixed(len) => align_up(len as usize, self.write_unit_bytes()),
        }
    }

    pub fn fixed_blob_data_offset(
        &self,
        sector_len: usize,
        mode: TsBlobMode,
        entry_index: usize,
    ) -> Result<usize> {
        let blob_len = self.fixed_blob_len(mode)?;
        let consumed = blob_len
            .checked_mul(entry_index + 1)
            .ok_or(Error::InvariantViolation("fixed blob offset overflow"))?;
        sector_len
            .checked_sub(consumed)
            .ok_or(Error::InvariantViolation("fixed blob offset underflow"))
    }

    pub fn fixed_entry_capacity(&self, sector_len: usize, mode: TsBlobMode) -> Result<usize> {
        let header_len = self.sector_header_len()?;
        let index_len = self.index_header_len(mode)?;
        let blob_len = self.fixed_blob_len(mode)?;
        let usable = sector_len
            .checked_sub(header_len)
            .ok_or(Error::InvariantViolation("sector too small for TS header"))?;
        Ok(usable / (index_len + blob_len))
    }

    pub fn sector_remaining(
        &self,
        sector_len: usize,
        mode: TsBlobMode,
        index_entries: usize,
        aligned_data_bytes: usize,
    ) -> Result<usize> {
        let front = self
            .sector_header_len()?
            .checked_add(
                self.index_header_len(mode)?
                    .checked_mul(index_entries)
                    .ok_or(Error::InvariantViolation("TS index span overflow"))?,
            )
            .ok_or(Error::InvariantViolation("TS front span overflow"))?;
        sector_len
            .checked_sub(front)
            .and_then(|remain| remain.checked_sub(aligned_data_bytes))
            .ok_or(Error::InvariantViolation("TS sector remaining underflow"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TsEndInfo {
    pub timestamp: u64,
    pub index: u32,
    pub status: usize,
}

impl TsEndInfo {
    pub const fn unused() -> Self {
        Self {
            timestamp: DATA_UNUSED_SENTINEL as u64,
            index: DATA_UNUSED_SENTINEL,
            status: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TsSectorHeader {
    pub store_status: usize,
    pub magic: u32,
    pub start_time: u64,
    pub end_info: [TsEndInfo; 2],
    pub reserved: u32,
}

impl TsSectorHeader {
    pub const fn new_empty() -> Self {
        Self {
            store_status: SECTOR_STORE_EMPTY,
            magic: TS_SECTOR_MAGIC,
            start_time: DATA_UNUSED_SENTINEL as u64,
            end_info: [TsEndInfo::unused(), TsEndInfo::unused()],
            reserved: DATA_UNUSED_SENTINEL,
        }
    }

    pub fn encode(&self, layout: &TsLayout, out: &mut [u8]) -> Result<()> {
        let total_len = layout.sector_header_len()?;
        if out.len() < total_len {
            return Err(Error::Decode(DecodeError::BufferTooShort));
        }
        let out = &mut out[..total_len];
        out.fill(ERASED_BYTE);

        layout
            .sector_status_scheme()
            .encode(self.store_status, out)?;

        let magic_offset = layout.sector_magic_offset()?;
        out[magic_offset..magic_offset + 4].copy_from_slice(&self.magic.to_le_bytes());
        encode_time(
            self.start_time,
            layout,
            &mut out[layout.sector_start_time_offset()?..],
        )?;

        encode_time(
            self.end_info[0].timestamp,
            layout,
            &mut out[layout.sector_end0_time_offset()?..],
        )?;
        out[layout.sector_end0_index_offset()?..layout.sector_end0_index_offset()? + 4]
            .copy_from_slice(&self.end_info[0].index.to_le_bytes());
        layout.tsl_status_scheme().encode(
            self.end_info[0].status,
            &mut out[layout.sector_end0_status_offset()?..],
        )?;

        encode_time(
            self.end_info[1].timestamp,
            layout,
            &mut out[layout.sector_end1_time_offset()?..],
        )?;
        out[layout.sector_end1_index_offset()?..layout.sector_end1_index_offset()? + 4]
            .copy_from_slice(&self.end_info[1].index.to_le_bytes());
        layout.tsl_status_scheme().encode(
            self.end_info[1].status,
            &mut out[layout.sector_end1_status_offset()?..],
        )?;

        let reserved_offset = layout.sector_reserved_offset()?;
        out[reserved_offset..reserved_offset + 4].copy_from_slice(&self.reserved.to_le_bytes());
        Ok(())
    }

    pub fn decode(layout: &TsLayout, bytes: &[u8]) -> Result<Self> {
        let total_len = layout.sector_header_len()?;
        if bytes.len() < total_len {
            return Err(Error::Decode(DecodeError::BufferTooShort));
        }
        let magic_offset = layout.sector_magic_offset()?;
        let header = Self {
            store_status: layout.sector_status_scheme().decode(bytes)?,
            magic: u32::from_le_bytes(bytes[magic_offset..magic_offset + 4].try_into().unwrap()),
            start_time: decode_time(layout, &bytes[layout.sector_start_time_offset()?..])?,
            end_info: [
                TsEndInfo {
                    timestamp: decode_time(layout, &bytes[layout.sector_end0_time_offset()?..])?,
                    index: u32::from_le_bytes(
                        bytes[layout.sector_end0_index_offset()?
                            ..layout.sector_end0_index_offset()? + 4]
                            .try_into()
                            .unwrap(),
                    ),
                    status: layout.tsl_status_scheme().decode(
                        &bytes[layout.sector_end0_status_offset()?
                            ..layout.sector_end1_time_offset()?],
                    )?,
                },
                TsEndInfo {
                    timestamp: decode_time(layout, &bytes[layout.sector_end1_time_offset()?..])?,
                    index: u32::from_le_bytes(
                        bytes[layout.sector_end1_index_offset()?
                            ..layout.sector_end1_index_offset()? + 4]
                            .try_into()
                            .unwrap(),
                    ),
                    status: layout.tsl_status_scheme().decode(
                        &bytes[layout.sector_end1_status_offset()?
                            ..layout.sector_reserved_offset()?],
                    )?,
                },
            ],
            reserved: u32::from_le_bytes(
                bytes[layout.sector_reserved_offset()?..layout.sector_reserved_offset()? + 4]
                    .try_into()
                    .unwrap(),
            ),
        };
        if header.magic != TS_SECTOR_MAGIC {
            return Err(Error::Decode(DecodeError::InvalidMagic));
        }
        Ok(header)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TsIndexHeader {
    pub status: usize,
    pub timestamp: u64,
    pub log_len: Option<u32>,
    pub log_addr: Option<u32>,
}

impl TsIndexHeader {
    pub const fn new(timestamp: u64) -> Self {
        Self {
            status: TSL_PRE_WRITE,
            timestamp,
            log_len: None,
            log_addr: None,
        }
    }

    pub fn variable(timestamp: u64, log_addr: u32, log_len: u32) -> Self {
        Self {
            status: TSL_PRE_WRITE,
            timestamp,
            log_len: Some(log_len),
            log_addr: Some(log_addr),
        }
    }

    pub fn encode(&self, layout: &TsLayout, mode: TsBlobMode, out: &mut [u8]) -> Result<()> {
        let total_len = layout.index_header_len(mode)?;
        if out.len() < total_len {
            return Err(Error::Decode(DecodeError::BufferTooShort));
        }
        let out = &mut out[..total_len];
        out.fill(ERASED_BYTE);
        layout.tsl_status_scheme().encode(self.status, out)?;
        encode_time(
            self.timestamp,
            layout,
            &mut out[layout.index_time_offset()?..],
        )?;
        if let TsBlobMode::Variable = mode {
            let log_len = self
                .log_len
                .ok_or(Error::Decode(DecodeError::InvalidLength))?;
            let log_addr = self
                .log_addr
                .ok_or(Error::Decode(DecodeError::InvalidLength))?;
            let log_len_offset = layout.index_log_len_offset()?;
            let log_addr_offset = layout.index_log_addr_offset()?;
            out[log_len_offset..log_len_offset + 4].copy_from_slice(&log_len.to_le_bytes());
            out[log_addr_offset..log_addr_offset + 4].copy_from_slice(&log_addr.to_le_bytes());
        }
        Ok(())
    }

    pub fn decode(layout: &TsLayout, mode: TsBlobMode, bytes: &[u8]) -> Result<Self> {
        let total_len = layout.index_header_len(mode)?;
        if bytes.len() < total_len {
            return Err(Error::Decode(DecodeError::BufferTooShort));
        }
        let mut header = Self {
            status: layout.tsl_status_scheme().decode(bytes)?,
            timestamp: decode_time(layout, &bytes[layout.index_time_offset()?..])?,
            log_len: None,
            log_addr: None,
        };
        if let TsBlobMode::Variable = mode {
            let log_len_offset = layout.index_log_len_offset()?;
            let log_addr_offset = layout.index_log_addr_offset()?;
            header.log_len = Some(u32::from_le_bytes(
                bytes[log_len_offset..log_len_offset + 4]
                    .try_into()
                    .unwrap(),
            ));
            header.log_addr = Some(u32::from_le_bytes(
                bytes[log_addr_offset..log_addr_offset + 4]
                    .try_into()
                    .unwrap(),
            ));
        }
        Ok(header)
    }
}

fn encode_time(value: u64, layout: &TsLayout, out: &mut [u8]) -> Result<()> {
    let field_len = layout.aligned_time_field_len()?;
    if out.len() < field_len {
        return Err(Error::Decode(DecodeError::BufferTooShort));
    }
    out[..field_len].fill(ERASED_BYTE);
    match layout.time_bytes() {
        4 => out[..4].copy_from_slice(&(value as u32).to_le_bytes()),
        8 => out[..8].copy_from_slice(&value.to_le_bytes()),
        _ => return Err(Error::InvariantViolation("invalid TS time size")),
    }
    Ok(())
}

fn decode_time(layout: &TsLayout, bytes: &[u8]) -> Result<u64> {
    let field_len = layout.aligned_time_field_len()?;
    if bytes.len() < field_len {
        return Err(Error::Decode(DecodeError::BufferTooShort));
    }
    Ok(match layout.time_bytes() {
        4 => u32::from_le_bytes(bytes[..4].try_into().unwrap()) as u64,
        8 => u64::from_le_bytes(bytes[..8].try_into().unwrap()),
        _ => return Err(Error::InvariantViolation("invalid TS time size")),
    })
}
