use flashdb_for_embassy::layout::align::{
    align_down, align_to_write_size, align_up, aligned_tail_size,
};

#[test]
fn aligns_boundaries() {
    assert_eq!(align_up(0, 8).unwrap(), 0);
    assert_eq!(align_up(1, 8).unwrap(), 8);
    assert_eq!(align_up(9, 8).unwrap(), 16);
    assert_eq!(align_down(9, 8).unwrap(), 8);
    assert_eq!(align_to_write_size(63, 32).unwrap(), 64);
    assert_eq!(aligned_tail_size(65, 32).unwrap(), 31);
}
