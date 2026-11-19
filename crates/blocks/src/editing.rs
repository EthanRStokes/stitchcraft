//! The canvas's edit commands over an open [`ModProject`]: strands, blocks,
//! values, notes, variables, lists, dicts, custom blocks and undo.
//!
//! [`Session::dispatch`] takes the command names and camelCase arguments the
//! QML sends - the same vocabulary blockstitch's `PaletteDragController` and
//! `BlockCanvas` speak, and the one Blockwork's editor uses - so the Qt layer
//! is a single `invoke` and every rule lives here, where it is tested without
//! Qt.
//!
//! Instructions cross in the frontend's flat shape and are read back with
//! [`graph::from_frontend`]; nothing here guesses at JSON shapes.

use crate::block::McBlock;
use crate::document::ModProject;
use blockstitch_core::editor::{
    Created, EditCommand, EditContext, EditError, EditOutcome, EditSession, History, InstrPath,
    ValueBuffers, ValueDestination, ValueLocation, apply_buffer_effects, drop_strand_buffers,
    prune_value_buffers, retain_live_buffers,
};
use blockstitch_core::graph::{
    self, BlockDef, BlockGraph, BlockKind, BlockPiece, BlockShape, DictEntry, Fragment,
    Instruction, ListItem, normalize_block_color,
};
use blockstitch_core::value::Value;
use serde::de::DeserializeOwned;
use serde_json::{Value as Json, json};

/// How many undo steps a session keeps.
pub const UNDO_LIMIT: usize = 200;

/// Every command [`Session::dispatch`] answers.
pub const COMMANDS: &[&str] = &[
    "add_strand",
    "remove_strand",
    "move_strand",
    "split_strand",
    "merge_strand",
    "merge_tail",
    "clear_instructions",
    "insert_instruction",
    "edit_instruction",
    "remove_instruction",
    "remove_instructions",
    "copy_instructions",
    "paste_fragment",
    "duplicate_instruction",
    "edit_value_field",
    "take_value",
    "put_value",
    "move_value",
    "create_floating_value",
    "move_floating_value",
    "remove_floating_value",
    "create_comment",
    "create_attached_comment",
    "move_comment",
    "edit_comment_text",
    "set_comment_collapsed",
    "remove_comment",
    "create_variable",
    "rename_variable",
    "delete_variable",
    "create_list",
    "rename_list",
    "delete_list",
    "set_list_items",
    "set_list_editor_state",
    "create_dict",
    "rename_dict",
    "delete_dict",
    "set_dict_entries",
    "set_dict_editor_state",
    "create_block",
    "edit_block",
    "delete_block",
    "undo",
    "redo",
];

/// One open project and what editing it needs: undo history and half-typed
/// numbers. `revision` moves every time the document changes, which is how
/// the Qt layer knows to push it out again.
pub struct Session {
    pub project: ModProject,
    pub history: History<BlockGraph<McBlock>>,
    pub buffers: ValueBuffers,
    revision: u64,
}

impl Default for Session {
    fn default() -> Self {
        Self::new(ModProject::default())
    }
}

fn arg<T: DeserializeOwned>(args: &Json, name: &str) -> Result<T, String> {
    let value = args.get(name).cloned().unwrap_or(Json::Null);
    serde_json::from_value(value).map_err(|e| format!("invalid argument '{name}': {e}"))
}

/// An instruction in the frontend's flat shape (or the stored one).
fn instruction_arg(args: &Json, name: &str) -> Result<Instruction<McBlock>, String> {
    let value = args.get(name).cloned().unwrap_or(Json::Null);
    graph::from_frontend(value).map_err(|e| format!("That block isn't one Stitchcraft knows: {e}"))
}

fn ok() -> Result<Json, String> {
    Ok(Json::Null)
}

fn shape_of(name: &str) -> BlockShape {
    match name {
        "Ending" => BlockShape::Ending,
        "ReturnsValue" => BlockShape::ReturnsValue,
        "ReturnsBool" => BlockShape::ReturnsBool,
        _ => BlockShape::Normal,
    }
}

/// A custom block's definition, as `BlockDefinitionEditor` drafts it.
struct BlockDraft {
    pieces: Vec<BlockPiece>,
    shape: BlockShape,
    color: String,
    icon: String,
    identifier: String,
    description: String,
}

impl BlockDraft {
    fn from_args(args: &Json) -> Result<Self, String> {
        let pieces: Vec<BlockPiece> = arg(args, "pieces")?;
        // Stitchcraft's exporter has no way to run a branch, so the editor
        // never offers one; a hand-edited draft is refused here too.
        if pieces.iter().any(|p| matches!(p, BlockPiece::Branch { .. })) {
            return Err("Custom blocks can't have branches in a mod yet".to_string());
        }
        BlockDef::validate_pieces(&pieces)?;
        BlockDef::validate_input_defaults(&pieces)?;
        let color: String = arg(args, "color").unwrap_or_default();
        Ok(Self {
            pieces,
            shape: shape_of(&arg::<String>(args, "shape").unwrap_or_default()),
            color: normalize_block_color(&color).unwrap_or_else(graph::default_block_color),
            icon: arg(args, "icon").unwrap_or_default(),
            identifier: arg(args, "identifier").unwrap_or_default(),
            description: arg(args, "description").unwrap_or_default(),
        })
    }
}

impl Session {
    pub fn new(project: ModProject) -> Self {
        Self {
            project,
            history: History::new(UNDO_LIMIT),
            buffers: ValueBuffers::new(),
            revision: 0,
        }
    }

    /// Bumped by every change to the document.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Swaps in another project with a clean history.
    pub fn replace(&mut self, project: ModProject) {
        self.project = project;
        self.history.clear();
        self.buffers.clear();
        self.changed();
    }

    /// Marks a change made outside [`Session::dispatch`] (the mod settings).
    pub fn changed(&mut self) {
        self.revision += 1;
    }

    fn graph(&mut self) -> &mut BlockGraph<McBlock> {
        &mut self.project.graph
    }

    /// Checkpoints before a structural edit and ends any typing group.
    fn checkpoint(&mut self) {
        self.checkpoint_for(None);
    }

    /// Checkpoints unless this continues the typing group `session`.
    fn checkpoint_for(&mut self, session: Option<EditSession>) {
        let snapshot = self.project.graph.clone();
        self.history.checkpoint(snapshot, session);
    }

    /// One typed edit through the core: refused and no-op edits cost no undo
    /// step.
    fn run_edit(&mut self, command: EditCommand<McBlock>) -> Result<EditOutcome, EditError> {
        let prepared = self
            .project
            .graph
            .prepare_edit(command, &EditContext::default())?;
        if prepared.changes() {
            let snapshot = self.project.graph.clone();
            self.history.checkpoint(snapshot, prepared.coalesce().cloned());
        }
        let outcome = self.project.graph.commit_edit(prepared);
        apply_buffer_effects(&mut self.buffers, &outcome.buffers);
        if outcome.changed {
            self.changed();
        }
        Ok(outcome)
    }

    /// An edit tried on a copy first, so a refusal costs no undo step.
    fn on_copy<T>(
        &mut self,
        edit: impl FnOnce(&mut BlockGraph<McBlock>) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut copy = self.project.graph.clone();
        let result = edit(&mut copy)?;
        self.checkpoint();
        self.project.graph = copy;
        self.changed();
        Ok(result)
    }

    /// Runs `cmd`. An unknown command is an error, never ignored.
    pub fn dispatch(&mut self, cmd: &str, args: &Json) -> Result<Json, String> {
        match cmd {
            // ── Strands ──────────────────────────────────────────────────
            "add_strand" => {
                let instruction = match args.get("instruction") {
                    Some(Json::Null) | None => None,
                    Some(_) => Some(instruction_arg(args, "instruction")?),
                };
                let (dx, dy) = self.project.graph.next_strand_position();
                let x = arg::<Option<i32>>(args, "x")?.unwrap_or(dx);
                let y = arg::<Option<i32>>(args, "y")?.unwrap_or(dy);
                self.checkpoint();
                let id = self.graph().add_strand(x, y, instruction.into_iter().collect());
                self.declare_variable(arg(args, "variableToCreate")?);
                self.changed();
                Ok(json!(id))
            }
            "remove_strand" => {
                let id: String = arg(args, "strandId")?;
                self.remove_strand(&id);
                ok()
            }
            "move_strand" => {
                // Where a stack sits is saved but isn't an undo step.
                let id: String = arg(args, "strandId")?;
                if self.graph().move_strand(&id, arg(args, "x")?, arg(args, "y")?) {
                    self.changed();
                }
                ok()
            }
            "split_strand" => {
                let (id, path): (String, InstrPath) = (arg(args, "strandId")?, arg(args, "path")?);
                let (x, y): (i32, i32) = (arg(args, "x")?, arg(args, "y")?);
                let new_id = self.on_copy(|g| g.split_strand(&id, &path, x, y))?;
                self.buffers.clear();
                Ok(json!(new_id))
            }
            "merge_strand" => {
                let (dragged, target): (String, String) =
                    (arg(args, "draggedId")?, arg(args, "targetId")?);
                let path: InstrPath = arg(args, "path")?;
                let target_y: Option<i32> = arg(args, "targetY")?;
                self.on_copy(|g| {
                    g.merge_strand(&dragged, &target, &path)?;
                    hold_top(g, &target, &path, target_y);
                    Ok(())
                })?;
                drop_strand_buffers(&mut self.buffers, &dragged);
                ok()
            }
            "merge_tail" => {
                let (id, path): (String, InstrPath) = (arg(args, "strandId")?, arg(args, "path")?);
                let (target, target_path): (String, InstrPath) =
                    (arg(args, "targetId")?, arg(args, "targetPath")?);
                let target_y: Option<i32> = arg(args, "targetY")?;
                self.on_copy(|g| {
                    g.merge_tail(&id, &path, &target, &target_path)?;
                    hold_top(g, &target, &target_path, target_y);
                    Ok(())
                })?;
                self.buffers.clear();
                ok()
            }
            "clear_instructions" => {
                self.checkpoint();
                self.graph().clear_strands();
                self.buffers.clear();
                self.changed();
                ok()
            }

            // ── Blocks ───────────────────────────────────────────────────
            "insert_instruction" => {
                let (id, path): (String, InstrPath) = (arg(args, "strandId")?, arg(args, "path")?);
                let instruction = instruction_arg(args, "instruction")?;
                let target_y: Option<i32> = arg(args, "targetY")?;
                let variable: Option<String> = arg(args, "variableToCreate")?;
                self.on_copy(|g| {
                    if !g.insert_instruction(&id, &path, instruction)? {
                        return Err("That stack isn't there any more".to_string());
                    }
                    hold_top(g, &id, &path, target_y);
                    Ok(())
                })?;
                self.declare_variable(variable);
                drop_strand_buffers(&mut self.buffers, &id);
                ok()
            }
            "edit_instruction" => {
                let (id, path): (String, InstrPath) = (arg(args, "strandId")?, arg(args, "path")?);
                let instruction = instruction_arg(args, "instruction")?;
                // Typing in one block's text field is one undo step.
                self.checkpoint_for(Some(EditSession::Instruction {
                    strand_id: id.clone(),
                    index: path.clone(),
                }));
                if self.graph().replace_instruction(&id, &path, instruction) {
                    self.changed();
                    ok()
                } else {
                    Err("That block isn't there any more".to_string())
                }
            }
            "remove_instruction" | "remove_instructions" => {
                let id: String = arg(args, "strandId")?;
                let path: InstrPath = arg(args, "path")?;
                let count: Option<usize> = arg(args, "count")?;
                if let Some(block_id) = self.header_block_at(&id, &path) {
                    // A custom block's header is the block: removing it takes
                    // the definition and its calls along, not just the hat.
                    self.delete_block(&block_id);
                    return ok();
                }
                let emptied = path.len() == 1
                    && path[0].index == 0
                    && self
                        .project
                        .strand(&id)
                        .is_some_and(|s| s.instructions.len() <= count.unwrap_or(1).max(1));
                if emptied {
                    self.remove_strand(&id);
                    return ok();
                }
                self.run_edit(EditCommand::RemoveInstructions {
                    strand_id: id,
                    path,
                    count: count.unwrap_or(1),
                })
                .map_err(String::from)?;
                ok()
            }
            "copy_instructions" => {
                let (id, path): (String, InstrPath) = (arg(args, "strandId")?, arg(args, "path")?);
                let fragment = self
                    .project
                    .graph
                    .copy_fragment(&id, &path, arg(args, "count")?)?;
                graph::to_frontend(&fragment).map_err(|e| e.to_string())
            }
            "paste_fragment" => {
                let fragment: Fragment<McBlock> =
                    graph::from_frontend(args.get("fragment").cloned().unwrap_or(Json::Null))
                        .map_err(|e| format!("Nothing to paste: {e}"))?;
                let outcome = self.run_edit(EditCommand::PasteFragment {
                    fragment,
                    x: arg(args, "x")?,
                    y: arg(args, "y")?,
                })?;
                let strand = outcome.created.into_iter().find_map(|c| match c {
                    Created::Strand(id) => Some(id),
                    _ => None,
                });
                Ok(json!(strand))
            }
            "duplicate_instruction" => {
                self.run_edit(EditCommand::DuplicateInstruction {
                    strand_id: arg(args, "strandId")?,
                    path: arg(args, "path")?,
                })?;
                ok()
            }

            // ── Values ───────────────────────────────────────────────────
            "edit_value_field" => {
                // A location that no longer resolves is quietly ignored: the
                // field was deleted while it was being typed in.
                let _ = self.run_edit(EditCommand::EditFieldText {
                    location: arg(args, "location")?,
                    text: arg(args, "text")?,
                });
                ok()
            }
            "take_value" => {
                let location: ValueLocation = arg(args, "location")?;
                self.checkpoint();
                let taken = self.graph().take_value(&location);
                prune_value_buffers(&mut self.buffers, &location);
                match taken {
                    Some(value) => {
                        self.changed();
                        Ok(json!(value))
                    }
                    None => Err("Nothing to take at that location".to_string()),
                }
            }
            "put_value" => {
                let location: ValueLocation = arg(args, "location")?;
                let value: Value = arg(args, "value")?;
                let mut copy = self.project.graph.clone();
                if !copy.put_value(&location, value) {
                    return Err("That value can't go there".to_string());
                }
                self.checkpoint();
                self.project.graph = copy;
                prune_value_buffers(&mut self.buffers, &location);
                self.changed();
                ok()
            }
            "move_value" => {
                let to = match arg::<Option<ValueLocation>>(args, "to")? {
                    Some(to) => ValueDestination::Slot(to),
                    None => ValueDestination::Canvas {
                        x: arg::<Option<i32>>(args, "x")?.unwrap_or(0),
                        y: arg::<Option<i32>>(args, "y")?.unwrap_or(0),
                        origin_block_id: arg(args, "originBlockId")?,
                    },
                };
                self.run_edit(EditCommand::MoveValue {
                    from: arg(args, "from")?,
                    to,
                })?;
                ok()
            }
            "create_floating_value" => {
                let value: Value = arg(args, "value")?;
                self.checkpoint();
                let id = self.graph().add_floating_value(
                    arg(args, "x")?,
                    arg(args, "y")?,
                    value,
                    arg(args, "originBlockId")?,
                );
                self.changed();
                Ok(json!(id))
            }
            "move_floating_value" => {
                let id: String = arg(args, "floatingId")?;
                if self
                    .graph()
                    .move_floating_value(&id, arg(args, "x")?, arg(args, "y")?)
                {
                    self.changed();
                }
                ok()
            }
            "remove_floating_value" => {
                let id: String = arg(args, "floatingId")?;
                if self.project.floating_value(&id).is_some() {
                    self.checkpoint();
                    self.graph().remove_floating_value(&id);
                    self.changed();
                }
                ok()
            }

            // ── Notes ────────────────────────────────────────────────────
            "create_comment" | "create_attached_comment" => {
                let attached: Option<String> = if cmd == "create_attached_comment" {
                    Some(arg(args, "instructionId")?)
                } else {
                    None
                };
                let (x, y): (i32, i32) = if attached.is_some() {
                    (arg(args, "dx")?, arg(args, "dy")?)
                } else {
                    (arg(args, "x")?, arg(args, "y")?)
                };
                let text: Option<String> = arg(args, "text")?;
                self.checkpoint();
                let id = self
                    .graph()
                    .add_comment(x, y, text.unwrap_or_default(), attached);
                self.changed();
                Ok(json!(id))
            }
            "move_comment" => {
                let id: String = arg(args, "commentId")?;
                if self.graph().move_comment(&id, arg(args, "x")?, arg(args, "y")?) {
                    self.changed();
                }
                ok()
            }
            "edit_comment_text" => {
                let id: String = arg(args, "commentId")?;
                self.checkpoint_for(Some(EditSession::Comment {
                    comment_id: id.clone(),
                }));
                if self.graph().set_comment_text(&id, arg(args, "text")?) {
                    self.changed();
                }
                ok()
            }
            "set_comment_collapsed" => {
                let id: String = arg(args, "commentId")?;
                if self
                    .graph()
                    .set_comment_collapsed(&id, arg(args, "collapsed")?)
                {
                    self.changed();
                }
                ok()
            }
            "remove_comment" => {
                let id: String = arg(args, "commentId")?;
                self.checkpoint();
                if self.graph().remove_comment(&id) {
                    self.changed();
                }
                ok()
            }

            // ── Variables, lists, dicts ──────────────────────────────────
            "create_variable" => {
                let name: String = arg(args, "name")?;
                let made = self.on_copy(|g| g.create_variable(&name))?;
                Ok(json!(made))
            }
            "rename_variable" => {
                let (old, new): (String, String) = (arg(args, "oldName")?, arg(args, "newName")?);
                self.on_copy(|g| g.rename_variable(&old, &new))?;
                ok()
            }
            "delete_variable" => {
                let name: String = arg(args, "name")?;
                self.checkpoint();
                self.graph().remove_variable(&name);
                self.changed();
                ok()
            }
            "create_list" => {
                let name: String = arg(args, "name")?;
                Ok(json!(self.on_copy(|g| g.create_list(&name))?))
            }
            "rename_list" => {
                let (old, new): (String, String) = (arg(args, "oldName")?, arg(args, "newName")?);
                self.on_copy(|g| g.rename_list(&old, &new))?;
                ok()
            }
            "delete_list" => {
                let name: String = arg(args, "name")?;
                self.checkpoint();
                self.graph().remove_list(&name);
                self.changed();
                ok()
            }
            "set_list_items" => {
                let name: String = arg(args, "name")?;
                let items: Vec<ListItem> = arg(args, "items")?;
                self.on_copy(|g| g.set_list_items(&name, items))?;
                ok()
            }
            "set_list_editor_state" => {
                // Where a card sits is a view preference, not an undo step.
                let name: String = arg(args, "name")?;
                self.graph().set_list_editor_state(
                    &name,
                    arg(args, "visible")?,
                    arg(args, "x")?,
                    arg(args, "y")?,
                )?;
                self.changed();
                ok()
            }
            "create_dict" => {
                let name: String = arg(args, "name")?;
                Ok(json!(self.on_copy(|g| g.create_dict(&name))?))
            }
            "rename_dict" => {
                let (old, new): (String, String) = (arg(args, "oldName")?, arg(args, "newName")?);
                self.on_copy(|g| g.rename_dict(&old, &new))?;
                ok()
            }
            "delete_dict" => {
                let name: String = arg(args, "name")?;
                self.checkpoint();
                self.graph().remove_dict(&name);
                self.changed();
                ok()
            }
            "set_dict_entries" => {
                let name: String = arg(args, "name")?;
                let entries: Vec<DictEntry> = arg(args, "entries")?;
                self.on_copy(|g| g.set_dict_entries(&name, entries))?;
                ok()
            }
            "set_dict_editor_state" => {
                let name: String = arg(args, "name")?;
                self.graph().set_dict_editor_state(
                    &name,
                    arg(args, "visible")?,
                    arg(args, "x")?,
                    arg(args, "y")?,
                )?;
                self.changed();
                ok()
            }

            // ── Custom blocks ────────────────────────────────────────────
            "create_block" => {
                let draft = BlockDraft::from_args(args)?;
                let (dx, dy) = self.project.graph.next_strand_position();
                let x = arg::<Option<i32>>(args, "x")?.unwrap_or(dx);
                let y = arg::<Option<i32>>(args, "y")?.unwrap_or(dy);
                let id = self.on_copy(|g| {
                    let id = g.create_block(draft.pieces, draft.shape, draft.color, x, y, |id| {
                        McBlock::BlockHeader {
                            block_id: id.to_string(),
                        }
                    });
                    g.set_block_details(&id, &draft.identifier, &draft.description, &draft.icon)?;
                    Ok(id)
                })?;
                Ok(json!(id))
            }
            "edit_block" => {
                let id: String = arg(args, "blockId")?;
                let draft = BlockDraft::from_args(args)?;
                self.on_copy(|g| {
                    g.update_block(&id, draft.pieces, draft.shape, &draft.color)?;
                    g.set_block_details(&id, &draft.identifier, &draft.description, &draft.icon)
                })?;
                ok()
            }
            "delete_block" => {
                let id: String = arg(args, "blockId")?;
                self.delete_block(&id);
                ok()
            }

            // ── Undo ─────────────────────────────────────────────────────
            "undo" | "redo" => {
                let current = self.project.graph.clone();
                let restored = if cmd == "undo" {
                    self.history.undo(current)
                } else {
                    self.history.redo(current)
                };
                if let Some(graph) = restored {
                    self.project.graph = graph;
                    self.buffers.clear();
                    self.changed();
                }
                ok()
            }
            other => Err(format!("The editor sent `{other}`, which this build doesn't know")),
        }
    }

    /// Declares the variable a palette block named, in the same undo step
    /// as the drop. One that already exists is left alone.
    fn declare_variable(&mut self, name: Option<String>) {
        if let Some(name) = name {
            let _ = self.graph().create_variable(&name);
        }
    }

    /// The custom block whose header sits at `path`, if one does.
    fn header_block_at(&self, strand_id: &str, path: &InstrPath) -> Option<String> {
        self.project
            .instruction_at(strand_id, path)
            .and_then(|ins| ins.kind.block_header_id().map(str::to_string))
    }

    /// Removes a whole stack; a custom block's definition stack takes the
    /// definition and its calls with it.
    fn remove_strand(&mut self, strand_id: &str) {
        let header = self
            .project
            .strand(strand_id)
            .and_then(|s| s.block_header_id().map(str::to_string));
        if let Some(block_id) = header {
            self.delete_block(&block_id);
            return;
        }
        if self.project.strand(strand_id).is_none() {
            return;
        }
        self.checkpoint();
        self.graph().remove_strand(strand_id);
        drop_strand_buffers(&mut self.buffers, strand_id);
        self.changed();
    }

    fn delete_block(&mut self, block_id: &str) {
        if self.project.block_def(block_id).is_none() {
            return;
        }
        self.checkpoint();
        self.graph().remove_block(block_id);
        let live: Vec<String> = self.project.strands.iter().map(|s| s.id.clone()).collect();
        retain_live_buffers(&mut self.buffers, &live);
        self.changed();
    }
}

/// Keeps a stack's top where a block prepended to it was dropped.
fn hold_top(
    graph: &mut BlockGraph<McBlock>,
    strand_id: &str,
    path: &InstrPath,
    target_y: Option<i32>,
) {
    if let Some(y) = target_y
        && path.len() == 1
        && path[0].index == 0
        && let Some(strand) = graph.strand_mut(strand_id)
    {
        strand.y = y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blockstitch_core::editor::PathStep;

    fn session() -> Session {
        crate::reporters::register();
        Session::new(ModProject::new("demo"))
    }

    fn run(s: &mut Session, cmd: &str, args: Json) -> Json {
        s.dispatch(cmd, &args)
            .unwrap_or_else(|e| panic!("{cmd} failed: {e}"))
    }

    fn broadcast(text: &str) -> Json {
        json!({ "type": "Broadcast", "text": { "kind": "Text", "value": text } })
    }

    fn stack(s: &mut Session, blocks: Vec<McBlock>) -> String {
        s.project
            .graph
            .add_strand(0, 0, blocks.into_iter().map(Instruction::new).collect())
    }

    fn say(text: &str) -> McBlock {
        McBlock::Broadcast {
            text: Value::text(text),
        }
    }

    #[test]
    fn every_dispatched_command_is_listed_and_known() {
        let mut s = session();
        for cmd in COMMANDS {
            // Missing arguments are fine; "unknown command" is not.
            if let Err(e) = s.dispatch(cmd, &json!({})) {
                assert!(!e.contains("doesn't know"), "{cmd}: {e}");
            }
        }
        assert!(s.dispatch("run_mod", &json!({})).is_err());
    }

    #[test]
    fn a_palette_drop_lands_flat_and_undo_takes_it_back() {
        let mut s = session();
        let id = run(
            &mut s,
            "add_strand",
            json!({ "x": 10, "y": 20, "instruction": { "id": "palette-1", "type": "OnServerTick" } }),
        );
        let id = id.as_str().unwrap().to_string();
        run(
            &mut s,
            "insert_instruction",
            json!({ "strandId": id, "path": [{ "index": 1 }], "instruction": broadcast("hi") }),
        );
        assert_eq!(s.project.strand(&id).unwrap().instructions.len(), 2);
        run(&mut s, "undo", json!({}));
        assert_eq!(s.project.strand(&id).unwrap().instructions.len(), 1);
        run(&mut s, "redo", json!({}));
        assert_eq!(s.project.strand(&id).unwrap().instructions.len(), 2);
    }

    #[test]
    fn a_header_dropped_mid_stack_is_refused_without_an_undo_step() {
        let mut s = session();
        let id = stack(&mut s, vec![McBlock::OnServerTick, say("a")]);
        let refused = s.dispatch(
            "insert_instruction",
            &json!({ "strandId": id, "path": [{ "index": 1 }], "instruction": { "type": "OnPlayerJoin" } }),
        );
        assert!(refused.is_err());
        assert!(!s.can_undo());
    }

    #[test]
    fn trashing_a_tail_takes_the_blocks_below_it() {
        let mut s = session();
        let id = stack(&mut s, vec![McBlock::OnServerTick, say("keep"), say("a"), say("b")]);
        run(
            &mut s,
            "remove_instructions",
            json!({ "strandId": id, "path": [{ "index": 2 }], "count": 2 }),
        );
        assert_eq!(s.project.strand(&id).unwrap().instructions.len(), 2);
    }

    #[test]
    fn trashing_a_whole_stack_leaves_no_empty_husk() {
        let mut s = session();
        let id = stack(&mut s, vec![McBlock::OnServerTick, say("a")]);
        run(
            &mut s,
            "remove_instructions",
            json!({ "strandId": id, "path": [{ "index": 0 }], "count": 2 }),
        );
        assert!(s.project.strands.is_empty());
    }

    #[test]
    fn a_nested_tail_leaves_the_outer_stack_alone() {
        let mut s = session();
        let id = stack(
            &mut s,
            vec![
                McBlock::OnServerTick,
                McBlock::Repeat {
                    count: Value::number(2.0),
                    body: vec![Instruction::new(say("keep")), Instruction::new(say("gone"))],
                },
                say("after"),
            ],
        );
        run(
            &mut s,
            "remove_instructions",
            json!({ "strandId": id, "path": [{ "index": 1, "slot": 0 }, { "index": 1 }], "count": 1 }),
        );
        let strand = s.project.strand(&id).unwrap();
        assert_eq!(strand.instructions.len(), 3);
        assert_eq!(strand.instructions[1].kind.body(0).unwrap().len(), 1);
        let _ = PathStep::at(0);
    }

    #[test]
    fn custom_blocks_are_made_edited_and_deleted_with_their_calls() {
        let mut s = session();
        let draft = json!({
            "pieces": [
                { "kind": "Label", "id": "l", "text": "shout" },
                { "kind": "Input", "id": "i", "name": "times", "value_type": "Any" }
            ],
            "shape": "Normal", "color": "#4C97FF", "icon": "megaphone",
            "identifier": "Shout", "description": "Says it loudly."
        });
        let block_id = run(&mut s, "create_block", draft.clone());
        let block_id = block_id.as_str().unwrap().to_string();
        let def = s.project.block_def(&block_id).unwrap();
        assert_eq!(def.icon, "megaphone");
        assert_eq!(def.identifier, "Shout");

        let caller = stack(
            &mut s,
            vec![
                McBlock::OnServerTick,
                McBlock::CallBlock {
                    block_id: block_id.clone(),
                    args: vec![Value::number(1.0)],
                },
            ],
        );
        let mut edited = draft.clone();
        edited["blockId"] = json!(block_id);
        edited["pieces"][0]["text"] = json!("yell");
        run(&mut s, "edit_block", edited);
        assert!(matches!(
            &s.project.block_def(&block_id).unwrap().pieces[0],
            BlockPiece::Label { text, .. } if text == "yell"
        ));

        let header = s
            .project
            .strands
            .iter()
            .find(|st| st.block_header_id() == Some(block_id.as_str()))
            .unwrap()
            .id
            .clone();
        run(&mut s, "remove_strand", json!({ "strandId": header }));
        assert!(s.project.block_def(&block_id).is_none());
        assert_eq!(s.project.strand(&caller).unwrap().instructions.len(), 1);
    }

    #[test]
    fn a_branch_in_a_draft_is_refused() {
        let mut s = session();
        let draft = json!({
            "pieces": [{ "kind": "Label", "id": "l", "text": "x" }, { "kind": "Branch", "id": "b", "name": "then" }],
            "shape": "Normal", "color": "#4C97FF"
        });
        assert!(s.dispatch("create_block", &draft).is_err());
        assert!(!s.can_undo());
    }

    #[test]
    fn copy_and_paste_round_trip_with_fresh_ids() {
        let mut s = session();
        let id = stack(&mut s, vec![McBlock::OnServerTick, say("a")]);
        let fragment = run(
            &mut s,
            "copy_instructions",
            json!({ "strandId": id, "path": [{ "index": 1 }], "count": null }),
        );
        assert_eq!(fragment["instructions"][0]["type"], "Broadcast");
        let pasted = run(&mut s, "paste_fragment", json!({ "x": 0, "y": 200, "fragment": fragment }));
        let pasted = s.project.strand(pasted.as_str().unwrap()).unwrap();
        assert_ne!(
            pasted.instructions[0].id,
            s.project.strand(&id).unwrap().instructions[1].id
        );
    }

    #[test]
    fn moving_a_value_is_one_undo_step() {
        let mut s = session();
        let id = stack(&mut s, vec![McBlock::OnServerTick, say("hi")]);
        let from = json!({ "kind": "Field", "strand_id": id, "index": [{ "index": 1 }], "field_id": "text", "path": [] });
        run(&mut s, "move_value", json!({ "from": from, "x": 300, "y": 40 }));
        assert_eq!(s.project.floating_values.len(), 1);
        run(&mut s, "undo", json!({}));
        assert!(s.project.floating_values.is_empty());
        assert!(!s.can_undo());
    }

    #[test]
    fn revision_moves_only_on_change() {
        let mut s = session();
        let before = s.revision();
        let _ = s.dispatch("move_strand", &json!({ "strandId": "nope", "x": 1, "y": 1 }));
        assert_eq!(s.revision(), before);
        run(&mut s, "create_variable", json!({ "name": "score" }));
        assert!(s.revision() > before);
    }
}
