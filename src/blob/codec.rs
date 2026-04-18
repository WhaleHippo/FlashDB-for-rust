pub trait BlobCodec<T> {
    type Error;

    fn encode(value: &T, out: &mut [u8]) -> Result<usize, Self::Error>;
    fn decode(bytes: &[u8]) -> Result<T, Self::Error>;
}
