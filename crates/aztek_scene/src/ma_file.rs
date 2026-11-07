//! Maya ASCII (.ma) read and write, and the `file` command.
//!
//! Reading runs in two passes. The statements become a plain list of nodes, setAttr data and connections (`Doc`), with
//! no graph involved; then what Aztek can represent is built from that list, and the rest is reported by node type.
//! Files are read tolerantly: unknown commands and attributes are skipped, unknown node types become a warning.
//!
//! Writing bakes every mesh to stored geometry (.vt .ed .fc), since Aztek's history nodes are not all Maya's. A skinned
//! mesh gets an intermediate Orig shape holding its bind geometry, as Maya does. Aztek's internal nodes (ikSolve, its
//! matrix-fed constraints) are written as the nodes Maya expects: ikEffector and a solver, constraints with their
//! target compounds.
//!
//! Numbers that are easy to get wrong, all as Maya writes them: signed edges in .fc use ~e for reversed, so -2 is edge
//! 1 reversed; matrices list Aztek's columns in order, which are Maya's rows; .kix is in seconds and .kiy in internal
//! units (radians on angle curves); tangent codes in .kit and .kot are the file's own (see `tangent_code`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::sync::Arc;

use aztek_geo::{names, Index, Mesh, INVALID};
use aztek_ops::{fail, ArgType, Args, CmdResult, CommandDef, Commands, FlagDef};
use glam::{DMat4, DQuat, DVec3};
use serde_json::{json, Value};

use crate::anim::{self, Key, Keys, Tangent};
use crate::graph::{Graph, SelectMode};
use crate::mesh_nodes::MeshNode;
use crate::node::{types, EvalContext, NodeBehavior, NodeData, NodeType};
use crate::plug::{flags, AttrDef, NodeId, PlugRef, PlugType, PlugValue, NO_NODE};
use crate::rig::{self, Joint};
use crate::session::Session;
use crate::skin::{self, SkinBind};
use crate::transform::{euler_to_matrix, DagNode, RotateOrder, Transform};

// ---------------------------------------------------------------------------------------------------------------------
// Tokens

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Word(String),
    Str(String),
}

impl Tok {
    fn text(&self) -> Option<&str> {
        match self {
            Tok::Word(s) | Tok::Str(s) => Some(s),
            Tok::Num(_) => None,
        }
    }
    fn num(&self) -> Option<f64> {
        match self {
            Tok::Num(v) => Some(*v),
            Tok::Word(s) => truth_word(s).map(|b| b as i32 as f64),
            Tok::Str(_) => None,
        }
    }
    fn truth(&self) -> Option<bool> {
        match self {
            Tok::Num(v) => Some(*v != 0.0),
            Tok::Word(s) | Tok::Str(s) => truth_word(s),
        }
    }
    fn is_flag(&self) -> bool {
        matches!(self, Tok::Word(s) if s.starts_with('-'))
    }
}

fn truth_word(s: &str) -> Option<bool> {
    match s {
        "yes" | "on" | "true" => Some(true),
        "no" | "off" | "false" => Some(false),
        _ => None,
    }
}

/// Splits source into statements: whitespace-separated tokens up to a semicolon, spanning any number of lines.
struct Lexer<'a> {
    text: &'a str,
    i: usize,
}

impl<'a> Lexer<'a> {
    fn new(text: &'a str) -> Self {
        Lexer { text, i: 0 }
    }

    fn skip_space(&mut self) {
        let b = self.text.as_bytes();
        loop {
            while self.i < b.len() && b[self.i].is_ascii_whitespace() {
                self.i += 1;
            }
            if b[self.i..].starts_with(b"//") {
                while self.i < b.len() && b[self.i] != b'\n' {
                    self.i += 1;
                }
            } else if b[self.i..].starts_with(b"/*") {
                match self.text[self.i..].find("*/") {
                    Some(end) => self.i += end + 2,
                    None => self.i = b.len(),
                }
            } else {
                return;
            }
        }
    }

    fn string(&mut self) -> String {
        let b = self.text.as_bytes();
        self.i += 1;
        let mut out = Vec::new();
        while self.i < b.len() && b[self.i] != b'"' {
            if b[self.i] == b'\\' && self.i + 1 < b.len() {
                self.i += 1;
                out.push(match b[self.i] {
                    b'n' => b'\n',
                    b't' => b'\t',
                    b'r' => b'\r',
                    c => c,
                });
            } else {
                out.push(b[self.i]);
            }
            self.i += 1;
        }
        self.i += 1;
        String::from_utf8_lossy(&out).into_owned()
    }

    /// The next statement into out; false at the end of the text.
    fn statement(&mut self, out: &mut Vec<Tok>) -> bool {
        out.clear();
        let b = self.text.as_bytes();
        let mut joining = false;
        loop {
            self.skip_space();
            if self.i >= b.len() {
                return !out.is_empty();
            }
            match b[self.i] {
                b';' => {
                    self.i += 1;
                    return true;
                }
                b'"' => {
                    let s = self.string();
                    match (joining, out.last_mut()) {
                        (true, Some(Tok::Str(last))) => last.push_str(&s),
                        _ => out.push(Tok::Str(s)),
                    }
                    joining = false;
                }
                _ => {
                    let start = self.i;
                    while self.i < b.len() && !b[self.i].is_ascii_whitespace() && !matches!(b[self.i], b';' | b'"') {
                        self.i += 1;
                    }
                    let w = &self.text[start..self.i];
                    // Long strings are written "abc" + "def".
                    if w == "+" && matches!(out.last(), Some(Tok::Str(_))) {
                        joining = true;
                        continue;
                    }
                    out.push(word_token(w));
                }
            }
        }
    }
}

fn word_token(w: &str) -> Tok {
    let b = w.as_bytes();
    let numeric = match b[0] {
        b'0'..=b'9' => true,
        b'-' | b'+' | b'.' => b.len() > 1 && (b[1].is_ascii_digit() || (b[0] != b'.' && b[1] == b'.' && b.len() > 2)),
        _ => false,
    };
    if numeric {
        if let Ok(v) = w.parse::<f64>() {
            return Tok::Num(v);
        }
    }
    Tok::Word(w.to_string())
}

// ---------------------------------------------------------------------------------------------------------------------
// The document

#[derive(Clone, Debug, PartialEq)]
struct Seg {
    name: String,
    range: Option<(i64, i64)>,
}

/// Indices past this are not data; a bad file should not ask for that much memory.
const MAX_INDEX: i64 = 1 << 28;

/// ".uvst[0].uvsp[0:13]" as segments.
fn parse_path(s: &str) -> Vec<Seg> {
    let s = s.strip_prefix('.').unwrap_or(s);
    s.split('.')
        .map(|part| {
            let (name, range) = match part.find('[') {
                Some(open) if part.ends_with(']') => {
                    let inner = &part[open + 1..part.len() - 1];
                    let r = match inner.split_once(':') {
                        Some((a, b)) => a.parse().ok().zip(b.parse().ok()),
                        None => inner.parse().ok().map(|a: i64| (a, a)),
                    };
                    (&part[..open], r.filter(|&(a, b): &(i64, i64)| 0 <= a && a <= b && b < MAX_INDEX))
                }
                _ => (part, None),
            };
            Seg { name: name.to_string(), range }
        })
        .collect()
}

#[derive(Debug)]
struct SetAttr {
    path: Vec<Seg>,
    lock: Option<bool>,
    values: Vec<Tok>,
}

#[derive(Debug, Default)]
struct AddAttr {
    long: String,
    short: String,
    at: String,
    dt: String,
    dv: Option<f64>,
    min: Option<f64>,
    max: Option<f64>,
    en: String,
    parent: String,
    keyable: bool,
}

#[derive(Debug, Default)]
struct MaNode {
    ty: String,
    name: String,
    parent: Option<usize>,
    /// Named with `select -ne :time1` or `":time1.o"`: a node every scene has, not one the file creates.
    default: bool,
    sets: Vec<SetAttr>,
    adds: Vec<AddAttr>,
}

impl MaNode {
    /// The last non-empty setAttr on this plain attribute.
    fn set_of(&self, name: &[&str]) -> Option<&SetAttr> {
        self.sets
            .iter()
            .rev()
            .find(|s| s.path.len() == 1 && s.path[0].range.is_none() && name.contains(&s.path[0].name.as_str()) && !s.values.is_empty())
    }
    fn num(&self, name: &[&str]) -> Option<f64> {
        self.set_of(name)?.values.first()?.num()
    }
    fn truth(&self, name: &[&str]) -> Option<bool> {
        self.set_of(name)?.values.first()?.truth()
    }
    fn text(&self, name: &[&str]) -> Option<&str> {
        self.set_of(name)?.values.first()?.text()
    }
    fn vec3(&self, name: &[&str]) -> Option<DVec3> {
        let v = &self.set_of(name)?.values;
        Some(DVec3::new(v.first()?.num()?, v.get(1)?.num()?, v.get(2)?.num()?))
    }
    /// Elements of an array attribute, stride values each: `.kit[0:2]  1 2 3` and `.kit[4] 1`, in file order.
    fn elements(&self, name: &[&str], stride: usize) -> Vec<(usize, &[Tok])> {
        let mut out = Vec::new();
        for s in &self.sets {
            if s.path.len() != 1 || !name.contains(&s.path[0].name.as_str()) || s.values.is_empty() {
                continue;
            }
            let start = s.path[0].range.map_or(0, |r| r.0.max(0) as usize);
            for (k, c) in s.values.chunks(stride).enumerate() {
                if c.len() == stride {
                    out.push((start + k, c));
                }
            }
        }
        out
    }
    /// A single matrix element given as 16 numbers: `.pm[0]`.
    fn matrix_element(&self, name: &[&str], element: i64) -> Option<DMat4> {
        let s = self.sets.iter().rev().find(|s| {
            s.path.len() == 1 && name.contains(&s.path[0].name.as_str()) && s.path[0].range.is_some_and(|r| r.0 <= element && element <= r.1) && !s.values.is_empty()
        })?;
        let skip = (element - s.path[0].range?.0) as usize * 16;
        matrix_from(s.values.get(skip..skip + 16)?)
    }
}

fn matrix_from(v: &[Tok]) -> Option<DMat4> {
    if v.len() < 16 {
        return None;
    }
    let mut a = [0.0; 16];
    for (d, t) in a.iter_mut().zip(v) {
        *d = t.num()?;
    }
    Some(DMat4::from_cols_array(&a))
}

#[derive(Clone, Debug)]
struct Conn {
    src: usize,
    src_attr: String,
    dst: usize,
    dst_attr: String,
}

/// An attribute path without its indices: "ip[0].ig" is "ip.ig".
fn plain_attr(a: &str) -> String {
    let mut out = String::with_capacity(a.len());
    let mut depth = 0;
    for c in a.chars() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// The first index in an attribute path: 3 in "tg[3].tpm".
fn first_index(a: &str) -> Option<i64> {
    let open = a.find('[')?;
    let close = a[open..].find(']')? + open;
    a[open + 1..close].parse().ok().filter(|e| (0..MAX_INDEX).contains(e))
}

#[derive(Debug)]
struct Doc {
    nodes: Vec<MaNode>,
    by_name: HashMap<String, Vec<usize>>,
    conns: Vec<Conn>,
    /// Centimetres per file unit, degrees per file unit.
    lin: f64,
    ang: f64,
    fps: Option<f64>,
    warnings: Vec<String>,
}

fn linear_unit(s: &str) -> Option<f64> {
    Some(match s {
        "mm" | "millimeter" => 0.1,
        "cm" | "centimeter" => 1.0,
        "m" | "meter" => 100.0,
        "km" | "kilometer" => 100000.0,
        "in" | "inch" => 2.54,
        "ft" | "foot" => 30.48,
        "yd" | "yard" => 91.44,
        "mi" | "mile" => 160934.4,
        _ => return None,
    })
}

const TIME_UNITS: &[(&str, f64)] = &[("game", 15.0), ("film", 24.0), ("pal", 25.0), ("ntsc", 30.0), ("show", 48.0), ("palf", 50.0), ("ntscf", 60.0)];

fn time_unit(s: &str) -> Option<f64> {
    if let Some((_, f)) = TIME_UNITS.iter().find(|(n, _)| *n == s) {
        return Some(*f);
    }
    s.strip_suffix("fps").and_then(|n| n.parse().ok())
}

fn time_unit_name(fps: f64) -> String {
    match TIME_UNITS.iter().find(|(_, f)| (*f - fps).abs() < 1e-9) {
        Some((n, _)) => n.to_string(),
        None => format!("{fps}fps"),
    }
}

impl Doc {
    fn find(&self, path: &str) -> Option<usize> {
        let path = path.strip_prefix(':').unwrap_or(path);
        let parts: Vec<&str> = path.split('|').collect();
        let last = parts.last()?;
        let list = self.by_name.get(*last)?;
        for &i in list.iter().rev() {
            if parts.len() == 1 {
                return Some(i);
            }
            let rooted = parts[0].is_empty();
            let mut at = self.nodes[i].parent;
            let mut ok = true;
            for want in parts[..parts.len() - 1].iter().rev() {
                if want.is_empty() {
                    break;
                }
                match at {
                    Some(p) if self.nodes[p].name == *want => at = self.nodes[p].parent,
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok && (!rooted || at.is_none()) {
                return Some(i);
            }
        }
        None
    }

    fn add_node(&mut self, n: MaNode) -> usize {
        let i = self.nodes.len();
        self.by_name.entry(n.name.clone()).or_default().push(i);
        self.nodes.push(n);
        i
    }

    /// A node by name; a `:name` the file never created is one of the scene's default nodes.
    fn find_or_default(&mut self, path: &str) -> Option<usize> {
        if let Some(i) = self.find(path) {
            return Some(i);
        }
        let name = path.strip_prefix(':')?;
        Some(self.add_node(MaNode { name: name.to_string(), default: true, ..Default::default() }))
    }

    fn rename(&mut self, i: usize, new: &str) {
        let old = std::mem::replace(&mut self.nodes[i].name, new.to_string());
        if let Some(v) = self.by_name.get_mut(&old) {
            v.retain(|&x| x != i);
        }
        self.by_name.entry(new.to_string()).or_default().push(i);
    }

    fn parse(text: &str) -> Doc {
        let mut d = Doc { nodes: Vec::new(), by_name: HashMap::new(), conns: Vec::new(), lin: 1.0, ang: 1.0, fps: None, warnings: Vec::new() };
        let mut lex = Lexer::new(text);
        let mut st = Vec::new();
        let mut current: Option<usize> = None;
        let mut raw_conns: Vec<(String, String)> = Vec::new();
        while lex.statement(&mut st) {
            let Some(cmd) = st.first().and_then(Tok::text).map(str::to_string) else { continue };
            match cmd.as_str() {
                "createNode" => current = d.create_node(&st).or(current),
                "setAttr" => d.set_attr(&st, current),
                "addAttr" => d.add_attr(&st, current),
                "connectAttr" => {
                    let plain = plain_args(&st, &["-l", "-lock", "-na", "-nextAvailable", "-f", "-force", "-rd", "-referenceDest"]);
                    if let [a, b, ..] = plain.as_slice() {
                        raw_conns.push((a.to_string(), b.to_string()));
                    }
                }
                "disconnectAttr" => {
                    let plain = plain_args(&st, &["-na", "-nextAvailable"]);
                    if let [a, b, ..] = plain.as_slice() {
                        raw_conns.retain(|(x, y)| !(x == a && y == b));
                    }
                }
                "rename" => {
                    if !st.iter().any(|t| t.text() == Some("-uid")) {
                        let plain = plain_args(&st, &["-is", "-ignoreShape"]);
                        match plain.as_slice() {
                            [new] => {
                                if let Some(i) = current {
                                    d.rename(i, new);
                                }
                            }
                            [old, new, ..] => {
                                if let Some(i) = d.find(old) {
                                    d.rename(i, new);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                "select" => {
                    if st.iter().any(|t| matches!(t.text(), Some("-ne") | Some("-noExpand"))) {
                        if let Some(name) = plain_args(&st, &["-ne", "-noExpand", "-r", "-add"]).last() {
                            if let Some(i) = d.find_or_default(name) {
                                current = Some(i);
                            }
                        }
                    }
                }
                "parent" => d.parent(&st),
                "currentUnit" => d.current_unit(&st),
                "file" => {
                    if st.iter().any(|t| matches!(t.text(), Some("-r") | Some("-reference"))) {
                        let path = st.iter().rev().find_map(|t| match t {
                            Tok::Str(s) => Some(s.clone()),
                            _ => None,
                        });
                        d.warnings.push(format!("reference skipped: {}", path.unwrap_or_default()));
                    }
                }
                _ => {}
            }
        }
        for (a, b) in raw_conns {
            let (Some((sn, sa)), Some((dn, da))) = (a.split_once('.'), b.split_once('.')) else { continue };
            // Plugs on default nodes the file never touched are of no interest: only nodes it knows connect.
            let (Some(src), Some(dst)) = (d.find(sn), d.find(dn)) else { continue };
            d.conns.push(Conn { src, src_attr: sa.to_string(), dst, dst_attr: da.to_string() });
        }
        d
    }

    fn create_node(&mut self, st: &[Tok]) -> Option<usize> {
        let ty = st.get(1)?.text()?.to_string();
        let mut name = String::new();
        let mut parent = None;
        let mut k = 2;
        while k < st.len() {
            match st[k].text() {
                Some("-n") | Some("-name") => {
                    name = st.get(k + 1)?.text()?.to_string();
                    k += 1;
                }
                Some("-p") | Some("-parent") => {
                    parent = st.get(k + 1)?.text().and_then(|p| self.find(p));
                    k += 1;
                }
                _ => {}
            }
            k += 1;
        }
        if name.is_empty() {
            let n = self.nodes.iter().filter(|n| n.ty == ty).count() + 1;
            name = format!("{ty}{n}");
        }
        Some(self.add_node(MaNode { ty, name, parent, ..Default::default() }))
    }

    fn set_attr(&mut self, st: &[Tok], current: Option<usize>) {
        let mut plug = None;
        let mut lock = None;
        let mut values = Vec::with_capacity(st.len().saturating_sub(2));
        let mut k = 1;
        while k < st.len() {
            let t = &st[k];
            if plug.is_none() || t.is_flag() {
                match t.text() {
                    Some("-s") | Some("-size") | Some("-ch") | Some("-capacityHint") | Some("-ca") | Some("-caching") | Some("-k") | Some("-keyable")
                    | Some("-cb") | Some("-channelBox") | Some("-av") | Some("-alteredValue") | Some("-c") | Some("-clamp") => {
                        // These take a value, or (-av, -c, a bare -k) nothing; a plug or another flag next means nothing.
                        if st.get(k + 1).is_some_and(|n| n.num().is_some() || n.truth().is_some()) {
                            k += 1;
                        }
                    }
                    Some("-l") | Some("-lock") => {
                        lock = st.get(k + 1).and_then(Tok::truth).or(Some(true));
                        if st.get(k + 1).is_some_and(|n| n.truth().is_some()) {
                            k += 1;
                        }
                    }
                    Some("-type") | Some("-typ") => {
                        k += 1;
                    }
                    Some(p) if !t.is_flag() && plug.is_none() => plug = Some(p.to_string()),
                    _ => {}
                }
            } else {
                values.push(t.clone());
            }
            k += 1;
        }
        let Some(plug) = plug else { return };
        let (node, attr) = if let Some(rel) = plug.strip_prefix('.') {
            (current, rel.to_string())
        } else {
            match plug.split_once('.') {
                Some((n, a)) => (self.find_or_default(n), a.to_string()),
                None => (None, String::new()),
            }
        };
        let Some(node) = node else { return };
        self.nodes[node].sets.push(SetAttr { path: parse_path(&attr), lock, values });
    }

    fn add_attr(&mut self, st: &[Tok], current: Option<usize>) {
        let Some(node) = current else { return };
        let mut a = AddAttr::default();
        let mut k = 1;
        while k < st.len() {
            let val = st.get(k + 1);
            let text = || val.and_then(Tok::text).unwrap_or("").to_string();
            let mut used = true;
            match st[k].text().unwrap_or("") {
                "-ln" | "-longName" => a.long = text(),
                "-sn" | "-shortName" => a.short = text(),
                "-at" | "-attributeType" => a.at = text(),
                "-dt" | "-dataType" => a.dt = text(),
                "-en" | "-enumName" => a.en = text(),
                "-p" | "-parent" => a.parent = text(),
                "-dv" | "-defaultValue" => a.dv = val.and_then(Tok::num),
                "-min" | "-minValue" => a.min = val.and_then(Tok::num),
                "-max" | "-maxValue" => a.max = val.and_then(Tok::num),
                "-k" | "-keyable" => a.keyable = val.and_then(Tok::truth).unwrap_or(true),
                _ => used = false,
            }
            k += if used { 2 } else { 1 };
        }
        if a.long.is_empty() {
            a.long = a.short.clone();
        }
        if !a.long.is_empty() {
            self.nodes[node].adds.push(a);
        }
    }

    fn parent(&mut self, st: &[Tok]) {
        let flagged = |names: &[&str]| st.iter().any(|t| t.text().is_some_and(|x| names.contains(&x)));
        if flagged(&["-add", "-addObject"]) || flagged(&["-s", "-shape"]) {
            self.warnings.push("instances (parent -add) are not represented".to_string());
            return;
        }
        let plain = plain_args(st, &["-r", "-relative", "-a", "-absolute", "-nc", "-noConnections", "-w", "-world"]);
        if flagged(&["-w", "-world"]) {
            if let Some(i) = plain.first().and_then(|n| self.find(n)) {
                self.nodes[i].parent = None;
            }
        } else if let [child, parent, ..] = plain.as_slice() {
            if let (Some(c), Some(p)) = (self.find(child), self.find(parent)) {
                self.nodes[c].parent = Some(p);
            }
        }
    }

    fn current_unit(&mut self, st: &[Tok]) {
        let mut k = 1;
        while k + 1 < st.len() {
            let v = st[k + 1].text().unwrap_or("");
            match st[k].text().unwrap_or("") {
                "-l" | "-linear" => self.lin = linear_unit(v).unwrap_or(1.0),
                "-a" | "-angle" => self.ang = if v.starts_with("rad") { 180.0 / std::f64::consts::PI } else { 1.0 },
                "-t" | "-time" => self.fps = time_unit(v),
                _ => {}
            }
            k += 2;
        }
    }

    /// The one connection into node's attribute (indices ignored unless the name holds them), and its source.
    fn input(&self, node: usize, attrs: &[&str]) -> Option<&Conn> {
        self.conns.iter().find(|c| c.dst == node && attrs.contains(&plain_attr(&c.dst_attr).as_str()))
    }

    fn children(&self, node: usize) -> impl Iterator<Item = usize> + '_ {
        self.nodes.iter().enumerate().filter(move |(_, n)| n.parent == Some(node)).map(|(i, _)| i)
    }
}

/// Text and number tokens after the command, without flags. Flags in `bare` take no value; the rest take one.
fn plain_args(st: &[Tok], bare: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    let mut k = 1;
    while k < st.len() {
        let t = &st[k];
        if t.is_flag() {
            if !t.text().is_some_and(|x| bare.contains(&x)) {
                k += 1;
            }
        } else if let Some(s) = t.text() {
            out.push(s.to_string());
        }
        k += 1;
    }
    out
}

// ---------------------------------------------------------------------------------------------------------------------
// Meshes in the file

#[derive(Debug, Default)]
struct MaFace {
    edges: Vec<i64>,
    /// (set index, UV per corner).
    uvs: Vec<(i64, Vec<i64>)>,
}

#[derive(Debug, Default)]
struct UvSet {
    name: String,
    uvs: Vec<[f32; 2]>,
}

#[derive(Debug, Default)]
struct MaMesh {
    points: Vec<DVec3>,
    tweaks: BTreeMap<usize, DVec3>,
    edges: Vec<[i64; 3]>,
    faces: Vec<MaFace>,
    holes: usize,
    uv_sets: BTreeMap<i64, UvSet>,
    current_uv: String,
}

impl MaMesh {
    fn from_node(n: &MaNode) -> MaMesh {
        let mut m = MaMesh::default();
        for s in &n.sets {
            let Some(first) = s.path.first() else { continue };
            let start = first.range.map_or(0, |r| r.0.max(0) as usize);
            match (first.name.as_str(), s.path.get(1).map(|x| x.name.as_str())) {
                ("vt" | "vrts", None) => {
                    for (k, c) in s.values.chunks_exact(3).enumerate() {
                        put(&mut m.points, start + k, tok_vec3(c));
                    }
                }
                ("pt" | "pnts", None) => {
                    for (k, c) in s.values.chunks_exact(3).enumerate() {
                        m.tweaks.insert(start + k, tok_vec3(c));
                    }
                }
                ("pt" | "pnts", Some(child)) => {
                    if let (Some(v), Some(axis)) = (s.values.first().and_then(Tok::num), axis_of(child)) {
                        m.tweaks.entry(start).or_default()[axis] = v;
                    }
                }
                ("ed" | "edge", None) => {
                    for (k, c) in s.values.chunks_exact(3).enumerate() {
                        let e = [c[0].num().unwrap_or(0.0) as i64, c[1].num().unwrap_or(0.0) as i64, c[2].num().unwrap_or(1.0) as i64];
                        put(&mut m.edges, start + k, e);
                    }
                }
                ("fc" | "face", None) => m.read_faces(start, s),
                ("uvst" | "uvSet", Some(child)) => {
                    let set = m.uv_sets.entry(first.range.map_or(0, |r| r.0)).or_default();
                    match child {
                        "uvsn" | "uvSetName" => set.name = s.values.first().and_then(Tok::text).unwrap_or("").to_string(),
                        "uvsp" | "uvSetPoints" => {
                            let at = s.path[1].range.map_or(0, |r| r.0.max(0) as usize);
                            for (k, c) in s.values.chunks_exact(2).enumerate() {
                                let uv = [c[0].num().unwrap_or(0.0) as f32, c[1].num().unwrap_or(0.0) as f32];
                                put(&mut set.uvs, at + k, uv);
                            }
                        }
                        _ => {}
                    }
                }
                ("cuvs" | "currentUVSet", None) => m.current_uv = s.values.first().and_then(Tok::text).unwrap_or("").to_string(),
                _ => {}
            }
        }
        m
    }

    fn read_faces(&mut self, start: usize, s: &SetAttr) {
        let v = &s.values;
        let mut at = start;
        let mut k = 0;
        let mut started = false;
        // A list of up to n ints after a count.
        let list = |k: &mut usize| -> Vec<i64> {
            let n = v.get(*k).and_then(Tok::num).unwrap_or(0.0) as usize;
            *k += 1;
            let mut out = Vec::with_capacity(n);
            for _ in 0..n {
                match v.get(*k).and_then(Tok::num) {
                    Some(x) => out.push(x as i64),
                    None => break,
                }
                *k += 1;
            }
            out
        };
        while k < v.len() {
            let word = v[k].text().unwrap_or("");
            k += 1;
            match word {
                "f" => {
                    if started {
                        at += 1;
                    }
                    started = true;
                    let edges = list(&mut k);
                    if self.faces.len() <= at {
                        self.faces.resize_with(at + 1, MaFace::default);
                    }
                    self.faces[at].edges = edges;
                }
                "h" => {
                    let _ = list(&mut k);
                    self.holes += 1;
                }
                "mu" | "mf" => {
                    let set = if word == "mu" {
                        let s = v.get(k).and_then(Tok::num).unwrap_or(0.0) as i64;
                        k += 1;
                        s
                    } else {
                        0
                    };
                    let uvs = list(&mut k);
                    if let Some(f) = self.faces.get_mut(at) {
                        f.uvs.push((set, uvs));
                    }
                }
                // Colour assignments: fc n c..., mc set n c...
                "mc" => {
                    k += 1;
                    let _ = list(&mut k);
                }
                "fc" => {
                    let _ = list(&mut k);
                }
                _ => {}
            }
        }
    }

    fn has_geometry(&self) -> bool {
        !self.points.is_empty() && !self.faces.is_empty()
    }

    /// The Aztek mesh: points in file order, faces in file order, the current UV set as uv0.
    fn build(&self, name: &str, warnings: &mut Vec<String>) -> Mesh {
        let mut sizes = Vec::with_capacity(self.faces.len());
        let mut indices = Vec::new();
        let mut kept: Vec<usize> = Vec::with_capacity(self.faces.len());
        let mut bad = 0;
        for (fi, f) in self.faces.iter().enumerate() {
            let start = indices.len();
            let mut ok = f.edges.len() >= 3;
            for &e in &f.edges {
                let (index, reversed) = if e < 0 { (!e, true) } else { (e, false) };
                match self.edges.get(index as usize) {
                    Some(ed) => indices.push(if reversed { ed[1] } else { ed[0] } as Index),
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            let pts = &indices[start..];
            let repeats = pts.iter().enumerate().any(|(i, p)| pts[..i].contains(p));
            if !ok || repeats || pts.iter().any(|&p| p as usize >= self.points.len()) {
                indices.truncate(start);
                bad += 1;
                continue;
            }
            sizes.push(f.edges.len() as Index);
            kept.push(fi);
        }
        if bad > 0 {
            warnings.push(format!("{name}: {bad} unusable faces dropped"));
        }
        if self.holes > 0 {
            warnings.push(format!("{name}: {} face holes dropped", self.holes));
        }
        let points: Vec<DVec3> = self.points.iter().enumerate().map(|(i, p)| *p + self.tweaks.get(&i).copied().unwrap_or(DVec3::ZERO)).collect();
        let mut m = Mesh::from_polygons(&points, &sizes, &indices);

        let mut hard = Vec::new();
        for e in &self.edges {
            let in_range = |p: i64| 0 <= p && (p as usize) < self.points.len();
            if e[2] == 0 && in_range(e[0]) && in_range(e[1]) {
                let id = m.find_edge(e[0] as Index, e[1] as Index);
                if id != INVALID {
                    hard.push(id);
                }
            }
        }
        if !hard.is_empty() {
            let a = m.edge_attrs_mut().get_or_add::<bool>(names::HARD);
            for id in hard {
                a[id as usize] = true;
            }
        }

        // The current set first: Aztek's tools work on uv0.
        let mut order: Vec<i64> = self.uv_sets.keys().copied().collect();
        order.retain(|k| !self.uv_sets[k].uvs.is_empty());
        order.sort_by_key(|k| (self.uv_sets[k].name != self.current_uv, *k));
        for (slot, set_id) in order.iter().enumerate() {
            let set = &self.uv_sets[set_id];
            let mut ids = vec![-1i64; m.corner_capacity() as usize];
            let mut uv = vec![[0.0f32; 2]; m.corner_capacity() as usize];
            for (face, &fi) in kept.iter().enumerate() {
                let f = &self.faces[fi];
                let Some((_, list)) = f.uvs.iter().find(|(s, _)| s == set_id) else { continue };
                // Corners by the point they sit on: the file's order starts wherever its first edge starts.
                for c in m.face_corners(face as Index) {
                    let p = m.corner_point(c);
                    let at = f.edges.iter().position(|&e| {
                        let (index, reversed) = if e < 0 { (!e, true) } else { (e, false) };
                        self.edges.get(index as usize).is_some_and(|ed| (if reversed { ed[1] } else { ed[0] }) as Index == p)
                    });
                    if let Some(id) = at.and_then(|k| list.get(k)).copied() {
                        ids[c as usize] = id;
                        if let Some(v) = set.uvs.get(id.max(0) as usize) {
                            uv[c as usize] = *v;
                        }
                    }
                }
            }
            m.corner_attrs_mut().get_or_add::<[f32; 2]>(&names::uv(slot as i32)).copy_from_slice(&uv);
            if slot == 0 {
                mark_seams(&mut m, &ids);
            }
        }
        m
    }
}

fn put<T: Default + Clone>(v: &mut Vec<T>, at: usize, x: T) {
    if v.len() <= at {
        v.resize(at + 1, T::default());
    }
    v[at] = x;
}

fn tok_vec3(c: &[Tok]) -> DVec3 {
    DVec3::new(c[0].num().unwrap_or(0.0), c[1].num().unwrap_or(0.0), c[2].num().unwrap_or(0.0))
}

fn axis_of(child: &str) -> Option<usize> {
    match child.chars().last()? {
        'x' => Some(0),
        'y' => Some(1),
        'z' => Some(2),
        _ => None,
    }
}

/// Edges whose two sides use different UVs at one spot get the seam flag, so coincident UVs stay separate.
fn mark_seams(m: &mut Mesh, ids: &[i64]) {
    let mut seams = Vec::new();
    for c in 0..m.corner_capacity() {
        if !m.corner_valid(c) {
            continue;
        }
        let r = m.corner_radial(c);
        if r == c || r == INVALID || m.corner_radial(r) != c {
            continue;
        }
        let (p, q) = (m.corner_point(c), m.corner_next_point(c));
        let other = |point: Index| if m.corner_point(r) == point { r } else { m.corner_next(r) };
        let (cq, rp, rq) = (m.corner_next(c), other(p), other(q));
        if ids[c as usize] != ids[rp as usize] || ids[cq as usize] != ids[rq as usize] {
            seams.push(m.corner_edge(c));
        }
    }
    if !seams.is_empty() {
        let a = m.edge_attrs_mut().get_or_add::<bool>("seam");
        for e in seams {
            a[e as usize] = true;
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Attribute names

const XFORM_CHANNELS: &[&str] = &[
    "visibility", "translate", "rotate", "scale", "rotateOrder", "rotatePivot", "scalePivot", "rotatePivotTranslate", "scalePivotTranslate", "rotateAxis", "shear", "offsetParentMatrix",
    "inheritsTransform", "displayHandle", "displayRotatePivot", "displayScalePivot", "displayLocalAxis", "selectHandle", "template", "lodVisibility", "hiddenInOutliner",
];

/// Nodes written and read attribute by attribute, with their connections: utility nodes and expressions.
fn is_plain_node(ty: &str) -> bool {
    ty == "expression" || ty == crate::script_node::TYPE || (ty != "unitConversion" && crate::utility_nodes::classification(ty).is_some())
}
const LINEAR_CHANNELS: &[&str] = &["translate", "rotatePivot", "scalePivot", "rotatePivotTranslate", "scalePivotTranslate"];
const ANGLE_CHANNELS: &[&str] = &["rotate", "rotateAxis", "jointOrient"];

/// Creator nodes: (type, [(Maya short name, Aztek attribute)]).
const CREATORS: &[(&str, &[(&str, &str)])] = &[
    ("polyCube", &[("w", "width"), ("h", "height"), ("d", "depth"), ("sw", "subdivisionsWidth"), ("sh", "subdivisionsHeight"), ("sd", "subdivisionsDepth")]),
    ("polySphere", &[("r", "radius"), ("sa", "subdivisionsAxis"), ("sh", "subdivisionsHeight")]),
    ("polyCylinder", &[("r", "radius"), ("h", "height"), ("sa", "subdivisionsAxis"), ("sh", "subdivisionsHeight"), ("sc", "subdivisionsCaps")]),
    ("polyCone", &[("r", "radius"), ("h", "height"), ("sa", "subdivisionsAxis"), ("sh", "subdivisionsHeight"), ("sc", "subdivisionsCap")]),
    ("polyPlane", &[("w", "width"), ("h", "height"), ("sw", "subdivisionsWidth"), ("sh", "subdivisionsHeight")]),
    ("polyTorus", &[("r", "radius"), ("sr", "sectionRadius"), ("sa", "subdivisionsAxis"), ("sh", "subdivisionsHeight")]),
];

fn creator_attrs(ty: &str) -> Option<&'static [(&'static str, &'static str)]> {
    CREATORS.iter().find(|(t, _)| *t == ty).map(|(_, a)| *a)
}

fn channels_of(t: &NodeType) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    if t.is_a("transform") {
        v.extend(XFORM_CHANNELS);
    }
    if t.is_a("joint") {
        v.extend(["jointOrient", "radius", "segmentScaleCompensate"]);
    }
    if t.is_a("ikHandle") {
        v.extend(["poleVector", "twist"]);
    }
    if t.is_a("locator") {
        v.extend(["visibility", "localPosition", "localScale"]);
    }
    if t.is_a("mesh") {
        v.extend(["visibility", "intermediateObject"]);
    }
    if let Some(attrs) = creator_attrs(&t.name) {
        v.extend(attrs.iter().map(|(_, a)| *a));
    }
    v
}

/// The Aztek plug a Maya attribute name stands for on a node of this type: long or short, a vector or one component.
fn channel(t: &NodeType, attr: &str) -> Option<(usize, i32)> {
    let attr = attr.split('[').next().unwrap_or(attr);
    let mut attr = attr.to_string();
    if t.is_a("locator") {
        if let Some(rest) = attr.strip_prefix("los") {
            attr = format!("ls{rest}");
        }
    }
    if t.is_a("joint") && attr == "ssc" {
        attr = "segmentScaleCompensate".into();
    }
    if let Some(table) = creator_attrs(&t.name) {
        if let Some((_, ours)) = table.iter().find(|(m, _)| *m == attr) {
            attr = ours.to_string();
        }
    }
    let allowed = channels_of(t);
    for (i, a) in t.attrs.iter().enumerate() {
        if !a.has(flags::USER) && !allowed.contains(&a.name.as_str()) {
            continue;
        }
        if a.name == attr || (!a.short_name.is_empty() && a.short_name == attr) {
            return Some((i, -1));
        }
        if a.ty == PlugType::Vec3 {
            for c in 0..3 {
                if a.child_names[c] == attr || a.child_short[c] == attr {
                    return Some((i, c as i32));
                }
            }
        }
    }
    None
}

/// The Maya name of an Aztek plug on a written node, for setAttr and connectAttr.
fn maya_name(a: &AttrDef, component: i32) -> String {
    let short = match a.name.as_str() {
        "localScale" => "los",
        "segmentScaleCompensate" => "ssc",
        _ if a.short_name.is_empty() => &a.name,
        _ => &a.short_name,
    };
    match component {
        c if c >= 0 => format!("{short}{}", ["x", "y", "z"][c as usize]),
        _ => short.to_string(),
    }
}

fn plug_ref(id: NodeId, attr: usize, component: i32) -> PlugRef {
    if component >= 0 {
        PlugRef::component(id, attr as i32, component)
    } else {
        PlugRef::new(id, attr as i32)
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Curves not driven by time

const DRIVEN_CURVES: &[&str] = &["animCurveUL", "animCurveUA", "animCurveUU"];

/// A curve whose input is a plug instead of time: a set driven key.
struct DrivenCurve;

impl NodeBehavior for DrivenCurve {
    fn compute(&mut self, _ctx: &EvalContext, d: &mut NodeData) {
        let ty = d.ty().clone();
        let at = |n: &str| ty.attr_index(n).unwrap();
        let v = match d.value(at("keys")) {
            PlugValue::Data(Some(k)) => k.as_any().downcast_ref::<Keys>().map(|k| k.eval(d.float(at("input")), infinity_of(d.int(at("preInfinity"))), infinity_of(d.int(at("postInfinity"))))),
            _ => None,
        };
        d.set_output(at("output"), PlugValue::Float(v.unwrap_or(0.0)));
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn infinity_of(i: i64) -> anim::Infinity {
    use anim::Infinity::*;
    match i {
        1 => Linear,
        3 => Cycle,
        4 => CycleRelative,
        5 => Oscillate,
        _ => Constant,
    }
}

fn register_types() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        anim::register_types();
        for name in DRIVEN_CURVES {
            if types::find(name).is_some() {
                continue;
            }
            let curve = types::find("animCurveTU").expect("animCurveTU");
            let mut own: Vec<AttrDef> = curve.attrs.iter().filter(|a| matches!(a.name.as_str(), "keys" | "preInfinity" | "postInfinity" | "output")).cloned().collect();
            own.push(AttrDef {
                name: "input".into(),
                short_name: "i".into(),
                ty: PlugType::Float,
                default_value: PlugValue::Float(0.0),
                flags: flags::INPUT | flags::STORABLE,
                doc: "The driver's value.".into(),
                ..Default::default()
            });
            let t = NodeType {
                name: name.to_string(),
                icon: "chart-line".into(),
                doc: "Keys over a driving plug's value: a set driven key.".into(),
                own_attrs: own,
                create: Some(Arc::new(|| Box::new(DrivenCurve))),
                ..Default::default()
            };
            types::register_type(t).expect("driven curve");
        }
    });
}

// ---------------------------------------------------------------------------------------------------------------------
// Tangents

/// Tangent codes in .kit and .kot and .tan, as files hold them. 0 is global, 4 the old smooth, 6 and 7 slow and fast.
fn tangent_from_code(i: i64) -> Tangent {
    match i {
        1 => Tangent::Fixed,
        2 => Tangent::Linear,
        3 => Tangent::Flat,
        4 | 6 | 7 | 9 => Tangent::Spline,
        5 => Tangent::Step,
        8 | 10 => Tangent::Clamped,
        16 => Tangent::Plateau,
        17 => Tangent::StepNext,
        _ => Tangent::Auto,
    }
}

fn tangent_code(t: Tangent) -> i64 {
    match t {
        Tangent::Fixed => 1,
        Tangent::Linear => 2,
        Tangent::Flat => 3,
        Tangent::Step => 5,
        Tangent::Spline => 9,
        Tangent::Clamped => 10,
        Tangent::Plateau => 16,
        Tangent::StepNext => 17,
        Tangent::Auto => 18,
    }
}

fn curve_keys(n: &MaNode, doc: &Doc, fps: f64) -> Keys {
    let linear = n.ty.ends_with('L');
    let angle = n.ty.ends_with('A');
    let vscale = if linear {
        doc.lin
    } else if angle {
        doc.ang
    } else {
        1.0
    };
    let default = n.num(&["tan", "tangentType"]).map_or(Tangent::Auto, |v| tangent_from_code(v as i64));
    let mut keys: Vec<Key> = Vec::new();
    for (at, c) in n.elements(&["ktv", "keyTimeValue"], 2) {
        let (Some(t), Some(v)) = (c[0].num(), c[1].num()) else { continue };
        if keys.len() <= at {
            keys.resize(at + 1, Key::new(0.0, 0.0, default, default));
        }
        keys[at] = Key::new(t, v * vscale, default, default);
    }
    for (at, c) in n.elements(&["kit", "keyTanInType"], 1) {
        if let (Some(k), Some(v)) = (keys.get_mut(at), c[0].num()) {
            k.itt = tangent_from_code(v as i64);
        }
    }
    for (at, c) in n.elements(&["kot", "keyTanOutType"], 1) {
        if let (Some(k), Some(v)) = (keys.get_mut(at), c[0].num()) {
            k.ott = tangent_from_code(v as i64);
        }
    }
    for (at, c) in n.elements(&["kbd", "keyBreakdown"], 1) {
        if let Some(k) = keys.get_mut(at) {
            k.breakdown = c[0].truth().unwrap_or(false);
        }
    }
    // A fixed tangent's slope: y over x, x in seconds, y in internal units (radians on angle curves).
    let slopes = |xs: &[&str], ys: &[&str]| -> HashMap<usize, f64> {
        let x: HashMap<usize, f64> = n.elements(xs, 1).into_iter().filter_map(|(i, c)| Some((i, c[0].num()?))).collect();
        let mut out = HashMap::new();
        for (i, c) in n.elements(ys, 1) {
            let (Some(y), Some(&dx)) = (c[0].num(), x.get(&i)) else { continue };
            let per_second = if dx.abs() < 1e-12 { y.signum() * 1e6 } else { y / dx };
            let unit = if angle { 180.0 / std::f64::consts::PI } else { 1.0 };
            out.insert(i, per_second * unit / fps);
        }
        out
    };
    for (i, s) in slopes(&["kix", "keyTanInX"], &["kiy", "keyTanInY"]) {
        if let Some(k) = keys.get_mut(i) {
            k.in_slope = s;
        }
    }
    for (i, s) in slopes(&["kox", "keyTanOutX"], &["koy", "keyTanOutY"]) {
        if let Some(k) = keys.get_mut(i) {
            k.out_slope = s;
        }
    }
    let mut k = Keys { keys, ..Default::default() };
    k.sort();
    k
}

// ---------------------------------------------------------------------------------------------------------------------
// Building the scene

/// Node types read without comment: Maya's scene furniture, and nodes whose data Aztek takes from elsewhere.
const QUIET_TYPES: &[&str] = &[
    "",
    "groupId",
    "groupParts",
    "shadingEngine",
    "lambert",
    "blinn",
    "phong",
    "phongE",
    "anisotropic",
    "surfaceShader",
    "lightLinker",
    "materialInfo",
    "partition",
    "renderLayerManager",
    "renderLayer",
    "displayLayerManager",
    "script",
    "nodeGraphEditorInfo",
    "hyperGraphInfo",
    "hyperLayout",
    "hyperView",
    "sequenceManager",
    "sequencer",
    "shapeEditorManager",
    "poseInterpolatorManager",
    "objectSet",
    "tweak",
    "time",
    "brush",
    "reference",
    "unitConversion",
    "ikRPsolver",
    "ikSCsolver",
    "ikEffector",
    "ikSystem",
    "camera",
    "renderGlobals",
    "defaultRenderingList",
    "polyBlindData",
    "blindDataTemplate",
];

const DEFAULT_CAMERAS: &[&str] = &["persp", "top", "front", "side", "perspShape", "topShape", "frontShape", "sideShape"];

/// What feeds a mesh shape in the file, walking its input back.
#[derive(Debug, Default)]
struct Chain {
    creator: Option<usize>,
    /// The nearest mesh upstream with stored geometry.
    base: Option<usize>,
    /// polyTweak nodes between that source and the shape, source first.
    tweaks: Vec<usize>,
    skin: Option<usize>,
    /// Nodes on the way that cannot be replayed.
    lost: Vec<usize>,
}

const INPUT_ATTRS: &[&str] = &["i", "inMesh", "ip", "inputPolymesh", "ig", "inputGeometry", "ip.ig", "input.inputGeometry"];

fn walk_chain(doc: &Doc, shape: usize, stored: &HashMap<usize, MaMesh>) -> Chain {
    let mut c = Chain::default();
    let mut seen = HashSet::new();
    let mut at = shape;
    while seen.insert(at) {
        let Some(conn) = doc.input(at, INPUT_ATTRS) else { break };
        let next = conn.src;
        let n = &doc.nodes[next];
        match n.ty.as_str() {
            "mesh" => {
                if stored.get(&next).is_some_and(MaMesh::has_geometry) {
                    c.base = Some(next);
                    break;
                }
            }
            "polyTweak" => c.tweaks.insert(0, next),
            "skinCluster" if c.skin.is_none() => c.skin = Some(next),
            "groupParts" | "groupId" | "tweak" | "unitConversion" => {}
            t if creator_attrs(t).is_some() => {
                c.creator = Some(next);
                break;
            }
            _ => c.lost.push(next),
        }
        at = next;
    }
    c
}

struct Build<'a> {
    s: &'a mut Session,
    doc: &'a Doc,
    ids: Vec<Option<NodeId>>,
    /// Handled here: not reported as unrepresented.
    used: Vec<bool>,
    meshes: HashMap<usize, MaMesh>,
    warnings: Vec<String>,
}

impl Build<'_> {
    fn g(&mut self) -> &mut Graph {
        &mut self.s.graph
    }

    fn id(&self, node: usize) -> Option<NodeId> {
        self.ids[node]
    }

    fn nearest_parent(&mut self, node: usize) -> NodeId {
        let mut at = self.doc.nodes[node].parent;
        while let Some(p) = at {
            if self.ids[p].is_none() && self.keeps(p) {
                self.create_dag(p);
            }
            if let Some(id) = self.ids[p] {
                return id;
            }
            at = self.doc.nodes[p].parent;
        }
        NO_NODE
    }

    /// DAG nodes built as themselves. Meshes marked intermediate are consumed by the mesh pass.
    fn keeps(&self, i: usize) -> bool {
        let n = &self.doc.nodes[i];
        if self.used[i] && (n.ty == "transform" || n.ty == "camera") {
            return false;
        }
        match n.ty.as_str() {
            "transform" | "joint" | "locator" | "nurbsCurve" | "nurbsSurface" | "camera" => true,
            "mesh" => !n.truth(&["io", "intermediateObject"]).unwrap_or(false),
            _ => false,
        }
    }

    fn create_dag(&mut self, i: usize) {
        if self.ids[i].is_some() || !self.keeps(i) {
            return;
        }
        let parent = self.nearest_parent(i);
        let n = &self.doc.nodes[i];
        let Some(t) = types::find(&n.ty) else { return };
        let name = if Graph::valid_name(&n.name) { n.name.clone() } else { String::new() };
        let is_shape = t.is_a("shape");
        let r = (|| -> Result<NodeId, String> {
            let g = &mut self.s.graph;
            // A shape with no transform above it gets one, as Maya gives it.
            let parent = if is_shape && parent == NO_NODE {
                let tt = types::find("transform").unwrap();
                let stem = crate::dag::transform_stem(&n.ty).to_string();
                g.create_node(&tt, &format!("{stem}1"), NO_NODE, true).map_err(|e| e.to_string())?
            } else {
                parent
            };
            g.create_node(&t, &name, parent, false).map_err(|e| e.to_string())
        })();
        match r {
            Ok(id) => {
                self.ids[i] = Some(id);
                self.used[i] = true;
            }
            Err(e) => self.warnings.push(format!("{}: {e}", n.name)),
        }
    }

    fn add_user_attrs(&mut self, i: usize) {
        let Some(id) = self.id(i) else { return };
        let n = &self.doc.nodes[i];
        for a in &n.adds {
            if !a.parent.is_empty() {
                continue;
            }
            let at = a.at.as_str();
            let ty = match (at, a.dt.as_str()) {
                ("double" | "float" | "doubleLinear" | "doubleAngle" | "time", _) => PlugType::Float,
                ("long" | "short" | "byte" | "char", _) => PlugType::Int,
                ("bool", _) => PlugType::Bool,
                ("enum", _) => PlugType::Enum,
                ("double3" | "float3", _) => PlugType::Vec3,
                (_, "string") => PlugType::String,
                _ => {
                    self.warnings.push(format!("{}.{}: attribute type {}{} skipped", n.name, a.long, a.at, a.dt));
                    continue;
                }
            };
            let default_value = match ty {
                PlugType::Float => PlugValue::Float(a.dv.unwrap_or(0.0)),
                PlugType::Int | PlugType::Enum => PlugValue::Int(a.dv.unwrap_or(0.0) as i64),
                PlugType::Bool => PlugValue::Bool(a.dv.unwrap_or(0.0) != 0.0),
                PlugType::Vec3 => PlugValue::Vec3(DVec3::ZERO),
                _ => PlugValue::String(String::new()),
            };
            let mut flags_on = flags::INPUT | flags::STORABLE | flags::USER;
            if a.keyable {
                flags_on |= flags::KEYABLE;
            }
            let mut def = AttrDef {
                name: a.long.clone(),
                short_name: if a.short == a.long { String::new() } else { a.short.clone() },
                ty,
                default_value,
                flags: flags_on,
                min_value: a.min,
                max_value: a.max,
                enum_names: a.en.split(':').filter(|s| !s.is_empty()).map(|s| s.split('=').next().unwrap_or(s).to_string()).collect(),
                ..Default::default()
            };
            if ty == PlugType::Vec3 {
                def = AttrDef { ..crate::node::vec3_attr(&def.name, &def.short_name, DVec3::ZERO, flags_on, "") };
            }
            if let Err(e) = self.g().add_attr(id, def) {
                self.warnings.push(format!("{}.{}: {e}", n.name, a.long));
            }
        }
    }

    /// The setAttr values on a built node's channels, unit-converted.
    fn apply_channels(&mut self, i: usize) {
        let Some(id) = self.id(i) else { return };
        let n = &self.doc.nodes[i];
        let t = self.s.graph.node(id).unwrap().type_arc().clone();
        for set in &n.sets {
            if set.path.len() != 1 || set.path[0].range.is_some() || set.values.is_empty() {
                continue;
            }
            let Some((attr, comp)) = channel(&t, &set.path[0].name) else { continue };
            let def = &t.attrs[attr];
            let factor = if LINEAR_CHANNELS.contains(&def.name.as_str()) {
                self.doc.lin
            } else if ANGLE_CHANNELS.contains(&def.name.as_str()) {
                self.doc.ang
            } else {
                1.0
            };
            let Some(v) = plug_value(def, comp, &set.values, factor) else { continue };
            if let Err(e) = self.s.graph.set_value(plug_ref(id, attr, comp), v) {
                self.warnings.push(format!("{}.{}: {e}", n.name, set.path[0].name));
            }
        }
    }

    /// A utility node or expression: made as it is, its setAttrs applied by name. Attributes Aztek's node doesn't have
    /// (Maya's nodeState and the like) are passed over.
    fn create_plain(&mut self, i: usize) {
        let n = &self.doc.nodes[i];
        self.used[i] = true;
        let Some(t) = types::find(&n.ty) else { return };
        let id = match self.s.graph.create_node(&t, &n.name, NO_NODE, false) {
            Ok(id) => id,
            Err(e) => {
                self.warnings.push(format!("{}: {e}", n.name));
                return;
            }
        };
        self.ids[i] = Some(id);
        let node_path = self.s.graph.path(id);
        for set in &n.sets {
            if set.values.is_empty() {
                continue;
            }
            // One range may hold several elements: .i[0:2] 1 2 3.
            let (lo, hi) = set.path.last().and_then(|s| s.range).or(set.path.first().and_then(|s| s.range)).unwrap_or((-1, -1));
            let count = (hi - lo + 1).max(1) as usize;
            let stride = (set.values.len() / count).max(1);
            for (k, chunk) in set.values.chunks(stride).enumerate().take(count) {
                let text: Vec<String> = set
                    .path
                    .iter()
                    .map(|seg| match seg.range {
                        Some(r) => format!("{}[{}]", seg.name, r.0 + k as i64),
                        None => seg.name.clone(),
                    })
                    .collect();
                let Ok(p) = self.s.graph.plug(&format!("{node_path}.{}", text.join("."))) else { continue };
                let def = self.s.graph.attr(&p).clone();
                let Some(v) = plug_value(&def, p.component, chunk, 1.0) else { continue };
                if let Err(e) = self.s.graph.set_value(p, v) {
                    self.warnings.push(format!("{}.{}: {e}", n.name, text.join(".")));
                }
            }
        }
    }

    /// A connection with a utility node or expression at either end.
    fn plain_connection(&mut self, c: &Conn, sn: usize, sa: &str, src: NodeId, dst: NodeId) {
        let (a, b) = (format!("{}.{sa}", self.s.graph.path(src)), format!("{}.{}", self.s.graph.path(dst), c.dst_attr));
        let plugs = self.s.graph.plug(&a).and_then(|x| self.s.graph.plug(&b).map(|y| (x, y)));
        let r = plugs.and_then(|(x, y)| self.s.graph.connect(&x, &y, true));
        if let Err(e) = r {
            self.warnings.push(format!("{}.{sa} -> {}.{}: {e}", self.doc.nodes[sn].name, self.doc.nodes[c.dst].name, c.dst_attr));
        }
    }

    fn apply_locks(&mut self, i: usize) {
        let Some(id) = self.id(i) else { return };
        let n = &self.doc.nodes[i];
        let t = self.s.graph.node(id).unwrap().type_arc().clone();
        for set in &n.sets {
            let (Some(lock), [seg]) = (set.lock, set.path.as_slice()) else { continue };
            if let Some((attr, comp)) = channel(&t, &seg.name) {
                let _ = self.s.graph.set_locked(&plug_ref(id, attr, comp), lock);
            }
        }
    }

    fn create_generator(&mut self, i: usize) -> Option<NodeId> {
        if let Some(id) = self.id(i) {
            return Some(id);
        }
        let n = &self.doc.nodes[i];
        let t = types::find(&n.ty)?;
        let name = if Graph::valid_name(&n.name) { n.name.clone() } else { String::new() };
        let id = self.s.graph.create_node(&t, &name, NO_NODE, false).ok()?;
        self.ids[i] = Some(id);
        self.used[i] = true;
        self.apply_channels(i);
        if let Some(ax) = n.vec3(&["ax", "axis"]) {
            let k = (0..3).max_by(|&a, &b| ax[a].abs().total_cmp(&ax[b].abs())).unwrap();
            let index = if ax[k] < 0.0 { k + 3 } else { k } as i64;
            if let Some(a) = t.attr_index("upAxis") {
                let _ = self.s.graph.set_value(PlugRef::new(id, a as i32), PlugValue::Int(index));
            }
        }
        Some(id)
    }

    /// Geometry for a mesh shape: its source plug for a skinCluster or the shape, or a mesh to store.
    /// A curve from `.cc`, or from the makeNurbCircle feeding it when it has no stored curve.
    fn build_curve(&mut self, i: usize) {
        use aztek_geo::nurbs::{self, Form, NurbsCurve};
        let Some(shape) = self.id(i) else { return };
        let doc = self.doc;
        let n = &doc.nodes[i];
        let mut curve = None;
        if let Some(cc) = n.set_of(&["cc", "cached"]) {
            // The rational flag is a word: only the numbers count here.
            let nums: Vec<f64> = cc.values.iter().filter_map(|t| if let Tok::Num(v) = t { Some(*v) } else { None }).collect();
            // degree spans form [rational, a word] dimension knotCount knots... cvCount cvs...
            if nums.len() >= 6 {
                let (degree, form) = (nums[0] as usize, nums[2] as i64);
                let dim = nums[3] as usize;
                let nk = nums[4] as usize;
                if nums.len() > 5 + nk {
                    let knots = nums[5..5 + nk].to_vec();
                    let nc = nums[5 + nk] as usize;
                    let start = 6 + nk;
                    let stride = dim.max(2);
                    let mut cvs = Vec::with_capacity(nc);
                    for k in 0..nc {
                        let at = start + k * stride;
                        if at + stride > nums.len() {
                            break;
                        }
                        cvs.push(if stride == 2 { DVec3::new(nums[at], nums[at + 1], 0.0) } else { DVec3::new(nums[at], nums[at + 1], nums[at + 2]) });
                    }
                    curve = Some(NurbsCurve::new(degree, Form::from_index(form), cvs, knots));
                }
            }
        }
        if curve.is_none() {
            if let Some(c) = doc.input(i, &["cr", "create"]) {
                let g = &doc.nodes[c.src];
                if g.ty == "makeNurbCircle" {
                    let nr = g.vec3(&["nr", "normal"]).unwrap_or(DVec3::Z);
                    let cen = g.vec3(&["c", "center"]).unwrap_or(DVec3::ZERO);
                    let fp = g.vec3(&["fp", "firstPoint"]).unwrap_or(DVec3::X);
                    let r = g.num(&["r", "radius"]).unwrap_or(1.0);
                    let deg = g.num(&["d", "degree"]).unwrap_or(3.0) as usize;
                    let sec = g.num(&["s", "sections"]).unwrap_or(8.0) as usize;
                    let sw = g.num(&["sw", "sweep"]).unwrap_or(360.0);
                    curve = Some(nurbs::circle(cen, nr, fp, r, deg, sec, sw));
                }
            }
        }
        let Some(c) = curve else {
            self.warnings.push(format!("{}: no stored curve", n.name));
            return;
        };
        let at = crate::curves::CurveShape::CACHED;
        let _ = self.s.graph.set_value(PlugRef::new(shape, at as i32), PlugValue::Data(Some(Arc::new(crate::curves::CurveData(c)))));
    }

    /// A surface from `.cc`. History with no stored surface is a warning: Maya always writes one for a shape it can't rebuild.
    fn build_surface(&mut self, i: usize) {
        use aztek_geo::nurbs::{Form, NurbsSurface};
        let Some(shape) = self.id(i) else { return };
        let n = &self.doc.nodes[i];
        let mut made = None;
        if let Some(cc) = n.set_of(&["cc", "cached"]) {
            let v: Vec<f64> = cc.values.iter().filter_map(|t| if let Tok::Num(x) = t { Some(*x) } else { None }).collect();
            // du dv fu fv [rational, a word] nku ku... nkv kv... ncv cvs...
            if v.len() > 5 {
                let nku = v[4] as usize;
                let kv_at = 5 + nku;
                if v.len() > kv_at {
                    let nkv = v[kv_at] as usize;
                    let cv_at = kv_at + 1 + nkv;
                    if v.len() > cv_at {
                        let (du, dv) = (v[0] as usize, v[1] as usize);
                        let (nu, nv) = (nku + 1 - du, nkv + 1 - dv);
                        let count = v[cv_at] as usize;
                        let stride = if count > 0 && v.len() >= cv_at + 1 + count * 3 { 3 } else { 0 };
                        if stride == 3 && count == nu * nv {
                            // U is the outer loop in the file, as in NurbsSurface.
                            let cvs: Vec<DVec3> = v[cv_at + 1..cv_at + 1 + count * 3].chunks(3).map(|c| DVec3::new(c[0], c[1], c[2])).collect();
                            made = Some(NurbsSurface { degree_u: du, degree_v: dv, form_u: Form::from_index(v[2] as i64), form_v: Form::from_index(v[3] as i64), nu, nv, cvs, knots_u: v[5..kv_at].to_vec(), knots_v: v[kv_at + 1..cv_at].to_vec() });
                        }
                    }
                }
            }
        }
        let Some(c) = made else {
            self.warnings.push(format!("{}: no stored surface", n.name));
            return;
        };
        let at = crate::surfaces::SurfaceShape::CACHED;
        let _ = self.s.graph.set_value(PlugRef::new(shape, at as i32), PlugValue::Data(Some(Arc::new(crate::surfaces::SurfaceData(c)))));
    }

    fn build_camera(&mut self, i: usize) {
        use crate::cameras as cam;
        let Some(shape) = self.id(i) else { return };
        let n = &self.doc.nodes[i];
        for (names, at) in [(["fl", "focalLength"], cam::FOCAL_LENGTH), (["hfa", "horizontalFilmAperture"], cam::H_APERTURE), (["vfa", "verticalFilmAperture"], cam::V_APERTURE), (["ncp", "nearClipPlane"], cam::NEAR_CLIP), (["fcp", "farClipPlane"], cam::FAR_CLIP), (["coi", "centerOfInterest"], cam::COI), (["ow", "orthographicWidth"], cam::ORTHO_WIDTH)] {
            if let Some(v) = n.num(&names) {
                let _ = self.s.graph.set_value(PlugRef::new(shape, at as i32), PlugValue::Float(v));
            }
        }
        if let Some(o) = n.truth(&["o", "orthographic"]) {
            let _ = self.s.graph.set_value(PlugRef::new(shape, cam::ORTHOGRAPHIC as i32), PlugValue::Bool(o));
        }
    }

    fn build_mesh(&mut self, i: usize) {
        let Some(shape) = self.id(i) else { return };
        let doc = self.doc;
        let n = &doc.nodes[i];
        let chain = walk_chain(doc, i, &self.meshes);
        let own = self.meshes.get(&i).is_some_and(MaMesh::has_geometry);
        let in_mesh = PlugRef::new(shape, MeshNode::IN_MESH as i32);

        if !chain.lost.is_empty() && !own {
            let names: Vec<String> = chain.lost.iter().map(|&l| format!("{} ({})", doc.nodes[l].name, doc.nodes[l].ty)).collect();
            self.warnings.push(format!("{}: no stored geometry past {}; shown as before them", n.name, names.join(", ")));
        }

        // What the mesh is built from.
        enum Source {
            Stored(Mesh),
            Creator(usize),
            Nothing,
        }
        let tweaked = |this: &mut Self, base: usize, tweaks: &[usize]| -> Mesh {
            this.used[base] = true;
            let mut warnings = Vec::new();
            let mut m = this.meshes[&base].build(&doc.nodes[base].name, &mut warnings);
            this.warnings.append(&mut warnings);
            apply_tweaks(doc, &mut m, tweaks);
            for &t in tweaks {
                this.used[t] = true;
            }
            m
        };
        let source = if chain.skin.is_none() && own {
            Source::Stored(tweaked(self, i, &[]))
        } else if let Some(base) = chain.base {
            Source::Stored(tweaked(self, base, &chain.tweaks))
        } else if let Some(c) = chain.creator {
            if !chain.tweaks.is_empty() {
                let names: Vec<&str> = chain.tweaks.iter().map(|&t| doc.nodes[t].name.as_str()).collect();
                self.warnings.push(format!("{}: {} not applied to a {}, whose point order may differ", n.name, names.join(", "), doc.nodes[c].ty));
            }
            Source::Creator(c)
        } else if own {
            Source::Stored(tweaked(self, i, &[]))
        } else {
            Source::Nothing
        };

        let Some(sk) = chain.skin else {
            match source {
                Source::Stored(m) => {
                    let _ = self.s.graph.set_value(in_mesh, PlugValue::Mesh(Some(Arc::new(m))));
                }
                Source::Creator(c) => {
                    if let Some(gen) = self.create_generator(c) {
                        let out = self.s.graph.node(gen).unwrap().ty().attr_index("outMesh").unwrap();
                        if let Err(e) = self.s.graph.connect(&PlugRef::new(gen, out as i32), &in_mesh, false) {
                            self.warnings.push(format!("{}: {e}", n.name));
                        }
                    }
                }
                Source::Nothing => self.warnings.push(format!("{}: no geometry found", n.name)),
            }
            return;
        };

        // Skinned: the bind geometry goes on an intermediate shape (or stays a creator) that feeds the skinCluster.
        let src = match source {
            Source::Stored(m) => {
                let parent = self.s.graph.parent(shape);
                let name = chain.base.map_or_else(|| format!("{}Orig", n.name), |b| doc.nodes[b].name.clone());
                let mt = types::find("mesh").unwrap();
                let Ok(orig) = self.s.graph.create_node(&mt, &name, parent, false) else { return };
                if let Some(b) = chain.base {
                    self.ids[b] = Some(orig);
                }
                let g = &mut self.s.graph;
                let _ = g.set_value(PlugRef::new(orig, DagNode::INTERMEDIATE_OBJECT as i32), PlugValue::Bool(true));
                let _ = g.set_value(PlugRef::new(orig, MeshNode::IN_MESH as i32), PlugValue::Mesh(Some(Arc::new(m))));
                Some(PlugRef::new(orig, MeshNode::OUT_MESH as i32))
            }
            Source::Creator(c) => self.create_generator(c).map(|gen| {
                let out = self.s.graph.node(gen).unwrap().ty().attr_index("outMesh").unwrap();
                PlugRef::new(gen, out as i32)
            }),
            Source::Nothing => {
                self.warnings.push(format!("{}: no bind geometry found", n.name));
                None
            }
        };
        let Some(src) = src else { return };
        self.create_skin(sk, i, shape, src);
    }

    fn create_skin(&mut self, sk: usize, _shape_node: usize, shape: NodeId, src: PlugRef) {
        let doc = self.doc;
        let n = &doc.nodes[sk];
        let t = types::find(skin::SKIN_TYPE).unwrap();
        let at = |name: &str| t.attr_index(name).unwrap();
        let name = if Graph::valid_name(&n.name) { n.name.clone() } else { String::new() };
        let Ok(id) = self.s.graph.create_node(&t, &name, NO_NODE, false) else { return };
        self.ids[sk] = Some(id);
        self.used[sk] = true;
        let g = &mut self.s.graph;
        let _ = g.connect(&src, &PlugRef::new(id, at("inputPolymesh") as i32), false);
        let _ = g.connect(&PlugRef::new(id, at("output") as i32), &PlugRef::new(shape, MeshNode::IN_MESH as i32), false);
        if let Some(v) = n.num(&["en", "envelope"]) {
            let _ = g.set_value(PlugRef::new(id, at("envelope") as i32), PlugValue::Float(v));
        }
        if let Some(v) = n.num(&["skm", "skinningMethod"]) {
            // -1 is "classic linear, weights blended" to Maya; here linear.
            let _ = g.set_value(PlugRef::new(id, at("skinningMethod") as i32), PlugValue::Int((v as i64).max(0)));
        }
        if let Some(v) = n.num(&["mi", "maxInfluences"]) {
            let _ = g.set_value(PlugRef::new(id, at("maxInfluences") as i32), PlugValue::Int(v as i64));
        }
        if let Some(v) = n.truth(&["mmi", "maintainMaxInfluences"]) {
            let _ = g.set_value(PlugRef::new(id, at("maintainMaxInfluences") as i32), PlugValue::Bool(v));
        }
        if let Some(v) = n.num(&["dr", "dropoffRate"]) {
            let _ = g.set_value(PlugRef::new(id, at("dropoffRate") as i32), PlugValue::Float(v));
        }
        if let Some(v) = n.num(&["nw", "normalizeWeights"]) {
            let _ = g.set_value(PlugRef::new(id, at("normalizeWeights") as i32), PlugValue::Int(v as i64));
        }
    }

    /// Influences, bind matrices and weights: after the skeleton exists and has its values.
    fn fill_skin(&mut self, sk: usize) {
        let Some(id) = self.id(sk) else { return };
        let doc = self.doc;
        let n = &doc.nodes[sk];
        let t = types::find(skin::SKIN_TYPE).unwrap();
        let ma = t.attr_index("matrix").unwrap();
        let mut infs: BTreeMap<i64, usize> = BTreeMap::new();
        for c in doc.conns.iter().filter(|c| c.dst == sk && matches!(plain_attr(&c.dst_attr).as_str(), "ma" | "matrix")) {
            if let Some(e) = first_index(&c.dst_attr).filter(|&e| e < 1 << 16) {
                infs.insert(e, c.src);
            }
        }
        let count = infs.keys().copied().max().map_or(0, |m| m as usize + 1);
        let mut bind = SkinBind { bind_pre: vec![DMat4::IDENTITY; count], geom: n.matrix_of(&["gm", "geomMatrix"]).unwrap_or(DMat4::IDENTITY), weights: Vec::new(), locks: vec![false; count] };
        for (&e, &src) in &infs {
            let Some(j) = self.id(src) else {
                self.warnings.push(format!("{}: influence {} is not in the scene", n.name, doc.nodes[src].name));
                continue;
            };
            let _ = self.s.graph.connect(&PlugRef::new(j, Transform::WORLD_MATRIX as i32), &PlugRef::element(id, ma as i32, e as i32), false);
            bind.bind_pre[e as usize] = match n.matrix_element(&["pm", "bindPreMatrix"], e) {
                Some(m) => m,
                None => rig::world(&mut self.s.graph, j).inverse(),
            };
            bind.locks[e as usize] = doc.nodes[src].truth(&["liw", "lockInfluenceWeights"]).unwrap_or(false);
        }
        bind.weights = read_weights(n);
        for w in &mut bind.weights {
            w.retain(|x| x.1 != 0.0 && (x.0 as usize) < count);
            w.sort_by_key(|x| x.0);
        }
        // One list per point of the bind mesh: the tools index it by point.
        let input = PlugRef::new(id, t.attr_index("inputPolymesh").unwrap() as i32);
        if let Some(m) = self.s.graph.value(&input).ok().and_then(|v| v.as_mesh().cloned()) {
            bind.weights.resize(m.point_capacity() as usize, Vec::new());
        }
        let bd = t.attr_index("bindData").unwrap();
        let _ = self.s.graph.set_value(PlugRef::new(id, bd as i32), PlugValue::Data(Some(Arc::new(bind))));
    }

    fn create_ik(&mut self, i: usize) {
        let doc = self.doc;
        let n = &doc.nodes[i];
        self.used[i] = true;
        let src_of = |attrs: &[&str]| doc.input(i, attrs).map(|c| c.src);
        let start = src_of(&["hsj", "startJoint"]);
        let effector = src_of(&["hee", "endEffector"]);
        let solver_ty = src_of(&["hsv", "ikSolver"]).map_or("ikRPsolver", |s| doc.nodes[s].ty.as_str());
        if !matches!(solver_ty, "ikRPsolver" | "ikSCsolver") {
            self.warnings.push(format!("{}: {solver_ty} is not represented; handle skipped", n.name));
            return;
        }
        let (Some(start), Some(effector)) = (start, effector) else {
            self.warnings.push(format!("{}: ikHandle has no start joint or effector; skipped", n.name));
            return;
        };
        self.used[effector] = true;
        // The end joint drives the effector's translate; otherwise it is the effector's sibling joint.
        let end = doc
            .input(effector, &["tx", "ty", "tz", "t", "translate", "translateX"])
            .map(|c| c.src)
            .or_else(|| doc.nodes[effector].parent.and_then(|p| doc.children(p).find(|&k| doc.nodes[k].ty == "joint")));
        let (Some(end), Some(sid)) = (end, self.id(start)) else {
            self.warnings.push(format!("{}: can't find the chain; handle skipped", n.name));
            return;
        };
        let Some(eid) = self.id(end) else { return };
        let (sp, ep) = (self.s.graph.path(sid), self.s.graph.path(eid));
        let name = if Graph::valid_name(&n.name) { n.name.as_str() } else { "ikHandle1" };
        let mut call = json!({"startJoint": sp, "endEffector": ep, "solver": solver_ty, "name": name});
        // Spline IK: the curve wired to the handle's inCurve, if it came through.
        if solver_ty == "ikSplineSolver" {
            let curve = doc.input(i, &["ic", "inCurve"]).and_then(|c| self.id(c.src));
            match curve {
                Some(c) => {
                    call["curve"] = json!(self.s.graph.path(crate::dag::transform_of(&self.s.graph, c)));
                    call["createCurve"] = json!(false);
                }
                None => self.warnings.push(format!("{}: spline IK's curve is not in the file; one is made through the joints", n.name)),
            }
        }
        match self.s.run("ikHandle", call) {
            Ok(v) => {
                let Some(h) = v.get(0).and_then(Value::as_str).and_then(|p| self.s.graph.find(p).ok().flatten()) else { return };
                self.ids[i] = Some(h);
                let parent = self.nearest_parent(i);
                if parent != NO_NODE {
                    let _ = self.s.graph.set_parent(h, parent, false, usize::MAX);
                }
                self.apply_channels(i);
            }
            Err(e) => self.warnings.push(format!("{}: {e}", n.name)),
        }
    }

    fn create_constraint(&mut self, i: usize, _kind: usize) {
        let doc = self.doc;
        let n = &doc.nodes[i];
        let mut targets: BTreeMap<i64, usize> = BTreeMap::new();
        for c in doc.conns.iter().filter(|c| c.dst == i && c.dst_attr.starts_with("tg[")) {
            let leaf = plain_attr(&c.dst_attr);
            if leaf != "tg.tw" {
                if let Some(e) = first_index(&c.dst_attr) {
                    targets.entry(e).or_insert(c.src);
                }
            }
        }
        // The driven object: where the outputs go, else what its parent matrix came from, else its DAG parent.
        let driven = doc
            .conns
            .iter()
            .find(|c| c.src == i && matches!(plain_attr(&c.src_attr).as_str(), "ctx" | "cty" | "ctz" | "ct" | "crx" | "cry" | "crz" | "cr" | "csx" | "csy" | "csz" | "cs"))
            .map(|c| c.dst)
            .or_else(|| doc.input(i, &["cpim"]).map(|c| c.src))
            .or(n.parent);
        let Some(driven_id) = driven.and_then(|d| self.id(d)) else {
            self.warnings.push(format!("{}: {} drives nothing in the scene; skipped", n.name, n.ty));
            return;
        };
        self.used[i] = true;

        let mut ids: Vec<(i64, NodeId)> = Vec::new();
        let mut weights = Vec::new();
        for (&e, &src) in &targets {
            let Some(t) = self.id(src) else {
                self.warnings.push(format!("{}: target {} is not in the scene", n.name, doc.nodes[src].name));
                continue;
            };
            ids.push((e as i64, t));
            // The weight: a value on the element, or the user attr (w0) wired to it.
            let wired = doc.conns.iter().find(|c| c.dst == i && c.src == i && first_index(&c.dst_attr) == Some(e) && plain_attr(&c.dst_attr).ends_with("tw"));
            let w = wired
                .and_then(|c| n.num(&[c.src_attr.as_str()]).or_else(|| n.adds.iter().find(|a| a.short == c.src_attr || a.long == c.src_attr).and_then(|a| n.num(&[a.long.as_str(), a.short.as_str()]))))
                .or_else(|| {
                    n.sets.iter().rev().find_map(|s| {
                        let [a, b] = s.path.as_slice() else { return None };
                        (a.range.is_some_and(|r| r.0 == e) && matches!(b.name.as_str(), "tw" | "targetWeight")).then(|| s.values.first()?.num()).flatten()
                    })
                })
                .unwrap_or(1.0);
            weights.push(w);
        }
        if ids.is_empty() {
            self.warnings.push(format!("{}: {} has no target in the scene; skipped", n.name, n.ty));
            return;
        }
        let cmd = n.ty.clone();
        let g = &self.s.graph;
        let mut args: Vec<Value> = ids.iter().map(|x| json!(g.path(x.1))).collect();
        args.push(json!(g.path(driven_id)));
        let mut call = json!({"args": args});
        if Graph::valid_name(&n.name) {
            call["name"] = json!(n.name);
        }
        let made = match self.s.run(&cmd, call) {
            Ok(v) => v,
            Err(e) => {
                self.warnings.push(format!("{}: {e}", n.name));
                return;
            }
        };
        let Some(cname) = made.get(0).and_then(Value::as_str).map(str::to_string) else { return };
        let Ok(Some(c)) = self.s.graph.find(&cname) else { return };
        self.ids[i] = Some(c);
        let t = self.s.graph.node(c).unwrap().type_arc().clone();
        let g = &mut self.s.graph;
        let set_vec = |g: &mut Graph, attr: &str, element: Option<i32>, v: DVec3| {
            if let Some(a) = t.attr_index(attr) {
                let p = match element {
                    Some(e) => PlugRef::element(c, a as i32, e),
                    None => PlugRef::new(c, a as i32),
                };
                let _ = g.set_value(p, PlugValue::Vec3(v));
            }
        };
        for (k, &(e, _)) in ids.iter().enumerate() {
            let element_vec = |leaf: &[&str]| -> Option<DVec3> {
                n.sets.iter().rev().find_map(|s| {
                    let [a, b] = s.path.as_slice() else { return None };
                    if !(matches!(a.name.as_str(), "tg" | "target") && a.range.is_some_and(|r| r.0 as i64 == e) && leaf.contains(&b.name.as_str())) {
                        return None;
                    }
                    let v = &s.values;
                    Some(DVec3::new(v.first()?.num()?, v.get(1)?.num()?, v.get(2)?.num()?))
                })
            };
            if let Some(v) = element_vec(&["tot", "targetOffsetTranslate"]) {
                set_vec(g, "target.targetOffsetTranslate", Some(k as i32), v * doc.lin);
            }
            if let Some(v) = element_vec(&["tor", "targetOffsetRotate"]) {
                set_vec(g, "target.targetOffsetRotate", Some(k as i32), v * doc.ang);
            }
        }
        // Rotation-like offsets are stored in the file's angle unit; a point constraint's in its linear unit; scale has none.
        if let Some(o) = n.vec3(&["o", "offset"]) {
            let unit = match cmd.as_str() {
                "pointConstraint" => doc.lin,
                "orientConstraint" | "aimConstraint" => doc.ang,
                _ => 1.0,
            };
            set_vec(g, "offset", None, o * unit);
        }
        for (names, attr) in [(["a", "aimVector"], "aimVector"), (["u", "upVector"], "upVector"), (["wu", "worldUpVector"], "worldUpVector")] {
            if let Some(v) = n.vec3(&names) {
                set_vec(g, attr, None, v);
            }
        }
        if let (Some(w), Some(at)) = (n.num(&["wut", "worldUpType"]), t.attr_index("worldUpType")) {
            let _ = g.set_value(PlugRef::new(c, at as i32), PlugValue::Int(w as i64));
        }
        let aliases = self.s.run(&cmd, json!({"args": [cname.clone()], "query": true, "queried": ["weightAliasList"]})).unwrap_or(Value::Null);
        for (alias, w) in aliases.as_array().into_iter().flatten().filter_map(Value::as_str).zip(&weights) {
            if (*w - 1.0).abs() > 1e-12 {
                let _ = self.s.run("setAttr", json!({"args": [format!("{cname}.{alias}"), *w]}));
            }
        }
    }

    fn create_pose(&mut self, i: usize) {
        let doc = self.doc;
        let n = &doc.nodes[i];
        self.used[i] = true;
        let mut members: BTreeMap<i64, usize> = BTreeMap::new();
        for c in doc.conns.iter().filter(|c| c.dst == i && matches!(plain_attr(&c.dst_attr).as_str(), "m" | "members")) {
            if let Some(e) = first_index(&c.dst_attr) {
                if doc.nodes[c.src].ty == "joint" {
                    members.insert(e, c.src);
                }
            }
        }
        let mut data = Vec::new();
        let mut ids = Vec::new();
        for (&e, &src) in &members {
            let Some(j) = self.id(src) else { continue };
            let xm = n.sets.iter().rev().find(|s| matches!(s.path.as_slice(), [a] if matches!(a.name.as_str(), "xm" | "xformMatrix") && a.range.is_some_and(|r| r.0 == e)));
            let local = xm.and_then(|s| xform_values(&s.values));
            let v = |g: &Graph, a: usize| g.node(j).map_or(DVec3::ZERO, |x| x.data().vec3(a));
            let g = &self.s.graph;
            let (t, r, s) = local.unwrap_or((v(g, Transform::TRANSLATE), v(g, Transform::ROTATE), v(g, Transform::SCALE)));
            data.push(json!({"t": [t.x, t.y, t.z], "r": [r.x, r.y, r.z], "s": [s.x, s.y, s.z]}));
            ids.push(j);
        }
        if ids.is_empty() {
            return;
        }
        let t = types::find("dagPose").unwrap();
        let name = if Graph::valid_name(&n.name) { n.name.clone() } else { String::new() };
        let g = &mut self.s.graph;
        let Ok(p) = g.create_node(&t, &name, NO_NODE, false) else { return };
        self.ids[i] = Some(p);
        let at = |a: &str| t.attr_index(a).unwrap();
        let _ = rig::set_json(g, p, at("poseData"), json!(data));
        let _ = g.set_value(PlugRef::new(p, at("bindPose") as i32), PlugValue::Bool(n.truth(&["bp", "bindPose"]).unwrap_or(false)));
        for (e, j) in ids.into_iter().enumerate() {
            let _ = g.connect(&PlugRef::new(j, Transform::WORLD_MATRIX as i32), &PlugRef::element(p, at("worldMatrix") as i32, e as i32), false);
        }
    }

    fn create_layer(&mut self, i: usize) {
        use crate::display_layers as dl;
        let n = &self.doc.nodes[i];
        self.used[i] = true;
        if n.name == dl::DEFAULT_LAYER {
            return;
        }
        let t = types::find(dl::TYPE_NAME).unwrap();
        let name = if Graph::valid_name(&n.name) { n.name.clone() } else { String::new() };
        let g = &mut self.s.graph;
        let Ok(id) = g.create_node(&t, &name, NO_NODE, false) else { return };
        self.ids[i] = Some(id);
        if let Some(v) = n.truth(&["v", "visibility"]) {
            let _ = g.set_value(PlugRef::new(id, dl::VISIBILITY as i32), PlugValue::Bool(v));
        }
        if let Some(v) = n.num(&["dt", "displayType"]) {
            let _ = g.set_value(PlugRef::new(id, dl::DISPLAY_TYPE as i32), PlugValue::Int(v as i64));
        }
        if let Some(v) = n.num(&["do", "displayOrder"]) {
            let _ = g.set_value(PlugRef::new(id, dl::DISPLAY_ORDER as i32), PlugValue::Int(v as i64));
        }
    }

    fn create_curve(&mut self, i: usize, fps: f64) {
        let doc = self.doc;
        let n = &doc.nodes[i];
        let Some(t) = types::find(&n.ty) else { return };
        let name = if Graph::valid_name(&n.name) { n.name.clone() } else { String::new() };
        let g = &mut self.s.graph;
        let Ok(id) = g.create_node(&t, &name, NO_NODE, false) else { return };
        self.ids[i] = Some(id);
        self.used[i] = true;
        let at = |a: &str| t.attr_index(a).unwrap();
        let keys = curve_keys(n, doc, fps);
        let _ = g.set_value(PlugRef::new(id, at("keys") as i32), PlugValue::Data(Some(Arc::new(keys))));
        if let Some(v) = n.num(&["pre", "preInfinity"]) {
            let _ = g.set_value(PlugRef::new(id, at("preInfinity") as i32), PlugValue::Int(v as i64));
        }
        if let Some(v) = n.num(&["pst", "postInfinity"]) {
            let _ = g.set_value(PlugRef::new(id, at("postInfinity") as i32), PlugValue::Int(v as i64));
        }
    }

    /// A connection's source, seen through unitConversion nodes.
    fn real_source(&self, c: &Conn) -> (usize, String) {
        let (mut src, mut attr) = (c.src, c.src_attr.clone());
        for _ in 0..8 {
            if self.doc.nodes[src].ty != "unitConversion" {
                break;
            }
            let Some(up) = self.doc.input(src, &["i", "input"]) else { break };
            src = up.src;
            attr = up.src_attr.clone();
        }
        (src, attr)
    }

    fn channel_connections(&mut self) {
        let doc = self.doc;
        for c in &doc.conns {
            let Some(dst) = self.id(c.dst) else { continue };
            let dt = self.s.graph.node(dst).unwrap().type_arc().clone();
            let (sn, sa) = self.real_source(c);
            let Some(src) = self.id(sn) else { continue };
            let st = self.s.graph.node(src).unwrap().type_arc().clone();
            if is_plain_node(&dt.name) || is_plain_node(&st.name) {
                self.plain_connection(c, sn, &sa, src, dst);
                continue;
            }
            let plain_dst = plain_attr(&c.dst_attr);
            let is_curve = st.attr_index("keys").is_some() && st.attr_index("output").is_some();
            // Into a driven curve, from a channel.
            let dst_plug = if DRIVEN_CURVES.contains(&dt.name.as_str()) {
                if !matches!(plain_dst.as_str(), "i" | "input") {
                    continue;
                }
                PlugRef::new(dst, dt.attr_index("input").unwrap() as i32)
            } else if st.name == crate::display_layers::TYPE_NAME {
                if !matches!(plain_dst.as_str(), "do" | "drawOverride") {
                    continue;
                }
                PlugRef::new(dst, DagNode::DISPLAY_LAYER as i32)
            } else {
                match channel(&dt, &plain_dst) {
                    Some((a, comp)) => plug_ref(dst, a, comp),
                    None => continue,
                }
            };
            let src_plug = if is_curve {
                if !matches!(sa.as_str(), "o" | "output") {
                    continue;
                }
                PlugRef::new(src, st.attr_index("output").unwrap() as i32)
            } else if st.name == crate::display_layers::TYPE_NAME {
                if !matches!(sa.as_str(), "di" | "drawInfo") {
                    continue;
                }
                PlugRef::new(src, crate::display_layers::DRAW_INFO as i32)
            } else {
                match channel(&st, &plain_attr(&sa)) {
                    Some((a, comp)) => plug_ref(src, a, comp),
                    None => continue,
                }
            };
            if let Err(e) = self.s.graph.connect(&src_plug, &dst_plug, false) {
                self.warnings.push(format!("{}.{} -> {}.{}: {e}", doc.nodes[sn].name, sa, doc.nodes[c.dst].name, c.dst_attr));
            }
        }
    }

    /// Curves with something left to drive; the rest are dropped with what they animated.
    fn curve_is_needed(&self, i: usize) -> bool {
        self.doc.conns.iter().any(|c| {
            c.src == i && {
                let d = &self.doc.nodes[c.dst];
                let plain = plain_attr(&c.dst_attr);
                match self.id(c.dst) {
                    Some(_) if is_plain_node(&d.ty) => true,
                    Some(id) => channel(self.s.graph.node(id).unwrap().ty(), &plain).is_some(),
                    None => d.ty == "unitConversion" || d.ty == "ikHandle",
                }
            }
        })
    }

    fn report_unused(&mut self) {
        let mut by_type: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (i, n) in self.doc.nodes.iter().enumerate() {
            if self.used[i] || n.default || QUIET_TYPES.contains(&n.ty.as_str()) {
                continue;
            }
            // Intermediate meshes and their creators nobody used were superseded by stored geometry.
            if n.ty == "mesh" || creator_attrs(&n.ty).is_some() {
                continue;
            }
            by_type.entry(n.ty.as_str()).or_default().push(n.name.as_str());
        }
        for (ty, names) in by_type {
            let shown: Vec<&str> = names.iter().take(4).copied().collect();
            let more = if names.len() > shown.len() { format!(" and {} more", names.len() - shown.len()) } else { String::new() };
            self.warnings.push(format!("{ty} not represented: {}{more}", shown.join(", ")));
        }
    }
}

/// .wl in its forms: `.wl[3].w[2]  v`, `.wl[3].w[0:1]  a b`, and `.wl[0:7].w  n i v i v ...` for each point in turn.
fn read_weights(n: &MaNode) -> Vec<Vec<(u32, f64)>> {
    let mut out: Vec<Vec<(u32, f64)>> = Vec::new();
    for s in &n.sets {
        let [a, b] = s.path.as_slice() else { continue };
        if !matches!(a.name.as_str(), "wl" | "weightList") || !matches!(b.name.as_str(), "w" | "weights") || s.values.is_empty() {
            continue;
        }
        let Some((p0, p1)) = a.range else { continue };
        if out.len() <= p1 as usize {
            out.resize(p1 as usize + 1, Vec::new());
        }
        match b.range {
            Some((w0, _)) => {
                for (k, t) in s.values.iter().enumerate() {
                    if let Some(v) = t.num() {
                        out[p0 as usize].push(((w0 + k as i64) as u32, v));
                    }
                }
            }
            None => {
                let mut k = 0;
                for p in p0..=p1 {
                    let count = s.values.get(k).and_then(Tok::num).unwrap_or(0.0) as usize;
                    k += 1;
                    for _ in 0..count {
                        let (Some(i), Some(v)) = (s.values.get(k).and_then(Tok::num), s.values.get(k + 1).and_then(Tok::num)) else { break };
                        out[p as usize].push((i as u32, v));
                        k += 2;
                    }
                }
            }
        }
    }
    out
}

/// A matrix attribute's plain value.
trait MatrixOf {
    fn matrix_of(&self, name: &[&str]) -> Option<DMat4>;
}

impl MatrixOf for MaNode {
    fn matrix_of(&self, name: &[&str]) -> Option<DMat4> {
        matrix_from(&self.set_of(name)?.values)
    }
}

/// "xform" s[3] r[3] order t[3] ...: scale, rotate in degrees and translate.
fn xform_values(v: &[Tok]) -> Option<(DVec3, DVec3, DVec3)> {
    if v.first()?.text()? != "xform" {
        return None;
    }
    let n = |i: usize| v.get(i).and_then(Tok::num);
    let s = DVec3::new(n(1)?, n(2)?, n(3)?);
    let r = DVec3::new(n(4)?, n(5)?, n(6)?) * (180.0 / std::f64::consts::PI);
    let t = DVec3::new(n(8)?, n(9)?, n(10)?);
    Some((t, r, s))
}

fn plug_value(def: &AttrDef, comp: i32, values: &[Tok], factor: f64) -> Option<PlugValue> {
    let f0 = values.first()?;
    Some(match def.ty {
        _ if comp >= 0 => PlugValue::Float(f0.num()? * factor),
        PlugType::Float => PlugValue::Float(f0.num()? * factor),
        PlugType::Int | PlugType::Enum => PlugValue::Int(f0.num()? as i64),
        PlugType::Bool => PlugValue::Bool(f0.truth()?),
        PlugType::Vec3 => {
            if values.len() < 3 {
                return None;
            }
            PlugValue::Vec3(tok_vec3(values) * factor)
        }
        PlugType::String => PlugValue::String(f0.text()?.to_string()),
        PlugType::Matrix => PlugValue::Matrix(matrix_from(values)?),
        _ => return None,
    })
}

fn apply_tweaks(doc: &Doc, m: &mut Mesh, tweaks: &[usize]) {
    for &t in tweaks {
        for (at, c) in doc.nodes[t].elements(&["tk", "tweak"], 3) {
            if at < m.point_capacity() as usize && m.point_valid(at as Index) {
                let p = m.position(at as Index) + tok_vec3(c);
                m.set_position(at as Index, p);
            }
        }
    }
}

/// Builds the document in the session. Everything goes in one pass; the graph evaluates on demand for world matrices.
fn build(s: &mut Session, doc: &Doc, import: bool) -> Vec<String> {
    let prior: Vec<NodeId> = s.graph.selection().to_vec();
    let fps = if import { anim::fps() } else { doc.fps.unwrap_or_else(anim::fps) };
    if !import {
        if let Some(f) = doc.fps {
            let _ = s.run("playbackOptions", json!({"framesPerSecond": f}));
        }
    }
    let count = doc.nodes.len();
    let mut meshes = HashMap::new();
    for (i, n) in doc.nodes.iter().enumerate() {
        if n.ty == "mesh" {
            meshes.insert(i, MaMesh::from_node(n));
        }
    }
    let mut b = Build { s, doc, ids: vec![None; count], used: vec![false; count], meshes, warnings: doc.warnings.clone() };

    // Cameras are skipped with the transforms that exist only to hold them.
    for i in 0..count {
        if doc.nodes[i].ty != "camera" {
            continue;
        }
        let n = &doc.nodes[i];
        if !DEFAULT_CAMERAS.contains(&n.name.as_str()) {
            // A camera of the scene's own is made like any other shape.
            continue;
        }
        b.used[i] = true;
        if let Some(p) = n.parent {
            let only_cameras = doc.children(p).all(|k| doc.nodes[k].ty == "camera");
            if only_cameras {
                b.used[p] = true;
            }
        }
    }

    for i in 0..count {
        b.create_dag(i);
    }
    for i in 0..count {
        // Maya's own configuration scripts stay out: their content is read above and their MEL is for its interface.
        if is_plain_node(&doc.nodes[i].ty) && !matches!(doc.nodes[i].name.as_str(), "sceneConfigurationScriptNode" | "uiConfigurationScriptNode") {
            b.create_plain(i);
        }
    }
    for i in 0..count {
        b.add_user_attrs(i);
        if doc.nodes[i].ty != "mesh" {
            b.apply_channels(i);
        } else if b.ids[i].is_some() {
            b.apply_channels(i);
        }
    }
    for i in 0..count {
        if doc.nodes[i].ty == "mesh" && b.ids[i].is_some() {
            b.build_mesh(i);
        }
        if doc.nodes[i].ty == "nurbsSurface" && b.ids[i].is_some() {
            b.build_surface(i);
        }
        if doc.nodes[i].ty == "nurbsCurve" && b.ids[i].is_some() {
            b.build_curve(i);
        }
        if doc.nodes[i].ty == "camera" && b.ids[i].is_some() {
            b.build_camera(i);
        }
    }
    // Generators can be animated too; their channels were set as they were made.

    for i in 0..count {
        if doc.nodes[i].ty == "ikHandle" {
            b.create_ik(i);
        }
    }
    for i in 0..count {
        if doc.nodes[i].ty == crate::display_layers::TYPE_NAME {
            b.create_layer(i);
        }
    }
    for i in 0..count {
        let ty = doc.nodes[i].ty.as_str();
        if (anim::CURVE_TYPES.contains(&ty) || DRIVEN_CURVES.contains(&ty)) && b.curve_is_needed(i) {
            b.create_curve(i, fps);
        } else if anim::CURVE_TYPES.contains(&ty) || DRIVEN_CURVES.contains(&ty) {
            b.used[i] = true;
        }
    }
    b.channel_connections();

    // The time before anything that reads world matrices at this frame.
    if !import {
        if let Some(t) = doc.by_name.get("time1").and_then(|v| v.first()).and_then(|&i| doc.nodes[i].num(&["o", "outTime"])) {
            anim::set_frame(&mut b.s.graph, t);
        }
        if let Some(script) = doc.nodes.iter().find(|n| n.ty == "script" && n.name == "sceneConfigurationScriptNode") {
            playback_from_script(b.s, script);
        }
    }

    b.s.graph.evaluate();
    for i in 0..count {
        if doc.nodes[i].ty == skin::SKIN_TYPE {
            b.fill_skin(i);
        }
    }
    for i in 0..count {
        if let Some(kind) = rig::CONSTRAINT_TYPES.iter().position(|t| *t == doc.nodes[i].ty) {
            b.create_constraint(i, kind);
        }
    }
    for i in 0..count {
        if doc.nodes[i].ty == "dagPose" {
            b.create_pose(i);
        }
    }
    for i in 0..count {
        b.apply_locks(i);
    }
    b.report_unused();
    let warnings = std::mem::take(&mut b.warnings);
    let g = &mut s.graph;
    if import {
        g.select(&prior, SelectMode::Replace);
    } else {
        g.select(&[], SelectMode::Replace);
    }
    warnings
}

/// The playback range Maya keeps in a script node: `playbackOptions -min 1 -max 120 -ast 1 -aet 200 `.
fn playback_from_script(s: &mut Session, script: &MaNode) {
    let Some(text) = script.text(&["b", "before"]) else { return };
    let Some(at) = text.find("playbackOptions") else { return };
    let mut call = serde_json::Map::new();
    let words: Vec<&str> = text[at..].split(|c: char| c.is_whitespace() || c == ';').filter(|w| !w.is_empty()).collect();
    for w in words[1..].chunks(2) {
        let [flag, value] = w else { break };
        let Ok(v) = value.parse::<f64>() else { continue };
        let key = match *flag {
            "-min" => "minTime",
            "-max" => "maxTime",
            "-ast" => "animationStartTime",
            "-aet" => "animationEndTime",
            _ => continue,
        };
        call.insert(key.into(), json!(v));
    }
    if !call.is_empty() {
        let _ = s.run("playbackOptions", Value::Object(call));
    }
}

/// Reads .ma text into the session. `import` adds to the scene; otherwise the scene should already be empty.
pub fn read_text(s: &mut Session, text: &str, import: bool) -> Vec<String> {
    register_types();
    let doc = Doc::parse(text);
    let warnings = build(s, &doc, import);
    for w in &warnings {
        log::warn!("{w}");
    }
    warnings
}

// ---------------------------------------------------------------------------------------------------------------------
// Writing

fn quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

const VT_CHUNK: usize = 166;
const UV_CHUNK: usize = 250;
const FACE_CHUNK: usize = 250;

/// A mesh as Maya stores it: dense points, edges and faces, shared UVs per set.
struct Stored {
    points: Vec<DVec3>,
    /// Dense index per point id.
    point_of: Vec<i64>,
    /// (a, b, smooth).
    edges: Vec<(i64, i64, i64)>,
    /// Per face: signed edges and each set's UV per corner.
    faces: Vec<(Vec<i64>, Vec<Vec<i64>>)>,
    corners: usize,
    uv_sets: Vec<Vec<[f32; 2]>>,
}

fn stored_mesh(m: &Mesh) -> Stored {
    let mut point_of = vec![-1i64; m.point_capacity() as usize];
    let mut points = Vec::with_capacity(m.num_points() as usize);
    for p in 0..m.point_capacity() {
        if m.point_valid(p) {
            point_of[p as usize] = points.len() as i64;
            points.push(m.position(p));
        }
    }
    // Edges numbered and directed by first use around the faces, so writing what was read gives the same text.
    let hard = m.edge_attrs().get::<bool>(names::HARD);
    let mut edge_of = vec![-1i64; m.edge_capacity() as usize];
    let mut edges = Vec::with_capacity(m.num_edges() as usize);
    let smooth = |e: Index| !hard.get(e as usize).copied().unwrap_or(false) as i64 != 0;
    for f in 0..m.face_capacity() {
        if !m.face_valid(f) {
            continue;
        }
        for c in m.face_corners(f) {
            let e = m.corner_edge(c);
            if edge_of[e as usize] < 0 {
                edge_of[e as usize] = edges.len() as i64;
                edges.push((point_of[m.corner_point(c) as usize], point_of[m.corner_next_point(c) as usize], smooth(e) as i64));
            }
        }
    }
    for e in 0..m.edge_capacity() {
        if m.edge_valid(e) && edge_of[e as usize] < 0 {
            edge_of[e as usize] = edges.len() as i64;
            edges.push((point_of[m.edge_point(e, 0) as usize], point_of[m.edge_point(e, 1) as usize], smooth(e) as i64));
        }
    }
    // UV sets: uv0 through the same maps the UV Editor shows; others shared where point and value agree.
    let mut set_names: Vec<i32> = Vec::new();
    while m.corner_attrs().has(&names::uv(set_names.len() as i32)) {
        set_names.push(set_names.len() as i32);
    }
    let mut uv_sets: Vec<Vec<[f32; 2]>> = Vec::new();
    let mut corner_uv: Vec<Vec<i64>> = Vec::new();
    for &set in &set_names {
        let uv = m.corner_attrs().get::<[f32; 2]>(&names::uv(set));
        let mut map = vec![-1i64; m.corner_capacity() as usize];
        let mut vals: Vec<[f32; 2]> = Vec::new();
        if set == 0 {
            let topo = crate::uv_ops::topo(m);
            let mut dense: HashMap<Index, i64> = HashMap::new();
            for f in 0..m.face_capacity() {
                if !m.face_valid(f) {
                    continue;
                }
                for c in m.face_corners(f) {
                    let id = topo.corner_map[c as usize];
                    let d = *dense.entry(id).or_insert_with(|| {
                        vals.push(uv[c as usize]);
                        (vals.len() - 1) as i64
                    });
                    map[c as usize] = d;
                }
            }
        } else {
            let mut seen: HashMap<(Index, [u32; 2]), i64> = HashMap::new();
            for f in 0..m.face_capacity() {
                if !m.face_valid(f) {
                    continue;
                }
                for c in m.face_corners(f) {
                    let key = (m.corner_point(c), [uv[c as usize][0].to_bits(), uv[c as usize][1].to_bits()]);
                    let d = *seen.entry(key).or_insert_with(|| {
                        vals.push(uv[c as usize]);
                        (vals.len() - 1) as i64
                    });
                    map[c as usize] = d;
                }
            }
        }
        uv_sets.push(vals);
        corner_uv.push(map);
    }
    let mut faces = Vec::with_capacity(m.num_faces() as usize);
    let mut corners = 0;
    for f in 0..m.face_capacity() {
        if !m.face_valid(f) {
            continue;
        }
        let cs = m.face_corners(f);
        corners += cs.len();
        let signed: Vec<i64> = cs
            .iter()
            .map(|&c| {
                let e = m.corner_edge(c);
                let d = edge_of[e as usize];
                if edges[d as usize].0 == point_of[m.corner_point(c) as usize] {
                    d
                } else {
                    !d
                }
            })
            .collect();
        let uvs: Vec<Vec<i64>> = corner_uv.iter().map(|map| cs.iter().map(|&c| map[c as usize]).collect()).collect();
        faces.push((signed, uvs));
    }
    Stored { points, point_of, edges, faces, corners, uv_sets }
}

fn write_stored(out: &mut String, m: &Stored) {
    let _ = writeln!(out, "\tsetAttr \".vir\" yes;");
    let _ = writeln!(out, "\tsetAttr \".vif\" yes;");
    if m.uv_sets.is_empty() {
        let _ = writeln!(out, "\tsetAttr \".uvst[0].uvsn\" -type \"string\" \"map1\";");
    }
    for (k, set) in m.uv_sets.iter().enumerate() {
        let name = if k == 0 { "map1".to_string() } else { format!("map{}", k + 1) };
        let _ = writeln!(out, "\tsetAttr \".uvst[{k}].uvsn\" -type \"string\" \"{name}\";");
        let _ = writeln!(out, "\tsetAttr -s {} \".uvst[{k}].uvsp\";", set.len());
        for (c, chunk) in set.chunks(UV_CHUNK).enumerate() {
            let a = c * UV_CHUNK;
            let _ = write!(out, "\tsetAttr \".uvst[{k}].uvsp[{}]\" -type \"float2\"", range_text(a, a + chunk.len() - 1));
            for uv in chunk {
                let _ = write!(out, " {} {}", uv[0], uv[1]);
            }
            let _ = writeln!(out, " ;");
        }
    }
    let _ = writeln!(out, "\tsetAttr \".cuvs\" -type \"string\" \"map1\";");
    let _ = writeln!(out, "\tsetAttr \".dcc\" -type \"string\" \"Ambient+Diffuse\";");
    let _ = writeln!(out, "\tsetAttr \".covm[0]\"  0 1 1;");
    let _ = writeln!(out, "\tsetAttr \".cdvm[0]\"  0 1 1;");
    let _ = writeln!(out, "\tsetAttr -s {} \".vt\";", m.points.len());
    for (c, chunk) in m.points.chunks(VT_CHUNK).enumerate() {
        let a = c * VT_CHUNK;
        let _ = write!(out, "\tsetAttr \".vt[{}]\" ", range_text(a, a + chunk.len() - 1));
        for p in chunk {
            let _ = write!(out, " {}", vec_text(*p));
        }
        let _ = writeln!(out, ";");
    }
    let _ = writeln!(out, "\tsetAttr -s {} \".ed\";", m.edges.len());
    for (c, chunk) in m.edges.chunks(VT_CHUNK).enumerate() {
        let a = c * VT_CHUNK;
        let _ = write!(out, "\tsetAttr \".ed[{}]\" ", range_text(a, a + chunk.len() - 1));
        for e in chunk {
            let _ = write!(out, " {} {} {}", e.0, e.1, e.2);
        }
        let _ = writeln!(out, ";");
    }
    for (c, chunk) in m.faces.chunks(FACE_CHUNK).enumerate() {
        let a = c * FACE_CHUNK;
        let head = if c == 0 { format!("-s {} -ch {} ", m.faces.len(), m.corners) } else { String::new() };
        let _ = write!(out, "\tsetAttr {head}\".fc[{}]\" -type \"polyFaces\"", range_text(a, a + chunk.len() - 1));
        for (edges, uvs) in chunk {
            let _ = write!(out, " \n\t\tf {}", edges.len());
            for e in edges {
                let _ = write!(out, " {e}");
            }
            for (k, list) in uvs.iter().enumerate() {
                let _ = write!(out, " mu {k} {}", list.len());
                for u in list {
                    let _ = write!(out, " {u}");
                }
            }
        }
        let _ = writeln!(out, " ;");
    }
}

fn matrix_text(m: &DMat4) -> String {
    m.to_cols_array().iter().map(|&v| if v == 0.0 { 0.0 } else { v }.to_string()).collect::<Vec<_>>().join(" ")
}

/// An index range as Maya prints it: [3] or [3:7].
fn range_text(a: usize, b: usize) -> String {
    if a == b {
        format!("{a}")
    } else {
        format!("{a}:{b}")
    }
}

/// Three numbers, with no negative zeros.
fn vec_text(v: DVec3) -> String {
    let n = |x: f64| if x == 0.0 { 0.0 } else { x };
    format!("{} {} {}", n(v.x), n(v.y), n(v.z))
}

struct Writer<'a> {
    s: &'a mut Session,
    out: String,
    /// Connections, sorted when written.
    conns: Vec<String>,
    /// Lines with -na, written after the others.
    shading: Vec<String>,
    warnings: Vec<String>,
    /// Names the file uses, so made-up nodes (effectors, Orig shapes) don't clash.
    used: HashSet<String>,
    /// What gets written; None is everything.
    keep: HashSet<NodeId>,
    /// Joint -> rotate when the chain was made, written as preferred angle.
    preferred: HashMap<NodeId, DVec3>,
    /// Node text written after the DAG: skins, constraints, effectors, poses.
    pending: Vec<String>,
    /// Solver nodes already written.
    solvers: HashSet<String>,
}

impl Writer<'_> {
    fn fresh(&mut self, wanted: &str) -> String {
        let mut name = wanted.to_string();
        let mut k = 1;
        while self.used.contains(&name) {
            name = format!("{wanted}{k}");
            k += 1;
        }
        self.used.insert(name.clone());
        name
    }

    fn conn(&mut self, a: &str, b: &str) {
        self.conns.push(format!("connectAttr {} {};", quoted(a), quoted(b)));
    }

    /// The node's values that differ from their defaults, then its locks.
    fn write_channels(&mut self, id: NodeId) {
        let t = self.s.graph.node(id).unwrap().type_arc().clone();
        let channels = channels_of(&t);
        let is_shape = t.is_a("shape");
        if is_shape {
            let _ = writeln!(self.out, "\tsetAttr -k off \".v\";");
        }
        for (i, a) in t.attrs.iter().enumerate() {
            if !channels.contains(&a.name.as_str()) && !a.has(flags::USER) {
                continue;
            }
            let p = PlugRef::new(id, i as i32);
            let v = match self.s.graph.value(&p) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let name = maya_name(a, -1);
            if a.has(flags::USER) {
                let line = attr_add_lines(a);
                self.out.push_str(&line);
            }
            if v == a.default_value {
                continue;
            }
            let kflag = if is_shape && a.name == "visibility" { "-k off " } else { "" };
            match v {
                PlugValue::Vec3(x) => {
                    let _ = writeln!(self.out, "\tsetAttr \".{name}\" -type \"double3\" {} ;", vec_text(x));
                }
                PlugValue::Float(x) => {
                    let _ = writeln!(self.out, "\tsetAttr {kflag}\".{name}\" {x};");
                }
                PlugValue::Int(x) => {
                    let _ = writeln!(self.out, "\tsetAttr {kflag}\".{name}\" {x};");
                }
                PlugValue::Bool(x) => {
                    let _ = writeln!(self.out, "\tsetAttr {kflag}\".{name}\" {};", if x { "yes" } else { "no" });
                }
                PlugValue::String(x) => {
                    let _ = writeln!(self.out, "\tsetAttr \".{name}\" -type \"string\" {};", quoted(&x));
                }
                PlugValue::Matrix(m) => {
                    let _ = writeln!(self.out, "\tsetAttr \".{name}\" -type \"matrix\" {};", matrix_text(&m));
                }
                _ => {}
            }
        }
        if let Some(pa) = self.preferred.get(&id).copied() {
            if pa != DVec3::ZERO {
                let _ = writeln!(self.out, "\tsetAttr \".pa\" -type \"double3\" {} ;", vec_text(pa));
            }
        }
        for (i, a) in t.attrs.iter().enumerate() {
            if !channels.contains(&a.name.as_str()) && !a.has(flags::USER) {
                continue;
            }
            let mask = self.s.graph.node(id).unwrap().lock_mask(i);
            if mask == 0 {
                continue;
            }
            if a.ty == PlugType::Vec3 {
                for c in 0..3 {
                    if mask & (1 << c) != 0 {
                        let _ = writeln!(self.out, "\tsetAttr -l on \".{}\";", maya_name(a, c));
                    }
                }
            } else {
                let _ = writeln!(self.out, "\tsetAttr -l on \".{}\";", maya_name(a, -1));
            }
        }
    }

    fn create_line(&mut self, ty: &str, name: &str, parent: NodeId) {
        if parent == NO_NODE {
            let _ = writeln!(self.out, "createNode {ty} -n {};", quoted(name));
        } else {
            let p = self.s.graph.path(parent);
            let _ = writeln!(self.out, "createNode {ty} -n {} -p {};", quoted(name), quoted(&p));
        }
    }

    /// The written ancestor of a node, skipping nodes that aren't written.
    fn written_parent(&self, id: NodeId) -> NodeId {
        let g = &self.s.graph;
        let mut p = g.parent(id);
        while p != NO_NODE {
            if self.dag_written(p) {
                return p;
            }
            p = g.parent(p);
        }
        NO_NODE
    }

    fn dag_written(&self, id: NodeId) -> bool {
        let Some(n) = self.s.graph.node(id) else { return false };
        if !self.keep.contains(&id) {
            return false;
        }
        let t = n.ty();
        if t.is_a("mesh") || t.is_a("nurbsCurve") || t.is_a("nurbsSurface") {
            return !n.bool(DagNode::INTERMEDIATE_OBJECT);
        }
        // The default cameras are the application's own; every scene has them.
        if crate::cameras::is_default_camera(&self.s.graph, id) {
            return false;
        }
        t.name == "transform" || t.is_a("joint") || t.is_a("locator") || t.is_a("ikHandle") || t.is_a("camera")
    }

    fn write_dag(&mut self, id: NodeId) {
        if self.dag_written(id) {
            let parent = self.written_parent(id);
            let (name, ty) = {
                let n = self.s.graph.node(id).unwrap();
                (n.name().to_string(), n.ty().name.clone())
            };
            self.create_line(&ty, &name, parent);
            self.write_channels(id);
            if ty == "mesh" {
                self.write_mesh(id, &name, parent);
            }
            if ty == "nurbsCurve" {
                self.write_nurbs_curve(id);
            }
            if ty == "nurbsSurface" {
                self.write_nurbs_surface(id);
            }
            if ty == "camera" {
                self.write_camera(id);
            }
            if ty == "ikHandle" {
                let _ = writeln!(self.out, "\tsetAttr \".roc\" yes;");
            }
            if ty == "joint" && parent != NO_NODE && self.s.graph.node(parent).is_some_and(|n| n.ty().is_a("joint")) {
                let (a, b) = (self.s.graph.path(parent), self.s.graph.path(id));
                self.conn(&format!("{a}.s"), &format!("{b}.is"));
            }
        }
        for c in self.s.graph.children(id).to_vec() {
            self.write_dag(c);
        }
    }

    /// The curve as `.cc`: degree, spans, form, rational, dimension, knots, CVs.
    fn write_nurbs_curve(&mut self, shape: NodeId) {
        let Some(c) = crate::curves::local_curve(&mut self.s.graph, shape) else { return };
        let _ = writeln!(self.out, "\tsetAttr \".cc\" -type \"nurbsCurve\" ");
        let _ = writeln!(self.out, "\t\t{} {} {} no 3", c.degree, c.spans(), c.form.index());
        let knots: Vec<String> = c.knots.iter().map(|k| fmt_num(*k)).collect();
        let _ = writeln!(self.out, "\t\t{} {}", c.knots.len(), knots.join(" "));
        let _ = writeln!(self.out, "\t\t{}", c.cvs.len());
        for p in &c.cvs {
            let _ = writeln!(self.out, "\t\t{} {} {}", fmt_num(p.x), fmt_num(p.y), fmt_num(p.z));
        }
        let _ = writeln!(self.out, "\t\t;");
    }

    /// The surface as `.cc`: degrees, forms, rational, each knot vector, then CVs with U the outer loop.
    fn write_nurbs_surface(&mut self, shape: NodeId) {
        let Some(c) = crate::surfaces::local_surface(&mut self.s.graph, shape) else { return };
        let _ = writeln!(self.out, "\tsetAttr \".cc\" -type \"nurbsSurface\" ");
        let _ = writeln!(self.out, "\t\t{} {} {} {} no ", c.degree_u, c.degree_v, c.form_u.index(), c.form_v.index());
        for k in [&c.knots_u, &c.knots_v] {
            let t: Vec<String> = k.iter().map(|x| fmt_num(*x)).collect();
            let _ = writeln!(self.out, "\t\t{} {}", k.len(), t.join(" "));
        }
        let _ = writeln!(self.out, "\t\t{}", c.nu * c.nv);
        for u in 0..c.nu {
            for v in 0..c.nv {
                let p = c.cv(u, v);
                let _ = writeln!(self.out, "\t\t{} {} {}", fmt_num(p.x), fmt_num(p.y), fmt_num(p.z));
            }
        }
        let _ = writeln!(self.out, "\t\t;");
    }

    fn write_camera(&mut self, shape: NodeId) {
        use crate::cameras as cam;
        let Some(n) = self.s.graph.node(shape) else { return };
        let d = n.data();
        for (attr, at, def) in [(".fl", cam::FOCAL_LENGTH, 35.0), (".hfa", cam::H_APERTURE, 1.41732), (".vfa", cam::V_APERTURE, 0.94488), (".ncp", cam::NEAR_CLIP, 0.1), (".fcp", cam::FAR_CLIP, 10000.0), (".coi", cam::COI, 5.0), (".ow", cam::ORTHO_WIDTH, 30.0)] {
            let v = d.float(at);
            if (v - def).abs() > 1e-9 {
                let _ = writeln!(self.out, "\tsetAttr \"{attr}\" {};", fmt_num(v));
            }
        }
        if d.bool(cam::ORTHOGRAPHIC) {
            let _ = writeln!(self.out, "\tsetAttr \".o\" yes;");
        }
    }

    fn write_mesh(&mut self, shape: NodeId, name: &str, parent: NodeId) {
        let g = &mut self.s.graph;
        let path = g.path(shape);
        let skin = skin::skin_of(g, shape);
        let out_plug = PlugRef::new(shape, MeshNode::OUT_MESH as i32);
        self.shading.push(format!("connectAttr {} \":initialShadingGroup.dsm\" -na;", quoted(&format!("{path}.iog"))));
        match skin.filter(|sk| self.keep.contains(sk)) {
            Some(sk) => {
                let t = self.s.graph.node(sk).unwrap().type_arc().clone();
                let ip = t.attr_index("inputPolymesh").unwrap();
                let base = self.s.graph.value(&PlugRef::new(sk, ip as i32)).ok().and_then(|v| v.as_mesh().cloned());
                let Some(base) = base else { return };
                let stored = stored_mesh(&base);
                let orig = self.fresh(&format!("{name}Orig"));
                self.create_line("mesh", &orig, parent);
                let _ = writeln!(self.out, "\tsetAttr -k off \".v\";");
                let _ = writeln!(self.out, "\tsetAttr \".io\" yes;");
                write_stored(&mut self.out, &stored);
                self.write_skin_node_later(sk, &path, &orig, &stored);
            }
            None => {
                let m = self.s.graph.value(&out_plug).ok().and_then(|v| v.as_mesh().cloned());
                if let Some(m) = m {
                    write_stored(&mut self.out, &stored_mesh(&m));
                }
            }
        }
    }

    /// Skin nodes are written after the DAG; this keeps the point of the mesh they bind.
    fn write_skin_node_later(&mut self, sk: NodeId, shape_path: &str, orig: &str, stored: &Stored) {
        self.conn(&format!("{orig}.w"), &format!("{}.ip[0].ig", self.s.graph.path(sk)));
        self.conn(&format!("{}.og[0]", self.s.graph.path(sk)), &format!("{shape_path}.i"));
        let text = self.skin_text(sk, stored);
        self.pending_nodes_push(text);
    }

    fn pending_nodes_push(&mut self, text: String) {
        self.pending.push(text);
    }

    fn skin_text(&mut self, sk: NodeId, stored: &Stored) -> String {
        let mut o = String::new();
        let name = self.s.graph.path(sk);
        let _ = writeln!(o, "createNode skinCluster -n {};", quoted(&name));
        let Some(bind) = skin::bind_of(&self.s.graph, sk) else { return o };
        // Weights per written point.
        let mut per_point: Vec<&[(u32, f64)]> = vec![&[]; stored.points.len()];
        for (p, w) in bind.weights.iter().enumerate() {
            if let Some(&d) = stored.point_of.get(p) {
                if d >= 0 {
                    per_point[d as usize] = w;
                }
            }
        }
        let _ = writeln!(o, "\tsetAttr -s {} \".wl\";", per_point.len());
        for (p, w) in per_point.iter().enumerate() {
            let runs = weight_runs(w);
            if runs.len() > 1 {
                let _ = writeln!(o, "\tsetAttr -s {} \".wl[{p}].w\";", w.len());
            }
            for run in runs {
                let (first, last) = (run[0].0, run[run.len() - 1].0);
                let vals = run.iter().map(|x| x.1.to_string()).collect::<Vec<_>>().join(" ");
                if first == last {
                    let _ = writeln!(o, "\tsetAttr \".wl[{p}].w[{first}]\" {vals};");
                } else {
                    let _ = writeln!(o, "\tsetAttr -s {} \".wl[{p}].w[{first}:{last}]\"  {vals};", run.len());
                }
            }
        }
        let infs = skin::influences(&self.s.graph, sk);
        let _ = writeln!(o, "\tsetAttr -s {} \".pm\";", infs.len());
        for &(e, _) in &infs {
            let m = bind.bind_pre.get(e as usize).copied().unwrap_or(DMat4::IDENTITY);
            let _ = writeln!(o, "\tsetAttr \".pm[{e}]\" -type \"matrix\" {};", matrix_text(&m));
        }
        let _ = writeln!(o, "\tsetAttr \".gm\" -type \"matrix\" {};", matrix_text(&bind.geom));
        let _ = writeln!(o, "\tsetAttr -s {} \".ma\";", infs.len());
        let t = self.s.graph.node(sk).unwrap().type_arc().clone();
        let n = self.s.graph.node(sk).unwrap();
        let (mi, mmi, dr, en, sm, nw) = (
            n.data().int(t.attr_index("maxInfluences").unwrap()),
            n.bool(t.attr_index("maintainMaxInfluences").unwrap()),
            n.data().float(t.attr_index("dropoffRate").unwrap()),
            n.data().float(t.attr_index("envelope").unwrap()),
            n.data().int(t.attr_index("skinningMethod").unwrap()),
            n.data().int(t.attr_index("normalizeWeights").unwrap()),
        );
        if !infs.is_empty() {
            let _ = writeln!(o, "\tsetAttr -s {} \".dpf[0:{}]\"  {};", infs.len(), infs.iter().map(|x| x.0).max().unwrap_or(0), vec![dr.to_string(); infs.iter().map(|x| x.0).max().unwrap_or(0) as usize + 1].join(" "));
        }
        let _ = writeln!(o, "\tsetAttr -s {} \".lw\";", infs.len());
        if en != 1.0 {
            let _ = writeln!(o, "\tsetAttr \".en\" {en};");
        }
        let _ = writeln!(o, "\tsetAttr \".mmi\" {};", if mmi { "yes" } else { "no" });
        let _ = writeln!(o, "\tsetAttr \".mi\" {mi};");
        let _ = writeln!(o, "\tsetAttr \".dr\" {dr};");
        let _ = writeln!(o, "\tsetAttr \".skm\" {sm};");
        let _ = writeln!(o, "\tsetAttr \".nw\" {nw};");
        let _ = writeln!(o, "\tsetAttr \".ucm\" yes;");
        for &(e, j) in &infs {
            let jp = self.s.graph.path(j);
            if bind.locks.get(e as usize).copied().unwrap_or(false) {
                let _ = writeln!(o, "setAttr {} yes;", quoted(&format!("{jp}.liw")));
            }
            self.conn(&format!("{jp}.wm"), &format!("{name}.ma[{e}]"));
            self.conn(&format!("{jp}.liw"), &format!("{name}.lw[{e}]"));
            self.conn(&format!("{jp}.obcc"), &format!("{name}.ifcl[{e}]"));
        }
        o
    }

    fn write_curve(&mut self, id: NodeId) {
        let n = self.s.graph.node(id).unwrap();
        let t = n.type_arc().clone();
        let name = n.name().to_string();
        let keys = anim::keys_of(&self.s.graph, id);
        let n = self.s.graph.node(id).unwrap();
        let (pre, post) = (n.data().int(t.attr_index("preInfinity").unwrap()), n.data().int(t.attr_index("postInfinity").unwrap()));
        let _ = writeln!(self.out, "createNode {} -n {};", t.name, quoted(&name));
        let _ = writeln!(self.out, "\tsetAttr \".tan\" 18;");
        let _ = writeln!(self.out, "\tsetAttr \".wgt\" no;");
        let count = keys.keys.len();
        if count > 0 {
            let last = range_text(0, count - 1);
            let pairs = keys.keys.iter().map(|k| format!("{} {}", k.t, k.v)).collect::<Vec<_>>().join(" ");
            let _ = writeln!(self.out, "\tsetAttr -s {count} \".ktv[{last}]\"  {pairs};");
            let list = |f: &dyn Fn(&Key) -> String| keys.keys.iter().map(f).collect::<Vec<_>>().join(" ");
            if keys.keys.iter().any(|k| k.itt != Tangent::Auto) {
                let _ = writeln!(self.out, "\tsetAttr -s {count} \".kit[{last}]\"  {};", list(&|k| tangent_code(k.itt).to_string()));
            }
            if keys.keys.iter().any(|k| k.ott != Tangent::Auto) {
                let _ = writeln!(self.out, "\tsetAttr -s {count} \".kot[{last}]\"  {};", list(&|k| tangent_code(k.ott).to_string()));
            }
            let broken = |k: &Key| (k.itt == Tangent::Fixed || k.ott == Tangent::Fixed) && k.in_slope != k.out_slope;
            if keys.keys.iter().any(broken) {
                let _ = writeln!(self.out, "\tsetAttr -s {count} \".ktl[{last}]\"  {};", list(&|k| if broken(k) { "no".into() } else { "yes".to_string() }));
            }
            if keys.keys.iter().any(|k| k.breakdown) {
                let _ = writeln!(self.out, "\tsetAttr -s {count} \".kbd[{last}]\"  {};", list(&|k| if k.breakdown { "yes".into() } else { "no".to_string() }));
            }
            // Fixed slopes as vectors one frame long.
            let unit = if t.name.ends_with('A') { std::f64::consts::PI / 180.0 } else { 1.0 };
            let dx = 1.0 / anim::fps();
            for (i, k) in keys.keys.iter().enumerate() {
                if k.itt == Tangent::Fixed {
                    let _ = writeln!(self.out, "\tsetAttr \".kix[{i}]\" {dx};");
                    let _ = writeln!(self.out, "\tsetAttr \".kiy[{i}]\" {};", k.in_slope * unit);
                }
                if k.ott == Tangent::Fixed {
                    let _ = writeln!(self.out, "\tsetAttr \".kox[{i}]\" {dx};");
                    let _ = writeln!(self.out, "\tsetAttr \".koy[{i}]\" {};", k.out_slope * unit);
                }
            }
        }
        if pre != 0 {
            let _ = writeln!(self.out, "\tsetAttr \".pre\" {pre};");
        }
        if post != 0 {
            let _ = writeln!(self.out, "\tsetAttr \".pst\" {post};");
        }
    }

    fn write_constraint(&mut self, id: NodeId, kind: usize) {
        let g = &self.s.graph;
        let n = g.node(id).unwrap();
        let t = n.type_arc().clone();
        let name = n.name().to_string();
        let tg = t.attr_index("target.targetMatrix").unwrap() as i32;
        let mut targets: Vec<(i32, NodeId)> = g.connections().into_iter().filter(|(d, _)| d.node == id && d.attr == tg && d.element >= 0).map(|(d, s)| (d.element, s.node)).collect();
        targets.sort();
        let Some(driven) = crate::constraint::driven_of(g, id).filter(|d| self.keep.contains(d)) else { return };
        if targets.iter().any(|(_, t)| !self.keep.contains(t)) {
            self.warnings.push(format!("{name}: a target is not exported; constraint omitted"));
            return;
        }
        let path = g.path(driven);
        let aliases = self.s.run(&t.name, json!({"args": [name.clone()], "query": true, "queried": ["weightAliasList"]})).unwrap_or(Value::Null);
        let mut weights: Vec<f64> = Vec::new();
        for alias in aliases.as_array().into_iter().flatten().filter_map(Value::as_str) {
            weights.push(self.s.run("getAttr", json!({"args": [format!("{name}.{alias}")]})).ok().and_then(|v| v.as_f64()).unwrap_or(1.0));
        }
        let g = &self.s.graph;
        let mut body = String::new();
        let mut conns: Vec<(String, String)> = Vec::new();
        let _ = writeln!(body, "createNode {} -n {} -p {};", t.name, quoted(&name), quoted(&path));
        let c = g.path(driven) + "|" + &name;
        let _ = writeln!(body, "\tsetAttr -s {} \".tg\";", targets.len());
        for &(e, target) in &targets {
            let tp = g.path(target);
            let tname = g.node(target).unwrap().name().rsplit(':').next().unwrap_or("").to_string();
            let w = weights.get(e as usize).copied().unwrap_or(1.0);
            let _ = writeln!(body, "\taddAttr -dcb 0 -ci true -k true -sn \"w{e}\" -ln \"{tname}W{e}\" -dv 1 -min 0 -at \"double\";");
            if w != 1.0 {
                let _ = writeln!(body, "\tsetAttr -k on \".w{e}\" {w};");
            }
            conns.push((format!("{c}.w{e}"), format!("{c}.tg[{e}].tw")));
            let is_joint = g.node(target).unwrap().ty().is_a("joint");
            let leaves: &[(&str, &str)] = match kind {
                0 => &[("t", "tt"), ("rp", "trp"), ("rpt", "trt"), ("r", "tr"), ("ro", "tro"), ("s", "ts"), ("pm", "tpm")],
                1 | 4 => &[("t", "tt"), ("rp", "trp"), ("rpt", "trt"), ("pm", "tpm")],
                2 => &[("r", "tr"), ("ro", "tro"), ("pm", "tpm")],
                _ => &[("s", "ts"), ("pm", "tpm")],
            };
            for (a, b) in leaves {
                conns.push((format!("{tp}.{a}"), format!("{c}.tg[{e}].{b}")));
            }
            if is_joint && matches!(kind, 0 | 2) {
                conns.push((format!("{tp}.jo"), format!("{c}.tg[{e}].tjo")));
                conns.push((format!("{tp}.ssc"), format!("{c}.tg[{e}].tsc")));
                conns.push((format!("{tp}.is"), format!("{c}.tg[{e}].tis")));
            }
            if t.name == "parentConstraint" {
                let n = g.node(id).unwrap();
                let at = |a: &str, e: i32| -> DVec3 {
                    t.attr_index(a).and_then(|i| n.data().element(i, e).cloned()).and_then(|v| if let PlugValue::Vec3(x) = v { Some(x) } else { None }).unwrap_or(DVec3::ZERO)
                };
                let (tot, tor) = (at("target.targetOffsetTranslate", e), at("target.targetOffsetRotate", e));
                if tot != DVec3::ZERO {
                    let _ = writeln!(body, "\tsetAttr \".tg[{e}].tot\" -type \"double3\" {} ;", vec_text(tot));
                }
                if tor != DVec3::ZERO {
                    let _ = writeln!(body, "\tsetAttr \".tg[{e}].tor\" -type \"double3\" {} ;", vec_text(tor));
                }
            }
        }
        // The other kinds hold one offset for the node.
        if let Some(i) = t.attr_index("offset").filter(|_| t.name != "parentConstraint") {
            let v = g.node(id).unwrap().data().vec3(i);
            let default = if t.name == "scaleConstraint" { DVec3::ONE } else { DVec3::ZERO };
            if v != default {
                let _ = writeln!(body, "\tsetAttr \".o\" -type \"double3\" {} ;", vec_text(v));
            }
        }
        if t.name == "aimConstraint" {
            let n = g.node(id).unwrap();
            for (attr, short) in [("aimVector", "a"), ("upVector", "u"), ("worldUpVector", "wu")] {
                if let Some(i) = t.attr_index(attr) {
                    let _ = writeln!(body, "\tsetAttr \".{short}\" -type \"double3\" {} ;", vec_text(n.data().vec3(i)));
                }
            }
            if let Some(i) = t.attr_index("worldUpType") {
                let _ = writeln!(body, "\tsetAttr \".wut\" {};", n.data().int(i));
            }
        }
        conns.push((format!("{path}.pim"), format!("{c}.cpim")));
        if kind != 3 {
            conns.push((format!("{path}.ro"), format!("{c}.cro")));
        }
        if matches!(kind, 0 | 1 | 4) {
            conns.push((format!("{path}.rp"), format!("{c}.crp")));
            conns.push((format!("{path}.rpt"), format!("{c}.crt")));
        }
        let outs: &[(&str, &str)] = match kind {
            0 => &[("ct", "t"), ("cr", "r")],
            1 => &[("ct", "t")],
            3 => &[("cs", "s")],
            _ => &[("cr", "r")],
        };
        for (o, a) in outs {
            for axis in ["x", "y", "z"] {
                conns.push((format!("{c}.{o}{axis}"), format!("{path}.{a}{axis}")));
            }
        }
        self.pending.push(body);
        for (a, b) in conns {
            self.conn(&a, &b);
        }
    }

    fn write_ik(&mut self, handle: NodeId) {
        let g = &self.s.graph;
        let t = g.node(handle).unwrap().type_arc().clone();
        let _ = t;
        let wm = PlugRef::new(handle, Transform::WORLD_MATRIX as i32);
        let Some(sv) = g.destinations(&wm).into_iter().map(|d| d.node).find(|&d| g.node(d).is_some_and(|n| n.ty().name == "ikSolve")) else { return };
        let st = g.node(sv).unwrap().type_arc().clone();
        let mut chain = crate::ik::joints_of(g, sv);
        let Some(end) = crate::ik::end_of(g, handle) else { return };
        // The solver drives the chain above the end joint; the end joint belongs to it all the same.
        if chain.last() != Some(&end) {
            chain.push(end);
        }
        let Some(&start) = chain.first() else { return };
        if chain.iter().any(|j| !self.keep.contains(j)) {
            let name = g.path(handle);
            self.warnings.push(format!("{name}: its joints are not exported; effector and solver omitted"));
            return;
        }
        let data = rig::json_of(g.node(sv).unwrap().data(), st.attr_index("chain").unwrap());
        let solver = data.get("solver").and_then(Value::as_str).unwrap_or("ikRPsolver").to_string();
        let parent = g.parent(end);
        if parent == NO_NODE {
            return;
        }
        let eff_name = crate::ik::effector_of(&self.s.graph, handle).and_then(|e| self.s.graph.node(e).map(|n| n.name().to_string())).unwrap_or_else(|| self.fresh("effector1"));
        let g = &self.s.graph;
        let hp = g.path(handle);
        let (sp, ep, pp) = (g.path(start), g.path(end), g.path(parent));
        let eff_path = format!("{pp}|{eff_name}");
        let end_t = g.node(end).unwrap().data().vec3(Transform::TRANSLATE);
        let mut body = String::new();
        let _ = writeln!(body, "createNode ikEffector -n {} -p {};", quoted(&eff_name), quoted(&pp));
        let _ = writeln!(body, "\tsetAttr \".v\" no;");
        let _ = writeln!(body, "\tsetAttr \".t\" -type \"double3\" {} ;", vec_text(end_t));
        let _ = writeln!(body, "\tsetAttr \".hd\" yes;");
        self.pending.push(body);
        if self.solvers.insert(solver.clone()) {
            self.pending.push(format!("createNode {solver} -n {};\n", quoted(&solver)));
            self.shading.push(format!("connectAttr {} \":ikSystem.sol\" -na;", quoted(&format!("{solver}.msg"))));
        }
        self.conn(&format!("{sp}.msg"), &format!("{hp}.hsj"));
        self.conn(&format!("{eff_path}.hp"), &format!("{hp}.hee"));
        self.conn(&format!("{solver}.msg"), &format!("{hp}.hsv"));
        for axis in ["x", "y", "z"] {
            self.conn(&format!("{ep}.t{axis}"), &format!("{eff_path}.t{axis}"));
        }
        self.conn(&format!("{ep}.ro"), &format!("{eff_path}.ro"));
        self.conn(&format!("{ep}.jo"), &format!("{eff_path}.jo"));
        self.conn(&format!("{pp}.s"), &format!("{eff_path}.is"));
    }

    fn write_pose(&mut self, id: NodeId) {
        let g = &self.s.graph;
        let n = g.node(id).unwrap();
        let t = n.type_arc().clone();
        let name = n.name().to_string();
        let wm = t.attr_index("worldMatrix").unwrap() as i32;
        let data = rig::json_of(n.data(), t.attr_index("poseData").unwrap());
        let bind = n.bool(t.attr_index("bindPose").unwrap());
        let mut members: Vec<(i32, NodeId)> = g.connections().into_iter().filter(|(d, _)| d.node == id && d.attr == wm && d.element >= 0).map(|(d, s)| (d.element, s.node)).collect();
        members.sort();
        if members.is_empty() || members.iter().any(|(_, j)| !self.keep.contains(j)) {
            return;
        }
        let mut body = String::new();
        let _ = writeln!(body, "createNode dagPose -n {};", quoted(&name));
        let _ = writeln!(body, "\tsetAttr -s {} \".wm\";", members.len());
        let _ = writeln!(body, "\tsetAttr -s {} \".xm\";", members.len());
        let v = |e: &Value, k: &str, d: f64| {
            let x = e.get(k).and_then(Value::as_array);
            let f = |i: usize| x.and_then(|x| x.get(i)).and_then(Value::as_f64).unwrap_or(d);
            DVec3::new(f(0), f(1), f(2))
        };
        for &(e, j) in &members {
            let item = data.get(e as usize).cloned().unwrap_or(Value::Null);
            let (tr, r, s) = (v(&item, "t", 0.0), v(&item, "r", 0.0), v(&item, "s", 1.0));
            let jn = g.node(j).unwrap();
            let jo = DQuat::from_mat4(&euler_to_matrix(jn.data().vec3(Joint::JOINT_ORIENT), RotateOrder::Xyz));
            let ro = jn.data().int(Transform::ROTATE_ORDER);
            let rad = r * (std::f64::consts::PI / 180.0);
            let _ = writeln!(
                body,
                "\tsetAttr \".xm[{e}]\" -type \"matrix\" \"xform\" {} {} {ro} {} 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 1 {} {} {} {} 1 1 1 yes;",
                vec_text(s),
                vec_text(rad),
                vec_text(tr),
                jo.x,
                jo.y,
                jo.z,
                jo.w,
            );
        }
        let _ = writeln!(body, "\tsetAttr -s {} \".m\";", members.len());
        let _ = writeln!(body, "\tsetAttr -s {} \".p\";", members.len());
        let _ = writeln!(body, "\tsetAttr \".bp\" {};", if bind { "yes" } else { "no" });
        let members_at: HashMap<NodeId, i32> = members.iter().map(|&(e, j)| (j, e)).collect();
        let mut lines = Vec::new();
        for &(e, j) in &members {
            let jp = g.path(j);
            lines.push((format!("{jp}.msg"), format!("{name}.m[{e}]")));
            lines.push((format!("{jp}.bps"), format!("{name}.wm[{e}]")));
            match members_at.get(&g.parent(j)) {
                Some(p) => lines.push((format!("{name}.m[{p}]"), format!("{name}.p[{e}]"))),
                None => lines.push((format!("{name}.w"), format!("{name}.p[{e}]"))),
            }
        }
        // The skinCluster on these joints points back at its bind pose.
        if bind {
            let skins: Vec<NodeId> = g.nodes().into_iter().filter(|&s| g.node(s).is_some_and(|n| n.ty().name == skin::SKIN_TYPE) && self.keep.contains(&s) && skin::influences(g, s).iter().any(|(_, j)| members_at.contains_key(j))).collect();
            for sk in skins {
                lines.push((format!("{name}.msg"), format!("{}.bp", g.path(sk))));
            }
        }
        self.pending.push(body);
        for (a, b) in lines {
            self.conn(&a, &b);
        }
    }

    fn write_layer(&mut self, id: NodeId) {
        use crate::display_layers as dl;
        let g = &self.s.graph;
        let n = g.node(id).unwrap();
        let name = n.name().to_string();
        let members: Vec<NodeId> = dl::members(g, id).into_iter().filter(|m| self.keep.contains(m)).collect();
        let mut body = String::new();
        let _ = writeln!(body, "createNode displayLayer -n {};", quoted(&name));
        if !n.bool(dl::VISIBILITY) {
            let _ = writeln!(body, "\tsetAttr \".v\" no;");
        }
        let dt = n.data().int(dl::DISPLAY_TYPE);
        if dt != 0 {
            let _ = writeln!(body, "\tsetAttr \".dt\" {dt};");
        }
        let _ = writeln!(body, "\tsetAttr \".do\" {};", n.data().int(dl::DISPLAY_ORDER));
        let lines: Vec<String> = members.iter().map(|&m| g.path(m)).collect();
        self.pending.push(body);
        for m in lines {
            self.conn(&format!("{name}.di"), &format!("{m}.do"));
        }
    }

    /// A utility node or expression: its type, name and every stored value that isn't the default or driven.
    fn write_plain(&mut self, id: NodeId) {
        let g = &self.s.graph;
        let n = g.node(id).unwrap();
        let t = n.type_arc().clone();
        let path = g.path(id);
        let mut text = format!("createNode {} -n {};\n", t.name, quoted(&path));
        let line = |p: &PlugRef, v: &PlugValue, out: &mut String| {
            let name = g.plug_name(p);
            let attr = name.split_once('.').map_or("", |x| x.1);
            match v {
                PlugValue::Vec3(x) => {
                    let _ = writeln!(out, "\tsetAttr \".{attr}\" -type \"double3\" {} ;", vec_text(*x));
                }
                PlugValue::Float(x) => {
                    let _ = writeln!(out, "\tsetAttr \".{attr}\" {x};");
                }
                PlugValue::Int(x) => {
                    let _ = writeln!(out, "\tsetAttr \".{attr}\" {x};");
                }
                PlugValue::Bool(x) => {
                    let _ = writeln!(out, "\tsetAttr \".{attr}\" {};", if *x { "yes" } else { "no" });
                }
                PlugValue::String(x) => {
                    let _ = writeln!(out, "\tsetAttr \".{attr}\" -type \"string\" {};", quoted(x));
                }
                PlugValue::Matrix(m) => {
                    let _ = writeln!(out, "\tsetAttr \".{attr}\" -type \"matrix\" {};", matrix_text(m));
                }
                _ => {}
            }
        };
        for (i, a) in t.attrs.iter().enumerate() {
            if !a.has(flags::INPUT) || !a.has(flags::STORABLE) {
                continue;
            }
            if a.is_array() {
                for (k, v) in n.elements(i) {
                    let p = PlugRef::element(id, i as i32, *k);
                    if g.source(&p).is_none() {
                        line(&p, v, &mut text);
                    }
                }
            } else {
                let p = PlugRef::new(id, i as i32);
                if g.source(&p).is_none() && n.value(i) != &a.default_value {
                    line(&p, n.value(i), &mut text);
                }
            }
        }
        self.out.push_str(&text);
    }

    fn write_connections(&mut self) {
        let list = self.s.graph.connections();
        for (dst, src) in list {
            let g = &self.s.graph;
            let (Some(dn), Some(sn)) = (g.node(dst.node), g.node(src.node)) else { continue };
            if !self.keep.contains(&dst.node) || !self.keep.contains(&src.node) {
                continue;
            }
            let (dt, st) = (dn.ty(), sn.ty());
            let plain = |t: &NodeType| is_plain_node(&t.name) || t.name == "unitConversion";
            if plain(dt) || plain(st) {
                // By full attribute name. Maya's matrix outputs are arrays: worldMatrix[0].
                let name = |p: &PlugRef| {
                    let text = g.plug_name(p);
                    let one_of = matches!(g.attr(p).name.as_str(), "worldMatrix" | "worldInverseMatrix" | "parentMatrix" | "parentInverseMatrix");
                    if one_of && p.element < 0 {
                        format!("{text}[0]")
                    } else {
                        text
                    }
                };
                let (a, b) = (name(&src), name(&dst));
                self.conn(&a, &b);
                continue;
            }
            let curve = anim::is_curve(g, src.node) || DRIVEN_CURVES.contains(&st.name.as_str());
            let driven_curve = DRIVEN_CURVES.contains(&dt.name.as_str());
            if !(curve || self.dag_written(src.node)) || !(driven_curve || self.dag_written(dst.node)) {
                continue;
            }
            let plug_of = |t: &NodeType, p: &PlugRef| -> Option<String> {
                let a = &t.attrs[p.attr as usize];
                channels_of(t).contains(&a.name.as_str()).then(|| maya_name(a, p.component)).or_else(|| a.has(flags::USER).then(|| maya_name(a, p.component)))
            };
            let from = if curve {
                (src.attr as usize == st.attr_index("output").unwrap_or(usize::MAX)).then(|| "o".to_string())
            } else {
                plug_of(st, &src)
            };
            let to = if driven_curve {
                (dst.attr as usize == dt.attr_index("input").unwrap_or(usize::MAX)).then(|| "i".to_string())
            } else {
                plug_of(dt, &dst)
            };
            let (Some(from), Some(to)) = (from, to) else { continue };
            let (a, b) = (g.path(src.node), g.path(dst.node));
            self.conn(&format!("{a}.{from}"), &format!("{b}.{to}"));
        }
    }
}

/// addAttr lines for a user attribute.
fn attr_add_lines(a: &AttrDef) -> String {
    let short = if a.short_name.is_empty() { a.name.as_str() } else { a.short_name.as_str() };
    let k = if a.has(flags::KEYABLE) { " -k true" } else { "" };
    let range = |o: &mut String| {
        if let Some(m) = a.min_value {
            let _ = write!(o, " -min {m}");
        }
        if let Some(m) = a.max_value {
            let _ = write!(o, " -max {m}");
        }
    };
    let mut o = String::new();
    match a.ty {
        PlugType::Float | PlugType::Int | PlugType::Bool => {
            let at = match a.ty {
                PlugType::Float => "double",
                PlugType::Int => "long",
                _ => "bool",
            };
            let _ = write!(o, "\taddAttr -ci true{k} -sn {} -ln {}", quoted(short), quoted(&a.name));
            let dv = a.default_value.scalar().unwrap_or(0.0);
            if dv != 0.0 {
                let _ = write!(o, " -dv {dv}");
            }
            range(&mut o);
            let _ = writeln!(o, " -at {};", quoted(at));
        }
        PlugType::Enum => {
            let _ = writeln!(o, "\taddAttr -ci true{k} -sn {} -ln {} -min 0 -max {} -en {} -at \"enum\";", quoted(short), quoted(&a.name), a.enum_names.len().saturating_sub(1), quoted(&a.enum_names.join(":")));
        }
        PlugType::String => {
            let _ = writeln!(o, "\taddAttr -ci true -sn {} -ln {} -dt \"string\";", quoted(short), quoted(&a.name));
        }
        PlugType::Vec3 => {
            let _ = writeln!(o, "\taddAttr -ci true{k} -sn {} -ln {} -at \"double3\" -nc 3;", quoted(short), quoted(&a.name));
            for c in 0..3 {
                let child_short = if a.child_short[c].is_empty() { &a.child_names[c] } else { &a.child_short[c] };
                let _ = writeln!(o, "\taddAttr -ci true -sn {} -ln {} -at \"double\" -p {};", quoted(child_short), quoted(&a.child_names[c]), quoted(&a.name));
            }
        }
        _ => {}
    }
    o
}

/// Contiguous runs of influence indices in a point's sorted weights.
fn weight_runs(w: &[(u32, f64)]) -> Vec<&[(u32, f64)]> {
    let mut out = Vec::new();
    let mut start = 0;
    for i in 1..=w.len() {
        if i == w.len() || w[i].0 != w[i - 1].0 + 1 {
            if start < i {
                out.push(&w[start..i]);
            }
            start = i;
        }
    }
    out
}

/// What export selected takes: the nodes, what lies below them, their parents, and the skeletons their skins use.
fn closure(g: &Graph, roots: &[NodeId]) -> HashSet<NodeId> {
    let mut keep: HashSet<NodeId> = HashSet::new();
    let add_dag = |g: &Graph, keep: &mut HashSet<NodeId>, id: NodeId| {
        keep.insert(id);
        keep.extend(g.descendants(id));
        let mut p = g.parent(id);
        while p != NO_NODE {
            keep.insert(p);
            p = g.parent(p);
        }
    };
    for &r in roots {
        add_dag(g, &mut keep, r);
    }
    // Skins on the meshes, with their influences.
    let shapes: Vec<NodeId> = keep.iter().copied().filter(|&n| g.node(n).is_some_and(|x| x.ty().is_a("mesh"))).collect();
    for shape in shapes {
        if let Some(sk) = skin::skin_of(g, shape) {
            keep.insert(sk);
            for (_, j) in skin::influences(g, sk) {
                add_dag(g, &mut keep, j);
            }
        }
    }
    // Everything else that only serves what is kept.
    for id in g.nodes() {
        let Some(n) = g.node(id) else { continue };
        let t = n.ty();
        if t.is_a("animCurve") || DRIVEN_CURVES.contains(&t.name.as_str()) {
            if anim::driven_by(g, id).iter().any(|d| keep.contains(&d.node)) {
                keep.insert(id);
            }
        } else if t.name == crate::display_layers::TYPE_NAME {
            if crate::display_layers::members(g, id).iter().any(|m| keep.contains(m)) {
                keep.insert(id);
            }
        } else if rig::CONSTRAINT_TYPES.contains(&t.name.as_str()) || t.name == "dagPose" {
            keep.insert(id);
        }
    }
    keep
}

/// The scene as .ma text. `only` limits it to some objects (export selected). Returns the text and warnings.
pub fn write_text(s: &mut Session, file_name: &str, only: Option<&[NodeId]>) -> (String, Vec<String>) {
    register_types();
    s.graph.evaluate();
    let g = &s.graph;
    let keep = match only {
        Some(list) => closure(g, list),
        None => g.nodes().into_iter().collect(),
    };
    let used: HashSet<String> = g.nodes().iter().filter_map(|&id| g.node(id).map(|n| n.name().to_string())).collect();
    let roots = g.roots().to_vec();
    let mut w = Writer { s, out: String::new(), conns: Vec::new(), shading: Vec::new(), warnings: Vec::new(), used, keep, preferred: HashMap::new(), pending: Vec::new(), solvers: HashSet::new() };

    // Preferred angles from the chains the IK solvers remember.
    let g = &w.s.graph;
    let solves: Vec<NodeId> = g.nodes().into_iter().filter(|&n| g.node(n).is_some_and(|x| x.ty().name == "ikSolve")).collect();
    for sv in solves {
        let g = &w.s.graph;
        let st = g.node(sv).unwrap().type_arc().clone();
        let data = rig::json_of(g.node(sv).unwrap().data(), st.attr_index("chain").unwrap());
        let rests = data.get("joints").and_then(Value::as_array).cloned().unwrap_or_default();
        let mut entries = Vec::new();
        let driven = crate::ik::joints_of(g, sv);
        for (rest, &j) in rests.iter().zip(&driven) {
            let f = |k: usize| rest.get("r").and_then(|r| r.get(k)).and_then(Value::as_f64).unwrap_or(0.0);
            entries.push((j, DVec3::new(f(0), f(1), f(2))));
        }
        w.preferred.extend(entries);
    }

    let fps = anim::fps();
    let name = std::path::Path::new(file_name).file_name().and_then(|n| n.to_str()).unwrap_or(file_name).to_string();
    let _ = writeln!(w.out, "//Maya ASCII 2024 scene");
    let _ = writeln!(w.out, "//Name: {name}");
    let _ = writeln!(w.out, "//Codeset: UTF-8");
    let _ = writeln!(w.out, "requires maya \"2024\";");
    let _ = writeln!(w.out, "currentUnit -l centimeter -a degree -t {};", time_unit_name(fps));
    let _ = writeln!(w.out, "fileInfo \"application\" \"aztek\";");
    let _ = writeln!(w.out, "fileInfo \"version\" \"{}\";", env!("CARGO_PKG_VERSION"));

    for r in roots {
        w.write_dag(r);
    }

    // The nodes hanging off the DAG, in creation order.
    let mut unwritten: BTreeMap<String, usize> = BTreeMap::new();
    for id in w.s.graph.nodes() {
        if !w.keep.contains(&id) {
            continue;
        }
        let Some(n) = w.s.graph.node(id) else { continue };
        let t = n.type_arc().clone();
        let ty = t.name.as_str();
        if w.dag_written(id) {
            if ty == "ikHandle" {
                w.write_ik(id);
            }
            continue;
        }
        if t.is_a("animCurve") || DRIVEN_CURVES.contains(&ty) {
            let output = PlugRef::new(id, t.attr_index("output").unwrap() as i32);
            let drives = w.s.graph.destinations(&output).iter().any(|d| {
                let dt = w.s.graph.node(d.node).unwrap().ty().name.as_str();
                w.keep.contains(&d.node) && (w.dag_written(d.node) || DRIVEN_CURVES.contains(&dt) || is_plain_node(dt))
            });
            if drives {
                w.write_curve(id);
            } else {
                w.warnings.push(format!("{}: keys on what is not written; omitted", n.name()));
            }
        } else if let Some(kind) = rig::CONSTRAINT_TYPES.iter().position(|c| *c == ty) {
            w.write_constraint(id, kind);
        } else if is_plain_node(ty) || ty == "unitConversion" {
            w.write_plain(id);
        } else if ty == "dagPose" {
            w.write_pose(id);
        } else if ty == crate::display_layers::TYPE_NAME {
            w.write_layer(id);
        } else if ty == skin::SKIN_TYPE || ty == "ikSolve" {
            // Written with their mesh and handle.
        } else if crate::deform::is_deformer(ty) || ty == "deformHandle" {
            *unwritten.entry(ty.to_string()).or_default() += 1;
        } else if t.is_a("mesh") {
            // Intermediate shapes: geometry is baked.
        } else if matches!(n.name(), "defaultLightSet" | "defaultObjectSet") {
            // Every scene has these.
        } else if !n_is_baked(&t) {
            *unwritten.entry(ty.to_string()).or_default() += 1;
        }
    }
    let pending = std::mem::take(&mut w.pending);
    for text in pending {
        w.out.push_str(&text);
    }
    for (ty, count) in unwritten {
        w.warnings.push(format!("{ty}: {count} not written; meshes are baked, other nodes omitted"));
    }

    w.write_connections();

    // Maya keeps the playback range in this script node.
    let r = anim::range();
    let _ = writeln!(w.out, "createNode script -n \"sceneConfigurationScriptNode\";");
    let _ = writeln!(w.out, "\tsetAttr \".b\" -type \"string\" \"playbackOptions -min {} -max {} -ast {} -aet {} \";", r.min, r.max, r.anim_start, r.anim_end);
    let _ = writeln!(w.out, "\tsetAttr \".st\" 6;");

    let frame = anim::frame(&w.s.graph);
    let _ = writeln!(w.out, "select -ne :time1;");
    let _ = writeln!(w.out, "\tsetAttr \".o\" {frame};");
    let _ = writeln!(w.out, "\tsetAttr \".unw\" {frame};");
    let shading_count = w.shading.iter().filter(|l| l.contains(":initialShadingGroup")).count();
    if shading_count > 0 {
        let _ = writeln!(w.out, "select -ne :initialShadingGroup;");
        let _ = writeln!(w.out, "\tsetAttr -s {shading_count} \".dsm\";");
        let _ = writeln!(w.out, "\tsetAttr \".ro\" yes;");
    }

    let mut lines = std::mem::take(&mut w.conns);
    lines.sort();
    lines.dedup();
    for l in lines {
        let _ = writeln!(w.out, "{l}");
    }
    let mut lines = std::mem::take(&mut w.shading);
    lines.sort();
    for l in lines {
        let _ = writeln!(w.out, "{l}");
    }
    let _ = writeln!(w.out, "// End of {name}");
    (w.out, w.warnings)
}

/// Nodes whose result the mesh holds once baked: anything that outputs a mesh.
fn n_is_baked(t: &NodeType) -> bool {
    // Curve history bakes into the curve's .cc as mesh history does into .vt; the info nodes are written below.
    if t.name.starts_with("makeNurb") || matches!(t.name.as_str(), "loft" | "revolve" | "extrude" | "boundary" | "rebuildSurface" | "reverseSurface" | "insertKnotSurface" | "offsetSurface" | "closeSurface" | "squareSrf" | "planarTrim" | "boundaryBase") {
        return true;
    }
    const CURVE_HISTORY: &[&str] = &["makeNurbCircle", "makeNurbsSquare", "rebuildCurve", "reverseCurve", "closeCurve", "smoothCurve", "fitBspline", "extendCurve", "offsetCurve", "insertKnotCurve"];
    CURVE_HISTORY.contains(&t.name.as_str()) || t.name == "camera" || t.attrs.iter().any(|a| a.ty == PlugType::Mesh && a.has(flags::OUTPUT))
}

/// A number as short as it stays exact.
fn fmt_num(v: f64) -> String {
    format!("{v}")
}

// ---------------------------------------------------------------------------------------------------------------------
// Files

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    MayaAscii,
    Aztek,
}

impl Format {
    pub fn of_path(path: &str) -> Format {
        if extension(path) == "ma" {
            Format::MayaAscii
        } else {
            Format::Aztek
        }
    }
    fn of_type(ty: &str) -> CmdResult<Format> {
        match ty {
            "mayaAscii" => Ok(Format::MayaAscii),
            "aztek" => Ok(Format::Aztek),
            "mayaBinary" => fail!("file: mayaBinary can't be read or written; use mayaAscii"),
            other => fail!("file: unknown file type {other}"),
        }
    }
    fn type_name(self) -> &'static str {
        match self {
            Format::MayaAscii => "mayaAscii",
            Format::Aztek => "aztek",
        }
    }
}

fn extension(path: &str) -> String {
    std::path::Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

/// Reads a file's text, refusing Maya binary files.
fn read_scene_text(path: &str) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    if extension(path) == "mb" || bytes.starts_with(b"FOR4") || bytes.starts_with(b"FOR8") {
        return Err(format!("{path}: Maya binary files can't be read; save it as Maya ASCII"));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Writes the scene to path in the format given. Returns warnings; does not change the scene's name.
pub fn write_file(s: &mut Session, path: &str, format: Format, only: Option<&[NodeId]>) -> Result<Vec<String>, String> {
    if extension(path) == "mb" && format == Format::Aztek {
        return Err(format!("{path}: Maya binary files can't be written; use .ma"));
    }
    match format {
        Format::MayaAscii => {
            let (text, warnings) = write_text(s, path, only);
            std::fs::write(path, text).map_err(|e| format!("{path}: {e}"))?;
            for w in &warnings {
                log::warn!("{w}");
            }
            Ok(warnings)
        }
        Format::Aztek => {
            s.graph.evaluate();
            let mut doc = crate::scene_file::serialize(&s.graph, false);
            let r = anim::range();
            doc["playbackOptions"] = json!({"minTime": r.min, "maxTime": r.max, "animationStartTime": r.anim_start, "animationEndTime": r.anim_end, "framesPerSecond": anim::fps(), "currentTime": anim::frame(&s.graph)});
            let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
            std::fs::write(path, text).map_err(|e| format!("{path}: {e}"))?;
            Ok(Vec::new())
        }
    }
}

/// Saves under path and makes it the scene's name.
pub fn save_scene(s: &mut Session, path: &str, format: Format) -> Result<Vec<String>, String> {
    let w = write_file(s, path, format, None)?;
    s.scene_path = Some(path.to_string());
    Ok(w)
}

/// Replaces the scene with the file's: `.ma` or Aztek's own format, by extension. Undo restarts.
pub fn open_scene(s: &mut Session, path: &str) -> Result<Vec<String>, String> {
    let text = read_scene_text(path)?;
    let warnings = if Format::of_path(path) == Format::MayaAscii {
        let _ = s.run("newScene", json!({}));
        let g = &mut s.graph;
        g.begin_batch();
        g.undo_stack_mut().set_enabled(false);
        let w = read_text(s, &text, false);
        let g = &mut s.graph;
        g.undo_stack_mut().set_enabled(true);
        g.undo_stack_mut().clear();
        g.end_batch();
        w
    } else {
        let doc: Value = serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
        let _ = s.run("newScene", json!({}));
        let w = crate::scene_file::deserialize(&mut s.graph, &doc).map_err(|e| format!("{path}: {e}"))?;
        if let Some(p) = doc.get("playbackOptions").and_then(Value::as_object) {
            let mut call = p.clone();
            let now = call.remove("currentTime");
            let _ = s.run("playbackOptions", Value::Object(call));
            if let Some(t) = now {
                let _ = s.run("currentTime", json!({"args": [t]}));
            }
        }
        w
    };
    let _ = s.run("flushUndo", json!({}));
    s.scene_path = Some(path.to_string());
    Ok(warnings)
}

/// Adds a `.ma` or Aztek scene file's contents to the scene. One undo step.
pub fn import_scene(s: &mut Session, path: &str) -> Result<Vec<String>, String> {
    let text = read_scene_text(path)?;
    if Format::of_path(path) == Format::MayaAscii {
        return Ok(read_text(s, &text, true));
    }
    let doc: Value = serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
    let mut warnings = Vec::new();
    crate::scene_file::instantiate(&mut s.graph, &doc, &mut warnings);
    Ok(warnings)
}

// ---------------------------------------------------------------------------------------------------------------------
// The file command

fn file_command(s: &mut Session, a: &Args) -> CmdResult {
    let path_arg = a.pos_str(0).map(str::to_string);
    let type_flag = a.get("type").and_then(Value::as_str).filter(|t| !t.is_empty()).map(str::to_string);

    if a.bool("query") {
        return Ok(file_query(s, a, path_arg.as_deref()));
    }
    if a.bool("reference") || a.bool("loadReference") {
        fail!("file: references are not supported");
    }
    if let Some(t) = &type_flag {
        s.scene_type = Some(Format::of_type(t)?);
    }
    let format_for = |s: &Session, path: &str| -> Format {
        match s.scene_type {
            Some(f) if extension(path).is_empty() || type_flag.is_some() => f,
            _ => Format::of_path(path),
        }
    };
    if a.bool("new") {
        let _ = s.run("newScene", json!({}))?;
        s.scene_path = None;
        s.scene_type = None;
        return Ok(json!("untitled"));
    }
    if let Some(name) = a.get("rename").and_then(Value::as_str) {
        s.scene_path = Some(name.to_string());
        return Ok(json!(name));
    }
    if a.bool("open") {
        let Some(path) = path_arg else { fail!("file: -open needs a path") };
        let warnings = open_scene(s, &path).map_err(aztek_ops::CommandError::new)?;
        s.scene_type = None;
        // read_text logged .ma warnings already.
        if Format::of_path(&path) != Format::MayaAscii {
            warnings.iter().for_each(|w| log::warn!("{w}"));
        }
        return Ok(json!(path));
    }
    if a.bool("import") {
        let Some(path) = path_arg else { fail!("file: -import needs a path") };
        s.graph.open_chunk("Import");
        let r = import_scene(s, &path);
        s.graph.close_chunk();
        let warnings = r.map_err(aztek_ops::CommandError::new)?;
        if Format::of_path(&path) != Format::MayaAscii {
            warnings.iter().for_each(|w| log::warn!("{w}"));
        }
        return Ok(json!(path));
    }
    if a.bool("exportSelected") || a.bool("exportAll") {
        let Some(path) = path_arg else { fail!("file: export needs a path") };
        let path = with_type_extension(&path, type_flag.as_deref());
        let format = format_for(s, &path);
        let only: Option<Vec<NodeId>> = a.bool("exportSelected").then(|| s.graph.selection().to_vec());
        if only.as_ref().is_some_and(Vec::is_empty) {
            fail!("file: nothing selected to export");
        }
        if only.is_some() && format == Format::Aztek {
            fail!("file: -exportSelected writes mayaAscii only");
        }
        write_file(s, &path, format, only.as_deref()).map_err(aztek_ops::CommandError::new)?;
        return Ok(json!(path));
    }
    if a.bool("save") {
        let Some(path) = path_arg.or_else(|| s.scene_path.clone()) else { fail!("file: the scene has no name; use file -rename first") };
        let path = with_type_extension(&path, s.scene_type.map(Format::type_name));
        let format = format_for(s, &path);
        save_scene(s, &path, format).map_err(aztek_ops::CommandError::new)?;
        return Ok(json!(path));
    }
    // -type or -options alone only set what the next save uses.
    Ok(Value::Null)
}

/// A bare name takes the extension of the type asked for, as Maya does.
fn with_type_extension(path: &str, ty: Option<&str>) -> String {
    if extension(path).is_empty() {
        match ty {
            Some("mayaAscii") => return format!("{path}.ma"),
            Some("aztek") => return format!("{path}.aztek"),
            _ => {}
        }
    }
    path.to_string()
}

fn file_query(s: &Session, a: &Args, path: Option<&str>) -> Value {
    if a.bool("exists") {
        return json!(path.map(str::to_string).or_else(|| s.scene_path.clone()).is_some_and(|p| std::path::Path::new(&p).exists()));
    }
    if a.has("type") {
        let t = s.scene_type.or(s.scene_path.as_deref().map(Format::of_path)).unwrap_or(Format::MayaAscii);
        return json!([t.type_name()]);
    }
    if a.bool("modified") {
        return json!(s.graph.undo_stack().can_undo());
    }
    if a.has("options") {
        return json!("");
    }
    let name = s.scene_path.clone().unwrap_or_default();
    if a.bool("shortName") {
        return json!(std::path::Path::new(&name).file_name().and_then(|n| n.to_str()).unwrap_or(""));
    }
    json!(name)
}

pub fn register(c: &mut Commands<Session>) {
    register_types();
    let b = |name: &str, short: &str, doc: &str| FlagDef::new(name, short, ArgType::Bool, Value::Null, doc);
    let t = |name: &str, short: &str, doc: &str| FlagDef::new(name, short, ArgType::String, Value::Null, doc);
    c.register(
        CommandDef::<Session>::new(
            "file",
            "Scene files: -new, -open path, -save, -rename name, -import path, -exportAll path, -exportSelected path; -type \
             mayaAscii. In query mode -sceneName (with -shortName for the file alone), -exists, -type, -modified.",
            file_command,
        )
        .positional("the file")
        .undoable(false)
        .flag(b("new", "new", "A new, empty scene."))
        .flag(b("open", "o", "Replace the scene with the file."))
        .flag(b("save", "s", "Write the scene under its name."))
        .flag(t("rename", "rn", "Set the scene's name, for the next save."))
        .flag(b("import", "i", "Add the file's contents to the scene."))
        .flag(b("exportAll", "ea", "Write the whole scene to the file."))
        .flag(b("exportSelected", "es", "Write the selection, what lies below it, its parents and the skeletons of its skins."))
        .flag(FlagDef::new("type", "typ", ArgType::Any, Value::Null, "mayaAscii, or aztek for Aztek's own format."))
        .flag(FlagDef::new("options", "op", ArgType::Any, Value::Null, "File translator options. Accepted for Maya scripts."))
        .flag(b("force", "f", "Don't ask about unsaved changes. Nothing asks here."))
        .flag(b("query", "q", "Query mode."))
        .flag(b("sceneName", "sn", "Query: the scene's path; empty when untitled."))
        .flag(b("shortName", "shn", "Query: with -sceneName, the file name alone."))
        .flag(b("exists", "ex", "Query: whether the file (or the scene's file) exists."))
        .flag(b("modified", "mf", "Query: whether the scene has edits."))
        .flag(b("reference", "r", "Not supported; refused."))
        .flag(b("loadReference", "lr", "Not supported; refused."))
        .flag(b("prompt", "pmt", "Accepted for Maya scripts."))
        .flag(b("ignoreVersion", "iv", "Accepted for Maya scripts."))
        .flag(b("preserveReferences", "pr", "Accepted for Maya scripts."))
        .flag(b("constructionHistory", "ch", "Accepted for Maya scripts; history is baked in .ma."))
        .flag(b("channels", "chn", "Accepted for Maya scripts."))
        .flag(b("constraints", "con", "Accepted for Maya scripts."))
        .flag(b("expressions", "exp", "Accepted for Maya scripts."))
        .flag(b("shader", "sh", "Accepted for Maya scripts."))
        .flag(b("returnNewNodes", "rnn", "Accepted for Maya scripts."))
        .flag(t("namespace", "ns", "Accepted for Maya scripts.")),
    );
}
