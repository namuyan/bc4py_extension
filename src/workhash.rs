use bc4py_plotter::pochash::{HASH_LOOP_COUNT,HASH_LENGTH};
use blake2b_simd::blake2bp::blake2bp;
use blake2b_simd::Hash;
use bigint::U256;
use std::path::Path;
use std::io::{Seek, SeekFrom, BufReader, Read};
use std::fs::File;
use std::mem::transmute;
use std::time::Instant;

const SEEK_TIMEOUT: u64 = 5;

#[inline]
fn work_check(work: &[u8], target: &[u8]) -> bool {
    // hash < target => true
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
pub fn get_work_hash(time: u32, scope_hash: &[u8], previous_hash: &[u8]) -> Hash {
    // work = blake2bp([blockTime 4bytes]-[scopeHash 32bytes]-[previousHash 32bytes])
    let mut v = Vec::with_capacity(4 + 32 + 4);
    let bytes: [u8; 4] = unsafe { transmute(time.to_le()) };
    v.extend_from_slice(&bytes);
    v.extend_from_slice(scope_hash);
    v.extend_from_slice(previous_hash);
    blake2bp(&v)
}

#[inline]
pub fn get_scope_index(previous_hash: &[u8]) -> u32 {
    // index = (previous_hash to little endian 32bytes int) % scope_length
    let mut previous_hash = previous_hash.to_owned();
    previous_hash.reverse();
    let val: U256 = previous_hash.as_slice().into();
    let div: U256 = (HASH_LOOP_COUNT * HASH_LENGTH / 32).into();
    let index = val % div;
    index.into()
}

pub fn seek_file(path: &str, start: usize, end: usize, previous_hash: &[u8], target: &[u8], time: u32)
    -> Result<(u32, Vec<u8>), String> {
    // return (nonce, workHash)
    let now = Instant::now();
    let path = Path::new(path);
    if !path.exists() {
        return Err(String::from(format!("not found file \"{}\"", path.display())));
    }

    // get file object
    let fs = File::open(path).map_err(|err| return err.to_string())?;
    let mut fs = BufReader::new(fs);
    let scope_index = get_scope_index(previous_hash) as usize;
    let start_pos = (scope_index * 32 * (end - start)) as u64;
    fs.seek(SeekFrom::Start(start_pos)).map_err(|err| return err.to_string())?;
    let mut scope_hash = [0u8;32];
    for nonce in start..end {
        match fs.read(&mut scope_hash){
            Ok(32) => {
                if nonce % 2000 == 0 && now.elapsed().as_secs() > SEEK_TIMEOUT {
                    return Err(String::from(format!("timeout on {} nonce checking", nonce)));
                }
                let work = get_work_hash(time, &scope_hash, previous_hash);
                let work = work.as_bytes();
                let work = &work[..32];
                if work_check(&work, target) {
                    return Ok((nonce as u32, work.to_vec()));
                }
            },
            Ok(size) => return Err(format!("not correct read size \"{}\"bytes", size)),
            Err(err) => return Err(err.to_string())
        }
    }
    Err(String::from("full seeked but not found enough work"))
}
