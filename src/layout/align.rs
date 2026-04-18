use crate::error::{AlignmentError, Error, Result};

pub fn align_up(value: usize, align: usize) -> Result<usize> {
    if align == 0 {
        return Err(Error::Alignment(AlignmentError::ZeroAlignment));
    }
    let rem = value % align;
    if rem == 0 {
        Ok(value)
    } else {
        value
            .checked_add(align - rem)
            .ok_or(Error::InvariantViolation("align_up overflow"))
    }
}

pub fn align_down(value: usize, align: usize) -> Result<usize> {
    if align == 0 {
        return Err(Error::Alignment(AlignmentError::ZeroAlignment));
    }
    Ok(value - (value % align))
}

pub fn align_to_write_size(value: usize, write_size: usize) -> Result<usize> {
    align_up(value, write_size)
}

pub fn aligned_tail_size(len: usize, write_size: usize) -> Result<usize> {
    Ok(align_up(len, write_size)? - len)
}
