use bc4py_plotter::utils::params2bech;
use bech32::{Bech32,convert_bits};
use pyo3::prelude::*;
use pyo3::exceptions::ValueError;
use pyo3::types::{PyBytes, PyType};
use pyo3::PyObjectProtocol;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::convert::TryFrom;
use std::str::FromStr;


#[pyclass]
pub struct PyAddress {
    bech: Bech32,
}

#[pyproto]
impl PyObjectProtocol for PyAddress {
    fn __repr__(&self) -> PyResult<String> {
        Ok(self.bech.to_string())
    }

    fn __hash__(&self) -> PyResult<isize> {
        // only check version + identifier
        let mut hasher = DefaultHasher::new();
        for i in self.bech.data().iter() {
            hasher.write_u8(i.to_u8());
        }
        let max = usize::max_value() as u64;
        let h = (hasher.finish() % max) as i64 - (max / 2) as i64;
        isize::try_from(h).map_err(
            |_| ValueError::py_err("failed __hash__ generate"))
    }
}

#[pymethods]
impl PyAddress {
    /// from_string(address: str) -> Address
    /// --
    ///
    /// generate Address obj from string
    #[classmethod]
    fn from_string(_cls: &PyType, address: &str) -> PyResult<PyAddress> {
        let bech = Bech32::from_str(address)
            .map_err(|err| ValueError::py_err(err.to_string()))?;
        Ok(PyAddress{bech})
    }

    /// from_binary(hrp: str, data: bytes) -> Address
    /// --
    ///
    /// generate Address obj from 3 param
    #[classmethod]
    fn from_binary(_cls: &PyType, hrp: &str, data: &PyBytes) -> PyResult<PyAddress> {
        let data = data.as_bytes();
        if data.len() != 21 {
            return Err(ValueError::py_err("data is 21 bytes"));
        }
        let bech = params2bech(hrp, data[0], &data[1..])
            .map_err(|err| ValueError::py_err(err.to_string()))?;
        Ok(PyAddress{bech})
    }

    /// from_param(hrp: str, ver: int, identifier: bytes) -> Address
    /// --
    ///
    /// generate Address obj from 3 param
    #[classmethod]
    fn from_param(_cls: &PyType, hrp: &str, ver: u8, identifier: &PyBytes) -> PyResult<PyAddress> {
        let identifier = identifier.as_bytes();
        if identifier.len() != 20 {
            return Err(ValueError::py_err("identifier is 20 bytes"));
        }
        let bech = params2bech(hrp, ver, identifier)
            .map_err(|err| ValueError::py_err(err.to_string()))?;
        Ok(PyAddress{bech})
    }

    #[getter]
    fn hrp(&self) -> String {
        self.bech.hrp().to_string()
    }

    #[getter]
    fn version(&self) -> u8 {
        self.bech.data()[0].to_u8()
    }

    #[getter]
    fn address(&self) -> String {
        self.bech.to_string()
    }

    /// identifier() -> bytes
    /// --
    ///
    /// return 20bytes identifier
    fn identifier(&self, py: Python) -> PyResult<PyObject> {
        let identifier = convert_bits(&self.bech.data()[1..], 5, 8, false)
            .map_err(|err| ValueError::py_err(err.to_string()))?;
        Ok(PyBytes::new(py, &identifier).to_object(py))
    }

    /// binary() -> bytes
    /// --
    ///
    /// return 21bytes version + identifier
    fn binary(&self, py: Python) -> PyResult<PyObject> {
        let identifier = convert_bits(&self.bech.data()[1..], 5, 8, false)
            .map_err(|err| ValueError::py_err(err.to_string()))?;
        let mut bin = Vec::with_capacity(21);
        bin.push(self.bech.data()[0].to_u8());
        bin.extend_from_slice(identifier.as_slice());
        Ok(PyBytes::new(py, bin.as_slice()).to_object(py))
    }
}
