use pyo3::prelude::*;
use pyo3::exceptions::AssertionError;
use pyo3::PyIterProtocol;


#[pyclass]
pub struct PyIter {
    finish: bool,
    reader: Vec<PyObject>,
    reversed: bool,
}

#[pyproto]
impl<T> PyIterProtocol for PyIter {
    fn __iter__(slf: PyRefMut<Self>) -> PyResult<PyObject> {
        let py = unsafe { Python::assume_gil_acquired() };
        Ok(slf.into_py(py))
    }

    fn __next__(mut slf: PyRefMut<Self>) -> PyResult<Option<PyObject>> {
        if slf.finish {
            Err(AssertionError::py_err("iter is already used"))
        } else {
            if slf.reader.len() == 0 {
                slf.finish = true;
                Ok(None)
            } else if slf.reversed {
                Ok(slf.reader.pop())  // get from last (reversed)
            } else {
                Ok(Some(slf.reader.remove(0)))  // get from top
            }
        }
    }
}

impl PyIter {
    pub fn new(reader: Vec<PyObject>, reversed: bool) -> Self {
        PyIter {finish: false, reader, reversed }
    }
}
