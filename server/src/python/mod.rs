#![allow(dead_code)]
#![allow(non_snake_case)]

use std::{sync::{Arc}, ffi::{c_void, CStr }};
use cpython::{PyResult, PyCapsule, GILProtected, Python, py_class, py_exception, PyErr, PyList, PyString, PythonObject, PyDict, PyNone, PyObject};
use serde::Serialize;
use crate::database::DB;

#[cfg(test)]
pub mod tests;

pub static DEFAULT_PYTHON: &'static str = include_str!("default_organizer.py");

// Rust-side data, wrapped in a PyCapsule on the Python side

const PY_CAPSULE_NAME: &str = "clapshot_native_data";
const PY_CAPSULE_NAME_CSTR: &CStr = unsafe { CStr::from_bytes_with_nul_unchecked("clapshot_native_data\0".as_bytes()) };

#[repr(C)]
struct ClapshotData {
    pub db: Arc<DB>,
    pub span: tracing::Span,
}


// Custom error types for Python

py_exception!(clapshot, ClapshotError, cpython::exc::Exception);
py_exception!(clapshot, ClapshotDBError, ClapshotError);

// Implicit conversion from databse::DBError to PyErr
impl From<crate::database::error::DBError> for PyErr {
    fn from(e: crate::database::error::DBError) -> Self {
        let gil = Python::acquire_gil();
        let py = gil.python();    
        ClapshotDBError::new(py, e.to_string())
    }
}


// Python-side API class

macro_rules! get_cd {
    ($py:ident, $self:ident) => {
        unsafe { $self.caps($py).data_ref_cstr::<ClapshotData>(PY_CAPSULE_NAME_CSTR) }
}}
macro_rules! get_db { ($py:ident, $self:ident) => { &get_cd!($py, $self).db }}


py_class!(class ClapshotNative |py| {
    data caps: PyCapsule;

    def db_get_user_videos(&self, user_id: &str) -> PyResult<PyList> {
        db_vec_to_json_pylist(py, &get_db!(py, self).get_all_user_videos(user_id)?)
    }

    // Logging
    @property def DEBUG(&self) -> PyResult<i32> { Ok(-1) }
    @property def INFO(&self) -> PyResult<i32> { Ok(0) }
    @property def WARNING(&self) -> PyResult<i32> { Ok(1) }
    @property def ERROR(&self) -> PyResult<i32> { Ok(2) }
    def log(&self, msg: &str, level: i32=0) -> PyResult<PyNone> {
        let cd = get_cd!(py, self);
        match level {
            -1 => tracing::debug!(parent: &cd.span, "{}", msg),
            0 => tracing::info!(parent: &cd.span, "{}", msg),
            1 => tracing::warn!(parent: &cd.span, "{}", msg),
            2 => tracing::error!(parent: &cd.span, "{}", msg),
            _ => return Err(ClapshotError::new(py, format!("Invalid log level: {level}. Must be one of -1, 0, 1, 2")))
        }
        Ok(PyNone)
    }
});


// Boilerplate to bind Rust-side data to Python

fn db_vec_to_json_pylist<T>(py: Python, elems: &[T]) -> Result<PyList, PyErr>
    where T: crate::database::models::ToJson + Serialize,
{
    let strs = elems.iter()
        .map(|vid| vid.to_json(None)
            .map_err(|e| ClapshotError::new(py, e.to_string()))
            .map(|j| j.to_string())
            .map(|s| PyString::new(py, &s).into_object())
        ).collect::<Result<Vec<_>, _>>()?;
    Ok(PyList::new(py, strs.as_slice()))
}

pub struct PythonPluginOwner {
    globals: PyDict,
    clapshot: ClapshotData,
}

#[derive(Clone)]
pub struct PythonHandle {
    inner: Arc<GILProtected<PythonPluginOwner>>,
}

// ------ Rust-side Python handle ------

impl PythonHandle
{
    /// Initialize Python env and inject Clapshot API in it
    pub fn new(db: Arc<DB>) -> PyResult<PythonHandle> {
        let gil = Python::acquire_gil();
        let py = gil.python();
        let span = tracing::info_span!("PYTHON");
    
        let gpctx = Arc::new(GILProtected::new(PythonPluginOwner {
            globals: PyDict::new(py),
            clapshot: ClapshotData { db, span },
        }));
    
        let ctx = gpctx.get(py);
    
        let ptr = &(ctx.clapshot) as *const ClapshotData as *mut c_void;
        let caps = PyCapsule::new(py, ptr, PY_CAPSULE_NAME)
            .map_err(|e| ClapshotError::new(py, format!("Failed to create python capsule: {e}")))?;
    
        ctx.globals.set_item(py, "clapshot_native", ClapshotNative::create_instance(py, caps)?)?;
        Ok(PythonHandle { inner: gpctx })    
    }

    pub fn run(&self, code: &str) -> PyResult<()> {
        let gil = Python::acquire_gil();
        let py = gil.python();
        let globals = &self.inner.get(py).globals;
        py.run(code, Some(globals), None)
    }

    pub fn eval(&self, code: &str) -> PyResult<PyObject>
    {
        let gil = Python::acquire_gil();
        let py = gil.python();
        let globals = &self.inner.get(py).globals;
        py.eval(code, Some(globals), None)
    }

    pub fn extract<'a, T>(&self, obj: &'a PyObject) -> PyResult<T>
        where T: cpython::FromPyObject<'a>,
    {
        let gil = Python::acquire_gil();
        let py = gil.python();
        obj.extract(py)
    }
}

// ------------------------------------------------

#[cfg(test)]
mod tests_inline {
    use super::*;

    #[test]
    fn test_python_init() -> anyhow::Result<()> {
        let (db, ..) = crate::database::tests::make_test_db();
        let ph = PythonHandle::new(db).unwrap();

        let n_videos = std::thread::spawn(move || {
            const CODE: &str = r###"
import traceback

def py_test():
    try:
        return len(clapshot_native.db_get_user_videos('user.num1'))
    except Exception as e:
        traceback.print_exc()
        return -1
"###;
            ph.run(CODE).unwrap();
            ph.extract::<i32>(&ph.eval("py_test()").unwrap()).unwrap()
        }).join().unwrap();

        assert_eq!(n_videos, 3);  // test DB has 3 videos for 'user.num1', see database::tests::make_test_db()
        Ok(())
    }
}
