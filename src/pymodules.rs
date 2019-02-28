use super::bc4py_plotter::pochash::generator;
use crate::workhash::{get_work_hash, get_scope_index, seek_file, seek_files};
use blake2b_simd::blake2bp::blake2bp;
use pyo3::prelude::{Py,PyResult,PyObject,Python,PyModule,ToPyObject,pyfunction,pymodule};
use pyo3::types::{PyBytes,PyTuple};
use pyo3::wrap_pyfunction;
use std::mem::transmute;

#[inline]
fn u32_to_bytes(i: u32) -> [u8;4] {
    unsafe { transmute(i.to_le()) }
}

#[inline]
fn bytes_to_u32(bytes: &[u8]) -> u32 {
    let mut tmp= [0u8;4];
    for (a, b) in tmp.iter_mut().zip(bytes.iter()) {
        *a = *b
    }
    unsafe {transmute::<[u8; 4], u32>(tmp)}
}

#[pyfunction]
fn blake2bp_hash(_py: Python<'_>, hash: &PyBytes) -> Py<PyBytes> {
    let hash = blake2bp(hash.as_bytes());
    let hash = hash.as_bytes();
    PyBytes::new(_py, &hash[0..32])
}

#[pyfunction]
fn scope_index(previous_hash: &PyBytes) -> u32{
    let previous_hash = previous_hash.as_bytes();
    get_scope_index(previous_hash)
}

#[pyfunction]
fn poc_hash(_py: Python<'_>, address: &str, nonce: &PyBytes) -> Py<PyBytes> {
    let nonce = nonce.as_bytes();
    let b = generator(address, bytes_to_u32(nonce));
    PyBytes::new(_py, &b[..])
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
    return match seek_file(path, start, end, previous_hash, target, time) {
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

/// This module is a python module implemented in Rust.
#[pymodule]
fn bc4py_extension(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_wrapped(wrap_pyfunction!(blake2bp_hash))?;
    m.add_wrapped(wrap_pyfunction!(scope_index))?;
    m.add_wrapped(wrap_pyfunction!(poc_hash))?;
    m.add_wrapped(wrap_pyfunction!(poc_work))?;
    m.add_wrapped(wrap_pyfunction!(single_seek))?;
    m.add_wrapped(wrap_pyfunction!(multi_seek))?;
    Ok(())
}