use std::mem::transmute;


#[inline]
pub fn u32_to_bytes(i: u32) -> [u8;4] {
    unsafe { transmute(i.to_le()) }
}

#[inline]
pub fn bytes_to_u32(bytes: &[u8]) -> u32 {
    let mut tmp= [0u8;4];
    for (a, b) in tmp.iter_mut().zip(bytes.iter()) {
        *a = *b
    }
    unsafe {transmute::<[u8; 4], u32>(tmp)}
}
