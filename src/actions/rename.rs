use super::files::modified;
use super::{Outcome, Show, base, by_name, normalized};
use crate::i18n::{fill, tr};
use crate::store;
use anyhow::{Result, bail};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const FORBIDDEN: &str = r#"\/:*?"<>|"#;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Rule {
    Date,
    Name,
    Latin,
    Web,
}

impl Rule {
    pub const ALL: [Rule; 4] = [Rule::Date, Rule::Name, Rule::Latin, Rule::Web];

    pub fn title(self) -> &'static str {
        match self {
            Rule::Date => tr().rule_date,
            Rule::Name => tr().rule_name,
            Rule::Latin => tr().rule_latin,
            Rule::Web => tr().rule_web,
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Rule::Date => tr().rule_date_hint,
            Rule::Name => tr().rule_name_hint,
            Rule::Latin => tr().rule_latin_hint,
            Rule::Web => tr().rule_web_hint,
        }
    }
}

#[derive(Clone, Debug)]
struct Entry {
    path: PathBuf,
    date: String,
    folder: bool,
}

#[derive(Clone, Debug)]
pub struct Batch {
    entries: Vec<Entry>,
}

impl Batch {
    pub fn ready(&self, rule: Rule, name: &str) -> bool {
        rule != Rule::Name || !clean(name).is_empty()
    }

    pub fn plan(&self, rule: Rule, name: &str) -> Vec<(PathBuf, PathBuf)> {
        let label = clean(name);
        let mut groups: HashMap<&str, usize> = HashMap::new();
        for entry in &self.entries {
            *groups.entry(entry.date.as_str()).or_default() += 1;
        }
        let mut counters: HashMap<&str, usize> = HashMap::new();
        let total = self.entries.len();
        self.entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let (stem, suffix) = split(entry);
                let numbered = |prefix: &str, number: usize, count: usize| {
                    let width = count.to_string().len().max(2);
                    format!("{prefix}_{number:0width$}{suffix}")
                };
                let name = match rule {
                    Rule::Date => {
                        let counter = counters.entry(entry.date.as_str()).or_default();
                        *counter += 1;
                        numbered(&entry.date, *counter, groups.get(entry.date.as_str()).copied().unwrap_or(*counter))
                    }
                    Rule::Name if label.is_empty() => base(&entry.path),
                    Rule::Name => numbered(&label, index + 1, total),
                    Rule::Latin => format!("{}{suffix}", latin(&stem)),
                    Rule::Web => {
                        let slug = slug(&latin(&stem));
                        let slug = if slug.is_empty() { "file".to_string() } else { slug };
                        format!("{slug}{}", suffix.to_lowercase())
                    }
                };
                (entry.path.clone(), entry.path.with_file_name(name))
            })
            .collect()
    }
}

fn split(entry: &Entry) -> (String, String) {
    let name = base(&entry.path);
    let parts = name
        .rsplit_once('.')
        .filter(|(stem, _)| !stem.is_empty() && !entry.folder)
        .map(|(stem, extension)| (stem.to_string(), format!(".{extension}")));
    parts.unwrap_or((name, String::new()))
}

pub(super) fn prepare(files: &[PathBuf]) -> Result<Outcome> {
    let mut files: Vec<PathBuf> = files.iter().filter(|file| file.exists()).cloned().collect();
    if files.is_empty() {
        bail!(tr().unsupported);
    }
    by_name(&mut files);
    let entries = files.into_iter().map(|path| Entry { date: date(&path), folder: path.is_dir(), path }).collect();
    Ok(Outcome::showing(Show::Rename(Batch { entries })))
}

fn date(path: &Path) -> String {
    let chars: Vec<char> = base(path).chars().collect();
    let named = (0..chars.len())
        .filter(|&start| start == 0 || !chars.get(start - 1).is_some_and(char::is_ascii_digit))
        .find_map(|start| chars.get(start..).and_then(date_at));
    named.unwrap_or_else(|| {
        let stamp = fs::metadata(path).map(|metadata| modified(&metadata)).unwrap_or_default();
        format!("{:04}-{:02}-{:02}", stamp.year(), stamp.month(), stamp.day())
    })
}

fn date_at(chars: &[char]) -> Option<String> {
    let number = |from: usize, to: usize| -> Option<u32> {
        let part = chars.get(from..to)?;
        if !part.iter().all(char::is_ascii_digit) {
            return None;
        }
        part.iter().collect::<String>().parse().ok()
    };
    let separator = chars.get(4).copied().filter(|c| matches!(c, '-' | '_' | '.'));
    let gap = usize::from(separator.is_some());
    if separator.is_some() && chars.get(6 + gap).copied() != separator {
        return None;
    }
    let (year, month, day) = (number(0, 4)?, number(4 + gap, 6 + gap)?, number(6 + 2 * gap, 8 + 2 * gap)?);
    if chars.get(8 + 2 * gap).is_some_and(char::is_ascii_digit) {
        return None;
    }
    let valid = (1990..=2099).contains(&year) && (1..=12).contains(&month) && (1..=31).contains(&day);
    valid.then(|| format!("{year:04}-{month:02}-{day:02}"))
}

fn clean(name: &str) -> String {
    let name: String = name.chars().filter(|c| !c.is_control() && !FORBIDDEN.contains(*c)).collect();
    name.trim().trim_end_matches(['.', ' ']).to_string()
}

fn letter(c: char) -> Option<&'static str> {
    Some(match c {
        'а' => "a",
        'б' => "b",
        'в' => "v",
        'г' | 'ґ' => "g",
        'д' => "d",
        'е' | 'э' => "e",
        'ё' => "yo",
        'ж' => "zh",
        'з' => "z",
        'и' | 'і' => "i",
        'й' | 'ы' => "y",
        'к' => "k",
        'л' => "l",
        'м' => "m",
        'н' => "n",
        'о' => "o",
        'п' => "p",
        'р' => "r",
        'с' => "s",
        'т' => "t",
        'у' => "u",
        'ф' => "f",
        'х' => "kh",
        'ц' => "ts",
        'ч' => "ch",
        'ш' => "sh",
        'щ' => "shch",
        'ъ' | 'ь' => "",
        'ю' => "yu",
        'я' => "ya",
        'ї' => "yi",
        'є' => "ye",
        _ => return None,
    })
}

pub(super) fn latin(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut result = String::new();
    for (index, &c) in chars.iter().enumerate() {
        let lower = c.to_lowercase().next().unwrap_or(c);
        let Some(mapped) = letter(lower) else {
            result.push(c);
            continue;
        };
        if c == lower {
            result.push_str(mapped);
            continue;
        }
        let shouting = [index.checked_sub(1), Some(index + 1)]
            .into_iter()
            .flatten()
            .filter_map(|near| chars.get(near))
            .any(|near| near.is_uppercase());
        if shouting {
            result.push_str(&mapped.to_uppercase());
        } else {
            let mut letters = mapped.chars();
            result.extend(letters.next().map(|first| first.to_ascii_uppercase()));
            result.extend(letters);
        }
    }
    result
}

fn slug(text: &str) -> String {
    let mut slug = String::new();
    for c in text.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_string()
}

pub fn apply(plan: Vec<(PathBuf, PathBuf)>) -> Result<Vec<(PathBuf, PathBuf)>> {
    let moves: Vec<(PathBuf, PathBuf)> = plan.into_iter().filter(|(from, to)| from != to).collect();
    if moves.is_empty() {
        bail!(tr().rename_unchanged);
    }
    let sources: HashSet<String> = moves.iter().map(|(from, _)| normalized(from)).collect();
    let mut claimed = HashSet::new();
    let mut resolved = Vec::new();
    for (from, to) in moves {
        let mut target = to.clone();
        let mut number = 2;
        loop {
            let key = normalized(&target);
            if !claimed.contains(&key) && (sources.contains(&key) || !target.exists()) {
                claimed.insert(key);
                break;
            }
            let entry = Entry { path: to.clone(), date: String::new(), folder: from.is_dir() };
            let (stem, suffix) = split(&entry);
            target = to.with_file_name(format!("{stem} ({number}){suffix}"));
            number += 1;
        }
        resolved.push((from, target));
    }
    let stamp = store::id();
    let steps: Vec<Step> = resolved
        .into_iter()
        .enumerate()
        .map(|(index, (from, to))| Step { parked: from.with_file_name(format!(".traydex-{stamp}-{index}")), from, to })
        .collect();
    move_all(&steps)?;
    log::info!("Renamed {} file(s)", steps.len());
    Ok(steps.into_iter().map(|step| (step.from, step.to)).collect())
}

struct Step {
    from: PathBuf,
    parked: PathBuf,
    to: PathBuf,
}

fn move_all(steps: &[Step]) -> Result<()> {
    for (done, step) in steps.iter().enumerate() {
        if let Err(error) = fs::rename(&step.from, &step.parked) {
            let undo = steps.iter().take(done).map(|step| (&step.parked, &step.from));
            return Err(rolled_back(error, &step.from, undo));
        }
    }
    for (done, step) in steps.iter().enumerate() {
        if let Err(error) = fs::rename(&step.parked, &step.to) {
            let renamed = steps.iter().take(done).map(|step| (&step.to, &step.from));
            let parked = steps.iter().skip(done).map(|step| (&step.parked, &step.from));
            return Err(rolled_back(error, &step.from, renamed.chain(parked)));
        }
    }
    Ok(())
}

fn rolled_back<'a>(
    error: io::Error,
    failed: &Path,
    undo: impl Iterator<Item = (&'a PathBuf, &'a PathBuf)>,
) -> anyhow::Error {
    let stuck: Vec<String> = undo
        .filter_map(|(current, original)| {
            let error = fs::rename(current, original).err()?;
            log::error!("Couldn't restore {} to {}: {error}", current.display(), original.display());
            Some(current.display().to_string())
        })
        .collect();
    let error = anyhow::Error::from(error).context(base(failed));
    if stuck.is_empty() { error } else { error.context(fill(tr().rename_stuck, stuck.join(", "))) }
}

#[cfg(test)]
mod tests;
