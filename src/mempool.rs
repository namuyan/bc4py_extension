use crate::pyiter::PyIter;
use pyo3::prelude::*;
use pyo3::exceptions::AssertionError;
use pyo3::types::{PyBytes, PyTuple};
use pyo3::PyObjectProtocol;
use std::cmp::PartialEq;
use bigint::U256;


// NO COPY TRAIT!
struct Unconfirmed {
    // store TX object
    obj: PyObject,
    // for find priority
    hash: U256,
    depends: Vec<U256>,
    price: u32,
    time: u32,
    deadline: u32,
    size: u32,
}

impl Unconfirmed {
    pub fn bytes(&self, py: Python) -> PyObject {
        let mut slice = [0u8;32];
        self.hash.to_big_endian(&mut slice);
        PyBytes::new(py, &slice).to_object(py)
    }
}

impl PartialEq for Unconfirmed {
    fn eq(&self, other: &Unconfirmed) -> bool {
        self.hash == other.hash
    }
}

#[pyclass]
pub struct MemoryPool {
    // pre-ordered high to low
    unconfirmed: Vec<Unconfirmed>,
}

#[pyproto]
impl PyObjectProtocol for MemoryPool {
    fn __repr__(&self) -> PyResult<String> {
        Ok(format!("<MemPool len={}>", self.unconfirmed.len()))
    }
}

#[pymethods]
impl MemoryPool {
    #[new]
    fn new() -> Self {
        MemoryPool {
            unconfirmed: Vec::new(),
        }
    }


    /// get_obj(hash: int) -> TX
    /// --
    ///
    /// get TX object by hash
    fn get_obj(&self, py: Python, hash: &PyBytes) -> Option<PyObject> {
        let hash = U256::from(hash.as_bytes());
        let index = self.unconfirmed.iter()
            .position(|tx| hash == tx.hash)?;
        self.unconfirmed.get(index).map(|tx| tx.obj.clone_ref(py))
    }

    /// exist(hash: bytes) -> bool
    /// --
    ///
    /// check hash is exist
    fn exist(&self, hash: &PyBytes) -> bool {
        let hash = U256::from(hash.as_bytes());
        self.unconfirmed.iter()
            .position(|tx| hash == tx.hash)
            .is_some()
    }

    /// length() -> int
    /// --
    ///
    /// return unconfirmed tx's length
    fn length(&self) -> usize {
        self.unconfirmed.len()
    }

    /// position(hash: bytes) -> int
    /// --
    ///
    /// the unconfirmed tx's position (this means tx's priority)
    fn position(&self, hash: &PyBytes) -> Option<usize> {
        let hash = U256::from(hash.as_bytes());
        self.unconfirmed.iter()
            .position(|tx| hash == tx.hash)
    }

    /// push(obj: TX, hash: bytes, depends: list, price: int, time: int, deadline: int, size: int) -> int
    /// --
    ///
    /// push new unconfirmed tx, return inserted index
    fn push(&mut self, obj: &PyAny, hash: &PyBytes, depends: Vec<&PyBytes>, price: u32, time: u32, deadline: u32, size: u32)
        -> PyResult<usize> {
        let hash = U256::from(hash.as_bytes());
        let mut depends: Vec<U256> = depends.iter()
            .map(|hash| U256::from(hash.as_bytes())).collect();

        // remove duplicate depends
        depends.sort_unstable();
        depends.dedup();

        // most high position depend index
        let mut depend_index: Option<usize> = None;
        for (index, tx) in self.unconfirmed.iter().enumerate() {
            if depends.contains(&tx.hash) {
                depend_index = Some(index);
            }
            if hash == tx.hash {
                return Err(AssertionError::py_err("already inserted tx"));
            }
        }

        // most low position required index
        let mut required_index = None;
        for (index, tx) in self.unconfirmed.iter().enumerate().rev() {
            if tx.depends.contains(&hash) {
                required_index = Some(index);
            }
        }

        // find best relative condition
        let mut best_index: Option<usize> = None;
        for (index, tx) in self.unconfirmed.iter().enumerate() {
            // absolute conditions
            // ex
            //        0 1 2 3 4 5
            // vec = [a,b,c,d,e,f]
            //
            // You can insert positions(2,3,4) when you depends on b(1) and required by e(4)
            if depend_index.is_some() && index <= depend_index.unwrap() {
                continue;
            }
            if required_index.is_some() && index > required_index.unwrap() {
                continue;
            }

            // relative conditions
            if price < tx.price {
                continue;
            } else if price == tx.price {
                if time >= tx.time {
                    continue;
                }
            }
            // find
            if best_index.is_none() {
                best_index = Some(index);
                break;
            }
        }

        // minimum index is required_index
        if best_index.is_none() {
            best_index = required_index.clone();
        }

        // generate tx object
        let tx = {
            let gil = Python::acquire_gil();
            let obj = obj.to_object(gil.python());
            Unconfirmed {obj, hash, depends, price, time, deadline, size}
        };

        // insert
        match best_index {
            Some(best_index) => {
                // println!("best {} {:?} {:?}", best_index, depend_index, required_index);
                self.unconfirmed.insert(best_index, tx);
                Ok(best_index)
            },
            None => {
                // println!("last {:?} {:?}", depend_index, required_index);
                self.unconfirmed.push(tx);
                Ok(self.unconfirmed.len())
            },
        }
    }

    /// remove(hash: bytes) -> None
    /// --
    ///
    /// simple remove unconfirmed tx
    fn remove(&mut self, hash: &PyBytes) {
        let hash = U256::from(hash.as_bytes());
        self.unconfirmed.drain_filter(|tx| hash == tx.hash).for_each(drop);
    }

    /// remove_many(hashs: list) -> None
    /// --
    ///
    /// simple remove unconfirmed txs
    fn remove_many(&mut self, hashs: Vec<&PyBytes>) -> PyResult<()> {
        let hashs: Vec<U256> = hashs
            .iter().map(|hash| U256::from(hash.as_bytes())).collect();
        self.unconfirmed
            .drain_filter(|tx| hashs.contains(&tx.hash))
            .for_each(drop);
        Ok(())
    }

    /// remove_with_depends(hash: bytes) -> None
    /// --
    ///
    /// remove unconfirmed tx with depends
    fn remove_with_depends(&mut self, hash: &PyBytes) -> PyResult<usize> {
        let hash = U256::from(hash.as_bytes());
        self.remove_with_depend_myself(&hash)
            .map_err(|_| AssertionError::py_err("not found hash"))
    }

    /// list_size_limit(maxsize: int) -> Tuple[TX]
    /// --
    ///
    /// size limit unconfirmed tx's tuple for mining interface
    fn list_size_limit(&self, py: Python, maxsize: u32) -> PyObject {
        // unconfirmed is already sorted by priority
        let mut size = 0;
        let reader: Vec<PyObject> = self.unconfirmed.iter()
            .filter(|tx| {
                size += tx.size;
                size < maxsize
            })
            .map(|tx| tx.obj.clone_ref(py))
            .collect();
        PyTuple::new(py, &reader).to_object(py)
    }

    /// list_all_hash() -> Tuple[bytes]
    /// --
    ///
    /// all unconfirmed tx's hash tuple
    fn list_all_hash(&self, py: Python) -> PyObject {
        let outputs: Vec<PyObject> = self.unconfirmed
            .iter()
            .map(|tx| tx.bytes(py))
            .collect();
        PyTuple::new(py, &outputs).to_object(py)
    }

    /// list_all_obj(reversed: bool) -> Iterator[TX]
    /// --
    ///
    /// all unconfirmed tx's obj tuple
    fn list_all_obj(&self, py: Python, reversed: bool) -> PyIter {
        let reader: Vec<PyObject> = self.unconfirmed
            .iter()
            .map(|tx| tx.obj.clone_ref(py))
            .collect();
        PyIter::new(reader, reversed)
    }

    /// clear_all() -> None
    /// --
    ///
    /// clear all unconfirmed txs
    fn clear_all(&mut self) {
        self.unconfirmed.drain(..)
            .map(|tx| tx.obj)
            .for_each(drop);
        assert_eq!(self.unconfirmed.len(), 0);
    }

    /// clear_by_deadline(deadline: int) -> int
    /// --
    ///
    /// remove expired unconfirmed txs
    fn clear_by_deadline(&mut self, deadline: u32) -> usize {
        // remove too old tx with depends
        let mut count = 0;
        loop {
            let mut want_delete = None;
            for tx in self.unconfirmed.iter() {
                if tx.deadline < deadline {
                    want_delete = Some(tx.hash.clone());
                    break;
                }
            }
            count += match want_delete {
                Some(hash) => self.remove_with_depend_myself(&hash).unwrap_or(0),
                None => break,
            };
        }
        count
    }
}


impl MemoryPool {
    // use this method when tx is expired
    fn remove_with_depend_myself(&mut self, hash: &U256) -> Result<usize, ()> {
        // find position
        let delete_index = match self.unconfirmed.iter()
            .position(|tx| hash == &tx.hash) {
            Some(index) => index,
            None => return Err(()),
        };

        // delete tx
        let mut delete_count = 1;
        let delete_tx = self.unconfirmed.remove(delete_index);

        // check depends
        loop {
            let mut delete_hash = None;
            for tx in self.unconfirmed.iter() {
                if tx.depends.contains(&delete_tx.hash) {
                    delete_hash = Some(tx.hash.clone());
                    break;
                }
            }
            delete_count += match delete_hash {
                Some(hash) => self.remove_with_depend_myself(&hash).unwrap_or(0),
                None => break,
            };
        }
        Ok(delete_count)
    }
}
