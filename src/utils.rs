use std::mem::transmute;
use sha2::{Sha256, Digest};
use std::convert::TryFrom;


const MAX_POINTER_INT: u64 = usize::max_value() as u64;


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

#[inline]
pub fn sha256double(b: &[u8]) -> Vec<u8> {
    let hash = Sha256::digest(b);
    let hash = Sha256::digest(hash.as_slice());
    hash.to_vec()
}

#[inline]
pub fn work_check(work: &[u8], target: &[u8]) -> bool {
    // "hash < target" => true
    debug_assert_eq!(work.len(), target.len());
    for (work, target) in work.iter().rev().zip(target.iter().rev()) {
        if work > target {
            return false;
        } else if work < target {
            return true;
        }
    }
    false
}

#[inline]
pub fn python_hash(i: u64) -> isize {
    // for __hash__
    let h = (i % MAX_POINTER_INT) as i64 - (MAX_POINTER_INT / 2) as i64;
    isize::try_from(h).unwrap()
}
