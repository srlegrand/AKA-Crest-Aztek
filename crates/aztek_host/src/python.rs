//! Embedded Python: aztek.cmds (and maya.cmds) call the commands; print goes to the script history.

use pyo3::prelude::*;
use serde_json::{json, Map, Value};

use crate::core::{Core, LineKind};

thread_local! {
    /// The Core of the MEL run that called into Python. That run holds the host borrowed, so commands Python runs meanwhile
    /// go to this Core rather than borrow the host again.
    static MEL_CORE: std::cell::Cell<*mut Core> = const { std::cell::Cell::new(std::ptr::null_mut()) };
}

fn with_core<R>(f: impl FnOnce(&mut Core) -> R) -> R {
    let p = MEL_CORE.with(|c| c.get());
    if p.is_null() {
        crate::with(|h| f(&mut h.core))
    } else {
        // The pointer is set only while eval_from_mel runs on this thread, and that frame doesn't touch its Core until
        // Python returns.
        f(unsafe { &mut *p })
    }
}

#[pyfunction]
fn call(name: &str, args: &str, kwargs: &str) -> String {
    let args: Vec<Value> = serde_json::from_str(args).unwrap_or_default();
    let kwargs: Map<String, Value> = serde_json::from_str(kwargs).unwrap_or_default();
    with_core(|core| match core.py_call(name, args, kwargs) {
        Ok(v) => v.to_string(),
        Err(e) => json!({ "__error__": e }).to_string(),
    })
}

#[pyfunction]
fn mel(src: &str) -> String {
    with_core(|core| match core.mel_checked(src) {
        Ok(v) => v.to_string(),
        Err(e) => json!({ "__error__": e }).to_string(),
    })
}

#[pyfunction]
fn write(text: &str, kind: i32) {
    with_core(|core| {
        let k = match kind {
            1 => LineKind::Result,
            2 => LineKind::Error,
            _ => LineKind::Info,
        };
        core.push(text, k)
    })
}

#[pyfunction]
fn exists(name: &str) -> bool {
    with_core(|core| core.s.commands.find(name).is_some() || crate::mel::alias(name).is_some())
}

#[pyfunction]
fn doc(name: &str) -> String {
    with_core(|core| core.s.commands.find(name).map(|c| c.doc.clone()).unwrap_or_default())
}

#[pyfunction]
fn names() -> String {
    with_core(|core| {
        let mut v: Vec<String> = core.s.commands.all().map(|c| c.name.clone()).collect();
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

/// The interpreter starts once per process; its outcome stays for later callers.
static STARTED: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();

fn ensure() -> Result<(), String> {
    STARTED
        .get_or_init(|| {
            pyo3::append_to_inittab!(_aztek);
            Python::initialize();
            Python::attach(|py| -> PyResult<()> {
                let code = std::ffi::CString::new(include_str!("../python/bootstrap.py")).unwrap();
                py.run(&code, None, None)
            })
            .map_err(|e| format!("Python start-up: {e}"))
        })
        .clone()
}

/// MEL's python("..."): a lone expression's value comes back as JSON, statements give null. An exception is the error.
pub(crate) fn eval_from_mel(core: &mut Core, src: &str) -> Result<Value, String> {
    let saved = MEL_CORE.with(|c| c.replace(core as *mut Core));
    let r = ensure().and_then(|()| {
        Python::attach(|py| -> PyResult<String> {
            let main = py.import("__main__")?;
            main.getattr("_aztek_eval")?.call1((src,))?.extract()
        })
        .map_err(|e| e.to_string())
    });
    MEL_CORE.with(|c| c.set(saved));
    serde_json::from_str(&r?).map_err(|e| e.to_string())
}

/// Runs Python source in __main__. A lone expression's value is printed as Maya does: # Result: ... #
/// Must not be called from inside a host borrow: the commands it runs borrow the host themselves.
pub fn run(src: &str) {
    if let Err(e) = ensure() {
        with_core(|core| core.push(format!("# Error: {e} #"), LineKind::Error));
        return;
    }
    let r = Python::attach(|py| -> PyResult<()> {
        let main = py.import("__main__")?;
        main.getattr("_aztek_run")?.call1((src,))?;
        Ok(())
    });
    with_core(|core| {
        if let Err(e) = r {
            core.push(format!("# Error: {e} #"), LineKind::Error);
        }
        core.version += 1
    });
}
