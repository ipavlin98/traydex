use crate::store::{ItemId, mention};
use gpui::{Bounds, Pixels, Point, SharedString};
use std::collections::HashMap;
use std::ops::Range;

const FIRST: u32 = 0xE000;
const LAST: u32 = 0xF8FF;
const LABEL_LIMIT: usize = 32;
const QUERY_LIMIT: usize = 40;
const LEAD: &str = "\u{2003}\u{2060}\u{2002}\u{2060}";
const TAIL: &str = "\u{2060}\u{2004}";
const OPENERS: [char; 6] = ['(', '[', '{', '"', '\'', '«'];
pub(super) const TRIGGER: char = '\\';
const TRIGGERS: [char; 2] = [TRIGGER, '@'];

#[derive(Clone)]
pub struct Chip {
    pub label: SharedString,
    pub icon: &'static str,
    pub missing: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Peek {
    pub id: ItemId,
    pub bounds: Bounds<Pixels>,
}

pub enum AreaEvent {
    Suggest(Option<String>),
    Anchored,
    Step(isize),
    Choose,
    Hover(Option<Peek>),
    Open { id: ItemId, reveal: bool },
    Press { id: ItemId, position: Point<Pixels> },
    Paste,
}

pub(super) fn is_chip(c: char) -> bool {
    (FIRST..=LAST).contains(&(c as u32))
}

pub(super) fn strip(text: &str) -> String {
    text.chars().filter(|c| !is_chip(*c)).collect()
}

struct Ref {
    id: ItemId,
    name: String,
}

#[derive(Default)]
pub(super) struct Refs(Vec<Ref>);

impl Refs {
    pub(super) fn add(&mut self, id: &ItemId, name: &str) -> Option<char> {
        let index = match self.0.iter().position(|r| r.id == *id) {
            Some(index) => index,
            None => {
                self.0.push(Ref { id: id.clone(), name: name.to_owned() });
                self.0.len() - 1
            }
        };
        char::from_u32(FIRST + u32::try_from(index).ok()?).filter(|c| is_chip(*c))
    }

    fn get(&self, c: char) -> Option<&Ref> {
        self.0.get((c as u32).checked_sub(FIRST)? as usize)
    }

    pub(super) fn id(&self, c: char) -> Option<&ItemId> {
        self.get(c).map(|r| &r.id)
    }

    pub(super) fn ids(&self) -> impl Iterator<Item = &ItemId> {
        self.0.iter().map(|r| &r.id)
    }

    pub(super) fn encode(&mut self, stored: &str) -> String {
        let stored = strip(stored);
        let mut out = String::with_capacity(stored.len());
        let mut last = 0;
        for found in mention::mentions(&stored) {
            out.push_str(stored.get(last..found.range.start).unwrap_or_default());
            match self.add(&found.id, found.name) {
                Some(c) => out.push(c),
                None => out.push_str(stored.get(found.range.clone()).unwrap_or_default()),
            }
            last = found.range.end;
        }
        out.push_str(stored.get(last..).unwrap_or_default());
        out
    }

    pub(super) fn decode(&self, edit: &str) -> String {
        self.expand(edit, |r| mention::token(&r.id, &r.name))
    }

    pub(super) fn plain(&self, edit: &str) -> String {
        self.expand(edit, |r| format!("@{}", r.name))
    }

    fn expand(&self, edit: &str, write: impl Fn(&Ref) -> String) -> String {
        let mut out = String::with_capacity(edit.len());
        for c in edit.chars() {
            match self.get(c) {
                Some(r) => out.push_str(&write(r)),
                None if is_chip(c) => {}
                None => out.push(c),
            }
        }
        out
    }
}

pub(super) struct Span {
    pub(super) at: usize,
    pub(super) display: Range<usize>,
    pub(super) label: Range<usize>,
    pub(super) id: ItemId,
}

pub(super) struct Display {
    pub(super) text: SharedString,
    pub(super) spans: Vec<Span>,
}

impl Display {
    pub(super) fn build(edit: &str, refs: &Refs, looks: &HashMap<ItemId, Chip>) -> Self {
        let mut text = String::with_capacity(edit.len());
        let mut spans = Vec::new();
        for (at, c) in edit.char_indices() {
            let Some(found) = refs.get(c) else {
                text.push(if c == '\t' { ' ' } else { c });
                continue;
            };
            let label = looks.get(&found.id).map_or(found.name.as_str(), |chip| chip.label.as_ref());
            let start = text.len();
            text.push_str(LEAD);
            let label_start = text.len();
            text.push_str(&chip_label(label));
            let label = label_start..text.len();
            text.push_str(TAIL);
            spans.push(Span { at, display: start..text.len(), label, id: found.id.clone() });
        }
        Self { text: text.into(), spans }
    }

    pub(super) fn to_display(&self, edit: usize) -> usize {
        let mut shift = 0isize;
        for span in &self.spans {
            if span.at >= edit {
                break;
            }
            let end = span.at + CHIP_LEN;
            if edit < end {
                return span.display.end;
            }
            shift += span.display.len() as isize - CHIP_LEN as isize;
        }
        edit.saturating_add_signed(shift)
    }

    pub(super) fn to_edit(&self, display: usize) -> usize {
        let mut shift = 0isize;
        for span in &self.spans {
            if display <= span.display.start {
                break;
            }
            if display < span.display.end {
                let near_start = display - span.display.start < span.display.len() / 2;
                return if near_start { span.at } else { span.at + CHIP_LEN };
            }
            shift += span.display.len() as isize - CHIP_LEN as isize;
        }
        display.saturating_add_signed(-shift)
    }
}

const CHIP_LEN: usize = 3;

fn chip_label(label: &str) -> String {
    let squashed: String = label.split_whitespace().collect::<Vec<_>>().join("\u{00A0}");
    if squashed.chars().count() <= LABEL_LIMIT {
        return squashed;
    }
    let cut: String = squashed.chars().take(LABEL_LIMIT - 1).collect();
    format!("{}…", gpui::cut_before_ellipsis(&cut))
}

pub(super) fn query(edit: &str, cursor: usize) -> Option<(usize, &str)> {
    let before = edit.get(..cursor)?;
    let mut taken = 0;
    for (at, c) in before.char_indices().rev() {
        if TRIGGERS.contains(&c) {
            let opens = before[..at].chars().next_back().is_none_or(|p| p.is_whitespace() || OPENERS.contains(&p));
            return opens.then(|| (at, &before[at + 1..]));
        }
        taken += 1;
        if c.is_whitespace() || is_chip(c) || taken > QUERY_LIMIT {
            return None;
        }
    }
    None
}

#[cfg(test)]
mod tests;
