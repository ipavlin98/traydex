mod index;

pub use index::{Index, Indexed};

use std::ops::Range;

const LATIN: &str = "qwertyuiop[]asdfghjkl;'zxcvbnm,.";
const CYRILLIC: &str = "йцукенгшщзхъфывапролджэячсмитьбю";

pub type Located = (Vec<Range<usize>>, Vec<Range<usize>>);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Grade {
    Word,
    Fragment,
    Typo,
}

impl Grade {
    pub const ALL: [Grade; 3] = [Grade::Word, Grade::Fragment, Grade::Typo];
}

pub struct Query {
    terms: Vec<Vec<Variant>>,
}

struct Variant {
    chars: Vec<char>,
    text: String,
}

fn fold(c: char) -> char {
    match c.to_lowercase().next().unwrap_or(c) {
        'ё' => 'е',
        c => c,
    }
}

pub fn folded(text: &str) -> String {
    text.chars().map(fold).collect()
}

fn ideograph(c: char) -> bool {
    matches!(c, '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '\u{f900}'..='\u{faff}')
}

fn starts_word(previous: Option<char>, current: char) -> bool {
    let Some(previous) = previous else {
        return true;
    };
    !previous.is_alphanumeric()
        || ideograph(previous)
        || ideograph(current)
        || previous.is_numeric() != current.is_numeric()
        || (previous.is_lowercase() && current.is_uppercase())
}

fn word_starts(text: &str) -> Vec<bool> {
    let mut previous = None;
    text.chars()
        .map(|c| {
            let start = starts_word(previous, c);
            previous = Some(c);
            start
        })
        .collect()
}

fn anchored(needle: &[char]) -> bool {
    needle.first().is_some_and(|c| c.is_alphanumeric())
}

fn opens_word(needle: &str, content: &str) -> bool {
    let Some(first) = needle.chars().next() else {
        return false;
    };
    content
        .match_indices(needle)
        .any(|(at, _)| !first.is_alphanumeric() || starts_word(content[..at].chars().next_back(), first))
}

fn swap(word: &[char], from: &str, to: &str) -> Vec<char> {
    word.iter()
        .map(|c| from.chars().position(|f| f == *c).and_then(|index| to.chars().nth(index)).unwrap_or(*c))
        .collect()
}

fn exact(needle: &[char], hay: &[char], starts: Option<&[bool]>) -> Vec<Range<usize>> {
    let starts = starts.filter(|_| anchored(needle));
    let mut found = Vec::new();
    let mut start = 0;
    while start + needle.len() <= hay.len() {
        let allowed = starts.is_none_or(|starts| starts.get(start).copied().unwrap_or_default());
        if allowed && hay[start..start + needle.len()] == *needle {
            found.push(start..start + needle.len());
            start += needle.len();
        } else {
            start += 1;
        }
    }
    found
}

fn fuzzy(needle: &[char], hay: &[char]) -> Vec<Range<usize>> {
    let budget = match needle.len() {
        0..=3 => return Vec::new(),
        4..=7 => 1,
        _ => 2,
    };
    let (rows, width) = (needle.len() + 1, hay.len() + 1);
    let mut cost = vec![0usize; rows * width];
    let mut origin: Vec<usize> = (0..width).cycle().take(rows * width).collect();
    for i in 1..rows {
        cost[i * width] = i;
        origin[i * width] = 0;
        for j in 1..width {
            let diagonal = (i - 1) * width + j - 1;
            let mut best = (cost[diagonal] + usize::from(needle[i - 1] != hay[j - 1]), origin[diagonal]);
            let mut consider = |cell: usize| {
                if cost[cell] + 1 < best.0 {
                    best = (cost[cell] + 1, origin[cell]);
                }
            };
            consider((i - 1) * width + j);
            consider(i * width + j - 1);
            if i > 1 && j > 1 && needle[i - 1] == hay[j - 2] && needle[i - 2] == hay[j - 1] {
                consider((i - 2) * width + j - 2);
            }
            cost[i * width + j] = best.0;
            origin[i * width + j] = best.1;
        }
    }
    let last = (rows - 1) * width;
    let mut candidates: Vec<(usize, Range<usize>)> = (1..width)
        .filter(|&j| cost[last + j] <= budget && origin[last + j] < j)
        .map(|j| (cost[last + j], origin[last + j]..j))
        .collect();
    candidates.sort_by_key(|(errors, range)| (*errors, range.len().abs_diff(needle.len())));
    let mut chosen: Vec<Range<usize>> = Vec::new();
    for (_, range) in candidates {
        if chosen.iter().all(|c| range.end <= c.start || c.end <= range.start) {
            chosen.push(range);
        }
    }
    chosen
}

impl Query {
    pub fn parse(text: &str) -> Option<Self> {
        let terms: Vec<_> = text
            .split_whitespace()
            .map(|word| {
                let word: Vec<char> = word.chars().map(fold).collect();
                let mut variants = vec![word.clone()];
                for variant in [swap(&word, LATIN, CYRILLIC), swap(&word, CYRILLIC, LATIN)] {
                    if !variants.contains(&variant) {
                        variants.push(variant);
                    }
                }
                variants.into_iter().map(|chars| Variant { text: chars.iter().collect(), chars }).collect()
            })
            .collect();
        (!terms.is_empty()).then_some(Self { terms })
    }

    pub fn find(&self, title: &str, content: Option<&str>, loosest: Grade) -> Option<Vec<Range<usize>>> {
        let (offsets, hay): (Vec<usize>, Vec<char>) = title.char_indices().map(|(i, c)| (i, fold(c))).unzip();
        let starts = word_starts(title);
        let mut ranges = Vec::new();
        for variants in &self.terms {
            let hits = Grade::ALL
                .into_iter()
                .take_while(|grade| *grade <= loosest)
                .find_map(|grade| term(variants, &hay, &starts, content, grade))?;
            ranges.extend(hits);
        }
        ranges.sort_by_key(|r| r.start);
        let mut merged: Vec<Range<usize>> = Vec::new();
        for range in ranges {
            match merged.last_mut() {
                Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
                _ => merged.push(range),
            }
        }
        let byte = |index: usize| offsets.get(index).copied().unwrap_or(title.len());
        Some(merged.into_iter().map(|r| byte(r.start)..byte(r.end)).collect())
    }

    pub fn locate(&self, title: &str, detail: &str, loosest: Grade) -> Option<Located> {
        let title_hits = self.find(title, Some(&folded(detail)), loosest)?;
        let detail_hits = self.find(detail, Some(&folded(title)), loosest)?;
        Some((title_hits, detail_hits))
    }

    pub fn grade(&self, title: &str, detail: &str, loosest: Grade) -> Option<Grade> {
        Grade::ALL
            .into_iter()
            .take_while(|grade| *grade <= loosest)
            .find(|grade| self.locate(title, detail, *grade).is_some())
    }
}

fn term(
    variants: &[Variant],
    hay: &[char],
    starts: &[bool],
    content: Option<&str>,
    grade: Grade,
) -> Option<Vec<Range<usize>>> {
    let typed = variants.first()?;
    let (hits, in_content): (Vec<Range<usize>>, bool) = match grade {
        Grade::Word => (
            variants.iter().flat_map(|v| exact(&v.chars, hay, Some(starts))).collect(),
            content.is_some_and(|content| variants.iter().any(|v| opens_word(&v.text, content))),
        ),
        Grade::Fragment => {
            (exact(&typed.chars, hay, None), content.is_some_and(|content| content.contains(typed.text.as_str())))
        }
        Grade::Typo => (variants.iter().flat_map(|v| fuzzy(&v.chars, hay)).collect(), false),
    };
    if !hits.is_empty() {
        Some(hits)
    } else if in_content {
        Some(Vec::new())
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
