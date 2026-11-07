//! Load a .ma file with no UI and report timing and warnings: `cargo run --release -p aztek_scene --example load_ma -- file.ma`.

use aztek_scene::{ma_file, Session};
use std::collections::BTreeMap;
use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).expect("usage: load_ma file.ma");
    let mut s = Session::new();
    let t = Instant::now();
    let warnings = ma_file::open_scene(&mut s, &path).expect("open");
    eprintln!("open {:.2}s, {} warnings", t.elapsed().as_secs_f64(), warnings.len());
    let t = Instant::now();
    s.graph.evaluate();
    eprintln!("evaluate {:.2}s", t.elapsed().as_secs_f64());
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    for w in &warnings {
        let key: String = w.chars().map(|c| if c.is_ascii_digit() { '#' } else { c }).collect();
        *kinds.entry(key).or_default() += 1;
    }
    for (k, n) in kinds {
        println!("{n:6} {k}");
    }
}
