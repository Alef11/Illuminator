use crate::doc::Document;

/// A reversible mutation to the document. Tools never mutate the document
/// directly; they push commands onto the [`CommandStack`].
pub trait Command: Send {
    fn apply(&mut self, doc: &mut Document);
    fn undo(&mut self, doc: &mut Document);
    fn label(&self) -> &str;
}

#[derive(Default)]
pub struct CommandStack {
    undo: Vec<Box<dyn Command>>,
    redo: Vec<Box<dyn Command>>,
}

impl CommandStack {
    /// Apply a fresh command and push it to the undo stack.
    pub fn push(&mut self, mut cmd: Box<dyn Command>, doc: &mut Document) {
        cmd.apply(doc);
        self.undo.push(cmd);
        self.redo.clear();
    }

    /// Record a command whose effect is already present in `doc`.
    ///
    /// Tools that perform live previews (e.g. a move-drag) mutate the document
    /// directly each frame and only want a single undo step on release. They
    /// build the command summarising the cumulative change and hand it here.
    pub fn record_applied(&mut self, cmd: Box<dyn Command>) {
        self.undo.push(cmd);
        self.redo.clear();
    }

    pub fn undo(&mut self, doc: &mut Document) -> bool {
        if let Some(mut cmd) = self.undo.pop() {
            cmd.undo(doc);
            self.redo.push(cmd);
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self, doc: &mut Document) -> bool {
        if let Some(mut cmd) = self.redo.pop() {
            cmd.apply(doc);
            self.undo.push(cmd);
            true
        } else {
            false
        }
    }

    pub fn can_undo(&self) -> bool { !self.undo.is_empty() }
    pub fn can_redo(&self) -> bool { !self.redo.is_empty() }

    pub fn undo_label(&self) -> Option<&str> { self.undo.last().map(|c| c.label()) }
    pub fn redo_label(&self) -> Option<&str> { self.redo.last().map(|c| c.label()) }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}
