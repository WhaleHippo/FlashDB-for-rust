#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlobLocator {
    pub offset: u32,
    pub len: u32,
}
