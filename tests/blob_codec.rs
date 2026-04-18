use flashdb_for_rust::blob::{DecodeFromBytes, EncodeToBytes};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SamplePayload {
    kind: u16,
    reading: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SampleCodecError {
    BufferTooSmall,
    InvalidLength,
}

impl EncodeToBytes for SamplePayload {
    type Error = SampleCodecError;

    fn encoded_len(&self) -> usize {
        6
    }

    fn encode_to(&self, out: &mut [u8]) -> Result<usize, Self::Error> {
        if out.len() < self.encoded_len() {
            return Err(SampleCodecError::BufferTooSmall);
        }

        out[..2].copy_from_slice(&self.kind.to_le_bytes());
        out[2..6].copy_from_slice(&self.reading.to_le_bytes());
        Ok(self.encoded_len())
    }
}

impl DecodeFromBytes for SamplePayload {
    type Error = SampleCodecError;

    fn decode_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() != 6 {
            return Err(SampleCodecError::InvalidLength);
        }

        Ok(Self {
            kind: u16::from_le_bytes([bytes[0], bytes[1]]),
            reading: u32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]),
        })
    }
}

#[test]
fn codec_layer_keeps_typed_values_separate_from_raw_bytes() {
    let payload = SamplePayload {
        kind: 7,
        reading: 0x1122_3344,
    };
    let mut encoded = [0u8; 6];

    let written = payload.encode_to(&mut encoded).unwrap();
    let decoded = SamplePayload::decode_from(&encoded[..written]).unwrap();

    assert_eq!(written, 6);
    assert_eq!(encoded, [7, 0, 0x44, 0x33, 0x22, 0x11]);
    assert_eq!(decoded, payload);
}

#[test]
fn codec_layer_reports_encode_and_decode_contract_errors() {
    let payload = SamplePayload {
        kind: 1,
        reading: 2,
    };
    let mut short = [0u8; 5];

    assert_eq!(
        payload.encode_to(&mut short),
        Err(SampleCodecError::BufferTooSmall)
    );
    assert_eq!(
        SamplePayload::decode_from(&[1, 2, 3]),
        Err(SampleCodecError::InvalidLength)
    );
}
