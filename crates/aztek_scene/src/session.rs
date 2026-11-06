//! The context commands run against: the graph, the command registry, and the scene-level state the C++ keeps in
//! globals (pickWalk's ghosts, pending compounds, the naming prefs).

use std::collections::BTreeMap;

use aztek_ops::{CallSource, CommandDef, CommandError, CommandHost, Commands};
use serde_json::{json, Value};

use crate::error::GraphError;
use crate::graph::Graph;
use crate::naming::NamingPolicy;
use crate::node::types;
use crate::plug::NodeId;

impl From<GraphError> for CommandError {
    fn from(e: GraphError) -> Self {
        CommandError::new(e.to_string())
    }
}

/// A deleted object's place in the hierarchy, so a pickWalk right after the delete can walk on from it.
#[derive(Debug, Clone, Default)]
pub(crate) struct WalkGhost {
    pub id: NodeId,
    pub parent: NodeId,
    /// Walkable, itself included, in order.
    pub siblings: Vec<NodeId>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct WalkGhosts {
    pub selection_serial: u64,
    pub ghosts: Vec<WalkGhost>,
}

pub struct Session {
    pub graph: Graph,
    pub commands: Commands<Session>,
    pub(crate) walk_ghosts: WalkGhosts,
    /// Compounds declared with addAttr(at='compound') that have no children yet: multi or not.
    pub(crate) pending_compounds: BTreeMap<(NodeId, String), bool>,
    /// The "naming" prefs file's contents. The prefs store is not ported; this holds what it would.
    pub naming_prefs: Value,
    /// The last adjustable op run from the UI.
    pub(crate) last_op: crate::last_op::State,
    /// The paint skin weights tool's settings (artAttrSkinPaintCtx).
    pub paint_ctx: crate::skin::PaintCtx,
    /// What copyKey and cutKey took.
    pub key_clipboard: crate::anim_edit::KeyClipboard,
    /// selectKey's keys: curve and frame.
    pub key_selection: Vec<(NodeId, f64)>,
    /// The live surface: what Quad Draw and snapping draw on.
    pub live_surface: Vec<NodeId>,
    /// polyOptions' display settings, polySelectConstraint's state and polyClipboard's contents.
    pub poly_state: crate::mesh_edit::maya_poly::PolyState,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    /// A session with the built-in node types and every ported command registered.
    pub fn new() -> Self {
        types::register_builtins();
        let mut s = Session {
            graph: Graph::new(),
            commands: Commands::default(),
            walk_ghosts: WalkGhosts::default(),
            pending_compounds: BTreeMap::new(),
            naming_prefs: json!({}),
            last_op: Default::default(),
            paint_ctx: Default::default(),
            key_clipboard: Default::default(),
            key_selection: Vec::new(),
            live_surface: Vec::new(),
            poly_state: Default::default(),
        };
        crate::commands::register(&mut s.commands);
        crate::components::register(&mut s.commands);
        crate::modifiers::register(&mut s.commands);
        crate::modifiers::register_stack_commands(&mut s.commands);
        crate::instancing::register(&mut s.commands);
        crate::mesh_nodes::register(&mut s.commands);
        crate::component_query::register(&mut s.commands);
        crate::mesh_edit::register(&mut s.commands);
        crate::mesh_files::register(&mut s.commands);
        crate::object_sets::register(&mut s.commands);
        crate::split_rings_modifier::register(&mut s.commands);
        crate::auto_bevel::register(&mut s.commands);
        crate::conform_commands::register(&mut s.commands);
        crate::object_ops::register(&mut s.commands);
        crate::display_layers::register(&mut s.commands);
        crate::duplicate::register(&mut s.commands);
        crate::booleans::register(&mut s.commands);
        crate::anim::register(&mut s.commands);
        crate::uv_ops::register(&mut s.commands);
        crate::rig::register(&mut s.commands);
        crate::skin::register(&mut s.commands);
        crate::deform::register(&mut s.commands);
        crate::curves::register(&mut s.commands);
        crate::maya_cmds::register(&mut s.commands);
        crate::maya_core::register(&mut s.commands);
        // After the modules above: these replace or extend commands they registered.
        crate::mesh_edit::maya_poly::register(&mut s.commands);
        // Last: history node types take their attributes from the commands registered above.
        crate::history::register(&mut s.commands);
        crate::instancing::install(&mut s.graph);
        crate::duplicate::install(&mut s.graph);
        crate::anim::install(&mut s.graph);
        s.load_naming_policy();
        s
    }

    pub fn register(&mut self, def: CommandDef<Session>) {
        self.commands.register(def);
    }

    /// Runs a command from a script.
    pub fn run(&mut self, name: &str, call: Value) -> Result<Value, CommandError> {
        aztek_ops::run(self, name, call)
    }

    /// Undoes a step, keeping the last op in step with the stack.
    pub fn undo(&mut self) -> bool {
        let r = self.graph.undo();
        crate::last_op::sync(self);
        r
    }

    pub fn redo(&mut self) -> bool {
        let r = self.graph.redo();
        crate::last_op::sync(self);
        r
    }

    pub fn load_naming_policy(&mut self) {
        self.graph
            .set_naming_policy(NamingPolicy::from_json(&self.naming_prefs));
    }
}

impl CommandHost for Session {
    fn commands(&self) -> &Commands<Self> {
        &self.commands
    }
    fn commands_mut(&mut self) -> &mut Commands<Self> {
        &mut self.commands
    }
    fn selection_paths(&self) -> Vec<String> {
        self.graph
            .selection()
            .iter()
            .map(|&id| self.graph.path(id))
            .collect()
    }
    fn open_undo_chunk(&mut self, label: &str) {
        self.graph.open_chunk(label);
    }
    fn close_undo_chunk(&mut self) {
        self.graph.close_chunk();
    }
    fn running_args_changed(&mut self, args: Option<&aztek_ops::Args>) {
        // A command's own default name ("Group") without a name flag is made unique across the scene.
        self.graph
            .set_command_default_names(args.is_some_and(|a| !a.has("name")));
    }
    fn top_level_started(&mut self, name: &str, call: &Value, _source: CallSource) {
        crate::components::command_started(&mut self.graph);
        crate::last_op::command_started(self, name, call);
    }
    fn top_level_finished(&mut self, name: &str, call: &Value, source: CallSource, ok: bool) {
        crate::components::command_finished(&mut self.graph, name, source, ok);
        crate::last_op::command_finished(self, name, call, source, ok);
    }
}
