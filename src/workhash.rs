use bc4py_plotter::pochash::{HASH_LOOP_COUNT,HASH_LENGTH};
use blake2b_simd::blake2bp::blake2bp;
use blake2b_simd::Hash;
use bigint::U256;
use workerpool::Pool;
use workerpool::thunk::{Thunk,ThunkWorker};
use regex::Regex;
use std::path::Path;
use std::io::{Seek, SeekFrom, BufReader, Read};
use std::fs::{File, read_dir};
use std::mem::transmute;
use std::time::Instant;
use std::sync::mpsc::channel;

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

pub fn seek_file(path: &str, start: usize, end: usize, previous_hash: &[u8], target: &[u8], time: u32, now: Instant)
    -> Result<(u32, Vec<u8>), String> {
    // return (nonce, workHash)
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

pub fn seek_files(dir: &str, previous_hash: &[u8], target: &[u8],
                  time:u32, worker: usize) -> Result<(u32, Vec<u8>, String), String> {
    let now = Instant::now();
    let pool =
        Pool::<ThunkWorker<(Result<(u32, Vec<u8>), String>, String)>>::new(worker);
    let (tx, rx) = channel();
    let re = Regex::new("^optimized\\.([A-Z0-9]{40})\\-([0-9]+)\\-([0-9]+)\\.dat$").unwrap();

    let mut wait_count = 0;
    let paths = read_dir(dir).unwrap();
    for path in paths {
        let path = path.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap();
        match re.captures(name) {
            Some(c) => {
                if c.len() != 4 { continue }
                let address = c.get(1).unwrap().as_str().to_owned();
                let start: usize = c.get(2).unwrap().as_str().parse().unwrap();
                let end: usize = c.get(3).unwrap().as_str().parse().unwrap();
                let previous_hash = previous_hash.to_vec();
                let target = target.to_vec();
                let path = path.as_path().to_str().unwrap().to_owned();
                let now = now.clone();
                pool.execute_to(tx.clone(), Thunk::of(move || {
                    let previous_hash = previous_hash.as_slice();
                    let target = target.as_slice();
                    (seek_file(&path, start, end, previous_hash, target, time, now), address)
                }));
                wait_count += 1;
            },
            _ => ()
        }
    }

    for (result, address) in rx {
        wait_count -= 1;
        match result {
            Ok((nonce, workhash)) => return Ok((nonce, workhash, address)),
            _ => ()
        };
        if wait_count <= 0 {
            return Err(format!("full seeked but not found enough work {}mSec", now.elapsed().as_millis()));
        };
    }
    Err("out of loop, it's exception".to_owned())
}
