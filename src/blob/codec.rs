pub trait EncodeToBytes {
    type Error;

    fn encoded_len(&self) -> usize;
    fn encode_to(&self, out: &mut [u8]) -> Result<usize, Self::Error>;
}

pub trait DecodeFromBytes: Sized {
    type Error;

    fn decode_from(bytes: &[u8]) -> Result<Self, Self::Error>;
}

pub trait BlobCodec:
    EncodeToBytes + DecodeFromBytes<Error = <Self as EncodeToBytes>::Error>
{
}

impl<T> BlobCodec for T where T: EncodeToBytes + DecodeFromBytes<Error = <T as EncodeToBytes>::Error>
{}
