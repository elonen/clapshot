/*
use tracing_test::traced_test;
use crate::python::*;

use cpython::{ObjectProtocol};

fn py_func_call_test() -> PyResult<()> {
    let gil = Python::acquire_gil();
    let py = gil.python();

    let globals = PyDict::new(py);

    let code = r##"
#global test_var
test_var = 123

def test_func(txt):
    print("Calling from", txt)
    print("test_var =", test_var)

test_func("Python @ rust-cpython")
"##;
    py.run(code, Some(&globals), None)?;

    // Call the function from Python
    py.run("test_func('Python after def')", Some(&globals), None)?;

    // Call the function from Rust
    let test_func = globals.get_item(py, "test_func").unwrap();
    test_func.call(py, ("Rust",), None)?;

    Ok(())
}


#[test]
#[traced_test]
fn test_python_call() -> anyhow::Result<()>
{
    py_func_call_test().map_err(|e| anyhow::anyhow!("Error: {:?}", e))?;
    Ok(())
}
*/
