//! Prints curves and plug values from a .ma file for checking a rig by hand:
//! `rig_probe file.ma name.attr ...`; a curve shape's name alone prints its CVs.

use aztek_scene::{curves, ma_file, Session};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut s = Session::new();
    ma_file::open_scene(&mut s, &args[1]).expect("open");
    s.graph.evaluate();
    for a in &args[2..] {
        if a == "touch" {
            // Dirties everything and evaluates again: a result that moves was not settled.
            for id in s.graph.nodes() {
                s.graph.touch(id);
            }
            s.graph.evaluate();
            println!("-- touched");
            continue;
        }
        if a.contains('.') {
            let v = s.run("getAttr", serde_json::json!({"args": [a]}));
            println!("{a} = {v:?}");
        } else if let Ok(Some(id)) = s.graph.find(a) {
            println!("{a}: {:?}", curves::world_curve(&mut s.graph, id).map(|c| (c.degree, c.cvs.iter().map(|p| (*p * 100.0).round() / 100.0).collect::<Vec<_>>())));
        }
    }
}
