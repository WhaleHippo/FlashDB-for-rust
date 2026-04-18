pub const CRC32_INIT: u32 = 0xFFFF_FFFF;
pub const CRC32_POLY: u32 = 0xEDB8_8320;

pub fn crc32(bytes: &[u8]) -> u32 {
    !crc32_update(CRC32_INIT, bytes)
}

pub fn crc32_update(mut state: u32, bytes: &[u8]) -> u32 {
    let mut i = 0;
    while i < bytes.len() {
        state ^= bytes[i] as u32;
        let mut bit = 0;
        while bit < 8 {
            let mask = (state & 1).wrapping_neg();
            state = (state >> 1) ^ (CRC32_POLY & mask);
            bit += 1;
        }
        i += 1;
    }
    state
}

pub fn crc_chain(parts: &[&[u8]]) -> u32 {
    let mut state = CRC32_INIT;
    let mut i = 0;
    while i < parts.len() {
        state = crc32_update(state, parts[i]);
        i += 1;
    }
    !state
}

pub fn crc_with_ff_padding(data: &[u8], aligned_len: usize) -> u32 {
    let mut state = crc32_update(CRC32_INIT, data);
    let padding = aligned_len.saturating_sub(data.len());
    let mut i = 0;
    while i < padding {
        state = crc32_update(state, &[0xFF]);
        i += 1;
    }
    !state
}
