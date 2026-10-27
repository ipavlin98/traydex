use super::ItemId;
use std::borrow::Cow;
use std::ops::Range;

const OPEN: &str = "@[";
const SCHEME: &str = "](traydex:";
const CLOSE: char = ')';
const NAME_LIMIT: usize = 200;

pub struct Mention<'a> {
    pub range: Range<usize>,
    pub name: &'a str,
    pub id: ItemId,
}

pub fn token(id: &ItemId, name: &str) -> String {
    let name: String = name
        .chars()
        .map(|c| match c {
            '[' => '(',
            ']' => ')',
            c if c.is_control() => ' ',
            c => c,
        })
        .take(NAME_LIMIT)
        .collect();
    format!("{OPEN}{}{SCHEME}{id}{CLOSE}", name.trim())
}

pub fn mentions(text: &str) -> Vec<Mention<'_>> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(start) = text.get(from..).and_then(|rest| rest.find(OPEN)).map(|at| from + at) {
        from = start + OPEN.len();
        let Some(mention) = parse(text, start) else {
            continue;
        };
        from = mention.range.end;
        found.push(mention);
    }
    found
}

fn parse(text: &str, start: usize) -> Option<Mention<'_>> {
    let name_start = start + OPEN.len();
    let rest = text.get(name_start..)?;
    let name_len = rest.find(SCHEME)?;
    let name = rest.get(..name_len)?;
    if name.contains(['[', ']', '\n']) {
        return None;
    }
    let id_start = name_start + name_len + SCHEME.len();
    let id_len = text.get(id_start..)?.find(CLOSE)?;
    let id = text.get(id_start..id_start + id_len)?;
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return None;
    }
    Some(Mention { range: start..id_start + id_len + 1, name, id: ItemId(id.to_owned()) })
}

pub fn plain(text: &str) -> Cow<'_, str> {
    let found = mentions(text);
    if found.is_empty() {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for mention in found {
        out.push_str(text.get(last..mention.range.start).unwrap_or_default());
        out.push('@');
        out.push_str(mention.name);
        last = mention.range.end;
    }
    out.push_str(text.get(last..).unwrap_or_default());
    Cow::Owned(out)
}

#[cfg(test)]
mod tests;
