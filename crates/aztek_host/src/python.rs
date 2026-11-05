//! Embedded Python: aztek.cmds (and maya.cmds) call the commands; print goes to the script history.

use pyo3::prelude::*;
use serde_json::{json, Map, Value};

use crate::core::LineKind;

#[pyfunction]
fn call(name: &str, args: &str, kwargs: &str) -> String {
    let args: Vec<Value> = serde_json::from_str(args).unwrap_or_default();
    let kwargs: Map<String, Value> = serde_json::from_str(kwargs).unwrap_or_default();
    crate::with(|h| match h.core.py_call(name, args, kwargs) {
        Ok(v) => v.to_string(),
        Err(e) => json!({ "__error__": e }).to_string(),
    })
}

#[pyfunction]
fn mel(src: &str) -> String {
    crate::with(|h| h.core.mel(src).to_string())
}

#[pyfunction]
fn write(text: &str, kind: i32) {
    crate::with(|h| {
        let k = match kind {
            1 => LineKind::Result,
            2 => LineKind::Error,
            _ => LineKind::Info,
        };
        h.core.push(text, k)
    })
}

#[pyfunction]
fn exists(name: &str) -> bool {
    crate::with(|h| h.core.s.commands.find(name).is_some() || crate::mel::alias(name).is_some())
}

#[pyfunction]
fn doc(name: &str) -> String {
    crate::with(|h| h.core.s.commands.find(name).map(|c| c.doc.clone()).unwrap_or_default())
}

#[pyfunction]
fn names() -> String {
    crate::with(|h| {
        let mut v: Vec<String> = h.core.s.commands.all().map(|c| c.name.clone()).collect();
        v.extend(crate::mel::ALIASES.iter().map(|a| a.maya.to_string()));
        json!(v).to_string()
    })
}

#[pymodule]
fn _aztek(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(call, m)?)?;
    m.add_function(wrap_pyfunction!(mel, m)?)?;
    m.add_function(wrap_pyfunction!(write, m)?)?;
    m.add_function(wrap_pyfunction!(exists, m)?)?;
    m.add_function(wrap_pyfunction!(doc, m)?)?;
    m.add_function(wrap_pyfunction!(names, m)?)?;
    Ok(())
}

thread_local! {
    static READY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn ensure() -> Result<(), String> {
    if READY.with(|r| r.get()) {
        return Ok(());
    }
    pyo3::append_to_inittab!(_aztek);
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let code = std::ffi::CString::new(include_str!("../python/bootstrap.py")).unwrap();
        py.run(&code, None, None)
    })
    .map_err(|e| format!("Python start-up: {e}"))?;
    READY.with(|r| r.set(true));
    Ok(())
}

/// Runs Python source in __main__. A lone expression's value is printed as Maya does: # Result: ... #
/// Must not be called from inside a host borrow: the commands it runs borrow the host themselves.
pub fn run(src: &str) {
    if let Err(e) = ensure() {
        crate::with(|h| h.core.push(format!("# Error: {e} #"), LineKind::Error));
        return;
    }
    let r = Python::attach(|py| -> PyResult<()> {
        let main = py.import("__main__")?;
        main.getattr("_aztek_run")?.call1((src,))?;
        Ok(())
    });
    crate::with(|h| {
        if let Err(e) = r {
            h.core.push(format!("# Error: {e} #"), LineKind::Error);
        }
        h.core.version += 1
    });
}
