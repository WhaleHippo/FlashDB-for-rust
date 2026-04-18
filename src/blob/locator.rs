#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlobLocator {
    offset: u32,
    len: u32,
}

impl BlobLocator {
    pub const fn new(offset: u32, len: u32) -> Self {
        Self { offset, len }
    }

    pub const fn offset(&self) -> u32 {
        self.offset
    }

    pub const fn len(&self) -> u32 {
        self.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}
