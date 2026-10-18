use super::words::head;
use gpui::UTF16Selection;
use std::ops::Range;

pub(crate) fn utf8_offset(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (index, c) in text.char_indices() {
        if units >= utf16 {
            return index;
        }
        units += c.len_utf16();
    }
    text.len()
}

pub(crate) struct EditBuffer {
    text: String,
    selected: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
}

impl EditBuffer {
    pub(crate) fn new(text: String, cursor: usize) -> Self {
        Self { text, selected: cursor..cursor, reversed: false, marked: None }
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn selected(&self) -> Range<usize> {
        self.selected.clone()
    }

    pub(crate) fn marked(&self) -> Option<Range<usize>> {
        self.marked.clone()
    }

    pub(crate) fn cursor(&self) -> usize {
        if self.reversed { self.selected.start } else { self.selected.end }
    }

    pub(crate) fn move_to(&mut self, offset: usize) {
        self.select(offset..offset);
    }

    pub(crate) fn select(&mut self, range: Range<usize>) {
        self.selected = range;
        self.reversed = false;
    }

    pub(crate) fn select_to(&mut self, offset: usize) {
        if self.reversed {
            self.selected.start = offset;
        } else {
            self.selected.end = offset;
        }
        if self.selected.end < self.selected.start {
            self.reversed = !self.reversed;
            self.selected = self.selected.end..self.selected.start;
        }
    }

    pub(crate) fn reset(&mut self, text: String, selected: Range<usize>) -> String {
        self.select(selected);
        self.marked = None;
        std::mem::replace(&mut self.text, text)
    }

    pub(crate) fn replace(&mut self, range: Range<usize>, text: &str) {
        self.text.replace_range(range.clone(), text);
        self.move_to(range.start + text.len());
        self.marked = None;
    }

    pub(crate) fn mark(&mut self, range: Range<usize>, text: &str, selected: Option<Range<usize>>) {
        self.text.replace_range(range.clone(), text);
        self.marked = (!text.is_empty()).then(|| range.start..range.start + text.len());
        let cursor = range.start + text.len();
        self.select(selected.map_or(cursor..cursor, |r| {
            let (start, end) = (utf8_offset(text, r.start), utf8_offset(text, r.end));
            range.start + start.min(end)..range.start + start.max(end)
        }));
    }

    pub(crate) fn unmark(&mut self) {
        self.marked = None;
    }

    pub(crate) fn target(&self, range: Option<&Range<usize>>) -> Range<usize> {
        range.map(|r| self.range_from_utf16(r)).or_else(|| self.marked.clone()).unwrap_or_else(|| self.selected.clone())
    }

    pub(crate) fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        let (start, end) = (utf8_offset(&self.text, range.start), utf8_offset(&self.text, range.end));
        start.min(end)..start.max(end)
    }

    pub(crate) fn offset_to_utf16(&self, offset: usize) -> usize {
        head(&self.text, offset).chars().map(char::len_utf16).sum()
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    pub(crate) fn text_for_range(&self, range: &Range<usize>, adjusted: &mut Option<Range<usize>>) -> Option<String> {
        let range = self.range_from_utf16(range);
        adjusted.replace(self.range_to_utf16(&range));
        self.text.get(range).map(str::to_string)
    }

    pub(crate) fn selection_utf16(&self) -> UTF16Selection {
        UTF16Selection { range: self.range_to_utf16(&self.selected), reversed: self.reversed }
    }

    pub(crate) fn marked_utf16(&self) -> Option<Range<usize>> {
        self.marked.as_ref().map(|range| self.range_to_utf16(range))
    }
}
