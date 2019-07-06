use super::bc4py_plotter::pochash::{generator,HASH_LOOP_COUNT,HASH_LENGTH};
use super::bc4py_plotter::utils::*;
use crate::workhash::{get_work_hash, get_scope_index, seek_file, seek_files};
use crate::utils::{bytes_to_u32, u32_to_bytes, sha256double};
use blake2b_simd::blake2b;
use pyo3::prelude::*;
use pyo3::exceptions::ValueError;
use pyo3::types::{PyBytes,PyTuple,PyList};
use pyo3::wrap_pyfunction;
use std::time::Instant;


/// sha256d_hash(hash:bytes) -> bytes
/// --
///
/// generate sha256 double hash
#[pyfunction]
fn sha256d_hash(_py: Python<'_>, hash: &PyBytes) -> PyObject {
    let hash = sha256double(hash.as_bytes());
    PyBytes::new(_py, hash.as_slice()).to_object(_py)
}


/// merkleroot_hash(hashs:list) -> bytes
/// --
///
/// calculate merkleroot hash
#[pyfunction]
fn merkleroot_hash(_py: Python<'_>, hashs: &PyList) -> PyResult<PyObject> {
    let mut hashs: Vec<Vec<u8>> = hashs.extract()?;
    while 1 < hashs.len() {
        if hashs.len() % 2 == 0 {
            let mut new_hashs = Vec::with_capacity(hashs.len()/2);
            for i in 0..(hashs.len() / 2) {
                let mut data = Vec::with_capacity(64);
                data.extend_from_slice(&hashs[i*2]);
                data.extend_from_slice(&hashs[i*2+1]);
                new_hashs.push(sha256double(data.as_slice()));
            }
            hashs = new_hashs;
        } else {
            let last = match hashs.last() {
                Some(hash) => hash.clone(),
                None => return Err(ValueError::py_err("hashs length may be zero"))
            };
            hashs.push(last);
        }
    }
    let hash = match hashs.last() {
        Some(hash) => hash,
        None => return Err(ValueError::py_err("hashs length may be zero"))
    };
    Ok(PyBytes::new(_py, hash.as_slice()).to_object(_py))
}


/// blake2b_hash(hash:bytes) -> bytes
/// --
///
/// generate blake2b hash
#[pyfunction]
fn blake2b_hash(_py: Python<'_>, hash: &PyBytes) -> PyObject {
    let hash = blake2b(hash.as_bytes());
    let hash = hash.as_bytes();
    PyBytes::new(_py, &hash[0..32]).to_object(_py)
}


/// scope_index(previous_hash:bytes) -> int
/// --
///
/// get scope index from previous hash
/// index = (previous_hash to little endian 32bytes int) % scope_length
#[pyfunction]
fn scope_index(previous_hash: &PyBytes) -> u32 {
    let previous_hash = previous_hash.as_bytes();
    get_scope_index(previous_hash)
}


/// poc_hash(address:str, nonce:bytes) -> bytes
/// --
///
/// generate poc(proof of capacity) hash
/// return 524k bytes
#[pyfunction]
fn poc_hash(_py: Python<'_>, address: &str, nonce: &PyBytes) -> PyResult<PyObject> {
    let nonce = nonce.as_bytes();
    let ver_identifier = match addr2ver_identifier(address) {
        Ok(ver_identifier) => ver_identifier,
        Err(err) => return Err(ValueError::py_err(err))
    };
    let hash = _py.allow_threads(move || {
        let mut output =  Box::new([0u8;HASH_LOOP_COUNT*HASH_LENGTH]);
        generator(&ver_identifier, bytes_to_u32(nonce), &mut output);
        output.to_vec()
    });
    Ok(PyBytes::new(_py, hash.as_slice()).to_object(_py))
}


/// poc_work(time:int, scope_hash:bytes, previous_hash:bytes) -> bytes
/// --
///
/// generate poc work hash
#[pyfunction]
fn poc_work(_py: Python<'_>, time: u32, scope_hash: &PyBytes, previous_hash: &PyBytes)
    -> PyObject {
    let scope_hash = scope_hash.as_bytes();
    let previous_hash = previous_hash.as_bytes();
    let work = _py.allow_threads(move || {
        get_work_hash(time, scope_hash, previous_hash)
    });
    let work = work.as_bytes();
    let work = &work[..32];
    PyBytes::new(_py, work).to_object(_py)
}


/// single_seek(path:str, start:int, end:int, previous_hash:bytes, target:bytes, time:int) -> tuple
/// --
///
/// seek one optimized poc file
#[pyfunction]
fn single_seek(_py: Python<'_>, path: &str, start: usize, end: usize, previous_hash: &PyBytes, target: &PyBytes, time:u32)
    -> PyObject {
    let previous_hash = previous_hash.as_bytes();
    let target = target.as_bytes();
    let now = Instant::now();
    let result = _py.allow_threads(move || {
        seek_file(path, start, end, previous_hash, target, time, now)
    });
    match result {
        Ok((nonce, workhash)) => {
            PyTuple::new(_py, &[
                PyBytes::new(_py,&u32_to_bytes(nonce)).to_object(_py),
                PyBytes::new(_py, &workhash).to_object(_py)
            ]).to_object(_py)
        },
        Err(err) => PyTuple::new(_py, &[
            _py.None().to_object(_py),
            err.to_object(_py)
        ]).to_object(_py)
    }
}


/// multi_seek(dir:str, previous_hash:bytes, target:bytes, time:int, worker:int) -> tuple
/// --
///
/// seek optimized files from directory
#[pyfunction]
fn multi_seek(_py: Python<'_>, dir: &str, previous_hash: &PyBytes, target: &PyBytes, time:u32, worker: usize)
    -> PyObject {
    let previous_hash = previous_hash.as_bytes();
    let target = target.as_bytes();
    let result = _py.allow_threads(move || {
        seek_files(dir, previous_hash, target, time, worker)
    });
    match result {
        Ok((nonce, workhash, address)) => PyTuple::new(_py, &[
                PyBytes::new(_py,&u32_to_bytes(nonce)).to_object(_py),
                PyBytes::new(_py, workhash.as_slice()).to_object(_py),
                address.to_object(_py)
            ]).to_object(_py),
        Err(err) => PyTuple::new(_py, &[
            _py.None().to_object(_py),
            _py.None().to_object(_py),
            err.to_object(_py).to_object(_py)
        ]).to_object(_py)
    }
}


/// bech2address(hrp:str, ver:int, identifier:bytes) -> str
/// --
///
/// get bech32 address from params(hrp, version, identifier)
#[pyfunction]
fn bech2address(_py: Python<'_>, hrp: &str, ver: u8, identifier: &PyBytes)
    -> PyResult<PyObject> {
    // (hrp, ver, identifier) -> bech address
    let identifier = identifier.as_bytes();
    let bech = match params2bech(hrp, ver, identifier) {
        Ok(bech) => bech,
        Err(err) => return Err(ValueError::py_err(err.to_string()))
    };
    Ok(bech.to_string().to_object(_py))
}


/// address2bech(addr:str) -> tuple
/// --
///
/// get params(hrp, version, identifier) from address
#[pyfunction]
fn address2bech(_py: Python<'_>, addr: &str) -> PyResult<PyObject> {
    // bech address -> (hrp, ver, identifier)
    let (hrp, ver, identifier) = match addr2params(addr) {
        Ok((hrp, ver, identifier)) => (hrp, ver, identifier),
        Err(err) => return Err(ValueError::py_err(err.to_string()))
    };
    Ok(PyTuple::new(_py,&[
        hrp.to_object(_py),
        ver.to_object(_py),
        PyBytes::new(_py, &identifier).to_object(_py)
    ]).to_object(_py))
}


/// This module is a python module implemented in Rust.
#[pymodule]
fn bc4py_extension(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_wrapped(wrap_pyfunction!(sha256d_hash))?;
    m.add_wrapped(wrap_pyfunction!(merkleroot_hash))?;
    m.add_wrapped(wrap_pyfunction!(blake2b_hash))?;
    m.add_wrapped(wrap_pyfunction!(scope_index))?;
    m.add_wrapped(wrap_pyfunction!(poc_hash))?;
    m.add_wrapped(wrap_pyfunction!(poc_work))?;
    m.add_wrapped(wrap_pyfunction!(single_seek))?;
    m.add_wrapped(wrap_pyfunction!(multi_seek))?;
    m.add_wrapped(wrap_pyfunction!(bech2address))?;
    m.add_wrapped(wrap_pyfunction!(address2bech))?;
    Ok(())
}
