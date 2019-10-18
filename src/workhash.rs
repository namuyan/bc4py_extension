use crate::utils::work_check;
use bc4py_plotter::pochash::{HASH_LOOP_COUNT,HASH_LENGTH};
use blake2b_simd::blake2b;
use blake2b_simd::Hash;
use bigint::U256;
use threadpool::ThreadPool;
use regex::Regex;
use std::path::Path;
use std::io::{Seek, SeekFrom, BufReader, Read};
use std::fs::{File, read_dir};
use std::mem::transmute;
use std::time::Instant;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Sender, Receiver};

const SEEK_TIMEOUT: u128 = 1500;  // mSec


#[inline]
pub fn get_work_hash(time: u32, scope_hash: &[u8], previous_hash: &[u8]) -> Hash {
    // work = blake2b([blockTime 4bytes]-[scopeHash 32bytes]-[previousHash 32bytes])
    let mut v = Vec::with_capacity(4 + 32 + 4);
    let bytes: [u8; 4] = unsafe { transmute(time.to_le()) };
    v.extend_from_slice(&bytes);
    v.extend_from_slice(scope_hash);
    v.extend_from_slice(previous_hash);
    blake2b(&v)
}

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
    // seek single file with single thread
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

    // seek
    let mut scope_hash = [0u8;32];
    for nonce in start..end {
        match fs.read(&mut scope_hash){
            Ok(32) => {
                if nonce % 2000 == 0 && now.elapsed().as_millis() > SEEK_TIMEOUT {
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
    Err(format!("full seeked but not found enough work {}mSec", now.elapsed().as_millis()))
}

pub fn seek_thread(path: &str, start: usize, end: usize, previous_hash: &[u8], target: &[u8], time: u32, now: Instant, worker: usize)
                   -> Result<(u32, Vec<u8>), String> {
    // seek single file with multi threads
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

    // pool objects
    type ChannelType = Result<(u32, Vec<u8>), String>;
    let (tx, rx): (Sender<ChannelType>, Receiver<ChannelType>) = channel();
    let signal = Arc::new(Mutex::new(0));
    let pool = ThreadPool::new(worker);

    // throw tasks to seek
    let area_size = (end - start) / worker;
    let mut wait_count = 0;
    for i in 0..worker {
        let area_start = start + area_size * i;
        let area_end = start + area_size * (i + 1);
        let mut buffer = vec![0u8;area_size * 32];
        match fs.read(&mut buffer) {
            Ok(size) => {
                let now = now.clone();
                let previous_hash = previous_hash.to_vec();
                let target = target.to_vec();
                let tx: Sender<ChannelType> = tx.clone();
                let signal = signal.clone();
                pool.execute(move || {
                    for (pos, nonce) in (area_start..area_end).enumerate() {
                        if nonce % 2000 == 0 && now.elapsed().as_millis() > SEEK_TIMEOUT {
                            return tx.send(Err(format!("timeout on {} nonce checking", nonce))).unwrap();
                        }
                        if nonce % 2001 == 0 && *signal.lock().unwrap() != 0 {
                            return tx.send(Err("killed by signal".to_owned())).unwrap();
                        }
                        if size < pos * 32 + 32 {
                            return tx.send(Err(format!("out of {}b/{}b buffer", size, buffer.len()))).unwrap();
                        }
                        let scope_hash = &buffer[(pos * 32)..(pos * 32 + 32)];
                        let work = get_work_hash(time, scope_hash, &previous_hash);
                        let work = work.as_bytes();
                        let work = &work[..32];
                        if work_check(&work, &target) {
                            return tx.send(Ok((nonce as u32, work.to_vec()))).unwrap();
                        }
                    }
                    return tx.send(Err(format!("full seeked area {}-{} {}mSec",
                                       area_start, area_end, now.elapsed().as_millis()))).unwrap();
                });
                wait_count += 1;
            },
            Err(err) => return Err(err.to_string())
        }
    }

    let mut success: Option<ChannelType> = None;
    for result in rx.iter().take(wait_count) {
        if result.is_ok() {
            *signal.lock().unwrap() += 1;
            success = Some(result);
        } else if cfg!(debug_assertions) {
            eprintln!("debug: {}", result.err().unwrap());
        }
    }

    // send result
    match success {
        Some(data) => data,
        None => Err(format!("full seeked but not found enough work {}mSec", now.elapsed().as_millis()))
    }
}

pub fn seek_folder(dir: &str, previous_hash: &[u8], target: &[u8], time:u32, worker: usize)
                   -> Result<(u32, Vec<u8>, String), String> {
    let now = Instant::now();
    let re = Regex::new("^optimized\\.([a-z0-9]+)\\-([0-9]+)\\-([0-9]+)\\.dat$").unwrap();
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
                let path = path.as_path().to_str().unwrap().to_owned();
                let now = now.clone();
                match seek_thread(&path, start, end, previous_hash, target, time, now, worker) {
                    Ok((nonce, workhash)) => return Ok((nonce, workhash, address)),
                    Err(err) => {
                        if cfg!(debug_assertions) {
                            eprintln!("debug: {}", err);
                        }
                    }
                }
            },
            _ => ()
        }
    }
    Err(format!("full seeked but not found enough work {}mSec", now.elapsed().as_millis()))
}
