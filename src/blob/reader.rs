use crate::blob::locator::BlobLocator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlobRef<'a> {
    locator: BlobLocator,
    bytes: &'a [u8],
}

impl<'a> BlobRef<'a> {
    pub const fn new(locator: BlobLocator, bytes: &'a [u8]) -> Self {
        Self { locator, bytes }
    }

    pub const fn locator(&self) -> BlobLocator {
        self.locator
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub const fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct BlobBuf<'a> {
    locator: BlobLocator,
    bytes: &'a mut [u8],
}

impl<'a> BlobBuf<'a> {
    pub fn new(locator: BlobLocator, bytes: &'a mut [u8]) -> Self {
        Self { locator, bytes }
    }

    pub const fn locator(&self) -> BlobLocator {
        self.locator
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.bytes
    }

    pub fn as_mut_bytes(&mut self) -> &mut [u8] {
        self.bytes
    }
}
