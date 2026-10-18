use std::ops::Range;

pub(crate) fn head(text: &str, offset: usize) -> &str {
    text.get(..offset).unwrap_or_default()
}

pub(crate) fn tail(text: &str, offset: usize) -> &str {
    text.get(offset..).unwrap_or_default()
}

pub(super) fn previous(text: &str, offset: usize) -> usize {
    head(text, offset).char_indices().next_back().map_or(0, |(index, _)| index)
}

fn next(text: &str, offset: usize) -> usize {
    tail(text, offset).chars().next().map_or(offset, |c| offset + c.len_utf8())
}

pub(crate) fn step(text: &str, offset: usize, forward: bool, words: bool) -> usize {
    match (forward, words) {
        (true, true) => word_end(text, offset),
        (true, false) => next(text, offset),
        (false, true) => word_start(text, offset),
        (false, false) => previous(text, offset),
    }
}

pub(super) fn word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

pub(super) fn word_start(text: &str, offset: usize) -> usize {
    let before = head(text, offset);
    let trimmed = before.trim_end_matches(|c: char| c.is_whitespace() && c != '\n');
    let mut chars = trimmed.char_indices().rev().peekable();
    let Some(&(index, first)) = chars.peek() else {
        return 0;
    };
    if before.len() != trimmed.len() && first == '\n' {
        return trimmed.len();
    }
    if !word(first) {
        return index;
    }
    chars.take_while(|(_, c)| word(*c)).last().map_or(index, |(index, _)| index)
}

pub(super) fn word_end(text: &str, offset: usize) -> usize {
    let after = tail(text, offset);
    let trimmed = after.trim_start_matches(|c: char| c.is_whitespace() && c != '\n');
    let skipped = after.len() - trimmed.len();
    let mut chars = trimmed.char_indices();
    let Some((_, first)) = chars.next() else {
        return text.len();
    };
    if skipped > 0 && first == '\n' {
        return offset + skipped;
    }
    if !word(first) {
        return offset + skipped + first.len_utf8();
    }
    let end = trimmed.char_indices().find(|(_, c)| !word(*c)).map_or(trimmed.len(), |(index, _)| index);
    offset + skipped + end
}

pub(crate) fn word_around(text: &str, index: usize) -> Range<usize> {
    let at = tail(text, index).chars().next();
    let before = head(text, index).chars().next_back();
    if !at.is_some_and(word) && !(before.is_some_and(word) && at.is_none_or(char::is_whitespace)) {
        return index..next(text, index);
    }
    let start = head(text, index).char_indices().rev().take_while(|(_, c)| word(*c)).last().map_or(index, |(at, _)| at);
    let end = tail(text, index).char_indices().find(|(_, c)| !word(*c)).map_or(text.len(), |(at, _)| index + at);
    start..end
}

pub(super) fn clean(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect()
}
