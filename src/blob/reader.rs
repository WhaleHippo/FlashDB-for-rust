use crate::blob::locator::BlobLocator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlobRef<'a> {
    pub locator: BlobLocator,
    pub bytes: &'a [u8],
}
