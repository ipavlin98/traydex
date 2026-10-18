use super::Probe;
use crate::frames::{FRAME, Frames, offset};
use crate::{input, pause, wait, win};
use anyhow::{Context, Result, bail};
use std::fmt;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

const UNTIL: Duration = Duration::from_secs(5);
const SWEEP_STEPS: u32 = 60;
const SWEEP_PAUSE: Duration = Duration::from_millis(8);
const MODIFIERS: [VIRTUAL_KEY; 4] = [VK_CONTROL, VK_SHIFT, VK_MENU, VK_LWIN];

pub struct Step {
    text: String,
    action: Action,
}

enum Action {
    State,
    Marks(Option<String>),
    Shot(Option<String>, Option<String>),
    Click(String),
    RightClick(String),
    DoubleClick(String),
    Hover(String),
    Leave,
    Scroll(String, i32),
    Key(Vec<VIRTUAL_KEY>),
    Type(String),
    Wait(u64),
    Until(Condition),
    Sweep(String, String, f64),
    Fps,
}

enum Condition {
    Page(String),
    Mark(String),
    Gone(String),
    Shown,
    Hidden,
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.text)
    }
}

fn key(name: &str) -> Result<VIRTUAL_KEY> {
    let lower = name.to_lowercase();
    let named = match lower.as_str() {
        "ctrl" | "control" => Some(VK_CONTROL),
        "shift" => Some(VK_SHIFT),
        "alt" => Some(VK_MENU),
        "win" => Some(VK_LWIN),
        "esc" | "escape" => Some(VK_ESCAPE),
        "enter" | "return" => Some(VK_RETURN),
        "tab" => Some(VK_TAB),
        "space" => Some(VK_SPACE),
        "backspace" => Some(VK_BACK),
        "delete" | "del" => Some(VK_DELETE),
        "insert" => Some(VK_INSERT),
        "home" => Some(VK_HOME),
        "end" => Some(VK_END),
        "pageup" => Some(VK_PRIOR),
        "pagedown" => Some(VK_NEXT),
        "up" => Some(VK_UP),
        "down" => Some(VK_DOWN),
        "left" => Some(VK_LEFT),
        "right" => Some(VK_RIGHT),
        _ => None,
    };
    if let Some(key) = named {
        return Ok(key);
    }
    if let Some(number) = lower.strip_prefix('f').and_then(|n| n.parse::<u16>().ok()).filter(|n| (1..=24).contains(n)) {
        return Ok(VIRTUAL_KEY(VK_F1.0 + number - 1));
    }
    match lower.as_bytes() {
        [c @ (b'a'..=b'z' | b'0'..=b'9')] => Ok(VIRTUAL_KEY(c.to_ascii_uppercase() as u16)),
        _ => bail!(
            "unknown key \"{name}\": letters, digits, f1..f24, esc, enter, tab, space, backspace, delete, home, end, pageup, pagedown, arrows, ctrl, shift, alt, win"
        ),
    }
}

impl Step {
    pub fn parse(text: &str) -> Result<Step> {
        let text = text.trim();
        let (verb, rest) = text.split_once(char::is_whitespace).map_or((text, ""), |(v, r)| (v, r.trim()));
        let words: Vec<&str> = rest.split_whitespace().collect();
        let one = |what: &str| -> Result<String> {
            match words.as_slice() {
                [single] => Ok(single.to_string()),
                _ => bail!("\"{text}\": expected {verb} <{what}>"),
            }
        };
        let action = match verb {
            "state" => Action::State,
            "marks" => Action::Marks(words.first().map(|w| w.to_string())),
            "shot" => Action::Shot(words.first().map(|w| w.to_string()), words.get(1).map(|w| w.to_string())),
            "click" => Action::Click(one("mark")?),
            "rclick" => Action::RightClick(one("mark")?),
            "dclick" => Action::DoubleClick(one("mark")?),
            "hover" => Action::Hover(one("mark")?),
            "leave" => Action::Leave,
            "scroll" => match words.as_slice() {
                [target, notches] => Action::Scroll(
                    target.to_string(),
                    notches
                        .parse()
                        .with_context(|| format!("\"{text}\": number of wheel notches, + is down, − is up"))?,
                ),
                _ => bail!("\"{text}\": expected scroll <mark> <notches>"),
            },
            "key" => Action::Key(one("combo")?.split('+').map(key).collect::<Result<_>>()?),
            "type" if !rest.is_empty() => Action::Type(rest.to_string()),
            "wait" => Action::Wait(one("ms")?.parse().with_context(|| format!("\"{text}\": expected wait <ms>"))?),
            "until" => Action::Until(match one("condition")?.split_once('=') {
                Some(("page", page)) => Condition::Page(page.into()),
                Some(("mark", mark)) => Condition::Mark(mark.into()),
                Some(("gone", mark)) => Condition::Gone(mark.into()),
                None if rest == "shown" => Condition::Shown,
                None if rest == "hidden" => Condition::Hidden,
                _ => bail!("\"{text}\": conditions are page=X, mark=X, gone=X, shown, hidden"),
            }),
            "sweep" => match words.as_slice() {
                [from, to, seconds] => Action::Sweep(
                    from.to_string(),
                    to.to_string(),
                    seconds.parse().with_context(|| format!("\"{text}\": duration in seconds"))?,
                ),
                _ => bail!("\"{text}\": expected sweep <mark> <mark> <seconds>"),
            },
            "fps" => Action::Fps,
            _ => bail!("unknown step \"{text}\""),
        };
        Ok(Step { text: text.to_string(), action })
    }

    pub fn allowed(&self, real_input: bool, measure: bool) -> Result<()> {
        let needs_input = match &self.action {
            Action::Hover(_) | Action::Sweep(..) => true,
            Action::Key(keys) => keys.len() > 1 || keys.iter().any(|key| MODIFIERS.contains(key)),
            _ => false,
        };
        if needs_input && !real_input {
            bail!(
                "step \"{}\" needs --input: hover and Ctrl/Alt/Shift/Win combos can't be reproduced with window messages",
                self.text
            );
        }
        if matches!(self.action, Action::Fps) && !measure {
            bail!("step \"{}\" needs --measure", self.text);
        }
        Ok(())
    }
}

impl Probe<'_> {
    fn client_point(&self, point: POINT) -> Result<(i32, i32)> {
        let client = win::client(self.panel()?);
        Ok((point.x - client.left, point.y - client.top))
    }

    fn helper_point(&self) -> POINT {
        let helper = self.ctx.layout.helper;
        POINT { x: (helper.left + helper.right) / 2, y: (helper.top + helper.bottom) / 2 }
    }

    fn frames_summary(&mut self) -> Option<String> {
        let path = self.frames.clone()?;
        let summary = Frames::read(&path, self.frames_from, FRAME).summary();
        self.frames_from = offset(&path);
        Some(summary)
    }

    fn click(&self, spec: &str, right: bool, times: u32) -> Result<Option<String>> {
        let target = self.target(spec)?;
        for _ in 0..times {
            if !self.input {
                let (x, y) = self.client_point(target.point)?;
                win::post_click(self.panel()?, x, y, right);
            } else if right {
                input::right_click_at(target.point)?;
            } else {
                input::hover(target.point)?;
                input::click(target.point)?;
            }
        }
        Ok(Some(format!("\"{}\" at ({}, {})", target.name, target.point.x, target.point.y)))
    }

    pub(super) fn execute(&mut self, step: &Step) -> Result<Option<String>> {
        match &step.action {
            Action::State => self.state().map(|()| None),
            Action::Marks(prefix) => {
                let diag = self.diag()?;
                let lines: Vec<String> = diag
                    .marks
                    .iter()
                    .filter(|(mark, _)| prefix.as_ref().is_none_or(|p| mark.starts_with(p.as_str())))
                    .map(|(mark, [x, y, w, h])| format!("{mark}: x {x:.0}, y {y:.0}, {w:.0}×{h:.0}"))
                    .collect();
                Ok(Some(format!("marks (logical client-area pixels):\n    {}", lines.join("\n    "))))
            }
            Action::Shot(name, mark) => {
                let path = self.shot(name.as_deref(), mark.as_deref())?;
                Ok(Some(format!("window screenshot: {}", path.display())))
            }
            Action::Click(spec) => self.click(spec, false, 1),
            Action::RightClick(spec) => self.click(spec, true, 1),
            Action::DoubleClick(spec) => self.click(spec, false, 2),
            Action::Hover(spec) => {
                let target = self.target(spec)?;
                input::hover(target.point)?;
                Ok(Some(format!("\"{}\" at ({}, {})", target.name, target.point.x, target.point.y)))
            }
            Action::Leave => {
                if self.input {
                    input::hover(self.helper_point())?;
                } else {
                    win::post_leave(self.panel()?);
                }
                Ok(None)
            }
            Action::Scroll(spec, notches) => {
                let target = self.target(spec)?;
                if self.input {
                    input::wheel(target.point, *notches)?;
                } else {
                    win::post_wheel(self.panel()?, target.point, *notches);
                }
                Ok(None)
            }
            Action::Key(keys) => {
                if self.input {
                    let panel = self.panel()?;
                    if win::visible(panel) && win::foreground() != panel {
                        bail!("keys not sent: the panel isn't active");
                    }
                    input::chord(keys)?;
                } else if let [key] = keys.as_slice() {
                    win::post_key(self.panel()?, *key);
                }
                Ok(None)
            }
            Action::Type(text) => {
                if self.input {
                    input::text(text)?;
                } else {
                    let panel = self.panel()?;
                    for unit in text.encode_utf16() {
                        win::post_char(panel, unit);
                        pause(15)?;
                    }
                }
                Ok(None)
            }
            Action::Wait(ms) => pause(*ms).map(|()| None),
            Action::Until(condition) => {
                let app = self.app()?;
                let (what, met): (String, Box<dyn Fn() -> bool>) = match condition {
                    Condition::Page(page) => {
                        (format!("page {page}"), Box::new(|| app.diag().is_some_and(|d| d.page == *page)))
                    }
                    Condition::Mark(mark) => {
                        (format!("mark {mark}"), Box::new(|| app.shown() && app.diag().is_some_and(|d| d.has(mark))))
                    }
                    Condition::Gone(mark) => {
                        (format!("mark {mark} gone"), Box::new(|| app.diag().is_some_and(|d| !d.has(mark))))
                    }
                    Condition::Shown => ("panel shown".into(), Box::new(|| app.shown())),
                    Condition::Hidden => ("panel hidden".into(), Box::new(|| !app.shown())),
                };
                wait(&what, UNTIL, || met().then_some(())).map(|()| None)
            }
            Action::Sweep(from, to, seconds) => {
                let (first, second) = (self.target(from)?, self.target(to)?);
                input::hover(first.point)?;
                pause(200)?;
                self.frames_summary();
                let started = Instant::now();
                while started.elapsed().as_secs_f64() < *seconds {
                    input::glide(second.point, SWEEP_STEPS, SWEEP_PAUSE)?;
                    input::glide(first.point, SWEEP_STEPS, SWEEP_PAUSE)?;
                }
                pause(300)?;
                Ok(self.frames_summary())
            }
            Action::Fps => Ok(self.frames_summary()),
        }
    }
}
