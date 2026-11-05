//! The command line's syntax: `polyCube -w 2 -sx 3; select -r pCube1.f[0:3];`, turned into command calls.
//! Flags are looked up by long or short name; each takes as many values as its type needs.

use aztek_ops::{ArgType, Commands};
use aztek_scene::Session;
use serde_json::{json, Map, Value};

fn tokens(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    if c == '\\' {
                        if let Some(n) = chars.next() {
                            s.push(match n {
                                'n' => '\n',
                                't' => '\t',
                                o => o,
                            });
                        }
                    } else if c == '"' {
                        break;
                    } else {
                        s.push(c);
                    }
                }
                // Quoted: marked so a quoted "-x" isn't read as a flag.
                out.push(format!("\u{1}{s}"));
            }
            c if c.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Splits on `;` outside quotes.
pub fn statements(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in src.chars() {
        if c == '"' {
            quoted = !quoted;
        }
        if c == ';' && !quoted {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    out.push(cur);
    out.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty() && !s.starts_with("//")).collect()
}

fn word(t: &str) -> (&str, bool) {
    match t.strip_prefix('\u{1}') {
        Some(s) => (s, true),
        None => (t, false),
    }
}

fn scalar(t: &str) -> Value {
    let (w, quoted) = word(t);
    if quoted {
        return json!(w);
    }
    match w {
        "true" | "on" | "yes" => return json!(true),
        "false" | "off" | "no" => return json!(false),
        _ => {}
    }
    if let Ok(i) = w.parse::<i64>() {
        return json!(i);
    }
    if let Ok(f) = w.parse::<f64>() {
        return json!(f);
    }
    json!(w)
}

fn is_flag(t: &str) -> bool {
    let (w, quoted) = word(t);
    !quoted && w.len() > 1 && w.starts_with('-') && w[1..].chars().next().is_some_and(|c| c.is_ascii_alphabetic())
}

fn is_bool_word(t: &str) -> bool {
    let (w, quoted) = word(t);
    !quoted && matches!(w, "0" | "1" | "true" | "false" | "on" | "off" | "yes" | "no")
}

/// A Maya command spelled as one of ours: the command it runs, and its flags (long or short) as ours.
pub struct Alias {
    pub maya: &'static str,
    pub target: &'static str,
    pub flags: &'static [(&'static str, &'static str)],
}

/// Maya commands and flags Aztek runs under its own names.
pub const ALIASES: &[Alias] = &[
    Alias { maya: "polyExtrudeFacet", target: "polyExtrude", flags: &[("ltz", "thickness"), ("localTranslateZ", "thickness"), ("off", "offset")] },
    Alias { maya: "polyExtrudeEdge", target: "polyExtrude", flags: &[("ltz", "thickness"), ("localTranslateZ", "thickness"), ("off", "offset")] },
    Alias { maya: "polyBevel3", target: "polyBevel", flags: &[("o", "offset"), ("sg", "segments"), ("sa", "smoothingAngle")] },
    Alias { maya: "polySplitRing", target: "polyInsertLoop", flags: &[("wt", "fraction"), ("weight", "fraction")] },
    Alias { maya: "polyMergeVertex", target: "polyMerge", flags: &[("d", "distance")] },
    Alias { maya: "polyCloseBorder", target: "polyFillHole", flags: &[] },
    Alias { maya: "polyNormal", target: "polyFlip", flags: &[] },
    Alias { maya: "polyUnite", target: "polyCombine", flags: &[] },
    Alias { maya: "polySmooth", target: "polySubdivide", flags: &[] },
    Alias { maya: "polyQuad", target: "polyQuadrangulate", flags: &[("a", "maxAngle"), ("angle", "maxAngle")] },
    Alias { maya: "polyDelEdge", target: "polyDissolve", flags: &[] },
    Alias { maya: "polyDelVertex", target: "polyDissolve", flags: &[] },
    Alias { maya: "polyDelFacet", target: "deleteComponents", flags: &[] },
    Alias { maya: "polyCollapseEdge", target: "polyCollapse", flags: &[] },
    Alias { maya: "polyFlipEdge", target: "polySpinEdge", flags: &[] },
    Alias { maya: "polyMirrorFace", target: "polyMirror", flags: &[] },
    Alias { maya: "polyConnectComponents", target: "polyConnect", flags: &[] },
    Alias { maya: "polyChipOff", target: "polyExtract", flags: &[] },
    Alias { maya: "polyPlane", target: "polyPlane", flags: &[("h", "depth"), ("height", "depth"), ("sy", "subdivisionsZ"), ("subdivisionsY", "subdivisionsZ")] },
    Alias { maya: "polyCylinder", target: "polyCylinder", flags: &[("sc", "subdivisionsCaps"), ("sx", "subdivisionsAxis"), ("subdivisionsX", "subdivisionsAxis"), ("sy", "subdivisionsHeight"), ("subdivisionsY", "subdivisionsHeight"), ("sz", "subdivisionsCaps"), ("subdivisionsZ", "subdivisionsCaps")] },
    Alias { maya: "polyCone", target: "polyCone", flags: &[("sc", "subdivisionsCap"), ("sx", "subdivisionsAxis"), ("subdivisionsX", "subdivisionsAxis"), ("sy", "subdivisionsHeight"), ("subdivisionsY", "subdivisionsHeight"), ("sz", "subdivisionsCap"), ("subdivisionsZ", "subdivisionsCap")] },
    Alias { maya: "polyTorus", target: "polyTorus", flags: &[("sr", "sectionRadius"), ("sx", "subdivisionsAxis"), ("subdivisionsX", "subdivisionsAxis"), ("sy", "subdivisionsHeight"), ("subdivisionsY", "subdivisionsHeight")] },
    Alias { maya: "polySphere", target: "polySphere", flags: &[("sx", "subdivisionsAxis"), ("subdivisionsX", "subdivisionsAxis"), ("sy", "subdivisionsHeight"), ("subdivisionsY", "subdivisionsHeight")] },
    Alias { maya: "polyPipe", target: "polyPipe", flags: &[] },
    Alias { maya: "polyDisc", target: "polyDisc", flags: &[] },
    Alias { maya: "makeIdentity", target: "freezeTransforms", flags: &[("a", "apply")] },
    Alias { maya: "CenterPivot", target: "centerPivot", flags: &[] },
    Alias { maya: "polyPlanarProjection", target: "polyProjection", flags: &[] },
    Alias { maya: "polyCylindricalProjection", target: "polyProjection", flags: &[] },
    Alias { maya: "polySphericalProjection", target: "polyProjection", flags: &[] },
    Alias { maya: "unfold", target: "u3dUnfold", flags: &[] },
    Alias { maya: "polyMultiLayoutUV", target: "u3dLayout", flags: &[("ps", "shellSpacing")] },
    Alias { maya: "polyLayoutUV", target: "u3dLayout", flags: &[] },
    Alias { maya: "DeleteHistory", target: "delete", flags: &[] },
];

pub fn alias(name: &str) -> Option<&'static Alias> {
    ALIASES.iter().find(|a| a.maya == name)
}

/// Commands whose positional component strings (pCube1.f[0:3]) are what they work on: they're selected first.
pub fn takes_components(name: &str) -> bool {
    (name.starts_with("poly")
        && !matches!(
            name,
            "polyCube" | "polySphere" | "polyCylinder" | "polyCone" | "polyTorus" | "polyPlane" | "polyDisc" | "polyPipe" | "polyCapsule"
                | "polyIcoSphere" | "polyQuadSphere" | "polyCombine" | "polySeparate" | "polyBoolean"
                // UV commands read their own arguments, map[] included.
                | "polyProjection" | "polyAutoProjection" | "polyEditUV" | "polyMapCut" | "polyMapSew" | "polyMapSewMove"
                | "polyNormalizeUV" | "polyFlipUV" | "polyRotateUVs"
        ))
        || matches!(name, "deleteComponents" | "moveComponents" | "rotateComponents" | "scaleComponents" | "convertSelection" | "growSelection" | "shrinkSelection" | "selectLoop" | "selectBorder")
}

pub fn is_component(s: &str) -> bool {
    s.ends_with(']') && s.rfind('.').is_some_and(|d| s[d..].contains('['))
}

/// One statement to (command, call).
pub fn parse(cmds: &Commands<Session>, stmt: &str) -> Result<(String, Value), String> {
    let toks = tokens(stmt);
    let Some(name) = toks.first() else { return Err("empty".into()) };
    let typed = word(name).0.to_string();
    let al = alias(&typed);
    let name = al.map(|a| a.target.to_string()).unwrap_or(typed.clone());
    let def = cmds.find(&name).ok_or_else(|| format!("Cannot find procedure \"{typed}\"."))?;
    let mut flags = Map::new();
    let mut args = Vec::new();
    let own_query = def.flags.iter().any(|d| d.name == "query");
    let query = toks.iter().skip(1).any(|t| is_flag(t) && matches!(word(t).0, "-q" | "-query"));
    let edit = toks.iter().skip(1).any(|t| is_flag(t) && matches!(word(t).0, "-e" | "-edit"));
    let mut asked = Vec::new();
    let mut i = 1;
    while i < toks.len() {
        let t = &toks[i];
        if is_flag(t) && matches!(word(t).0, "-q" | "-query" | "-e" | "-edit") {
            i += 1;
            continue;
        }
        if is_flag(t) && query {
            // Query mode: flags name what is asked, taking a value only when one follows that fits.
            let f = &word(t).0[1..];
            let fd = def.flags.iter().find(|d| d.name == f || d.short_name == f);
            let name_of = fd.map(|d| d.name.clone()).unwrap_or(f.to_string());
            i += 1;
            let next = toks.get(i);
            // Query switches (xform -q -ws -t) are flags of their own.
            if fd.is_some_and(|d| d.ty == ArgType::Bool) {
                asked.push(json!(name_of.clone()));
                flags.insert(name_of, json!(true));
                continue;
            }
            let takes = match (fd.map(|d| d.ty), next) {
                (_, None) => false,
                (_, Some(n)) if is_flag(n) => false,
                // A trailing word is the object queried (xform -q -t joint1), unless the flag names attributes.
                (Some(ArgType::String) | Some(ArgType::Any), Some(_)) => {
                    own_query
                        && !matches!(name_of.as_str(), "outTangentType" | "inTangentType")
                        && (i + 1 < toks.len() || name_of == "attribute" || scalar(&toks[i]).is_number())
                }
                (Some(ArgType::Float) | Some(ArgType::Int), Some(n)) => scalar(n).is_number() && own_query && !matches!(name_of.as_str(), "timeChange" | "valueChange"),
                _ => false,
            };
            if takes {
                flags.insert(name_of, scalar(&toks[i]));
                i += 1;
            } else {
                asked.push(json!(name_of.clone()));
                if fd.is_some_and(|d| d.ty == ArgType::Bool) {
                    flags.insert(name_of, json!(true));
                }
            }
            continue;
        }
        if is_flag(t) {
            let mut f = &word(t).0[1..];
            if let Some(a) = al {
                if let Some((_, ours)) = a.flags.iter().find(|(m, _)| *m == f) {
                    f = ours;
                }
            }
            let Some(fd) = def.flags.iter().find(|d| d.name == f || d.short_name == f) else {
                // Maya flags with no counterpart that change nothing here: replace-selection, construction history
                // on, relative moves (Aztek's "by"), world space.
                if matches!((name.as_str(), f), ("select", "r") | ("select", "replace") | (_, "ws") | (_, "worldSpace") | (_, "os") | (_, "objectSpace")) {
                    i += 1;
                    continue;
                }
                if matches!(f, "r" | "relative") && matches!(name.as_str(), "move" | "rotate" | "scale") {
                    flags.insert("_relative".into(), json!(true));
                    i += 1;
                    continue;
                }
                if matches!(f, "a" | "absolute") && matches!(name.as_str(), "move" | "rotate" | "scale") {
                    i += 1;
                    continue;
                }
                // Flags Aztek doesn't have yet: skip them and their value rather than refuse the whole line.
                if f == "ch" || f == "constructionHistory" || f == "kft" || f == "keepFacesTogether" || f == "d" || f == "divisions" {
                    i += 1;
                    if i < toks.len() && !is_flag(&toks[i]) && !is_component(word(&toks[i]).0) {
                        i += 1;
                    }
                    continue;
                }
                return Err(format!("{typed}: invalid flag -{f}"));
            };
            i += 1;
            let v = match fd.ty {
                ArgType::Bool => {
                    if i < toks.len() && is_bool_word(&toks[i]) {
                        i += 1;
                        match scalar(&toks[i - 1]) {
                            Value::Number(n) => json!(n.as_i64() != Some(0)),
                            v => v,
                        }
                    } else {
                        json!(true)
                    }
                }
                ArgType::Vec3 => {
                    let mut v = Vec::new();
                    for _ in 0..3 {
                        if i < toks.len() {
                            v.push(scalar(&toks[i]));
                            i += 1;
                        }
                    }
                    Value::Array(v)
                }
                ArgType::List => {
                    let mut v = Vec::new();
                    while i < toks.len() && !is_flag(&toks[i]) {
                        v.push(scalar(&toks[i]));
                        i += 1;
                    }
                    Value::Array(v)
                }
                ArgType::Any if i + 1 < toks.len() && scalar(&toks[i]).is_number() && scalar(&toks[i + 1]).is_number() && !is_flag(&toks[i + 1]) => {
                    // Several numbers: a vector (-ax 1 0 0).
                    let mut v = Vec::new();
                    while i < toks.len() && !is_flag(&toks[i]) && scalar(&toks[i]).is_number() {
                        v.push(scalar(&toks[i]));
                        i += 1;
                    }
                    Value::Array(v)
                }
                _ => {
                    if i >= toks.len() {
                        return Err(format!("{name}: flag -{f} needs a value"));
                    }
                    i += 1;
                    scalar(&toks[i - 1])
                }
            };
            flags.insert(fd.name.clone(), v);
        } else {
            args.push(scalar(t));
            i += 1;
        }
    }
    // move/rotate/scale take the values first: "move -r 1 0 0 pCube1".
    if matches!(name.as_str(), "move" | "rotate" | "scale") && args.len() >= 3 && args[..3].iter().all(|a| a.is_number()) {
        let v: Vec<Value> = args.drain(..3).collect();
        let rel = flags.remove("_relative").is_some();
        let key = if rel || (name == "move" && false) { "by" } else { "to" };
        flags.insert(key.into(), Value::Array(v));
    }
    flags.remove("_relative");
    if !args.is_empty() {
        flags.insert("args".into(), Value::Array(args));
    }
    if own_query && (query || edit) {
        flags.insert(if query { "query" } else { "edit" }.into(), json!(true));
        if query {
            if def.flags.iter().any(|d| d.name == "queried") {
                flags.insert("queried".into(), Value::Array(asked));
            } else {
                // Commands without a queried list read query flags as switches.
                for a in asked {
                    if let Some(n) = a.as_str() {
                        flags.entry(n.to_string()).or_insert(json!(true));
                    }
                }
            }
        }
    } else if query || edit {
        // Maya's -q/-e on a node's settings (polyCube -q -w pCube1): Core turns these into getAttr/setAttr.
        flags.insert(if query { "_query" } else { "_edit" }.into(), Value::Array(asked));
    }
    Ok((name, Value::Object(flags)))
}

/// The MEL echo of a call, for the history pane.
pub fn echo(name: &str, call: &Value) -> String {
    // move/rotate/scale print as Maya's: values first, -r for relative.
    if matches!(name, "move" | "rotate" | "scale") {
        if let Some(m) = call.as_object() {
            for (k, rel) in [("by", true), ("to", false)] {
                if let Some(Value::Array(v)) = m.get(k) {
                    let mut out = name.to_string();
                    if rel {
                        out.push_str(" -r");
                    }
                    for (key, val) in m {
                        if key != "args" && key != k {
                            match val {
                                Value::Bool(true) => out.push_str(&format!(" -{key}")),
                                Value::Bool(false) => out.push_str(&format!(" -{key} 0")),
                                v => out.push_str(&format!(" -{key} {}", show(v))),
                            }
                        }
                    }
                    for x in v {
                        out.push(' ');
                        out.push_str(&show(x));
                    }
                    if let Some(Value::Array(a)) = m.get("args") {
                        for x in a {
                            out.push(' ');
                            out.push_str(&show(x));
                        }
                    }
                    out.push(';');
                    return out;
                }
            }
        }
    }
    let mut out = name.to_string();
    if let Some(m) = call.as_object() {
        for (k, v) in m {
            if k == "args" {
                continue;
            }
            match v {
                Value::Bool(true) => out.push_str(&format!(" -{k}")),
                Value::Bool(false) => out.push_str(&format!(" -{k} 0")),
                Value::Array(a) => {
                    out.push_str(&format!(" -{k}"));
                    for x in a {
                        out.push(' ');
                        out.push_str(&show(x));
                    }
                }
                v => out.push_str(&format!(" -{k} {}", show(v))),
            }
        }
        if let Some(Value::Array(a)) = m.get("args") {
            for x in a {
                out.push(' ');
                out.push_str(&show(x));
            }
        }
    }
    out.push(';');
    out
}

fn show(v: &Value) -> String {
    match v {
        Value::String(s) if s.contains(' ') || s.is_empty() => format!("\"{s}\""),
        Value::String(s) => s.clone(),
        // Six decimals at most, trailing zeros dropped, as Maya prints.
        Value::Number(n) if n.is_f64() => {
            let f = n.as_f64().unwrap();
            let t = format!("{f:.6}");
            let t = t.trim_end_matches('0').trim_end_matches('.').to_string();
            if t == "-0" { "0".into() } else { t }
        }
        v => v.to_string(),
    }
}

/// A result as Maya prints it.
pub fn result(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::Array(a) => Some(format!("// Result: {} //", a.iter().map(show).collect::<Vec<_>>().join(" "))),
        v => Some(format!("// Result: {} //", show(v))),
    }
}
