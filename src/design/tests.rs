use super::*;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

const STYLED: [&str; 19] = [
    ".text_size(",
    ".line_height(",
    ".rounded(",
    ".rounded_t(",
    ".rounded_b(",
    ".gap(",
    ".p(",
    ".px(",
    ".py(",
    ".pt(",
    ".pb(",
    ".pl(",
    ".pr(",
    ".m(",
    ".mt(",
    ".mb(",
    ".ml(",
    ".mr(",
    ".snap_to_window_with_margin(",
];
const SIZED: [&str; 4] = ["icon(", "leading_icon(", "spinner(", "grid("];
const TOKENS: [&str; 7] = ["text::", "line::", "space::", "radius::", "glyph::", "size::", "opacity::"];

fn sources() -> Vec<(PathBuf, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files =
        vec![root.join("input.rs"), root.join("textarea.rs"), root.join("view.rs"), root.join("geometry.rs")];
    let mut folders = vec![root.join("view")];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).expect("view folder").flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs")
                && path.file_name().is_some_and(|name| name != "tests.rs")
            {
                files.push(path);
            }
        }
    }
    files.into_iter().map(|path| (path.clone(), fs::read_to_string(&path).expect("source"))).collect()
}

fn closing(source: &str, open: usize) -> Option<usize> {
    let mut depth = 0;
    for (offset, c) in source.get(open..)?.char_indices() {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => return Some(open + offset),
            ')' => depth -= 1,
            _ => {}
        }
    }
    None
}

fn last_argument(arguments: &str) -> &str {
    let mut depth = 0;
    let mut start = 0;
    for (offset, c) in arguments.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => start = offset + 1,
            _ => {}
        }
    }
    arguments.get(start..).unwrap_or_default().trim()
}

fn words(code: &str) -> Vec<(usize, &str)> {
    let mut found = Vec::new();
    let mut start = None;
    for (offset, c) in code.char_indices().chain([(code.len(), ' ')]) {
        let part = c.is_ascii_alphanumeric() || c == '_';
        match (part, start) {
            (true, None) => start = Some(offset),
            (false, Some(begin)) => {
                found.push((begin, code.get(begin..offset).unwrap_or_default()));
                start = None;
            }
            _ => {}
        }
    }
    found
}

fn constants(sources: &[(PathBuf, String)]) -> HashSet<String> {
    let mut definitions = HashMap::new();
    for (_, source) in sources {
        for line in source.lines() {
            let line = line.trim_start().trim_start_matches("pub(super) ").trim_start_matches("pub ");
            let Some(rest) = line.strip_prefix("const ") else {
                continue;
            };
            if let Some((name, value)) = rest.split_once(": f32 = ") {
                definitions.insert(name.to_string(), value.trim_end_matches(';').to_string());
            }
        }
    }
    let mut derived = HashSet::new();
    loop {
        let before = derived.len();
        for (name, value) in &definitions {
            let tokened = TOKENS.iter().any(|token| value.contains(token));
            let built = words(value).iter().any(|(_, word)| derived.contains(*word));
            if tokened || built {
                derived.insert(name.clone());
            }
        }
        if derived.len() == before {
            return derived;
        }
    }
}

fn violation(argument: &str, derived: &HashSet<String>) -> Option<String> {
    for (offset, word) in words(argument) {
        let qualified = argument.get(..offset).is_some_and(|before| before.ends_with("::"));
        if word.starts_with(|c: char| c.is_ascii_digit()) {
            return Some(format!("the raw number {word}"));
        }
        let constant = word.len() > 1 && word.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
        if constant && !qualified && !derived.contains(word) {
            return Some(format!("{word}, which isn't built from design tokens"));
        }
    }
    None
}

#[test]
fn views_take_sizes_only_from_the_design_tokens() {
    let sources = sources();
    let derived = constants(&sources);
    let mut failures = Vec::new();
    for (path, source) in &sources {
        let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let mut check = |at: usize, call: &str, argument: &str| {
            let line = source.get(..at).map_or(0, |before| before.lines().count());
            if let Some(problem) = violation(argument, &derived) {
                failures.push(format!("{name}:{line}: {call}{argument}) uses {problem}"));
            }
            let fixed = argument.starts_with("px(") && !argument.contains("HAIRLINE");
            if fixed && call != ".snap_to_window_with_margin(" {
                failures.push(format!("{name}:{line}: {call}{argument}) ignores the font size, use rem"));
            }
        };
        for call in STYLED {
            for (at, _) in source.match_indices(call) {
                let open = at + call.len();
                if let Some(close) = closing(source, open) {
                    check(at, call, source.get(open..close).unwrap_or_default());
                }
            }
        }
        for call in SIZED {
            for (at, _) in source.match_indices(call) {
                let named = source.get(..at).and_then(|before| before.chars().last());
                if named.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
                    continue;
                }
                let open = at + call.len();
                if source.get(at.saturating_sub(3)..at) == Some("fn ") {
                    continue;
                }
                if let Some(close) = closing(source, open) {
                    check(at, call, last_argument(source.get(open..close).unwrap_or_default()));
                }
            }
        }
    }
    assert!(failures.is_empty(), "sizes outside the design system:\n{}", failures.join("\n"));
}

#[test]
fn scales_grow_and_text_fits_its_line() {
    let ascending = |values: &[f32]| values.windows(2).all(|pair| pair[0] < pair[1]);
    assert!(ascending(&[text::CAPTION, text::SMALL, text::BODY, text::TITLE]));
    assert!(ascending(&[line::CAPTION, line::SMALL, line::BODY, line::TITLE]));
    assert!(ascending(&[
        space::X2S,
        space::XS,
        space::S,
        space::M,
        space::L,
        space::XL,
        space::X2L,
        space::X3L,
        space::X4L,
        space::APART
    ]));
    assert!(ascending(&[radius::SMALL, radius::CONTROL, radius::LARGE]));
    assert!(ascending(&[glyph::XS, glyph::S, glyph::M, glyph::L, glyph::XL, glyph::COVER]));
    for (size, height) in [
        (text::CAPTION, line::CAPTION),
        (text::SMALL, line::SMALL),
        (text::BODY, line::BODY),
        (text::TITLE, line::TITLE),
    ] {
        assert!(height > size, "{size}px text needs a taller line than {height}px");
    }
}

const SCALES: [f32; 6] = [1., 1.1, 1.25, 1.5, 1.75, 2.];

#[test]
fn widths_rounded_up_keep_the_whole_text_at_every_scale() {
    for scale in SCALES {
        for width in [57., 56.95, 57.3, 100.5, 0.2] {
            let laid_out = (snap_up(width, scale) * scale).round();
            assert!(laid_out >= width * scale - 1e-3, "{width} at {scale}: {laid_out} device pixels is too narrow");
        }
    }
}

#[test]
fn snapped_values_land_on_device_pixels() {
    for scale in SCALES {
        for value in [0.4, 1., 7.3, 12.5, 333.33] {
            let on_grid = snap(value, scale) * scale;
            assert!((on_grid - on_grid.round()).abs() < 1e-3, "{value} at {scale} is {on_grid} device pixels");
            assert!((snap(value, scale) - value).abs() <= 0.5 / scale + 1e-3);
        }
    }
}
