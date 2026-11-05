use super::EditBuffer;
use std::ops::Range;

const DEPTH: usize = 200;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Edit {
    Typing,
    Erasing,
    Other,
}

struct Snapshot {
    text: String,
    selected: Range<usize>,
}

#[derive(Default)]
pub(crate) struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last: Option<Edit>,
}

impl History {
    pub(crate) fn remember(&mut self, edit: Edit, buffer: &EditBuffer) {
        if self.last == Some(edit) && edit != Edit::Other {
            return;
        }
        self.undo.push(Snapshot { text: buffer.text().to_string(), selected: buffer.selected() });
        if self.undo.len() > DEPTH {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.last = Some(edit);
    }

    pub(crate) fn interrupt(&mut self) {
        self.last = None;
    }

    pub(crate) fn restore(&mut self, redo: bool, buffer: &mut EditBuffer) -> bool {
        let (from, to) = if redo { (&mut self.redo, &mut self.undo) } else { (&mut self.undo, &mut self.redo) };
        let Some(snapshot) = from.pop() else {
            return false;
        };
        let selected = buffer.selected();
        to.push(Snapshot { text: buffer.reset(snapshot.text, snapshot.selected), selected });
        self.last = None;
        true
    }
}
