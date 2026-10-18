pub(crate) fn wrap_opportunities(text: &str) -> Vec<usize> {
    unicode_linebreak::linebreaks(text)
        .map(|(ix, _)| ix)
        .filter(|&ix| ix > 0 && ix < text.len() && !binds(&text[..ix], &text[ix..]))
        .collect()
}

fn binds(before: &str, after: &str) -> bool {
    if !before.ends_with(char::is_whitespace) {
        return false;
    }
    if after.starts_with(['—', '–', '-']) {
        return true;
    }
    let word = before.trim_end().rsplit(char::is_whitespace).next().unwrap_or_default();
    let word = word.trim_start_matches(|c: char| !c.is_alphanumeric());
    if word.is_empty() {
        return false;
    }
    let letters = word.chars().count();
    let short = word.chars().all(char::is_alphabetic)
        && (letters == 1 || (letters == 2 && word.chars().all(is_cyrillic)));
    short || word.chars().all(|c| c.is_ascii_digit())
}

fn is_cyrillic(c: char) -> bool {
    matches!(c, '\u{0400}'..='\u{04FF}')
}
