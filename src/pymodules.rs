use super::bc4py_plotter::pochash::{generator,HASH_LOOP_COUNT,HASH_LENGTH};
use super::bc4py_plotter::utils::*;
use crate::workhash::{get_work_hash, get_scope_index, seek_file, seek_files};
use crate::utils::{bytes_to_u32,u32_to_bytes};
use blake2b_simd::blake2b;
use pyo3::prelude::{Py,PyResult,PyObject,Python,PyModule,ToPyObject,pyfunction,pymodule};
use pyo3::exceptions::ValueError;
use pyo3::types::{PyBytes,PyTuple};
use pyo3::wrap_pyfunction;
use std::time::Instant;


#[pyfunction]
fn blake2b_hash(_py: Python<'_>, hash: &PyBytes) -> Py<PyBytes> {
    let hash = blake2b(hash.as_bytes());
    let hash = hash.as_bytes();
    PyBytes::new(_py, &hash[0..32])
}

#[pyfunction]
fn scope_index(previous_hash: &PyBytes) -> u32{
    let previous_hash = previous_hash.as_bytes();
    get_scope_index(previous_hash)
}

#[pyfunction]
fn poc_hash(_py: Python<'_>, address: &str, nonce: &PyBytes) -> PyResult<PyObject> {
    let nonce = nonce.as_bytes();
    let ver_identifier = match addr2ver_identifier(address) {
        Ok(ver_identifier) => ver_identifier,
        Err(err) => return Err(ValueError::py_err(err))
    };
    let mut output =  Box::new([0u8;HASH_LOOP_COUNT*HASH_LENGTH]);
    generator(&ver_identifier, bytes_to_u32(nonce), &mut output);
    Ok(PyBytes::new(_py, &output.to_vec()).to_object(_py))
}

#[pyfunction]
fn poc_work(_py: Python<'_>, time: u32, scope_hash: &PyBytes, previous_hash: &PyBytes) -> Py<PyBytes> {
    let scope_hash = scope_hash.as_bytes();
    let previous_hash = previous_hash.as_bytes();
    let work = get_work_hash(time, scope_hash, previous_hash);
    let work = work.as_bytes();
    let work = &work[..32];
    PyBytes::new(_py, work)
}

#[pyfunction]
fn single_seek(_py: Python<'_>, path: &str, start: usize, end: usize,
              previous_hash: &PyBytes, target: &PyBytes, time:u32) -> Py<PyTuple> {
    let previous_hash = previous_hash.as_bytes();
    let target = target.as_bytes();
    let now = Instant::now();
    return match seek_file(path, start, end, previous_hash, target, time, now) {
        Ok((nonce, workhash)) => {
            PyTuple::new(_py, &[
                PyObject::from(PyBytes::new(_py,&u32_to_bytes(nonce))),
                PyObject::from(PyBytes::new(_py, &workhash))
            ])
        },
        Err(err) => PyTuple::new(_py, &[
            PyObject::from(_py.None()),
            PyObject::from(err.to_object(_py))
        ])
    };
}

#[pyfunction]
fn multi_seek(_py: Python<'_>, dir: &str, previous_hash: &PyBytes,
              target: &PyBytes, time:u32, worker: usize) -> Py<PyTuple> {
    let previous_hash = previous_hash.as_bytes();
    let target = target.as_bytes();
    match seek_files(dir, previous_hash, target, time, worker) {
        Ok((nonce, workhash, address)) => PyTuple::new(_py, &[
                PyObject::from(PyBytes::new(_py,&u32_to_bytes(nonce))),
                PyObject::from(PyBytes::new(_py, workhash.as_slice())),
                PyObject::from(address.to_object(_py))
            ]),
        Err(err) => PyTuple::new(_py, &[
            PyObject::from(_py.None()),
            PyObject::from(_py.None()),
            PyObject::from(err.to_object(_py))
        ])
    }
}

#[pyfunction]
fn bech2address(_py: Python<'_>, hrp: &str, ver: u8, identifier: &PyBytes) -> PyResult<PyObject> {
    // (hrp, ver, identifier) -> bech address
    let identifier = identifier.as_bytes();
    let bech = match params2bech(hrp, ver, identifier) {
        Ok(bech) => bech,
        Err(err) => return Err(ValueError::py_err(err.to_string()))
    };
    println!("no bech={}", bech);
    Ok(bech.to_string().to_object(_py))
}

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
