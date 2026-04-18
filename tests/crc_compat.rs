use flashdb_for_rust::crc::{crc_chain, crc_with_ff_padding, crc32};

#[test]
fn crc32_known_vector_matches_standard() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
}

#[test]
fn chained_crc_matches_single_pass() {
    assert_eq!(crc_chain(&[b"1234", b"56789"]), crc32(b"123456789"));
}

#[test]
fn ff_padding_changes_digest_deterministically() {
    assert_eq!(
        crc_with_ff_padding(b"abc", 4),
        crc32(&[b'a', b'b', b'c', 0xFF])
    );
}
