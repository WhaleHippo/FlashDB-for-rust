use core::fmt::Debug;

pub type Result<T, E = core::convert::Infallible> = core::result::Result<T, Error<E>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    BufferTooShort,
    InvalidMagic,
    InvalidLength,
    InvalidState,
    InvalidValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignmentError {
    ZeroAlignment,
    UnalignedValue { value: u32, align: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error<E = core::convert::Infallible> {
    Storage(E),
    Decode(DecodeError),
    Alignment(AlignmentError),
    OutOfBounds,
    CorruptedHeader,
    CrcMismatch,
    NoSpace,
    BufferTooSmall { needed: usize, actual: usize },
    InvalidBlobOffset { offset: u32, len: u32 },
    UnsupportedFormatVersion(u16),
    InvariantViolation(&'static str),
    TimestampNotMonotonic { last: u64, next: u64 },
}

impl<E> From<DecodeError> for Error<E>
where
    E: Debug,
{
    fn from(value: DecodeError) -> Self {
        Self::Decode(value)
    }
}

impl<E> From<AlignmentError> for Error<E>
where
    E: Debug,
{
    fn from(value: AlignmentError) -> Self {
        Self::Alignment(value)
    }
}
