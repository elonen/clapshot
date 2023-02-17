#![allow(dead_code)]

use std::{sync::{Arc}, ffi::{c_void, CStr }};
use cpython::{PyResult, PyCapsule, GILProtected, Python, py_class, py_exception, PyErr, PyList, PyString, PythonObject, PyDict};
use serde::Serialize;
use crate::database::DB;

#[cfg(test)]
pub mod tests;


// Rust-side data, wrapped in a PyCapsule on the Python side

const PY_CAPSULE_NAME: &str = "clapshot_native_data";
const PY_CAPSULE_NAME_CSTR: &CStr = unsafe { CStr::from_bytes_with_nul_unchecked("clapshot_native_data\0".as_bytes()) };

#[repr(C)]
struct ClapshotData {
    pub db: Arc<DB>,
    //pub testi: i32,
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

py_class!(class ClapshotNative |py| {
    data caps: PyCapsule;

    def db_get_user_videos(&self, user_id: &str) -> PyResult<PyList> {
        let cd = unsafe { self.caps(py).data_ref_cstr::<ClapshotData>(PY_CAPSULE_NAME_CSTR) };
        db_vec_to_json_pylist(py, &cd.db.get_all_user_videos(user_id)?)
    }
});


// Boilerplate to bind Rust-side data to Python

fn db_vec_to_json_pylist<T>(py: Python, elems: &[T]) -> Result<PyList, PyErr>
    where T: crate::database::models::ToJson + Serialize,
{
    let strs = elems.iter()
        .map(|vid| vid.to_json()
            .map_err(|e| ClapshotError::new(py, e.to_string()))
            .map(|j| j.to_string())
            .map(|s| PyString::new(py, &s).into_object())
        ).collect::<Result<Vec<_>, _>>()?;
    Ok(PyList::new(py, strs.as_slice()))
}

struct PythonPluginOwner {
    globals: PyDict,
    clapshot: ClapshotData,
}
type PythonPluginOwnerHandle = Arc<GILProtected<PythonPluginOwner>>;

fn init_python_ctx( db: Arc<DB> ) -> PyResult<PythonPluginOwnerHandle> {
    let gil = Python::acquire_gil();
    let py = gil.python();

    let gpctx = Arc::new(GILProtected::new(PythonPluginOwner {
        globals: PyDict::new(py),
        clapshot: ClapshotData { db },
    }));

    let ctx = gpctx.get(py);

    let ptr = &(ctx.clapshot) as *const ClapshotData as *mut c_void;
    let caps = PyCapsule::new(py, ptr, PY_CAPSULE_NAME)
        .map_err(|e| ClapshotError::new(py, format!("Failed to create python capsule: {e}")))?;

    ctx.globals.set_item(py, "clapshot_native", ClapshotNative::create_instance(py, caps)?)?;
    Ok(gpctx)
}


// ------------------------------------------------

#[cfg(test)]
mod tests_inline {
    use super::*;

    #[test]
    fn test_python_init() -> anyhow::Result<()> {
        let (db, _, _, _) = crate::database::tests::make_test_db();
        let pgctx = init_python_ctx(db).unwrap();

        let n_videos = std::thread::spawn(move || {
            let gil = Python::acquire_gil();
            let py = gil.python();

            const CODE: &str = r###"
import traceback

def py_test():
    try:
        return len(clapshot_native.db_get_user_videos('user.num1'))
    except Exception as e:
        traceback.print_exc()
        return -1
"###;

            let ctx = pgctx.get(py);            
            py.run(CODE, Some(&ctx.globals), None).unwrap();

            py.eval("py_test()", Some(&ctx.globals), None)
                .unwrap().extract::<i32>(py).unwrap()

        }).join().unwrap();

        assert_eq!(n_videos, 3);  // test DB has 3 videos for 'user.num1', see database::tests::make_test_db()
        Ok(())
    }
}
