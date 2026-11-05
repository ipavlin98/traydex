use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

pub(crate) fn head(text: &str, offset: usize) -> &str {
    text.get(..offset).unwrap_or_default()
}

pub(crate) fn tail(text: &str, offset: usize) -> &str {
    text.get(offset..).unwrap_or_default()
}

pub(super) fn previous(text: &str, offset: usize) -> usize {
    head(text, offset).grapheme_indices(true).next_back().map_or(0, |(index, _)| index)
}

fn next(text: &str, offset: usize) -> usize {
    tail(text, offset).graphemes(true).next().map_or(offset, |grapheme| offset + grapheme.len())
}

pub(crate) fn step(text: &str, offset: usize, forward: bool, words: bool) -> usize {
    match (forward, words) {
        (true, true) => word_end(text, offset),
        (true, false) => next(text, offset),
        (false, true) => word_start(text, offset),
        (false, false) => previous(text, offset),
    }
}

fn word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn wordy(grapheme: &str) -> bool {
    grapheme.chars().next().is_some_and(word)
}

pub(super) fn word_start(text: &str, offset: usize) -> usize {
    let before = head(text, offset);
    let trimmed = before.trim_end_matches(|c: char| c.is_whitespace() && c != '\n');
    let mut graphemes = trimmed.grapheme_indices(true).rev().peekable();
    let Some(&(index, first)) = graphemes.peek() else {
        return 0;
    };
    if before.len() != trimmed.len() && first == "\n" {
        return trimmed.len();
    }
    if !wordy(first) {
        return index;
    }
    graphemes.take_while(|(_, grapheme)| wordy(grapheme)).last().map_or(index, |(index, _)| index)
}

pub(super) fn word_end(text: &str, offset: usize) -> usize {
    let after = tail(text, offset);
    let trimmed = after.trim_start_matches(|c: char| c.is_whitespace() && c != '\n');
    let skipped = after.len() - trimmed.len();
    let Some(first) = trimmed.graphemes(true).next() else {
        return text.len();
    };
    if skipped > 0 && first == "\n" {
        return offset + skipped;
    }
    if !wordy(first) {
        return offset + skipped + first.len();
    }
    let end = trimmed.grapheme_indices(true).find(|(_, grapheme)| !wordy(grapheme)).map_or(trimmed.len(), |(at, _)| at);
    offset + skipped + end
}

pub(crate) fn word_around(text: &str, index: usize) -> Range<usize> {
    let at = tail(text, index).graphemes(true).next();
    let before = head(text, index).graphemes(true).next_back();
    let blank = |grapheme: &str| grapheme.chars().all(char::is_whitespace);
    if !at.is_some_and(wordy) && !(before.is_some_and(wordy) && at.is_none_or(blank)) {
        return index..next(text, index);
    }
    let start = head(text, index)
        .grapheme_indices(true)
        .rev()
        .take_while(|(_, grapheme)| wordy(grapheme))
        .last()
        .map_or(index, |(at, _)| at);
    let end = tail(text, index)
        .grapheme_indices(true)
        .find(|(_, grapheme)| !wordy(grapheme))
        .map_or(text.len(), |(at, _)| index + at);
    start..end
}

pub(super) fn clean(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect()
}
