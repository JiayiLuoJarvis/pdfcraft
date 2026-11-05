//! SFNT checksum. A leaf so `font` and `subset` do not call each other.

pub(crate) fn checksum(data: &[u8]) -> u32 {
    let mut sum = 0_u32;
    let mut index = 0;
    while index + 4 <= data.len() {
        sum = sum.wrapping_add(u32be(&data[index..index + 4]));
        index += 4;
    }
    if index < data.len() {
        let mut tail = [0_u8; 4];
        tail[..data.len() - index].copy_from_slice(&data[index..]);
        sum = sum.wrapping_add(u32::from_be_bytes(tail));
    }
    sum
}

fn u32be(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}
