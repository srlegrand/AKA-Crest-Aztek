//! MEL: the lexer, parser and interpreter scripts, the command line and the script editor run through. `call` turns a
//! command's evaluated arguments into a command call; the state that outlives a run lives in `Core::mel_state`.

mod builtins;
mod call;
mod interp;
mod lexer;
mod parser;
mod pattern;
mod value;

pub use call::*;
pub use interp::{run, run_callback, MelState};
pub use value::{fmt_float, Value as MelValue};
