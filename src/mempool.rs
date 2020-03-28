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
    depends: Box<[U256]>,
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


    /// get_obj(hash: int) -> Optional[TX]
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
        // drop any excess capacity
        let depends = depends.into_boxed_slice();

        // generate tx object
        let unconfirmed = {
            let gil = Python::acquire_gil();
            let obj = obj.to_object(gil.python());
            Unconfirmed {obj, hash, depends, price, time, deadline, size}
        };

        // push
        self.push_unconfirmed(unconfirmed)
    }

    /// remove(hash: bytes) -> None
    /// --
    ///
    /// simple remove unconfirmed tx
    fn remove(&mut self, hash: &PyBytes) -> PyResult<()> {
        // require reorder after remove the hash
        let hash = U256::from(hash.as_bytes());
        let mut deleted = Vec::with_capacity(1);
        // remove all related txs
        self.remove_with_depend_myself(&hash, &mut deleted);
        if deleted.len() == 0 {
            return Err(AssertionError::py_err("not found hash"));
        }
        // remove root tx
        assert_eq!(hash, deleted.remove(0).hash);
        // insert all
        for tx in deleted {
            assert!(self.push_unconfirmed(tx).is_ok())
        }
        Ok(())
    }

    /// remove_many(hashs: list) -> None
    /// --
    ///
    /// simple remove unconfirmed txs (no error even if no delete tx)
    fn remove_many(&mut self, hashs: Vec<&PyBytes>) {
        //require reorder after remove the hash
        let hashs: Vec<U256> = hashs
            .iter().map(|hash| U256::from(hash.as_bytes())).collect();
        let mut deleted = Vec::with_capacity(hashs.len());
        // remove all related txs
        for hash in hashs.iter() {
            self.remove_with_depend_myself(hash, &mut deleted);
        }
        // remove root txs
        deleted.drain_filter(|_tx| hashs.contains(&_tx.hash)).for_each(drop);
        // insert all
        for tx in deleted {
            assert!(self.push_unconfirmed(tx).is_ok())
        }
    }

    /// remove_with_depends(hash: bytes) -> int
    /// --
    ///
    /// remove unconfirmed tx with depends and return delete count
    fn remove_with_depends(&mut self, hash: &PyBytes) -> PyResult<usize> {
        let hash = U256::from(hash.as_bytes());
        let mut deleted: Vec<Unconfirmed> = Vec::new();
        self.remove_with_depend_myself(&hash, &mut deleted);
        match deleted.len() {
            0 => Err(AssertionError::py_err("not found hash")),
            count => Ok(count),
        }
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

    /// clear_by_deadline(deadline: int) -> Tuple[TX]
    /// --
    ///
    /// remove expired unconfirmed txs
    fn clear_by_deadline(&mut self, py: Python, deadline: u32) -> PyObject {
        // remove too old tx with depends
        let mut deleted: Vec<Unconfirmed> = Vec::new();
        loop {
            let mut want_delete = None;
            for tx in self.unconfirmed.iter() {
                if tx.deadline < deadline {
                    want_delete = Some(tx.hash.clone());
                    break;
                }
            }
            match want_delete {
                Some(hash) => self.remove_with_depend_myself(&hash, &mut deleted),
                None => break,
            };
        }
        // output expired txs
        let elements: Vec<PyObject> = deleted
            .into_iter().map(|tx| tx.obj).collect();
        PyTuple::new(py, &elements).to_object(py)
    }
}


// row level methods only used inner
impl MemoryPool {
    // remove unconfirmed tx with depend it
    fn remove_with_depend_myself(&mut self, hash: &U256, deleted: &mut Vec<Unconfirmed>) {
        // find position
        let delete_index = match self.unconfirmed.iter()
            .position(|tx| hash == &tx.hash) {
            Some(index) => index,
            None => return,
        };

        // delete tx
        deleted.push(self.unconfirmed.remove(delete_index));

        // check depends
        loop {
            let mut delete_hash = None;
            for tx in self.unconfirmed.iter() {
                if tx.depends.contains(hash) {
                    delete_hash = Some(tx.hash.clone());
                    break;
                }
            }
            match delete_hash {
                Some(hash) => self.remove_with_depend_myself(&hash, deleted),
                None => break,
            }
        }
    }

    // push unconfirmed tx with dependency check
    // return inserted tx's index
    fn push_unconfirmed(&mut self, unconfirmed: Unconfirmed) -> PyResult<usize> {
        // most high position depend index
        let mut depend_index: Option<usize> = None;
        for (index, tx) in self.unconfirmed.iter().enumerate() {
            if unconfirmed.depends.contains(&tx.hash) {
                depend_index = Some(index);
            }
            if unconfirmed.hash == tx.hash {
                return Err(AssertionError::py_err("already inserted tx"));
            }
        }

        // most low position required index
        let mut required_index = None;
        let mut disturbs = Vec::new();
        for (index, tx) in self.unconfirmed.iter().enumerate().rev() {
            if tx.depends.contains(&unconfirmed.hash) {
                required_index = Some(index);
                // check absolute condition: depend_index < required_index
                if depend_index.is_some() && depend_index.unwrap() >= index {
                    disturbs.push(tx.hash.clone());
                }
            }
        }

        // exception: with disturbs
        if 0 < disturbs.len() {
            // 1. remove disturbs
            let mut deleted: Vec<Unconfirmed> = Vec::new();
            for disturb in disturbs {
                self.remove_with_depend_myself(&disturb, &mut deleted);
            }

            // 2. push original (not disturbed)
            let hash = unconfirmed.hash.clone();
            assert!(self.push_unconfirmed(unconfirmed).is_ok());

            // 3. push deleted disturbs
            for tx in deleted {
                assert!(self.push_unconfirmed(tx).is_ok());
            }

            // 4. find original position
            let position = self.unconfirmed.iter()
                .position(|tx| hash == tx.hash);
            return Ok(position.unwrap())
        }

        // normal: without disturbs
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
            if unconfirmed.price < tx.price {
                continue;
            } else if unconfirmed.price == tx.price {
                if unconfirmed.time >= tx.time {
                    continue;
                }
            }
            // find
            if best_index.is_none() {
                best_index = Some(index);
                break;
            }
        }

        // minimum index is required_index (or None)
        if best_index.is_none() {
            best_index = required_index.clone();
        }

        // insert
        match best_index {
            Some(best_index) => {
                // println!("best {} {:?} {:?}", best_index, depend_index, required_index);
                self.unconfirmed.insert(best_index, unconfirmed);
                Ok(best_index)
            },
            None => {
                // println!("last {:?} {:?}", depend_index, required_index);
                self.unconfirmed.push(unconfirmed);
                Ok(self.unconfirmed.len())
            },
        }
    }
}
