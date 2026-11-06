//! Port of `src/scene/Graph.h` / `Graph.cpp`: the scene's dependency graph plus its transform hierarchy.
//!
//! Differences from the C++ forced by ownership, none of them behavioural:
//! - `Graph::current()` is gone: the graph is an owned value.
//! - The undo stack lives in the graph (`UndoStack<Graph>`); `undo()`/`redo()` swap it out while it replays.
//! - Hooks and listeners are `Arc<dyn Fn>` called with the graph as an argument instead of capturing it.
//! - `Batch` is `begin_batch`/`end_batch` (or `batch(|g| ..)`), since an RAII guard would hold `&mut Graph`.
//! - The parallel evaluation takes each level's nodes out of the map, computes them on the rayon pool, and puts them
//!   back, instead of sharing raw pointers.

use std::any::Any;
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use aztek_undo::{FnAction, UndoAction, UndoShared, UndoStack};
use glam::{DMat4, DVec3};
use rayon::prelude::*;

use crate::error::{GraphError, Result};
use crate::naming::{apply_case, NamingMode, NamingPolicy, Underscores};
use crate::node::{registered, EvalContext, Node, NodeType};
use crate::plug::{
    default_for, flags, from_json, to_json, type_name, AttrDef, NodeId, PlugRef, PlugType,
    PlugValue, NO_NODE,
};
use crate::transform::Transform;

/// Cached playback on or off, and the memory it may use in MB (0: half the machine's): the general prefs' keys.
pub const CACHED_PLAYBACK_PREF: &str = "/cachedPlayback";
pub const CACHED_PLAYBACK_MEMORY_PREF: &str = "/cachedPlaybackMemoryMB";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Created,
    Deleted,
    Renamed,
    Reparented,
    Value,
    Connection,
    Selection,
    Cleared,
    /// Many changes at once (a batch ended): anything may have changed. Sent only to batched listeners.
    Reset,
    /// A node gained or lost a user attr.
    Attrs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectMode {
    Replace,
    Add,
    Remove,
    Toggle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrivenEdit {
    /// The connection wins.
    Reject,
    /// The hook changed the source (set a key, say).
    Handled,
    /// Keep the given value until the time or the source changes.
    Hold,
}

/// Decides what a set on a driven scalar plug does. `src` is the plug driving `dst`.
pub type DrivenSetHook =
    Arc<dyn Fn(&mut Graph, &PlugRef, &PlugRef, f64) -> DrivenEdit + Send + Sync>;
/// The same for a whole Vec3 driven by a Vec3 source (a rotation curve).
pub type DrivenVecSetHook =
    Arc<dyn Fn(&mut Graph, &PlugRef, &PlugRef, DVec3) -> DrivenEdit + Send + Sync>;
/// Called by the outermost `delete_node` with the node and its descendants before anything is disconnected.
pub type DeleteHook = Arc<dyn Fn(&mut Graph, &[NodeId]) + Send + Sync>;
/// Called by `evaluate` before computing anything dirty (`after == false`) and once done (`after == true`).
pub type EvaluateHook = Arc<dyn Fn(&mut Graph, bool) + Send + Sync>;
/// `immediate` listeners hear every change, even inside a batch; others get one `Change::Reset` when it ends.
/// `Graph::changed_plug()` names the plug during a Value or Connection call.
pub type Listener = Arc<dyn Fn(&Graph, Change, NodeId) + Send + Sync>;
/// The active space's world frame for a DAG node (spaces are not ported yet; they supply this).
pub type ActiveFrameFn = Arc<dyn Fn(&Node) -> Option<DMat4> + Send + Sync>;

#[derive(Default, Clone)]
struct Relations {
    parent: NodeId,
    children: Vec<NodeId>,
}

#[derive(Clone)]
struct AttrState {
    def: AttrDef,
    value: PlugValue,
    elements: Vec<(i32, PlugValue)>,
    lock: u8,
}

struct ListenerEntry {
    f: Listener,
    immediate: bool,
}

#[derive(Clone)]
struct PlaybackEntry {
    values: Vec<(usize, PlugValue)>,
    elements: Option<Vec<Vec<(i32, PlugValue)>>>,
    extra: Option<Arc<dyn Any + Send + Sync>>,
}

#[derive(Default)]
struct PlaybackInner {
    times: BTreeMap<i64, HashMap<NodeId, PlaybackEntry>>,
    /// Meshes already counted in bytes; the cache keeps them alive, so their addresses stay unique.
    counted: HashSet<usize>,
    bytes: usize,
    limit: usize,
}

struct PlaybackCache {
    inner: Mutex<PlaybackInner>,
}

fn time_key(seconds: f64) -> i64 {
    (seconds * 1e6).round() as i64
}

thread_local! {
    static DEFER_MESHES: Cell<i32> = const { Cell::new(0) };
}

/// While one is alive on this thread, `evaluate()` leaves meshes dirty, with whatever only they need (skins,
/// deformers), computing the transforms around them. For interactive edits that read joints many times per mouse move.
pub struct DeferMeshes(());

impl DeferMeshes {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        DEFER_MESHES.with(|d| d.set(d.get() + 1));
        DeferMeshes(())
    }
}

impl Drop for DeferMeshes {
    fn drop(&mut self) {
        DEFER_MESHES.with(|d| d.set(d.get() - 1));
    }
}

fn next_structure() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed) + 1
}

/// The attr part of a plug's name: attr, attr[3], or group[3].child for a grouped array's child.
fn attr_part(a: &AttrDef, p: &PlugRef) -> String {
    if p.component >= 0 {
        return a.child_names[p.component as usize].clone();
    }
    if p.element < 0 {
        return a.name.clone();
    }
    let index = format!("[{}]", p.element);
    match a.name.find('.') {
        None => format!("{}{index}", a.name),
        Some(dot) => format!("{}{index}{}", &a.name[..dot], &a.name[dot..]),
    }
}

/// A whole non-negative number, or None.
fn parse_index(digits: &str) -> Option<i32> {
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn lock_bits(a: &AttrDef, component: i32) -> u8 {
    if a.ty != PlugType::Vec3 {
        return 1;
    }
    if component >= 0 {
        1u8 << component
    } else {
        7
    }
}

fn measure_value(v: &PlugValue, shared: &mut UndoShared, owned: &mut usize) {
    match v {
        PlugValue::Mesh(Some(m)) => {
            shared.add(Arc::as_ptr(m) as *const () as usize, m.memory_bytes())
        }
        PlugValue::Data(Some(d)) => shared.add(Arc::as_ptr(d) as *const () as usize, d.bytes()),
        PlugValue::String(s) => *owned += s.capacity(),
        _ => {}
    }
}

fn vec3_component(v: &PlugValue, c: i32) -> f64 {
    v.as_vec3()[c as usize]
}

/// v as def's type: through JSON where that reads, else by its first number, else def's default.
fn convert_value(v: &PlugValue, def: &AttrDef) -> PlugValue {
    if v.holds(def.ty) {
        return v.clone();
    }
    let j = to_json(v);
    let mut numeric = true;
    let number = if let PlugValue::String(s) = v {
        // strtod: the longest leading number.
        let t = s.trim_start();
        let mut best = None;
        for end in (1..=t.len()).rev() {
            if t.is_char_boundary(end) {
                if let Ok(d) = t[..end].parse::<f64>() {
                    best = Some(d);
                    break;
                }
            }
        }
        numeric = best.is_some();
        best.unwrap_or(0.0)
    } else if let Some(first) = j
        .as_array()
        .and_then(|a| a.first())
        .and_then(|x| x.as_f64())
    {
        first
    } else if let Some(b) = j.as_bool() {
        if b {
            1.0
        } else {
            0.0
        }
    } else if let Some(d) = j.as_f64() {
        d
    } else {
        numeric = false;
        0.0
    };
    if def.ty == PlugType::String {
        let text = |d: f64| {
            // printf %g
            let s = format!("{d}");
            if s.len() > 12 {
                format!("{d:e}")
            } else {
                s
            }
        };
        if let PlugValue::Vec3(x) = v {
            return PlugValue::String(format!("{} {} {}", text(x.x), text(x.y), text(x.z)));
        }
        if let PlugValue::Bool(b) = v {
            return PlugValue::String(if *b { "true" } else { "false" }.into());
        }
        if numeric {
            return PlugValue::String(text(number));
        }
    }
    let mut tries = Vec::new();
    if numeric {
        if matches!(def.ty, PlugType::Int | PlugType::Enum) {
            tries.push(serde_json::json!(number.round() as i64));
        } else {
            tries.push(serde_json::json!(number));
        }
        if def.ty == PlugType::Vec3 {
            tries.push(serde_json::json!([number, number, number]));
        }
    }
    tries.push(j);
    for t in &tries {
        if let Ok(out) = from_json(def, t) {
            if let PlugValue::Int(e) = out {
                if def.ty == PlugType::Enum && (e < 0 || e >= def.enum_names.len() as i64) {
                    continue;
                }
            }
            return out;
        }
    }
    def.default_value.clone()
}

/// Holds a deleted node so undo can put it back.
struct DeleteAction {
    id: NodeId,
    parent: NodeId,
    index: usize,
    held: Option<Box<Node>>,
    /// Recorded for a create: undo removes, redo restores.
    inverted: bool,
}

impl DeleteAction {
    fn restore(&mut self, g: &mut Graph) {
        if let Some(n) = self.held.take() {
            g.attach(n, self.parent, self.index);
        }
    }
    fn remove(&mut self, g: &mut Graph) {
        self.held = g.detach(self.id).map(|(n, _)| n);
    }
}

impl UndoAction<Graph> for DeleteAction {
    fn undo(&mut self, g: &mut Graph) {
        if self.inverted {
            self.remove(g)
        } else {
            self.restore(g)
        }
    }
    fn redo(&mut self, g: &mut Graph) {
        if self.inverted {
            self.restore(g)
        } else {
            self.remove(g)
        }
    }
    fn bytes(&self) -> usize {
        let mut n = std::mem::size_of::<Self>();
        if let Some(h) = &self.held {
            let mut ignored = UndoShared::default();
            n += std::mem::size_of::<Node>() + h.name.capacity();
            for v in &h.data.values {
                measure_value(v, &mut ignored, &mut n);
            }
        }
        n
    }
    fn shared_bytes(&self, shared: &mut UndoShared) {
        if let Some(h) = &self.held {
            let mut ignored = 0;
            for v in &h.data.values {
                measure_value(v, shared, &mut ignored);
            }
        }
    }
}

/// Everything the evaluation of one level reads, borrowed from the graph while that level's nodes are out of it.
struct EvalView<'a> {
    nodes: &'a HashMap<NodeId, Box<Node>>,
    rel: &'a HashMap<NodeId, Relations>,
    inputs: &'a BTreeMap<PlugRef, PlugRef>,
    held: &'a BTreeSet<PlugRef>,
    display_turns: &'a HashMap<NodeId, DMat4>,
    turned: &'a HashSet<NodeId>,
    playback: Option<&'a PlaybackCache>,
    active_frame: Option<&'a ActiveFrameFn>,
    ctx: EvalContext,
    pass: u64,
    cache: bool,
}

fn inputs_of(
    inputs: &BTreeMap<PlugRef, PlugRef>,
    id: NodeId,
) -> impl Iterator<Item = (&PlugRef, &PlugRef)> {
    let lo = PlugRef {
        node: id,
        attr: i32::MIN,
        component: i32::MIN,
        element: i32::MIN,
    };
    let hi = PlugRef {
        node: id,
        attr: i32::MAX,
        component: i32::MAX,
        element: i32::MAX,
    };
    inputs.range(lo..=hi)
}

impl EvalView<'_> {
    fn parent(&self, id: NodeId) -> NodeId {
        self.rel.get(&id).map(|r| r.parent).unwrap_or(NO_NODE)
    }

    /// The most a node's parent and sources computed this pass: any recompute upstream invalidates a restore.
    fn upstream_playback(&self, n: &Node) -> u8 {
        let mut most = 0;
        let mut visit = |up: NodeId| {
            if let Some(u) = self.nodes.get(&up) {
                if u.evaluated_pass == self.pass {
                    most = most.max(u.playback);
                }
            }
        };
        let p = self.parent(n.id);
        if p != NO_NODE {
            visit(p);
        }
        for (_, src) in inputs_of(self.inputs, n.id) {
            visit(src.node);
        }
        most
    }

    /// Puts back what the node computed at the current time, if kept.
    fn restore_cached(&self, n: &mut Node) -> bool {
        let c = self.playback.unwrap();
        let extra;
        {
            let inner = c.inner.lock().unwrap();
            let Some(t) = inner.times.get(&time_key(self.ctx.time)) else {
                return false;
            };
            let Some(e) = t.get(&n.id) else { return false };
            for (attr, v) in &e.values {
                n.data.values[*attr] = v.clone();
            }
            if let Some(els) = &e.elements {
                n.data.elements = els.clone();
            }
            extra = e.extra.clone();
        }
        n.behavior.restore_playback_extra(extra.as_ref());
        true
    }

    /// Returns whether it was kept.
    fn keep_cached(&self, n: &Node) -> bool {
        let c = self.playback.unwrap();
        // Outputs and whatever arrives by connection; unconnected inputs only change by an edit, which drops the cache.
        let attrs = &n.data.ty.attrs;
        let mut keep: Vec<bool> = (0..n.data.values.len())
            .map(|i| i < attrs.len() && !attrs[i].has(flags::INPUT))
            .collect();
        let mut elements = false;
        for (dst, _) in inputs_of(self.inputs, n.id) {
            if dst.element >= 0 {
                elements = true;
            } else if (dst.attr as usize) < keep.len() {
                keep[dst.attr as usize] = true;
            }
        }
        if n.data.ty.dag {
            keep[Transform::PARENT_MATRIX] = true;
        }
        let values: Vec<(usize, PlugValue)> = keep
            .iter()
            .enumerate()
            .filter(|(_, k)| **k)
            .map(|(i, _)| (i, n.data.values[i].clone()))
            .collect();
        let extra = n.behavior.playback_extra(&self.ctx, &n.data);
        if !extra.keep {
            return false;
        }
        let entry = PlaybackEntry {
            values,
            elements: elements.then(|| n.data.elements.clone()),
            extra: extra.data,
        };

        let mut inner = c.inner.lock().unwrap();
        let key = time_key(self.ctx.time);
        let has = inner.times.get(&key).is_some_and(|t| t.contains_key(&n.id));
        if inner.bytes >= inner.limit && !has {
            return false;
        }
        let mut bytes = extra.bytes + entry.values.len() * std::mem::size_of::<PlugValue>();
        for (_, v) in &entry.values {
            if let PlugValue::Mesh(Some(m)) = v {
                if inner.counted.insert(Arc::as_ptr(m) as *const () as usize) {
                    bytes += m.memory_bytes();
                }
            }
        }
        inner.bytes += bytes;
        inner.times.entry(key).or_default().insert(n.id, entry);
        true
    }

    fn pull(&self, n: &mut Node) {
        let cache_node = self.cache && !self.turned.contains(&n.id);
        let upstream = if cache_node {
            self.upstream_playback(n)
        } else {
            0
        };
        if cache_node && upstream == 0 && self.restore_cached(n) {
            n.playback = 0;
            n.dirty = false;
            n.evaluated_pass = self.pass;
            return;
        }
        let dag = n.data.ty.dag;
        if dag {
            let p = self.parent(n.id);
            let pw = if p != NO_NODE {
                self.nodes[&p].matrix(Transform::WORLD_MATRIX)
            } else {
                DMat4::IDENTITY
            };
            n.data.values[Transform::PARENT_MATRIX] = PlugValue::Matrix(pw);
        }
        let ty = n.data.ty.clone();
        for (dp, sp) in inputs_of(self.inputs, n.id) {
            if !self.held.is_empty() && self.held.contains(dp) {
                continue;
            }
            let sv = &self.nodes[&sp.node].data.values[sp.attr as usize];
            let da = &ty.attrs[dp.attr as usize];
            let slot = if dp.element >= 0 {
                match n
                    .data
                    .elements
                    .get_mut(dp.attr as usize)
                    .and_then(|els| els.iter_mut().find(|(i, _)| *i == dp.element))
                {
                    Some((_, v)) => v,
                    None => continue,
                }
            } else {
                &mut n.data.values[dp.attr as usize]
            };
            if sp.component >= 0 || dp.component >= 0 {
                let s = if sp.component >= 0 {
                    vec3_component(sv, sp.component)
                } else {
                    sv.scalar().unwrap_or(0.0)
                };
                if dp.component >= 0 {
                    if let PlugValue::Vec3(x) = slot {
                        x[dp.component as usize] = s;
                    }
                } else {
                    slot.assign_scalar(s);
                }
                da.clamp(slot);
                continue;
            }
            // Numeric types convert across connections.
            if std::mem::discriminant(slot) == std::mem::discriminant(sv) {
                *slot = sv.clone();
            } else if let (Some(s), Some(_)) = (sv.scalar(), slot.scalar()) {
                slot.assign_scalar(s);
            }
            da.clamp(slot);
        }
        if dag {
            if let Some(f) = self.active_frame {
                if let Some(frame) = f(n) {
                    n.data.values[Transform::PARENT_MATRIX] = PlugValue::Matrix(frame);
                }
            }
            if let Some(turn) = self.display_turns.get(&n.id) {
                let pm = n.data.matrix(Transform::PARENT_MATRIX);
                n.data.values[Transform::PARENT_MATRIX] = PlugValue::Matrix(*turn * pm);
            }
        }
        let ctx = self.ctx;
        n.behavior.compute(&ctx, &mut n.data);
        // Computed from a placeholder upstream means a placeholder too.
        if cache_node {
            n.playback = if upstream < 2 && self.keep_cached(n) {
                1
            } else {
                2
            };
        }
        n.dirty = false;
        n.evaluated_pass = self.pass;
    }
}

/// The scene's dependency graph plus its transform hierarchy. Main thread only, except `evaluate()` which fans compute
/// out over the rayon pool. Every mutation records undo.
pub struct Graph {
    /// The evaluation pass rig nodes were last brought in step with the scene at (see rig.rs).
    pub(crate) rig_synced_pass: u64,
    nodes: HashMap<NodeId, Box<Node>>,
    /// Short name -> nodes.
    names: HashMap<String, Vec<NodeId>>,
    rel: HashMap<NodeId, Relations>,
    roots: Vec<NodeId>,
    /// dst -> src
    inputs: BTreeMap<PlugRef, PlugRef>,
    /// src node -> dst plugs it drives
    outgoing_dst: HashMap<NodeId, Vec<PlugRef>>,
    selection: Vec<NodeId>,
    selection_serial: u64,
    /// Driven plugs keeping a user-set value.
    held: BTreeSet<PlugRef>,
    driven_set: Option<DrivenSetHook>,
    driven_vec_set: Option<DrivenVecSetHook>,
    delete_hook: Option<DeleteHook>,
    delete_depth: i32,
    listeners: BTreeMap<i32, ListenerEntry>,
    next_listener: i32,
    changed_plug: Option<PlugRef>,
    batch_depth: i32,
    batch_pending: bool,
    next_id: NodeId,
    ctx: EvalContext,
    naming: NamingPolicy,
    version: AtomicU64,
    structure: u64,
    visibility_version: u64,
    /// nodes() sorted, kept until the next structure change.
    sorted: Mutex<(u64, Vec<NodeId>)>,
    any_dirty: bool,
    evaluation_pass: u64,
    playback: Option<PlaybackCache>,
    playback_memory_mb: i64,
    display_turns: HashMap<NodeId, DMat4>,
    /// The turned nodes and their descendants, kept out of the playback cache.
    turned: HashSet<NodeId>,
    undo: UndoStack<Graph>,
    evaluate_hook: Option<EvaluateHook>,
    extra_evaluate_hooks: Vec<EvaluateHook>,
    in_evaluate_hook: bool,
    /// Stands in for `sim::sampling()`: simulations sampling their inputs at other times set it.
    sampling: bool,
    active_frame: Option<ActiveFrameFn>,
    /// Stands in for "the running command has no name flag" (`ops::currentArgs`), set by the command layer.
    command_default_names: bool,
    /// The component selection and mode (the C++ `components` namespace globals).
    pub components: crate::components::ComponentState,
    /// The isolation set the viewport shows, if any.
    pub active_isolation: Option<crate::isolation::Set>,
    pub inherit_listeners: BTreeMap<i32, crate::isolation::InheritListener>,
    /// The scene's up axis pref.
    pub up_axis: crate::mesh_nodes::UpAxis,
    /// The sphere the latest polySpherize used.
    pub last_sphere: crate::conform_commands::SphereState,
    /// What the last hide hid, for showLastHidden.
    pub last_hidden: Vec<NodeId>,
    /// Whether poly edits record history nodes (constructionHistory).
    pub construction_history: bool,
    /// What Duplicate with Transform repeats; see duplicate::install.
    pub duplicate_pairs: crate::duplicate::Pairs,
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl Graph {
    pub fn new() -> Self {
        Self {
            rig_synced_pass: u64::MAX,
            nodes: HashMap::new(),
            names: HashMap::new(),
            rel: HashMap::new(),
            roots: Vec::new(),
            inputs: BTreeMap::new(),
            outgoing_dst: HashMap::new(),
            selection: Vec::new(),
            selection_serial: 0,
            held: BTreeSet::new(),
            driven_set: None,
            driven_vec_set: None,
            delete_hook: None,
            delete_depth: 0,
            listeners: BTreeMap::new(),
            next_listener: 1,
            changed_plug: None,
            batch_depth: 0,
            batch_pending: false,
            next_id: 1,
            ctx: EvalContext::default(),
            naming: NamingPolicy::default(),
            version: AtomicU64::new(1),
            structure: next_structure(),
            visibility_version: 0,
            sorted: Mutex::new((0, Vec::new())),
            any_dirty: false,
            evaluation_pass: 0,
            playback: None,
            playback_memory_mb: 0,
            display_turns: HashMap::new(),
            turned: HashSet::new(),
            undo: UndoStack::new(),
            evaluate_hook: None,
            extra_evaluate_hooks: Vec::new(),
            in_evaluate_hook: false,
            sampling: false,
            active_frame: None,
            command_default_names: false,
            components: Default::default(),
            active_isolation: None,
            inherit_listeners: BTreeMap::new(),
            up_axis: Default::default(),
            last_sphere: Default::default(),
            last_hidden: Vec::new(),
            construction_history: true,
            duplicate_pairs: Default::default(),
        }
    }

    fn restructured(&mut self) {
        self.structure = next_structure();
    }

    // ---------------------------------------------------------------------------------------------------------------
    // Undo

    pub fn undo_stack(&self) -> &UndoStack<Graph> {
        &self.undo
    }
    pub fn undo_stack_mut(&mut self) -> &mut UndoStack<Graph> {
        &mut self.undo
    }

    fn with_undo<R>(&mut self, f: impl FnOnce(&mut UndoStack<Graph>, &mut Graph) -> R) -> R {
        let mut stack = std::mem::replace(&mut self.undo, UndoStack::replaying_placeholder());
        let r = f(&mut stack, self);
        let mut placeholder = std::mem::replace(&mut self.undo, stack);
        self.undo.take_listeners_from(&mut placeholder);
        r
    }

    pub fn undo(&mut self) -> bool {
        self.with_undo(|s, g| s.undo(g))
    }
    pub fn redo(&mut self) -> bool {
        self.with_undo(|s, g| s.redo(g))
    }
    pub fn jump_to(&mut self, position: usize) -> bool {
        self.with_undo(|s, g| s.jump_to(position, g))
    }
    pub fn open_chunk(&mut self, label: impl Into<String>) {
        self.undo.open_chunk(label)
    }
    pub fn close_chunk(&mut self) {
        self.undo.close_chunk()
    }
    pub fn cancel_chunk(&mut self) {
        self.with_undo(|s, g| s.cancel_chunk(g))
    }
    pub fn rollback_to(&mut self, mark: usize) {
        self.with_undo(|s, g| s.rollback_to(mark, g))
    }
    pub fn open_amend(&mut self, label: impl Into<String>) -> bool {
        let label = label.into();
        self.with_undo(|s, g| s.open_amend(label, g))
    }
    pub fn reopen_newest(&mut self, label: impl Into<String>) -> bool {
        let label = label.into();
        self.with_undo(|s, g| s.reopen_newest(label, g))
    }
    /// Runs `f` as one undo step.
    pub fn chunk<R>(&mut self, label: &str, f: impl FnOnce(&mut Graph) -> R) -> R {
        self.open_chunk(label);
        let r = f(self);
        self.close_chunk();
        r
    }

    fn record(
        &mut self,
        undo: impl FnMut(&mut Graph) + 'static,
        redo: impl FnMut(&mut Graph) + 'static,
    ) {
        self.undo.record(FnAction::new(undo, redo));
    }

    fn record_held(
        &mut self,
        undo: impl FnMut(&mut Graph) + 'static,
        redo: impl FnMut(&mut Graph) + 'static,
        held: &[PlugValue],
    ) {
        let mut owned = std::mem::size_of::<FnAction<Graph>>();
        let mut shared = UndoShared::default();
        for v in held {
            measure_value(v, &mut shared, &mut owned);
        }
        self.undo
            .record(FnAction::new(undo, redo).sized(owned, shared));
    }

    // ---------------------------------------------------------------------------------------------------------------
    // Playback cache

    /// While on, what every node computes is kept per time, and a full `evaluate()` at a kept time restores it instead
    /// of computing. Any change to the scene but a selection drops what is kept.
    pub fn set_cached_playback(&mut self, on: bool) {
        if on == self.cached_playback() {
            return;
        }
        self.playback = on.then(|| PlaybackCache {
            inner: Mutex::new(PlaybackInner::default()),
        });
        self.clear_playback_cache();
    }
    pub fn cached_playback(&self) -> bool {
        self.playback.is_some()
    }
    /// The memory cached playback may use in MB; 0 is half the machine's.
    pub fn set_playback_memory_mb(&mut self, mb: i64) {
        self.playback_memory_mb = mb;
        self.clear_playback_cache();
    }

    pub fn clear_playback_cache(&mut self) {
        let Some(p) = &self.playback else { return };
        let mb = self.playback_memory_mb;
        let mut inner = p.inner.lock().unwrap();
        inner.times.clear();
        inner.counted.clear();
        inner.bytes = 0;
        inner.limit = if mb > 0 {
            (mb as usize) << 20
        } else {
            physical_memory_bytes() / 2
        };
    }

    /// Every kept time in seconds, ascending.
    pub fn playback_cache_times(&self) -> Vec<f64> {
        let Some(p) = &self.playback else {
            return Vec::new();
        };
        p.inner
            .lock()
            .unwrap()
            .times
            .keys()
            .map(|k| *k as f64 / 1e6)
            .collect()
    }

    /// Roughly what the kept results hold in memory.
    pub fn playback_cache_bytes(&self) -> usize {
        self.playback
            .as_ref()
            .map(|p| p.inner.lock().unwrap().bytes)
            .unwrap_or(0)
    }

    // ---------------------------------------------------------------------------------------------------------------
    // Nodes

    /// Not undoable.
    pub fn clear(&mut self) {
        self.restructured();
        self.nodes.clear();
        self.names.clear();
        self.rel.clear();
        self.roots.clear();
        self.inputs.clear();
        self.outgoing_dst.clear();
        self.selection.clear();
        self.selection_serial += 1;
        self.held.clear();
        self.display_turns.clear();
        self.turned.clear();
        self.ctx = EvalContext::default();
        self.undo.clear();
        self.components.cleared();
        self.last_hidden.clear();
        if let Some(set) = &mut self.active_isolation {
            set.clear();
        }
        self.changed(Change::Cleared, NO_NODE, None);
    }

    pub fn valid_name(name: &str) -> bool {
        if name.is_empty() {
            return false;
        }
        let mut start = true;
        for c in name.chars() {
            if c == ':' {
                start = true;
                continue;
            }
            let alpha = c.is_ascii_alphabetic() || c == '_';
            let digit = c.is_ascii_digit();
            if !(alpha || (digit && !start)) {
                return false;
            }
            start = false;
        }
        true
    }

    /// global: unique across the scene; otherwise among the children of parent.
    fn unique_name(&self, wanted: &str, parent: NodeId, self_id: NodeId, global: bool) -> String {
        let taken = |n: &str| {
            self.names.get(n).is_some_and(|ids| {
                ids.iter()
                    .any(|&id| id != self_id && (global || self.parent(id) == parent))
            })
        };
        if !taken(wanted) {
            return wanted.to_string();
        }
        let digits = wanted.len() - wanted.bytes().rev().take_while(u8::is_ascii_digit).count();
        let stem = &wanted[..digits];
        let mut n: i64 = if digits < wanted.len() {
            wanted[digits..].parse().unwrap_or(1)
        } else {
            1
        };
        loop {
            let candidate = format!("{stem}{n}");
            if !taken(&candidate) {
                return candidate;
            }
            n += 1;
        }
    }

    /// Applies the naming policy; errors in block mode when a user-given name doesn't conform.
    fn conform_name(&self, ty: &NodeType, wanted: &str, user_given: bool) -> Result<String> {
        let fixed = self.naming.conform(ty, wanted);
        if !user_given || fixed == wanted {
            return Ok(fixed);
        }
        match self.naming.mode {
            NamingMode::Fix => Ok(fixed),
            NamingMode::Warn => {
                log::warn!("'{wanted}' breaks the naming convention (expected '{fixed}')");
                Ok(wanted.to_string())
            }
            NamingMode::Block => Err(GraphError::invalid(format!(
                "'{wanted}' breaks the naming convention; use '{fixed}'"
            ))),
        }
    }

    /// A name nobody chose (empty, or a command's own default when its call has no name flag) is made unique across
    /// the scene, so new nodes don't make existing names ambiguous.
    fn default_name(&self, name: &str, apply_naming: bool) -> bool {
        name.is_empty() || (apply_naming && self.command_default_names)
    }

    /// The command layer sets this while a command runs without a name flag, so the default names it gives are made
    /// unique across the scene.
    pub fn set_command_default_names(&mut self, on: bool) {
        self.command_default_names = on;
    }

    /// What `wanted` would become for a node under `parent` (NO_NODE for a root or a non-DAG node).
    pub fn available_name(&self, wanted: &str, parent: NodeId) -> String {
        self.unique_name(wanted, parent, NO_NODE, self.naming.unique_names)
    }

    fn erase_name(&mut self, name: &str, id: NodeId) {
        if let Some(ids) = self.names.get_mut(name) {
            if let Some(i) = ids.iter().position(|&x| x == id) {
                ids.remove(i);
            }
            if ids.is_empty() {
                self.names.remove(name);
            }
        }
    }

    fn siblings_mut(&mut self, parent: NodeId) -> &mut Vec<NodeId> {
        if parent == NO_NODE {
            &mut self.roots
        } else {
            &mut self.rel.entry(parent).or_default().children
        }
    }

    fn siblings(&self, parent: NodeId) -> &[NodeId] {
        if parent == NO_NODE {
            &self.roots
        } else {
            self.children(parent)
        }
    }

    fn attach(&mut self, mut n: Box<Node>, parent: NodeId, index: usize) {
        self.restructured();
        let id = n.id;
        self.names.entry(n.name.clone()).or_default().push(id);
        if n.data.ty.dag {
            self.rel.entry(id).or_default().parent = parent;
            let sib = self.siblings_mut(parent);
            let at = index.min(sib.len());
            sib.insert(at, id);
        }
        n.dirty = true;
        self.any_dirty = true;
        self.nodes.insert(id, n);
        self.components.node_created(id);
        self.changed(Change::Created, id, None);
    }

    fn detach(&mut self, id: NodeId) -> Option<(Box<Node>, usize)> {
        let n = self.nodes.remove(&id)?;
        self.restructured();
        self.erase_name(&n.name.clone(), id);
        let before = self.selection.len();
        self.selection.retain(|&x| x != id);
        if self.selection.len() != before {
            self.selection_serial += 1;
        }
        let mut index = 0;
        if let Some(rel) = self.rel.remove(&id) {
            let sib = self.siblings_mut(rel.parent);
            if let Some(pos) = sib.iter().position(|&x| x == id) {
                index = pos;
                sib.remove(pos);
            }
        }
        self.components.node_deleted(id);
        self.changed(Change::Deleted, id, None);
        Some((n, index))
    }

    /// `apply_naming = false` keeps the name as given (file loads); it is still made unique among its siblings, or
    /// across the scene with `NamingPolicy::unique_names`. Default names (empty) are always made unique across the scene.
    pub fn create_node(
        &mut self,
        ty: &Arc<NodeType>,
        name: &str,
        parent: NodeId,
        apply_naming: bool,
    ) -> Result<NodeId> {
        let create = ty
            .create
            .clone()
            .ok_or_else(|| GraphError::invalid(format!("node type '{}' is abstract", ty.name)))?;
        if parent != NO_NODE && !self.node(parent).is_some_and(|p| p.ty().dag) {
            return Err(GraphError::invalid("parent must be a transform"));
        }
        if !name.is_empty() && !Self::valid_name(name) {
            return Err(GraphError::invalid(format!("invalid name '{name}'")));
        }

        let reg = registered(ty);
        let mut n = Box::new(Node::new(reg.clone(), create()));
        n.id = self.next_id;
        self.next_id += 1;
        // Maya's default: the type name numbered from 1 (transform1, polyCube1). A naming convention, when on, styles it.
        let fallback = if self.naming.enabled {
            format!("{}1", apply_case(&ty.name, self.naming.style_for(ty), &Underscores::default()))
        } else {
            format!("{}1", ty.name)
        };
        let p = if ty.dag { parent } else { NO_NODE };
        let wanted = if name.is_empty() {
            fallback
        } else if apply_naming {
            self.conform_name(ty, name, true)?
        } else {
            name.to_string()
        };
        n.name = self.unique_name(
            &wanted,
            p,
            NO_NODE,
            self.naming.unique_names || self.default_name(name, apply_naming),
        );
        if let Some(colon) = n.name.rfind(':') {
            n.namespace = n.name[..colon].to_string();
        }
        n.data.values = reg.attrs.iter().map(|a| a.default_value.clone()).collect();
        // A copy of a node with user attrs (ty is its private type) gets them too, as part of being created.
        if ty.user_of.is_some() {
            n.data.ty = Arc::new((**ty).clone());
            n.own_type = true;
            for a in &ty.attrs[n.data.values.len()..] {
                n.data.values.push(a.default_value.clone());
            }
        }

        let id = n.id;
        self.attach(n, p, usize::MAX);
        let index = self.siblings(p).len().saturating_sub(1);
        // Recorded as an inverted delete: undo removes, redo restores.
        self.undo.record(DeleteAction {
            id,
            parent: p,
            index,
            held: None,
            inverted: true,
        });
        Ok(id)
    }

    /// Marks a node as loaded from a file reference (0 = owned by the scene). Not undoable; references manage it.
    pub fn set_reference(&mut self, id: NodeId, reference: u32) {
        if let Some(n) = self.nodes.get_mut(&id) {
            n.reference = reference;
        }
    }

    pub fn delete_node(&mut self, id: NodeId) -> Result<()> {
        if self.node(id).is_none() {
            return Ok(());
        }
        if self.delete_depth == 0 {
            if let Some(hook) = self.delete_hook.clone() {
                let mut doomed = vec![id];
                doomed.extend(self.descendants(id));
                self.delete_depth += 1;
                hook(self, &doomed);
                self.delete_depth -= 1;
            }
        }
        self.delete_depth += 1;
        for child in self.children(id).to_vec() {
            if let Err(e) = self.delete_node(child) {
                self.delete_depth -= 1;
                return Err(e);
            }
        }
        let inbound: Vec<PlugRef> = self
            .inputs
            .iter()
            .filter(|(d, s)| d.node == id || s.node == id)
            .map(|(d, _)| *d)
            .collect();
        for dst in inbound {
            self.disconnect(&dst);
        }
        if self.is_selected(id) {
            let sel: Vec<NodeId> = self
                .selection
                .iter()
                .copied()
                .filter(|&x| x != id)
                .collect();
            self.select(&sel, SelectMode::Replace);
        }
        let p = self.parent(id);
        if let Some((held, index)) = self.detach(id) {
            self.undo.record(DeleteAction {
                id,
                parent: p,
                index,
                held: Some(held),
                inverted: false,
            });
        }
        self.delete_depth -= 1;
        Ok(())
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id).map(|b| b.as_ref())
    }

    /// For subsystems that own a node's behaviour state (simulations and the like). Values go through `set_value`.
    pub fn behavior_mut(&mut self, id: NodeId) -> Option<&mut dyn crate::node::NodeBehavior> {
        self.nodes.get_mut(&id).map(|n| n.behavior.as_mut())
    }

    fn node_mut(&mut self, id: NodeId) -> &mut Node {
        self.nodes.get_mut(&id).expect("no such node")
    }

    fn name_of(&self, id: NodeId) -> &str {
        &self.nodes[&id].name
    }

    /// id's ancestors' names match segs (leaf last, already known to match); absolute also requires a root.
    fn matches_path(&self, id: NodeId, segs: &[&str], absolute: bool) -> bool {
        let mut cur = id;
        for seg in segs[..segs.len() - 1].iter().rev() {
            cur = self.parent(cur);
            if cur == NO_NODE || self.name_of(cur) != *seg {
                return false;
            }
        }
        !absolute || self.parent(cur) == NO_NODE
    }

    /// Every node `path` matches, by id.
    pub fn find_all(&self, path: &str) -> Vec<NodeId> {
        let absolute = path.starts_with('|');
        let path = if absolute { &path[1..] } else { path };
        let segs: Vec<&str> = path.split('|').collect();
        if segs.iter().any(|s| s.is_empty()) {
            return Vec::new();
        }
        let mut out: Vec<NodeId> = self
            .names
            .get(*segs.last().unwrap())
            .map(|ids| {
                ids.iter()
                    .copied()
                    .filter(|&id| {
                        (segs.len() == 1 && !absolute) || self.matches_path(id, &segs, absolute)
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.sort_unstable();
        out
    }

    /// Resolves a short name ("Arm"), a partial path ("L|Arm", any unique run of parents ending at the node) or a full
    /// path ("|Body|L|Arm"). None when nothing matches; `Ambiguous`, listing the full paths, when several do.
    pub fn find(&self, path: &str) -> Result<Option<NodeId>> {
        if !path.contains('|') {
            match self.names.get(path).map(Vec::as_slice) {
                None | Some([]) => return Ok(None),
                Some([one]) => return Ok(Some(*one)),
                _ => {}
            }
        }
        let all = self.find_all(path);
        match all.len() {
            0 => Ok(None),
            1 => Ok(Some(all[0])),
            _ => {
                let list: Vec<String> = all.iter().map(|&id| self.full_path(id)).collect();
                Err(GraphError::Ambiguous(format!(
                    "'{path}' is ambiguous: it matches {}; use a path",
                    list.join(", ")
                )))
            }
        }
    }

    /// `find` that wants exactly one node.
    pub fn find_node(&self, path: &str) -> Result<&Node> {
        let id = self
            .find(path)?
            .ok_or_else(|| GraphError::invalid(format!("no node named '{path}'")))?;
        Ok(self.node(id).unwrap())
    }

    /// "|Body|L|Arm".
    pub fn full_path(&self, id: NodeId) -> String {
        let mut chain = Vec::new();
        let mut cur = id;
        while cur != NO_NODE {
            match self.nodes.get(&cur) {
                Some(n) => chain.push(n.name.as_str()),
                None => break,
            }
            cur = self.parent(cur);
        }
        chain.iter().rev().map(|n| format!("|{n}")).collect()
    }

    /// The shortest path that resolves back to this node alone: its name when that is unique. "" for no node.
    pub fn path(&self, id: NodeId) -> String {
        let Some(n) = self.nodes.get(&id) else {
            return String::new();
        };
        let same = self.names.get(&n.name).map(Vec::as_slice).unwrap_or(&[]);
        if same.len() <= 1 {
            return n.name.clone();
        }
        let mut chain = Vec::new(); // leaf first
        let mut cur = id;
        while cur != NO_NODE {
            chain.push(self.name_of(cur));
            cur = self.parent(cur);
        }
        for k in 1..=chain.len() {
            let segs: Vec<&str> = chain[..k].iter().rev().copied().collect();
            let matches = same
                .iter()
                .filter(|&&other| self.matches_path(other, &segs, false))
                .take(2)
                .count();
            if matches == 1 {
                return segs.join("|");
            }
        }
        self.full_path(id)
    }

    pub fn nodes(&self) -> Vec<NodeId> {
        let mut s = self.sorted.lock().unwrap();
        if s.0 != self.structure {
            let mut ids: Vec<NodeId> = self.nodes.keys().copied().collect();
            ids.sort_unstable();
            *s = (self.structure, ids);
        }
        s.1.clone()
    }

    fn rename_raw(&mut self, id: NodeId, name: &str) {
        let old = self.name_of(id).to_string();
        self.erase_name(&old, id);
        let n = self.node_mut(id);
        n.name = name.to_string();
        n.namespace = name
            .rfind(':')
            .map(|c| name[..c].to_string())
            .unwrap_or_default();
        self.names.entry(name.to_string()).or_default().push(id);
        self.changed(Change::Renamed, id, None);
    }

    /// Returns the name actually given, which is made unique (see `create_node`).
    pub fn rename(&mut self, id: NodeId, name: &str) -> Result<String> {
        let n = self
            .node(id)
            .ok_or_else(|| GraphError::invalid("no such node"))?;
        if !Self::valid_name(name) {
            return Err(GraphError::invalid(format!("invalid name '{name}'")));
        }
        let old_name = n.name.clone();
        let conformed = self.conform_name(n.ty(), name, true)?;
        let new_name = self.unique_name(&conformed, self.parent(id), id, self.naming.unique_names);
        if new_name == old_name {
            return Ok(new_name);
        }
        self.rename_raw(id, &new_name);
        let (o, nn) = (old_name, new_name.clone());
        self.record(
            move |g| g.rename_raw(id, &o),
            move |g| g.rename_raw(id, &nn),
        );
        Ok(new_name)
    }

    // ---------------------------------------------------------------------------------------------------------------
    // Hierarchy

    pub fn parent(&self, id: NodeId) -> NodeId {
        self.rel.get(&id).map(|r| r.parent).unwrap_or(NO_NODE)
    }

    pub fn children(&self, id: NodeId) -> &[NodeId] {
        self.rel
            .get(&id)
            .map(|r| r.children.as_slice())
            .unwrap_or(&[])
    }

    pub fn roots(&self) -> &[NodeId] {
        &self.roots
    }

    pub fn descendants(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = self.children(id).iter().rev().copied().collect();
        while let Some(n) = stack.pop() {
            out.push(n);
            stack.extend(self.children(n).iter().rev());
        }
        out
    }

    fn reparent_raw(&mut self, child: NodeId, new_parent: NodeId, index: usize) {
        let old_parent = self.parent(child);
        self.siblings_mut(old_parent).retain(|&x| x != child);
        self.rel.entry(child).or_default().parent = new_parent;
        let sib = self.siblings_mut(new_parent);
        let at = index.min(sib.len());
        sib.insert(at, child);
        self.mark_dirty(child);
        self.changed(Change::Reparented, child, None);
    }

    /// `index` is the position the child takes among the parent's children (`usize::MAX` appends). With the same
    /// parent and an index it only reorders.
    pub fn set_parent(
        &mut self,
        child: NodeId,
        new_parent: NodeId,
        keep_world: bool,
        index: usize,
    ) -> Result<()> {
        if !self.node(child).is_some_and(|c| c.ty().dag) {
            return Err(GraphError::invalid("only transforms can be parented"));
        }
        if new_parent != NO_NODE {
            if !self.node(new_parent).is_some_and(|p| p.ty().dag) {
                return Err(GraphError::invalid("parent must be a transform"));
            }
            let mut a = new_parent;
            while a != NO_NODE {
                if a == child {
                    return Err(GraphError::invalid("cannot parent a node under itself"));
                }
                a = self.parent(a);
            }
        }
        let old_parent = self.parent(child);
        let old_sib = self.siblings(old_parent);
        let old_index = old_sib
            .iter()
            .position(|&x| x == child)
            .unwrap_or(old_sib.len());
        if old_parent == new_parent {
            let new_index = index.min(old_sib.len() - 1);
            if index == usize::MAX || new_index == old_index {
                return Ok(());
            }
            self.reparent_raw(child, new_parent, new_index);
            self.record(
                move |g| g.reparent_raw(child, old_parent, old_index),
                move |g| g.reparent_raw(child, new_parent, new_index),
            );
            return Ok(());
        }

        let world = if keep_world {
            self.evaluate();
            self.node(child).unwrap().matrix(Transform::WORLD_MATRIX)
        } else {
            DMat4::IDENTITY
        };

        self.reparent_raw(child, new_parent, index);
        let new_index = self
            .siblings(new_parent)
            .iter()
            .position(|&x| x == child)
            .unwrap();
        self.record(
            move |g| g.reparent_raw(child, old_parent, old_index),
            move |g| g.reparent_raw(child, new_parent, new_index),
        );
        // Sibling names stay unique, so only a clash under the new parent renames.
        let old_name = self.name_of(child).to_string();
        let fixed = self.unique_name(&old_name, new_parent, child, self.naming.unique_names);
        if fixed != old_name {
            self.rename_raw(child, &fixed);
            let f = fixed.clone();
            self.record(
                move |g| g.rename_raw(child, &old_name),
                move |g| g.rename_raw(child, &f),
            );
        }

        crate::rig::relink_inverse_scale(self, child);
        // A constrained object stays where its constraint puts it, which already accounts for the new parent.
        if keep_world && !crate::constraint::is_constrained(self, child) {
            let parent_world = if new_parent != NO_NODE {
                self.node(new_parent)
                    .unwrap()
                    .matrix(Transform::WORLD_MATRIX)
            } else {
                DMat4::IDENTITY
            };
            Transform::set_local_matrix(self, child, &(parent_world.inverse() * world))?;
        }
        Ok(())
    }

    // ---------------------------------------------------------------------------------------------------------------
    // Plugs

    /// "node.attr" or "node.tx", the node part resolved as `find` does.
    pub fn plug(&self, path: &str) -> Result<PlugRef> {
        let dot = path
            .find('.')
            .ok_or_else(|| GraphError::invalid(format!("expected node.attr, got '{path}'")))?;
        let node_part = &path[..dot];
        let id = self
            .find(node_part)?
            .ok_or_else(|| GraphError::invalid(format!("no node named '{node_part}'")))?;
        let n = self.node(id).unwrap();
        let mut attr_name: String = path[dot + 1..].to_string();
        let mut element = -1;
        if let Some(close) = attr_name.find("].") {
            // group[3].child names the child attr "group.child", element 3.
            let open = attr_name[..close].rfind('[');
            element = open
                .and_then(|o| parse_index(&attr_name[o + 1..close]))
                .unwrap_or(-1);
            if element < 0 {
                return Err(GraphError::invalid(format!(
                    "bad element index in '{path}'"
                )));
            }
            let open = open.unwrap();
            attr_name = format!("{}.{}", &attr_name[..open], &attr_name[close + 2..]);
        } else if attr_name.ends_with(']') {
            let open = attr_name.rfind('[');
            element = open
                .and_then(|o| parse_index(&attr_name[o + 1..attr_name.len() - 1]))
                .unwrap_or(-1);
            if element < 0 {
                return Err(GraphError::invalid(format!(
                    "bad element index in '{path}'"
                )));
            }
            attr_name.truncate(open.unwrap());
        }
        for (i, a) in n.ty().attrs.iter().enumerate() {
            if a.name == attr_name || a.short_name == attr_name {
                if element >= 0 && !a.is_array() {
                    return Err(GraphError::invalid(format!(
                        "'{}.{}' is not an array",
                        n.name, a.name
                    )));
                }
                return Ok(PlugRef {
                    node: id,
                    attr: i as i32,
                    component: -1,
                    element,
                });
            }
            if a.ty == PlugType::Vec3 && element < 0 {
                for c in 0..3 {
                    if a.child_names[c] == attr_name || a.child_short[c] == attr_name {
                        return Ok(PlugRef::component(id, i as i32, c as i32));
                    }
                }
            }
        }
        // Numbered attrs an array replaced, as older scene files and scripts name them.
        if element < 0 {
            if let Some((i, Some(k))) = n.ty().file_attr_index(&attr_name) {
                return Ok(PlugRef::element(id, i as i32, k));
            }
        }
        Err(GraphError::invalid(format!(
            "'{}' has no attribute '{attr_name}'",
            n.name
        )))
    }

    /// Writes a value derived from other nodes (a solver's copy of its joints): no undo entry, since undoing the edit
    /// it came from derives it again.
    pub(crate) fn set_derived(&mut self, p: &PlugRef, v: &PlugValue) {
        self.set_raw(p, v);
    }

    /// `connect` without an undo entry or checks, for a connection that follows from the hierarchy.
    pub(crate) fn connect_derived(&mut self, src: &PlugRef, dst: &PlugRef) {
        self.disconnect_raw(dst);
        self.connect_raw(src, dst);
    }

    pub(crate) fn disconnect_derived(&mut self, dst: &PlugRef) {
        self.disconnect_raw(dst);
    }

    /// The node's shortest unique path, a dot and the attr.
    pub fn plug_name(&self, p: &PlugRef) -> String {
        match self.node(p.node) {
            Some(n) => format!(
                "{}.{}",
                self.path(p.node),
                attr_part(&n.ty().attrs[p.attr as usize], p)
            ),
            None => "?".into(),
        }
    }

    /// `plug_name` with the node's full path: stable while other nodes come and go, for files and baselines.
    pub fn full_plug_name(&self, p: &PlugRef) -> String {
        match self.node(p.node) {
            Some(n) => format!(
                "{}.{}",
                self.full_path(p.node),
                attr_part(&n.ty().attrs[p.attr as usize], p)
            ),
            None => "?".into(),
        }
    }

    pub fn attr(&self, p: &PlugRef) -> &AttrDef {
        &self.nodes[&p.node].data.ty.attrs[p.attr as usize]
    }

    fn element_slot(&mut self, p: &PlugRef) -> &mut PlugValue {
        let n = self.nodes.get_mut(&p.node).unwrap();
        let count = n.data.ty.attrs.len();
        if n.data.elements.len() < count {
            n.data.elements.resize_with(count, Vec::new);
        }
        let default = n.data.ty.attrs[p.attr as usize].default_value.clone();
        let els = &mut n.data.elements[p.attr as usize];
        let at = els.partition_point(|(i, _)| *i < p.element);
        if at == els.len() || els[at].0 != p.element {
            els.insert(at, (p.element, default));
        }
        &mut els[at].1
    }

    fn erase_element_raw(&mut self, p: &PlugRef) {
        let Some(n) = self.nodes.get_mut(&p.node) else {
            return;
        };
        if (p.attr as usize) >= n.data.elements.len() {
            return;
        }
        n.data.elements[p.attr as usize].retain(|(i, _)| *i != p.element);
        self.mark_dirty(p.node);
        self.changed(Change::Value, p.node, Some(*p));
    }

    fn set_elements_raw(&mut self, array: &PlugRef, elements: &[(i32, PlugValue)]) {
        let n = self.node_mut(array.node);
        let count = n.data.ty.attrs.len();
        if n.data.elements.len() < count {
            n.data.elements.resize_with(count, Vec::new);
        }
        n.data.elements[array.attr as usize] = elements.to_vec();
        self.mark_dirty(array.node);
        self.changed(Change::Value, array.node, Some(*array));
    }

    fn set_raw(&mut self, p: &PlugRef, v: &PlugValue) {
        if p.element >= 0 {
            *self.element_slot(p) = v.clone();
        } else {
            let slot = &mut self.node_mut(p.node).data.values[p.attr as usize];
            if p.component >= 0 {
                if let PlugValue::Vec3(x) = slot {
                    x[p.component as usize] = v.scalar().unwrap_or(0.0);
                }
            } else {
                *slot = v.clone();
            }
        }
        self.mark_dirty(p.node);
        self.changed(Change::Value, p.node, Some(*p));
    }

    // Locks

    pub fn is_locked(&self, p: &PlugRef) -> bool {
        let Some(n) = self.node(p.node) else {
            return false;
        };
        if p.attr < 0 {
            return false;
        }
        let bits = lock_bits(&n.ty().attrs[p.attr as usize], p.component);
        n.lock_mask(p.attr as usize) & bits == bits
    }

    /// True when the plug or any of its components is locked.
    pub fn any_locked(&self, p: &PlugRef) -> bool {
        self.node(p.node).is_some_and(|n| {
            p.attr >= 0
                && n.lock_mask(p.attr as usize)
                    & lock_bits(&n.ty().attrs[p.attr as usize], p.component)
                    != 0
        })
    }

    /// Maya's attribute lock: a locked plug rejects `set_value`. A whole Vec3 counts as locked when all three
    /// components are; setting it fails only when a locked component would change. Undoable.
    pub fn set_locked(&mut self, p: &PlugRef, locked: bool) -> Result<()> {
        let n = self
            .node(p.node)
            .filter(|_| p.attr >= 0)
            .ok_or_else(|| GraphError::invalid("no such plug"))?;
        let old = n.lock_mask(p.attr as usize);
        let bits = lock_bits(&n.ty().attrs[p.attr as usize], p.component);
        let now = if locked { old | bits } else { old & !bits };
        if now == old {
            return Ok(());
        }
        let (id, attr) = (p.node, p.attr as usize);
        self.set_lock_raw(id, attr, now);
        self.record(
            move |g| g.set_lock_raw(id, attr, old),
            move |g| g.set_lock_raw(id, attr, now),
        );
        Ok(())
    }

    fn set_lock_raw(&mut self, id: NodeId, attr: usize, mask: u8) {
        let Some(n) = self.nodes.get_mut(&id) else {
            return;
        };
        if n.locks.len() <= attr {
            let len = n.data.ty.attrs.len().max(attr + 1);
            n.locks.resize(len, 0);
        }
        n.locks[attr] = mask;
        self.changed(Change::Value, id, Some(PlugRef::new(id, attr as i32)));
    }

    /// v with the plug's locked parts put back to their stored values, for tools that move only what they may.
    pub fn keep_locked(&self, p: &PlugRef, mut v: PlugValue) -> PlugValue {
        let Some(n) = self.node(p.node) else { return v };
        if p.attr < 0 {
            return v;
        }
        let mask = n.lock_mask(p.attr as usize);
        if mask == 0 {
            return v;
        }
        let stored = n.value(p.attr as usize);
        if self.is_locked(p) {
            return if p.component >= 0 {
                PlugValue::Float(vec3_component(stored, p.component))
            } else {
                stored.clone()
            };
        }
        if let (PlugValue::Vec3(x), true) = (&mut v, p.component < 0) {
            let s = stored.as_vec3();
            for c in 0..3 {
                if mask & (1 << c) != 0 {
                    x[c] = s[c];
                }
            }
        }
        v
    }

    // Values

    /// Errors when the plug is driven by a connection, unless the driven-set hook takes the edit. Setting a whole Vec3
    /// whose components are driven stores it and offers each changed driven component to the hook.
    pub fn set_value(&mut self, p: PlugRef, mut v: PlugValue) -> Result<()> {
        let n = self
            .node(p.node)
            .ok_or_else(|| GraphError::invalid("no such node"))?;
        let a = n.ty().attrs[p.attr as usize].clone();
        if !a.has(flags::INPUT) {
            return Err(GraphError::invalid(format!(
                "{} is an output and can't be set",
                self.plug_name(&p)
            )));
        }
        if a.is_array() && !a.has(flags::STORABLE) {
            return Err(GraphError::invalid(format!(
                "{} is an array input and takes connections, not values",
                self.plug_name(&p)
            )));
        }
        if a.is_array() && (p.element < 0 || p.component >= 0) {
            let name = self.plug_name(&p);
            return Err(GraphError::invalid(format!(
                "{name} is an array: set one element ({name}[0]), or all with setElements"
            )));
        }
        if matches!(a.ty, PlugType::Int | PlugType::Float | PlugType::Vec3) {
            a.clamp(&mut v);
        }
        let mask = n.lock_mask(p.attr as usize);
        if mask != 0 {
            match (&v, p.component >= 0) {
                (PlugValue::Vec3(x), false) => {
                    let s = n.vec3(p.attr as usize);
                    for c in 0..3 {
                        if mask & (1 << c) != 0 && x[c] != s[c] {
                            return Err(GraphError::invalid(format!(
                                "{} is locked and can't be changed",
                                self.plug_name(&PlugRef::component(p.node, p.attr, c as i32))
                            )));
                        }
                    }
                }
                _ => {
                    if self.is_locked(&p) {
                        return Err(GraphError::invalid(format!(
                            "{} is locked and can't be changed",
                            self.plug_name(&p)
                        )));
                    }
                }
            }
        }
        if let Some(src) = self.source(&p) {
            if let (PlugValue::Vec3(vec), true) = (&v, p.component < 0) {
                if !self.set_driven_vec(&p, &src, *vec) {
                    return Err(GraphError::invalid(format!(
                        "{} is driven by a connection",
                        self.plug_name(&p)
                    )));
                }
                return Ok(());
            }
            match v.scalar() {
                Some(d) if self.set_driven(&p, &src, d) => return Ok(()),
                _ => {
                    return Err(GraphError::invalid(format!(
                        "{} is driven by a connection",
                        self.plug_name(&p)
                    )))
                }
            }
        }
        if p.component >= 0 {
            if let Some(src) = self.source(&p.whole()) {
                let mut whole = self.value(&p.whole())?.as_vec3();
                let Some(d) = v.scalar() else {
                    return Err(GraphError::invalid(format!(
                        "{} is driven by a connection",
                        self.plug_name(&p)
                    )));
                };
                whole[p.component as usize] = d;
                if !self.set_driven_vec(&p.whole(), &src, whole) {
                    return Err(GraphError::invalid(format!(
                        "{} is driven by a connection",
                        self.plug_name(&p)
                    )));
                }
                return Ok(());
            }
        }

        let n = self.node(p.node).unwrap();
        if p.element >= 0 {
            let was = n.element(p.attr as usize, p.element).cloned();
            if was.as_ref() == Some(&v) {
                return Ok(());
            }
            self.set_raw(&p, &v);
            self.record(
                move |g| match &was {
                    Some(old) => g.set_raw(&p, old),
                    None => g.erase_element_raw(&p),
                },
                move |g| g.set_raw(&p, &v),
            );
            return Ok(());
        }
        let stored = n.value(p.attr as usize);
        let old = if p.component >= 0 {
            PlugValue::Float(vec3_component(stored, p.component))
        } else {
            stored.clone()
        };
        if old == v {
            return Ok(());
        }
        self.set_raw(&p, &v);
        let held = if matches!(
            v,
            PlugValue::Mesh(_) | PlugValue::Data(_) | PlugValue::String(_)
        ) {
            vec![old.clone(), v.clone()]
        } else {
            Vec::new()
        };
        let (o, nv) = (old.clone(), v.clone());
        self.record_held(
            move |g| g.set_raw(&p, &o),
            move |g| g.set_raw(&p, &nv),
            &held,
        );

        if p.component < 0 && a.ty == PlugType::Vec3 {
            let (now, was) = (v.as_vec3(), old.as_vec3());
            for c in 0..3 {
                let comp = PlugRef::component(p.node, p.attr, c);
                if let Some(src) = self.source(&comp) {
                    if now[c as usize] != was[c as usize] {
                        self.set_driven(&comp, &src, now[c as usize]);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn set_driven_set_hook(&mut self, f: DrivenSetHook) {
        self.driven_set = Some(f);
    }
    pub fn set_driven_vec_set_hook(&mut self, f: DrivenVecSetHook) {
        self.driven_vec_set = Some(f);
    }
    /// Called by the outermost `delete_node` with the node and its descendants before anything is disconnected, so data
    /// other nodes depend on can move. Edits it makes record undo.
    pub fn set_delete_hook(&mut self, f: DeleteHook) {
        self.delete_hook = Some(f);
    }
    /// True while a driven plug shows a held value instead of its source's.
    pub fn is_held(&self, dst: &PlugRef) -> bool {
        self.held.contains(dst)
    }

    /// Routes a set on a driven scalar through the hook. False when it is rejected.
    fn set_driven(&mut self, dst: &PlugRef, src: &PlugRef, v: f64) -> bool {
        let edit = match self.driven_set.clone() {
            Some(h) => h(self, dst, src, v),
            None => DrivenEdit::Reject,
        };
        if edit == DrivenEdit::Reject {
            return false;
        }
        if edit == DrivenEdit::Hold {
            let was = self.held.contains(dst);
            let slot = self.nodes[&dst.node].value(dst.attr as usize);
            let old = if dst.component >= 0 {
                vec3_component(slot, dst.component)
            } else {
                slot.scalar().unwrap_or(0.0)
            };
            let d = *dst;
            self.hold_raw(&d, &PlugValue::Float(v));
            self.record(
                move |g| {
                    if was {
                        g.hold_raw(&d, &PlugValue::Float(old))
                    } else {
                        g.release_raw(&d)
                    }
                },
                move |g| g.hold_raw(&d, &PlugValue::Float(v)),
            );
        }
        true
    }

    fn set_driven_vec(&mut self, dst: &PlugRef, src: &PlugRef, v: DVec3) -> bool {
        if src.component >= 0 || self.attr(src).ty != PlugType::Vec3 {
            return false;
        }
        let edit = match self.driven_vec_set.clone() {
            Some(h) => h(self, dst, src, v),
            None => DrivenEdit::Reject,
        };
        if edit == DrivenEdit::Reject {
            return false;
        }
        if edit == DrivenEdit::Hold {
            let was = self.held.contains(dst);
            let old = self.nodes[&dst.node].vec3(dst.attr as usize);
            let d = *dst;
            self.hold_raw(&d, &PlugValue::Vec3(v));
            self.record(
                move |g| {
                    if was {
                        g.hold_raw(&d, &PlugValue::Vec3(old))
                    } else {
                        g.release_raw(&d)
                    }
                },
                move |g| g.hold_raw(&d, &PlugValue::Vec3(v)),
            );
        }
        true
    }

    fn hold_raw(&mut self, dst: &PlugRef, v: &PlugValue) {
        let slot = &mut self.node_mut(dst.node).data.values[dst.attr as usize];
        match (v, dst.component >= 0) {
            (PlugValue::Vec3(x), _) => *slot = PlugValue::Vec3(*x),
            (_, true) => {
                if let PlugValue::Vec3(x) = slot {
                    x[dst.component as usize] = v.scalar().unwrap_or(0.0);
                }
            }
            (_, false) => slot.assign_scalar(v.scalar().unwrap_or(0.0)),
        }
        self.mark_dirty(dst.node);
        self.held.insert(*dst);
        self.changed(Change::Value, dst.node, Some(*dst));
    }

    fn release_raw(&mut self, dst: &PlugRef) {
        self.held.remove(dst);
        self.mark_dirty(dst.node);
        self.changed(Change::Value, dst.node, Some(*dst));
    }

    /// Raw stored value, no evaluation.
    pub fn stored_value(&self, p: &PlugRef) -> &PlugValue {
        self.nodes[&p.node].value(p.attr as usize)
    }

    /// Evaluates first if the node is dirty. Component plugs return a float.
    pub fn value(&mut self, p: &PlugRef) -> Result<PlugValue> {
        let n = self
            .node(p.node)
            .ok_or_else(|| GraphError::invalid("no such node"))?;
        // Only what this plug depends on: reading a joint mid-drag must not recompute the skinned meshes downstream.
        if self.any_dirty && n.dirty {
            self.evaluate_with(Some(&[p.node]));
        }
        let n = self.node(p.node).unwrap();
        if p.element >= 0 {
            return Ok(n
                .element(p.attr as usize, p.element)
                .cloned()
                .unwrap_or_else(|| n.ty().attrs[p.attr as usize].default_value.clone()));
        }
        let v = n.value(p.attr as usize);
        Ok(if p.component >= 0 {
            PlugValue::Float(vec3_component(v, p.component))
        } else {
            v.clone()
        })
    }

    pub fn connections(&self) -> Vec<(PlugRef, PlugRef)> {
        self.inputs.iter().map(|(d, s)| (*d, *s)).collect()
    }

    fn depends_on(&self, n: NodeId, upstream: NodeId) -> bool {
        let mut stack = vec![n];
        let mut seen = HashSet::new();
        while let Some(cur) = stack.pop() {
            if cur == upstream {
                return true;
            }
            if !seen.insert(cur) {
                continue;
            }
            stack.extend(inputs_of(&self.inputs, cur).map(|(_, s)| s.node));
            let p = self.parent(cur);
            if p != NO_NODE {
                stack.push(p);
            }
        }
        false
    }

    // Connections

    fn connect_raw(&mut self, src: &PlugRef, dst: &PlugRef) {
        if dst.element >= 0 && self.nodes.contains_key(&dst.node) {
            self.element_slot(dst);
        }
        self.held.remove(dst);
        self.inputs.insert(*dst, *src);
        self.outgoing_dst.entry(src.node).or_default().push(*dst);
        self.mark_dirty(dst.node);
        self.changed(Change::Connection, dst.node, Some(*dst));
    }

    fn disconnect_raw(&mut self, dst: &PlugRef) {
        let Some(src) = self.inputs.remove(dst) else {
            return;
        };
        if let Some(v) = self.outgoing_dst.get_mut(&src.node) {
            v.retain(|d| d != dst);
        }
        self.held.remove(dst);
        if let Some(n) = self.nodes.get(&dst.node) {
            let a = &n.data.ty.attrs[dst.attr as usize];
            // A storable array keeps the element, holding the last value pulled through; an input-only one drops it.
            if dst.element >= 0 {
                if !a.has(flags::STORABLE) {
                    self.erase_element_raw(dst);
                }
            } else if a.has(flags::RESET_ON_DISCONNECT) {
                let d = a.default_value.clone();
                self.node_mut(dst.node).data.values[dst.attr as usize] = d;
            }
        }
        self.mark_dirty(dst.node);
        self.changed(Change::Connection, dst.node, Some(*dst));
    }

    /// Either end may be one component of a Vec3 (node.tx), which carries a float. A component destination and its
    /// whole Vec3 can't both be driven; `force` replaces whichever is there. A destination that is a whole array input
    /// gets a new element after its last one. Returns the destination connected.
    pub fn connect(&mut self, src: &PlugRef, to: &PlugRef, force: bool) -> Result<PlugRef> {
        if self.node(src.node).is_none() || self.node(to.node).is_none() {
            return Err(GraphError::invalid("no such node"));
        }
        let sa = self.attr(src).clone();
        let da = self.attr(to).clone();
        if !da.has(flags::INPUT) {
            return Err(GraphError::invalid(format!(
                "{} is not an input",
                self.plug_name(to)
            )));
        }
        if src.element >= 0 {
            return Err(GraphError::invalid(format!(
                "array elements are inputs and can't drive {}",
                self.plug_name(to)
            )));
        }
        if to.element >= 0 && !da.is_array() {
            return Err(GraphError::invalid(format!(
                "{} is not an array",
                self.plug_name(&to.whole())
            )));
        }
        if da.is_array() && to.component >= 0 {
            return Err(GraphError::invalid("array inputs have no components"));
        }
        let mut dst = *to;
        if da.is_array() && dst.element < 0 {
            let used = self.elements(&dst);
            dst.element = used.last().map(|e| e.element + 1).unwrap_or(0);
        }
        if (src.component >= 0 && sa.ty != PlugType::Vec3)
            || (dst.component >= 0 && da.ty != PlugType::Vec3)
        {
            return Err(GraphError::invalid("only vec3 plugs have components"));
        }
        let st = if src.component >= 0 {
            PlugType::Float
        } else {
            sa.ty
        };
        let dt = if dst.component >= 0 {
            PlugType::Float
        } else {
            da.ty
        };
        let numeric = |t: PlugType| {
            matches!(
                t,
                PlugType::Float | PlugType::Int | PlugType::Bool | PlugType::Enum
            )
        };
        if st != dt && !(numeric(st) && numeric(dt)) {
            return Err(GraphError::invalid(format!(
                "type mismatch: {} -> {}",
                type_name(st),
                type_name(dt)
            )));
        }
        if src.node == dst.node || self.depends_on(src.node, dst.node) {
            return Err(GraphError::invalid("connection would create a cycle"));
        }

        let mut clashing = Vec::new();
        if dst.component >= 0 {
            if self.source(&dst.whole()).is_some() {
                clashing.push(dst.whole());
            }
        } else if da.ty == PlugType::Vec3 {
            for c in 0..3 {
                let comp = PlugRef::component(dst.node, dst.attr, c);
                if self.source(&comp).is_some() {
                    clashing.push(comp);
                }
            }
        }
        if !clashing.is_empty() {
            if !force {
                return Err(GraphError::invalid(format!(
                    "{} overlaps a driven plug; use force",
                    self.plug_name(&dst)
                )));
            }
            for c in clashing {
                self.disconnect(&c);
            }
        }

        if let Some(existing) = self.source(&dst) {
            if existing == *src {
                return Ok(dst);
            }
            if !force {
                return Err(GraphError::invalid(format!(
                    "{} is already connected; use force",
                    self.plug_name(&dst)
                )));
            }
            self.disconnect(&dst);
        }
        // Undo puts back the value the plug held before it was driven, not the last one pulled through the connection.
        let mut before = None;
        let mut new_element = false;
        let heavy = matches!(da.ty, PlugType::Mesh | PlugType::Data);
        if dst.element >= 0 {
            match self
                .node(dst.node)
                .unwrap()
                .element(dst.attr as usize, dst.element)
            {
                None => new_element = true,
                Some(v) if !heavy => before = Some(v.clone()),
                Some(_) => {}
            }
        } else if !heavy {
            let v = self.node(dst.node).unwrap().value(dst.attr as usize);
            before = Some(if dst.component >= 0 {
                PlugValue::Float(vec3_component(v, dst.component))
            } else {
                v.clone()
            });
        }
        self.connect_raw(src, &dst);
        let s = *src;
        self.record(
            move |g| {
                g.disconnect_raw(&dst);
                if new_element {
                    g.erase_element_raw(&dst);
                } else if let Some(b) = &before {
                    if g.nodes.contains_key(&dst.node) {
                        g.set_raw(&dst, b);
                    }
                }
            },
            move |g| g.connect_raw(&s, &dst),
        );
        Ok(dst)
    }

    pub fn disconnect(&mut self, dst: &PlugRef) {
        let Some(s) = self.source(dst) else { return };
        let d = *dst;
        self.disconnect_raw(&d);
        self.record(
            move |g| g.connect_raw(&s, &d),
            move |g| g.disconnect_raw(&d),
        );
    }

    pub fn source(&self, dst: &PlugRef) -> Option<PlugRef> {
        self.inputs.get(dst).copied()
    }

    pub fn destinations(&self, src: &PlugRef) -> Vec<PlugRef> {
        self.outgoing_dst
            .get(&src.node)
            .map(|dsts| {
                dsts.iter()
                    .filter(|d| self.inputs.get(d) == Some(src))
                    .copied()
                    .collect()
            })
            .unwrap_or_default()
    }

    // Arrays

    /// An array's elements, connected or holding a value, by ascending index.
    pub fn elements(&self, array: &PlugRef) -> Vec<PlugRef> {
        self.node(array.node)
            .map(|n| {
                n.elements(array.attr as usize)
                    .iter()
                    .map(|(i, _)| PlugRef::element(array.node, array.attr, *i))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The element indices any child of a grouped array uses, ascending.
    pub fn group_indices(&self, id: NodeId, group: &str) -> Vec<i32> {
        let mut out = Vec::new();
        if let Some(n) = self.node(id) {
            for a in n.ty().group_children(group) {
                out.extend(n.elements(a).iter().map(|(i, _)| *i));
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Makes a storable array's elements values[0..n-1], dropping its other ones. Errors when one of them is connected.
    pub fn set_elements(&mut self, array: &PlugRef, values: &[PlugValue]) -> Result<()> {
        let n = self
            .node(array.node)
            .ok_or_else(|| GraphError::invalid("no such node"))?;
        let whole = array.whole();
        let a = &n.ty().attrs[array.attr as usize];
        if !a.is_array() || !a.has(flags::STORABLE) || !a.has(flags::INPUT) {
            return Err(GraphError::invalid(format!(
                "{} is not an array that holds values",
                self.plug_name(&whole)
            )));
        }
        if self.is_locked(&whole) {
            return Err(GraphError::invalid(format!(
                "{} is locked and can't be changed",
                self.plug_name(&whole)
            )));
        }
        for e in self.elements(&whole) {
            if self.source(&e).is_some() {
                return Err(GraphError::invalid(format!(
                    "{} is driven by a connection",
                    self.plug_name(&e)
                )));
            }
        }
        let before: Vec<(i32, PlugValue)> = n.elements(array.attr as usize).to_vec();
        let after: Vec<(i32, PlugValue)> = values
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, v)| (i as i32, v))
            .collect();
        if before == after {
            return Ok(());
        }
        self.set_elements_raw(&whole, &after);
        self.record(
            move |g| g.set_elements_raw(&whole, &before),
            move |g| g.set_elements_raw(&whole, &after),
        );
        Ok(())
    }

    /// Removes an element: its connection and its value.
    pub fn remove_element(&mut self, e: &PlugRef) -> Result<()> {
        if self.node(e.node).is_none() || e.element < 0 {
            return Err(GraphError::invalid(
                "removeElement: expected an element plug (node.attr[n])",
            ));
        }
        self.disconnect(e);
        let Some(was) = self
            .node(e.node)
            .unwrap()
            .element(e.attr as usize, e.element)
            .cloned()
        else {
            return Ok(());
        };
        let e = *e;
        self.erase_element_raw(&e);
        self.record(
            move |g| g.set_raw(&e, &was),
            move |g| g.erase_element_raw(&e),
        );
        Ok(())
    }

    /// Gives node `to` the elements node `from` has on the array attr (both of one type): the values the unconnected
    /// ones hold, and with `share_sources` the connected ones' sources too.
    pub fn copy_elements(
        &mut self,
        from: NodeId,
        to: NodeId,
        attr: i32,
        share_sources: bool,
    ) -> Result<()> {
        let stores = self
            .node(from)
            .ok_or_else(|| GraphError::invalid("no such node"))?
            .ty()
            .attrs[attr as usize]
            .has(flags::STORABLE);
        for e in self.elements(&PlugRef::new(from, attr)) {
            let dst = PlugRef::element(to, attr, e.element);
            if let Some(src) = self.source(&e) {
                if share_sources {
                    self.connect(&src, &dst, true)?;
                }
            } else if stores {
                let v = self.value(&e)?;
                self.set_value(dst, v)?;
            }
        }
        Ok(())
    }

    // User attrs

    /// Every connection and held plug on id's attrs renumbered by map(old index).
    fn remap_attr_refs(&mut self, id: NodeId, map: impl Fn(i32) -> i32) {
        let moved = |mut p: PlugRef| {
            if p.node == id {
                p.attr = map(p.attr);
            }
            p
        };
        self.inputs = std::mem::take(&mut self.inputs)
            .into_iter()
            .map(|(d, s)| (moved(d), moved(s)))
            .collect();
        for dsts in self.outgoing_dst.values_mut() {
            for d in dsts.iter_mut() {
                *d = moved(*d);
            }
        }
        self.held = std::mem::take(&mut self.held)
            .into_iter()
            .map(moved)
            .collect();
    }

    fn shift_attr_refs(&mut self, id: NodeId, from: i32, delta: i32) {
        self.remap_attr_refs(id, |a| if a >= from { a + delta } else { a });
    }

    /// The node's private type, made on first use.
    fn own_type_mut(&mut self, id: NodeId) -> &mut NodeType {
        let n = self.node_mut(id);
        if !n.own_type {
            let mut t = (*n.data.ty).clone();
            t.user_of = Some(registered(&n.data.ty));
            n.data.ty = Arc::new(t);
            n.own_type = true;
        }
        Arc::make_mut(&mut n.data.ty)
    }

    fn insert_attr_raw(&mut self, id: NodeId, index: usize, s: &AttrState) {
        self.own_type_mut(id);
        self.shift_attr_refs(id, index as i32, 1);
        let n = self.node_mut(id);
        Arc::make_mut(&mut n.data.ty)
            .attrs
            .insert(index, s.def.clone());
        n.data.values.insert(index, s.value.clone());
        if !n.locks.is_empty() || s.lock != 0 {
            let len = n.locks.len().max(index);
            n.locks.resize(len, 0);
            n.locks.insert(index, s.lock);
        }
        if !n.data.elements.is_empty() || !s.elements.is_empty() {
            let len = n.data.elements.len().max(index);
            n.data.elements.resize_with(len, Vec::new);
            n.data.elements.insert(index, s.elements.clone());
        }
        self.mark_dirty(id);
        self.changed(Change::Attrs, id, None);
    }

    fn remove_attr_raw(&mut self, id: NodeId, index: usize) -> AttrState {
        let n = self.node_mut(id);
        let s = AttrState {
            def: n.data.ty.attrs[index].clone(),
            value: n.data.values[index].clone(),
            elements: n.data.elements(index).to_vec(),
            lock: n.lock_mask(index),
        };
        Arc::make_mut(&mut n.data.ty).attrs.remove(index);
        n.data.values.remove(index);
        if index < n.locks.len() {
            n.locks.remove(index);
        }
        if index < n.data.elements.len() {
            n.data.elements.remove(index);
        }
        self.shift_attr_refs(id, index as i32 + 1, -1);
        self.mark_dirty(id);
        self.changed(Change::Attrs, id, None);
        s
    }

    fn check_attr_name(name: &str) -> Result<()> {
        // group.child: one level of grouping.
        let ok = match name.find('.') {
            None => Self::valid_name(name),
            Some(dot) => Self::valid_name(&name[..dot]) && Self::valid_name(&name[dot + 1..]),
        };
        if ok {
            Ok(())
        } else {
            Err(GraphError::invalid(format!(
                "invalid attribute name '{name}'"
            )))
        }
    }

    /// Adds an attr to one node, after its type's, flagged USER (plus INPUT and STORABLE). A Vec3 without child names
    /// gets <name>X/Y/Z. Errors when the name or short name is taken. Returns its index.
    pub fn add_attr(&mut self, id: NodeId, mut def: AttrDef) -> Result<usize> {
        let n = self
            .node(id)
            .ok_or_else(|| GraphError::invalid("no such node"))?;
        Self::check_attr_name(&def.name)?;
        let t = n.ty();
        for want in [&def.name, &def.short_name] {
            if !want.is_empty() && t.attr_index(want).is_some() {
                return Err(GraphError::invalid(format!(
                    "'{}' already has an attribute '{want}'",
                    n.name
                )));
            }
        }
        let dot = def.name.find('.');
        let group = dot.map(|d| &def.name[..d]).unwrap_or(&def.name);
        let clash = if dot.is_none() {
            !t.group_children(group).is_empty()
        } else {
            t.attr_index(group).is_some()
        };
        if clash {
            return Err(GraphError::invalid(format!(
                "'{}' already has an attribute '{group}'",
                n.name
            )));
        }
        def.flags |= flags::USER | flags::INPUT | flags::STORABLE;
        if def.ty == PlugType::Vec3 {
            for (c, axis) in ['X', 'Y', 'Z'].into_iter().enumerate() {
                if def.child_names[c].is_empty() {
                    def.child_names[c] = format!("{}{axis}", def.name);
                }
            }
        }
        let index = t.attrs.len();
        let s = AttrState {
            value: def.default_value.clone(),
            def,
            elements: Vec::new(),
            lock: 0,
        };
        self.insert_attr_raw(id, index, &s);
        self.record(
            move |g| {
                g.remove_attr_raw(id, index);
            },
            move |g| g.insert_attr_raw(id, index, &s),
        );
        Ok(index)
    }

    fn touching(&self, p: &PlugRef) -> Vec<PlugRef> {
        self.inputs
            .iter()
            .filter(|(d, s)| {
                (d.node == p.node && d.attr == p.attr) || (s.node == p.node && s.attr == p.attr)
            })
            .map(|(d, _)| *d)
            .collect()
    }

    /// Removes a user attr with its connections and value. Later user attrs move down one index.
    pub fn delete_attr(&mut self, p: &PlugRef) -> Result<()> {
        let n = self
            .node(p.node)
            .ok_or_else(|| GraphError::invalid("no such node"))?;
        if !n.ty().attrs[p.attr as usize].has(flags::USER) {
            return Err(GraphError::invalid(format!(
                "{} belongs to the node's type; only attributes added with addAttr can be deleted",
                self.plug_name(&p.whole())
            )));
        }
        for dst in self.touching(p) {
            self.disconnect(&dst);
        }
        let (id, index) = (p.node, p.attr as usize);
        let s = self.remove_attr_raw(id, index);
        self.record(
            move |g| g.insert_attr_raw(id, index, &s),
            move |g| {
                g.remove_attr_raw(id, index);
            },
        );
        Ok(())
    }

    fn replace_attr_raw(&mut self, id: NodeId, index: usize, s: &AttrState) {
        self.own_type_mut(id).attrs[index] = s.def.clone();
        let n = self.node_mut(id);
        n.data.values[index] = s.value.clone();
        if !n.data.elements.is_empty() || !s.elements.is_empty() {
            let len = n.data.elements.len().max(index + 1);
            n.data.elements.resize_with(len, Vec::new);
            n.data.elements[index] = s.elements.clone();
        }
        if !n.locks.is_empty() || s.lock != 0 {
            let len = n.locks.len().max(index + 1);
            n.locks.resize(len, 0);
            n.locks[index] = s.lock;
        }
        self.mark_dirty(id);
        self.changed(Change::Attrs, id, None);
    }

    /// Replaces a user attr's definition: name, type, hints, flags. Connections, keys and drivers stay on it unless the
    /// type changes, which drops them; the value converts to the new type where it can and takes the new default where
    /// it can't. Undoable.
    pub fn edit_attr(&mut self, p: &PlugRef, mut def: AttrDef) -> Result<()> {
        let n = self
            .node(p.node)
            .ok_or_else(|| GraphError::invalid("no such node"))?;
        let old = n.ty().attrs[p.attr as usize].clone();
        if !old.has(flags::USER) {
            return Err(GraphError::invalid(format!(
                "{} belongs to the node's type; only attributes added with addAttr can be edited",
                self.plug_name(&p.whole())
            )));
        }
        Self::check_attr_name(&def.name)?;
        let t = n.ty();
        for want in [&def.name, &def.short_name] {
            if want.is_empty() {
                continue;
            }
            if t.attr_index(want).is_some_and(|at| at != p.attr as usize) {
                return Err(GraphError::invalid(format!(
                    "'{}' already has an attribute '{want}'",
                    n.name
                )));
            }
            for (i, a) in t.attrs.iter().enumerate() {
                if i != p.attr as usize
                    && a.ty == PlugType::Vec3
                    && (a.child_names.iter().any(|c| c == want)
                        || a.child_short.iter().any(|c| c == want))
                {
                    return Err(GraphError::invalid(format!(
                        "'{}' already has an attribute '{want}'",
                        n.name
                    )));
                }
            }
        }
        def.flags |= flags::USER | flags::INPUT | flags::STORABLE;
        if def.ty == PlugType::Vec3
            && (def.child_names[0].is_empty()
                || (old.ty == PlugType::Vec3
                    && def.child_names == old.child_names
                    && old.child_names[0] == format!("{}X", old.name)))
        {
            for (c, axis) in ['X', 'Y', 'Z'].into_iter().enumerate() {
                def.child_names[c] = format!("{}{axis}", def.name);
            }
        }
        if def.ty != PlugType::Vec3 {
            def.child_names = Default::default();
        }
        if !def.default_value.holds(def.ty) {
            let probe = AttrDef {
                ty: def.ty,
                default_value: default_for(def.ty),
                enum_names: def.enum_names.clone(),
                ..Default::default()
            };
            def.default_value = convert_value(&def.default_value, &probe);
        }
        let retyped = def.ty != old.ty || (def.flags & flags::ARRAY) != (old.flags & flags::ARRAY);
        if retyped {
            for dst in self.touching(p) {
                self.disconnect(&dst);
            }
        }
        let (id, index) = (p.node, p.attr as usize);
        let n = self.node(id).unwrap();
        let before = AttrState {
            def: old.clone(),
            value: n.value(index).clone(),
            elements: n.elements(index).to_vec(),
            lock: n.lock_mask(index),
        };
        let mut after = AttrState {
            value: convert_value(&before.value, &def),
            def: def.clone(),
            elements: Vec::new(),
            lock: if retyped { 0 } else { before.lock },
        };
        if def.is_array() {
            if old.is_array() {
                after.elements = before
                    .elements
                    .iter()
                    .map(|(i, v)| (*i, convert_value(v, &def)))
                    .collect();
            }
            after.value = def.default_value.clone();
        } else if old.is_array() {
            after.value = before
                .elements
                .first()
                .map(|(_, v)| convert_value(v, &def))
                .unwrap_or_else(|| def.default_value.clone());
        }
        self.replace_attr_raw(id, index, &after);
        self.record(
            move |g| g.replace_attr_raw(id, index, &before),
            move |g| g.replace_attr_raw(id, index, &after),
        );
        Ok(())
    }

    fn move_attr_raw(&mut self, id: NodeId, from: usize, to: usize) {
        fn rot<T: Default>(v: &mut Vec<T>, len: usize, from: usize, to: usize) {
            if v.is_empty() {
                return;
            }
            if v.len() < len {
                v.resize_with(len, T::default);
            }
            if from < to {
                v[from..=to].rotate_left(1);
            } else {
                v[to..=from].rotate_right(1);
            }
        }
        let n = self.node_mut(id);
        let len = n.data.ty.attrs.len();
        if from < to {
            n.data.values[from..=to].rotate_left(1);
        } else {
            n.data.values[to..=from].rotate_right(1);
        }
        rot(&mut n.data.elements, len, from, to);
        rot(&mut n.locks, len, from, to);
        let attrs = &mut Arc::make_mut(&mut n.data.ty).attrs;
        if from < to {
            attrs[from..=to].rotate_left(1);
        } else {
            attrs[to..=from].rotate_right(1);
        }
        let (f, t) = (from as i32, to as i32);
        self.remap_attr_refs(id, |a| {
            if a == f {
                t
            } else if f < t && a > f && a <= t {
                a - 1
            } else if t < f && a >= t && a < f {
                a + 1
            } else {
                a
            }
        });
        self.mark_dirty(id);
        self.changed(Change::Attrs, id, None);
    }

    /// Moves a user attr to index `to`, another user attr's place, shifting the ones between. Undoable.
    pub fn move_attr(&mut self, id: NodeId, from: usize, to: usize) -> Result<()> {
        let n = self
            .node(id)
            .ok_or_else(|| GraphError::invalid("no such node"))?;
        let attrs = &n.ty().attrs;
        for i in [from, to] {
            if i >= attrs.len() || !attrs[i].has(flags::USER) {
                return Err(GraphError::invalid(
                    "only attributes added with addAttr can be reordered, among themselves",
                ));
            }
        }
        if from == to {
            return Ok(());
        }
        self.move_attr_raw(id, from, to);
        self.record(
            move |g| g.move_attr_raw(id, to, from),
            move |g| g.move_attr_raw(id, from, to),
        );
        Ok(())
    }

    // ---------------------------------------------------------------------------------------------------------------
    // Evaluation

    fn mark_dirty(&mut self, id: NodeId) {
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            let Some(n) = self.nodes.get_mut(&cur) else {
                continue;
            };
            // Already dirty means everything downstream already is.
            if n.dirty && cur != id {
                continue;
            }
            n.dirty = true;
            if let Some(dsts) = self.outgoing_dst.get(&cur) {
                for dst in dsts {
                    // A held value lasts only until its source changes.
                    if cur == id {
                        self.held.remove(dst);
                    }
                    stack.push(dst.node);
                }
            }
            stack.extend(
                self.rel
                    .get(&cur)
                    .map(|r| r.children.as_slice())
                    .unwrap_or(&[]),
            );
        }
        self.any_dirty = true;
        self.version.fetch_add(1, Ordering::Relaxed);
    }

    /// Marks a node for recompute on the next `evaluate()`, like an input change would.
    pub fn touch(&mut self, id: NodeId) {
        self.mark_dirty(id);
    }

    /// View-only turns (turntables): each node's parentMatrix is premultiplied by its matrix as it evaluates, so it and
    /// everything under it draws, picks and edits turned. Not undoable, not saved.
    pub fn set_display_turns(&mut self, turns: HashMap<NodeId, DMat4>) {
        let mut roots_changed = turns.len() != self.display_turns.len();
        let old: Vec<(NodeId, DMat4)> = self.display_turns.iter().map(|(k, v)| (*k, *v)).collect();
        for (id, m) in old {
            match turns.get(&id) {
                None => {
                    roots_changed = true;
                    self.mark_dirty(id);
                }
                Some(t) if *t != m => self.mark_dirty(id),
                _ => {}
            }
        }
        let new_ids: Vec<NodeId> = turns
            .keys()
            .filter(|k| !self.display_turns.contains_key(k))
            .copied()
            .collect();
        for id in new_ids {
            roots_changed = true;
            self.mark_dirty(id);
        }
        self.display_turns = turns;
        if !roots_changed {
            return;
        }
        self.turned.clear();
        let ids: Vec<NodeId> = self.display_turns.keys().copied().collect();
        for id in ids {
            self.turned.insert(id);
            let d = self.descendants(id);
            self.turned.extend(d);
        }
    }
    pub fn display_turns(&self) -> &HashMap<NodeId, DMat4> {
        &self.display_turns
    }

    pub fn time(&self) -> f64 {
        self.ctx.time
    }

    /// The C++ also sends a TimeChanged event when `notify` is on; the event bus is not ported, so listeners on time go
    /// through whatever owns the graph.
    pub fn set_time(&mut self, seconds: f64) {
        if self.ctx.time == seconds {
            return;
        }
        self.ctx.time = seconds;
        self.held.clear();
        let timed: Vec<NodeId> = self
            .nodes
            .iter()
            .filter(|(_, n)| n.data.ty.time_dependent)
            .map(|(id, _)| *id)
            .collect();
        for id in timed {
            self.mark_dirty(id);
        }
        self.version.fetch_add(1, Ordering::Relaxed);
    }

    /// Counts evaluations that computed something.
    pub fn evaluation_pass(&self) -> u64 {
        self.evaluation_pass
    }
    /// Bumped on every change; the renderer uses it to skip unchanged frames.
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Relaxed)
    }
    /// Changes whenever a node is created or deleted or the graph is cleared, to a value no other graph has had.
    pub fn structure(&self) -> u64 {
        self.structure
    }
    /// Bumped by every change that can alter which nodes are hidden.
    pub fn visibility_version(&self) -> u64 {
        self.visibility_version
    }

    /// Called on the main thread by `evaluate()`: before computing anything dirty and once done. Evaluations the hook
    /// makes itself don't call it again. Simulations use it to step on the main thread.
    pub fn set_evaluate_hook(&mut self, hook: EvaluateHook) {
        self.evaluate_hook = Some(hook);
    }
    /// Further hooks, called after the one set above in the order added.
    pub fn add_evaluate_hook(&mut self, hook: EvaluateHook) {
        self.extra_evaluate_hooks.push(hook);
    }
    /// Stands in for `sim::sampling()`.
    pub fn set_sampling(&mut self, on: bool) {
        self.sampling = on;
    }
    pub fn set_active_frame_fn(&mut self, f: ActiveFrameFn) {
        self.active_frame = Some(f);
    }

    pub fn evaluate(&mut self) {
        if DEFER_MESHES.with(|d| d.get()) > 0 && self.any_dirty {
            let roots: Vec<NodeId> = self
                .nodes
                .iter()
                .filter(|(_, n)| n.dirty && n.data.ty.dag && !n.data.ty.is_a("mesh"))
                .map(|(id, _)| *id)
                .collect();
            self.evaluate_with(Some(&roots));
            return;
        }
        self.evaluate_with(None);
    }

    /// Computes only what the given nodes depend on, leaving the rest dirty for the next `evaluate()`, without the
    /// evaluate hooks.
    pub fn evaluate_upstream(&mut self, nodes: &[NodeId]) {
        self.evaluate_dirty(Some(nodes));
    }

    fn evaluate_with(&mut self, only: Option<&[NodeId]>) {
        let mut hooks: Vec<EvaluateHook> = self.evaluate_hook.iter().cloned().collect();
        hooks.extend(self.extra_evaluate_hooks.iter().cloned());
        if hooks.is_empty() || self.in_evaluate_hook {
            self.evaluate_dirty(only);
            return;
        }
        let call = |g: &mut Graph, after: bool| {
            g.in_evaluate_hook = true;
            for h in &hooks {
                h(g, after);
            }
            g.in_evaluate_hook = false;
        };
        if self.any_dirty {
            call(self, false);
        }
        self.evaluate_dirty(only);
        call(self, true);
    }

    fn evaluate_dirty(&mut self, only: Option<&[NodeId]>) {
        if !self.any_dirty {
            return;
        }
        // Just the dirty nodes `only` depends on. Upstream of a clean node is clean, so the walk stops at clean nodes.
        let mut needed = HashSet::new();
        if let Some(only) = only {
            let mut stack = only.to_vec();
            while let Some(id) = stack.pop() {
                if !self.nodes.get(&id).is_some_and(|n| n.dirty) || !needed.insert(id) {
                    continue;
                }
                let p = self.parent(id);
                if p != NO_NODE {
                    stack.push(p);
                }
                stack.extend(inputs_of(&self.inputs, id).map(|(_, s)| s.node));
            }
            if needed.is_empty() {
                return;
            }
        }
        self.any_dirty = false;
        self.evaluation_pass += 1;
        let pass = self.evaluation_pass;
        // Partial and simulation-sampling evaluations compute only what a reader needs, not the scene at this time.
        let cache = self.playback.is_some() && only.is_none() && !self.sampling;

        // Kahn levels over the dirty subgraph; each level computes in parallel.
        let mut dirty = Vec::new();
        for (id, n) in &self.nodes {
            if n.dirty {
                if only.is_some() && !needed.contains(id) {
                    self.any_dirty = true;
                } else {
                    dirty.push(*id);
                }
            }
        }
        dirty.sort_unstable();
        let mut pending: HashMap<NodeId, i32> = dirty.iter().map(|&id| (id, 0)).collect();
        let mut downstream: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        let mut add_edge = |up: NodeId, down: NodeId, pending: &mut HashMap<NodeId, i32>| {
            if up == down || !pending.contains_key(&up) {
                return;
            }
            downstream.entry(up).or_default().push(down);
            *pending.get_mut(&down).unwrap() += 1;
        };
        for &id in &dirty {
            let p = self.parent(id);
            if p != NO_NODE {
                add_edge(p, id, &mut pending);
            }
        }
        for (dst, src) in &self.inputs {
            if pending.contains_key(&dst.node) {
                add_edge(src.node, dst.node, &mut pending);
            }
        }

        let mut level: Vec<NodeId> = dirty
            .iter()
            .copied()
            .filter(|id| pending[id] == 0)
            .collect();
        while !level.is_empty() {
            let mut owned: Vec<Box<Node>> = level
                .iter()
                .map(|id| self.nodes.remove(id).unwrap())
                .collect();
            {
                let view = EvalView {
                    nodes: &self.nodes,
                    rel: &self.rel,
                    inputs: &self.inputs,
                    held: &self.held,
                    display_turns: &self.display_turns,
                    turned: &self.turned,
                    playback: self.playback.as_ref(),
                    active_frame: self.active_frame.as_ref(),
                    ctx: self.ctx,
                    pass,
                    cache,
                };
                if owned.len() < 16 {
                    for n in owned.iter_mut() {
                        view.pull(n);
                    }
                } else {
                    owned.par_iter_mut().for_each(|n| view.pull(n));
                }
            }
            for n in owned {
                self.nodes.insert(n.id, n);
            }
            let mut next = Vec::new();
            for id in &level {
                if let Some(ds) = downstream.get(id) {
                    for d in ds {
                        let c = pending.get_mut(d).unwrap();
                        *c -= 1;
                        if *c == 0 {
                            next.push(*d);
                        }
                    }
                }
            }
            level = next;
        }
    }

    // ---------------------------------------------------------------------------------------------------------------
    // Selection

    pub fn selection(&self) -> &[NodeId] {
        &self.selection
    }

    fn select_raw(&mut self, ids: Vec<NodeId>) {
        self.selection = ids;
        self.selection_serial += 1;
        self.version.fetch_add(1, Ordering::Relaxed);
        self.changed(Change::Selection, NO_NODE, None);
    }

    pub fn select(&mut self, ids: &[NodeId], mode: SelectMode) {
        let mut next = if mode == SelectMode::Replace {
            Vec::new()
        } else {
            self.selection.clone()
        };
        for &id in ids {
            if self.node(id).is_none() {
                continue;
            }
            let at = next.iter().position(|&x| x == id);
            match (mode, at) {
                (SelectMode::Replace | SelectMode::Add, None) => next.push(id),
                (SelectMode::Remove, Some(i)) => {
                    next.remove(i);
                }
                (SelectMode::Toggle, Some(i)) => {
                    next.remove(i);
                }
                (SelectMode::Toggle, None) => next.push(id),
                _ => {}
            }
        }
        if next == self.selection {
            return;
        }
        let old = self.selection.clone();
        self.select_raw(next.clone());
        self.undo.record(
            FnAction::new(
                move |g: &mut Graph| g.select_raw(old.clone()),
                move |g: &mut Graph| g.select_raw(next.clone()),
            )
            .selection(true),
        );
    }

    pub fn is_selected(&self, id: NodeId) -> bool {
        self.selection.contains(&id)
    }
    /// Bumped whenever the selection changes, including when a deleted node leaves it.
    pub fn selection_serial(&self) -> u64 {
        self.selection_serial
    }

    // ---------------------------------------------------------------------------------------------------------------
    // Listeners

    pub fn add_listener(&mut self, f: Listener, immediate: bool) -> i32 {
        let id = self.next_listener;
        self.next_listener += 1;
        self.listeners.insert(id, ListenerEntry { f, immediate });
        id
    }
    pub fn remove_listener(&mut self, id: i32) {
        self.listeners.remove(&id);
    }
    /// Inside a listener's Value or Connection call: the plug that changed, when one did.
    pub fn changed_plug(&self) -> Option<&PlugRef> {
        self.changed_plug.as_ref()
    }

    /// Scene loads and imports: batched listeners hear nothing until the end, then one Reset. Nests.
    pub fn begin_batch(&mut self) {
        self.batch_depth += 1;
    }
    pub fn end_batch(&mut self) {
        self.batch_depth -= 1;
        if self.batch_depth > 0 || !self.batch_pending {
            return;
        }
        self.batch_pending = false;
        let ls: Vec<Listener> = self
            .listeners
            .values()
            .filter(|l| !l.immediate)
            .map(|l| l.f.clone())
            .collect();
        for f in ls {
            f(self, Change::Reset, NO_NODE);
        }
    }
    pub fn batch<R>(&mut self, f: impl FnOnce(&mut Graph) -> R) -> R {
        self.begin_batch();
        let r = f(self);
        self.end_batch();
        r
    }
    pub fn batching(&self) -> bool {
        self.batch_depth > 0
    }

    fn changes_visibility(&self, c: Change, id: NodeId, plug: Option<&PlugRef>) -> bool {
        if matches!(c, Change::Selection | Change::Renamed) {
            return false;
        }
        if !matches!(c, Change::Value | Change::Connection) {
            return true;
        }
        let Some(plug) = plug else { return true };
        if !self.node(id).is_some_and(|n| n.ty().dag) {
            return true;
        }
        plug.attr as usize == Transform::VISIBILITY
            || plug.attr as usize == Transform::DISPLAY_LAYER
    }

    fn changed(&mut self, c: Change, id: NodeId, plug: Option<PlugRef>) {
        self.version.fetch_add(1, Ordering::Relaxed);
        if self.changes_visibility(c, plug.map(|p| p.node).unwrap_or(id), plug.as_ref()) {
            self.visibility_version += 1;
        }
        if c != Change::Selection && !self.sampling {
            self.clear_playback_cache();
        }
        if self.listeners.is_empty() {
            return;
        }
        let outer = std::mem::replace(&mut self.changed_plug, plug);
        let mut call = Vec::new();
        for l in self.listeners.values() {
            if self.batch_depth == 0 || l.immediate {
                call.push(l.f.clone());
            } else {
                self.batch_pending = true;
            }
        }
        for f in call {
            f(self, c, id);
        }
        self.changed_plug = outer;
    }

    pub fn set_naming_policy(&mut self, policy: NamingPolicy) {
        self.naming = policy;
    }
    pub fn naming_policy(&self) -> &NamingPolicy {
        &self.naming
    }
    /// NamingPolicy::unique_names: names unique across the scene rather than among siblings.
    pub fn unique_names(&self) -> bool {
        self.naming.unique_names
    }
}

fn physical_memory_bytes() -> usize {
    let mut s = sysinfo::System::new();
    s.refresh_memory();
    s.total_memory() as usize
}
