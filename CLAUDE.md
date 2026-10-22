# Traydex

## Developer note
Ambitious ideas, simple systems, obvious-feeling software. Don't preserve complexity because it exists; don't add machinery because it looks impressive. Understand the real constraint, then fight for the smallest model that makes correct behavior unsurprising. "Measure twice, cut once" + YAGNI. Fight scope creep. Honor dev intent minimally and realistically.

## Build and run
Single working build: `target\debug\traydex.exe`. Never call `cargo` directly (not on PATH); `build.ps1` sets up MSVC + cargo.
- `.\build.ps1`: build.
- `-Run`: closes running Traydex, starts fresh debug build hidden in tray (`--autostart`), no window popup.
- `-Lint`: `cargo fmt --all` + `cargo clippy --workspace --all-targets -- -D warnings`. Non-zero = unfinished work.
- `-Test [filter]`: `cargo test` (unit, integration, headless GPUI, `vendor/gpui` patch regressions). No real windows/input/registry/user data. Filter = substring of test names.
- Task fully done → `-Lint` and `-Test` once each; if `e2e/`, element marks or `diagnostics.json` changed, also `-E2E list` (builds runner only, no desktop). All pass → `-Run`, then report. Not after intermediate edits. Standing requirement.
- No release builds (`cargo build --release`, `-Release`) unless user explicitly asks. Public releases are built only by GitHub Actions (`.github/workflows/release.yml`); `dist/` is not in git.

## Repository
Public GitHub repo; contributors and their AI assistants follow this file.
- Version: `version` in `Cargo.toml` (exe resources take it via `build.rs`). Release = bump version + `CHANGELOG.md` section, then tag `v<version>` or run the Release workflow. Never bump the version unasked.
- CI (`.github/workflows/ci.yml`, windows-latest): `cargo fmt --check`, clippy `-D warnings`, `cargo test`, all `--locked`. Same as `-Lint` + `-Test`; keep them equal. Plus `cargo test` for `i686-pc-windows-msvc` and `aarch64-pc-windows-msvc`.
- Targets: x64, x86, ARM64. Code builds on all three: no `isize`-specific casts for Win32 pointer-sized values (`as _`), no references into packed structs (`NOTIFYICONDATAW`, `SHFILEINFOW`; assign/copy whole fields). Locally: `cargo check --target i686-pc-windows-msvc` (x86 MSVC tools are installed, ARM64 aren't).
- Release assets (`release.yml`): per arch `Traydex-<version>-<arch>-setup.exe` (Inno Setup, `packaging/traydex.iss`, per-user) and `-portable.zip` (`packaging/portable.txt` beside the exe switches data to `data\`, see `Profile::portable_beside`), plus `SHA256SUMS.txt`.
- `vendor/unrar`, `vendor/unrar_sys`: crates.io copies with `extern "system"` for UnRAR (stdcall on x86). Keep the patch when updating.
- Docs, kept in sync with behavior:
  - `README.md` (English) and `README.ru.md` (Russian): user-facing overview, same structure. Change one → change the other.
  - `docs/manual.md`: full reference of every feature, setting, storage detail and `vendor/gpui`/`vendor/unrar` patch.
  - `docs/plugins.md`: plugin protocol.
- Language: everything in the repo is English except `README.ru.md` and the app translations (`src/i18n`, installer messages in `packaging/traydex.iss`). Cyrillic in tests is test data (non-ASCII paths, transliteration, layout), keep it.
  - `docs/media/*.webp`: demo clips (lossless animated WebP: GIF palettes distort the accent colors); don't regenerate or add unasked.
  - `CONTRIBUTING.md`: build and PR rules for humans; points here for architecture.

## Architecture and code rules
Standing requirements for all new code. Feature works but breaks rules = not done. Rule in the way → propose architecture change first, no workarounds.

### Layers
- `store`: shelf data (`Store`, `Item`), `Settings`, file storage (`Storage`), disk persistence. No UI/Windows knowledge.
- `actions`: tile domain: `Action`, `Kind`, property table `Kind::spec()`, `Tiles` list with stable `TileId`s, execution. External effects only via `&dyn Services`.
- `preview`, `search`, `keys`, `i18n`, `theme`, `input`, `textarea`, `geometry`: logic/widgets, no system access. `geometry` = sizes shared by `view` and `shelf` (title bar, tile grid, min window size).
- `os.rs`: safe wrappers for side-effect-free system queries (keyboard layout, UI language, code page, local file time); domain may call `crate::os`. Anything mutating system or stateful (windows, COM, processes, network, registry) → ports only.
- `system.rs`: ports + boundary types.
  - `Desktop`: window, focus, clipboard, DnD, registry. UI thread only, deliberately not `Send`.
  - `Services`: `Send + Sync`. Processes, network, dialogs, shell, OCR, background jobs.
  - Boundary types: `Event`, `Frame`, `Handler`, `Caption`.
- `platform`: sole Windows adapter; `Native` implements both ports.
  - Win32/COM, `unsafe`, `HWND`, `GUID`, `RECT` only here, in `os.rs`, and `main.rs` (composition root).
  - Expose project types, not Windows types.
  - `VIRTUAL_KEY` constants in `keys` = data, allowed.
- `shelf`: UI state + coordination.
- `view` (`src/view.rs`, `src/view/`): rendering only. `impl Shelf` deliberately outside `shelf`, so compiler hides `shelf`'s private fields and `pub(super)` methods. Never move view code back under `shelf/`.
- Direction: `view → shelf → domain (actions, store, preview, …) → system ← platform`. Domain doesn't import `platform`/`shelf`; `platform` doesn't import `shelf`. Reverse dependency = design error.

### State and GPUI
- Impossible states unrepresentable: page data in a `Screen` variant, not `Option` fields beside a tag.
  - New page = new `Screen` + `Page` variant.
  - Transitions only via `navigate`/`show`.
  - Leave-page cleanup (stop server, delete empty note) in `navigate` or screen data's `Drop`.
- Stable IDs: closures, subscriptions, tasks, events, fields hold `TileId`/`item.id`, never index into mutable list.
- `render` never mutates model.
  - Allowed: idempotent window sync (transparency, background); recording geometry from `canvas` probes (`Regions`, `Caption`).
  - Reactions go in the mutating method or a subscription: `observe`, `subscribe`, `subscribe_in` (needs `window`), `on_focus_lost`, `observe_window_appearance`, `observe_window_activation`.
- `Shelf` fields and nested structs private. View reads via `pub(crate)` accessors returning shared refs (`store()`, `tiles()`, `session()`, `jobs(cx)`, …); mutates only via `Shelf` methods, never field assignment in handlers. Need more → add read accessor or `Shelf` method; never hand out `&mut` to a field.
- Geometry known to view and needed by `shelf`: pass as argument (`scroll_lane(.., cover, ..)`) or put in `geometry`; `shelf` never imports `view`.
- `Shelf` = coordinator, not dumping ground.
  - Independent state with own timers/tasks → separate `Entity<T>` (`Notices`, `Jobs`), followed via `cx.observe`.
  - Related fields without tasks → substructure with private fields/methods (`Session`, `Selection`, `LanguageMenu`, `Recorder`).
  - Before adding a `Shelf` field, find its entity/substructure.
- `Shelf` methods split across `shelf/` files by topic (`items`, `tiles`, `settings`, `keyboard`, `filter`, `screen`, `tasks`, …). New method → topic file, not `mod.rs`.
- Async:
  - Blocking work only via `Shelf::background`, never UI thread: disk I/O beyond trivial checks, network, processes, decoding, recognition.
  - Job-indicator work only via `Shelf::run_job` (starts/ends job, reports `Ended::Finished`/`Ended::Stopped`; Stopped = by user, timeout = failure). Never call `jobs.start`/`jobs.end` manually.
  - `Desktop` calls that can re-enter wndproc (show, hide, focus, resize, clipboard, DnD, embedding) via `after_update`/`after_update_then`, not directly from handler.
  - `cx.spawn` captures `WeakEntity`; apply via `this.update`; entity gone → exit quietly.
  - Task cancelled on restart/page leave → `Option<Task<()>>` field. `detach()` only fire-and-forget.
  - Time only from `cx.background_executor().timer()` / `.now()` (tests drive via `elapse`).
- No flicker while waiting. Consider for every feature whose page/content depends on background work.
  - Show target page immediately in waiting state. Never keep old page during work; never blank/intermediate screen. Page-opening tile gets an `actions::Pending` variant (pre-work knowns: names, counts, settings); `perform` shows it as `Screen::Waiting`.
  - Placeholders = exact geometry of finished content: same marks, positions, heights. Unknown/variable-length text: one line, fade, tooltip only when clipped, so result can't change height.
  - Skeletons only from `skeleton`/`skeleton_line`; appear after short delay so fast results never flash.
  - Result applies only while its waiting screen is current (`waiting_for(job)`). User left → drop result, never reopen page. Failure → leave waiting screen for shelf, show error.
  - Tests: page shown before job runs; case added to `placeholders_keep_the_layout_of_the_result` (compares waiting vs finished layouts).
- Panics must not crash; restore saved state.
  - Every UI handler via `cx.safe_listener`.
  - Events and task callbacks via `guarded`.
  - `recover` restores last committed snapshot (`Store::reverted`), leaves page via `navigate`, resets transient state. New field with own lifecycle (open menu, capture, pending task) must be reset there.

### Design system
- Visual sizes only from `src/design.rs`: text sizes + paired line heights (`text`, `line`), `space`, `radius`, icons (`glyph`), controls (`size`), `opacity`, `timing`. Colors only from `Theme`.
- Sizes scale with font-size setting via rem (as Zed). Tokens = px at default font; view passes `rem(token)`, never `px(token)` (`design::tests` fails; hairlines stay `px`). View arithmetic in design units: `measure` (text) and `units` (viewport) convert into them. Values compared with real layout bounds or handed to `shelf` (`Rows`, lane covers, margins) go through `scaled`; `shelf` scales window sizes by `zoom()`.
- Prefer semantic tokens (`space::PAGE`, `SUBTITLE`, `LABEL`, `BLOCK`, `GROUP`, `SECTION`) over raw scale.
- Reuse building blocks: `page`, `heading`, `labelled`, `options`/`segment`, `choice`, `picked`, `text_button` with `Look`, `dimmed`, `group`, `section`, `setting`.
- Component geometry (tile size, fixed row height) may be named module constant. Spacing, type, radius, icon sizes must be token-built (`design::tests` enforces).
- Off-scale value = design change: new token only for genuinely new role, else snap to nearest step.

### Data and persistence
- Atomic writes: temp file → `rename` (as `Store::save`, `actions::save`).
- `shelf.json`, `actions.json` = contract with installed copies.
  - New field: `#[serde(default)]`. Rename: `#[serde(alias)]`.
  - Removing a field or changing meaning: migration + test reading old file.
- Never silently overwrite corrupted file: keep `*.broken.json` copy, tell user.
- Shelf items change via `Store` methods, then `save` + `items_changed`.

### Errors
- `anyhow::Result` in app; `.context(...)` where failure would be unclear; user sees errors via `fail` with `i18n` string.
- Outside tests never: `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!`, `unreachable!`, `dbg!`, out-of-bounds-capable indexing. Use `?`, `let … else`, `get`. Clippy enforces macros/methods (`[lints.clippy]` in `Cargo.toml`; `clippy.toml` allows in tests). Tests: `expect("what was expected")`, `panic!("why")` fine.
- Don't swallow errors. `let _ =`/`.ok()` only for truly optional work (temp cleanup, logging); else `log::warn!` or `fail`.

### Types and API
- New concept = new type: newtypes for IDs/system values (`TileId`, `Frame`, `Handler`); enum instead of string or special-meaning `bool` (`Single`, `Corner`, `ShakeLevel`).
- Variant-dependent properties in one table (`Kind::spec()`), not parallel `match`es. Tables/dispatchers over own enums: exhaustive `match`, no `_ =>`.
- New tile needs: `Kind` variant, `spec()` row, `slot`, `execute`, `default_title`, chip, strings in every language.
- Minimal visibility: private → `pub(super)`/`pub(crate)` → `pub` only for sibling-module needs. Fields of invariant-holding types private.
- Magic numbers → named module constants. Layout sizes = view constants; those `shelf` needs → `geometry`.
- Hot paths (`render`, pointer move): no needless clone of collections/strings; borrow.
- No `Rc<RefCell>`/`Arc<Mutex>` for state that fits a GPUI entity. `Rc<Cell>`/`Rc<RefCell>` only to pass geometry out of prepaint.
- No speculative abstractions: no single-impl trait (except `system.rs` ports), no unused params/flags/settings.
- New dependency only if std and existing crates have no reasonable alternative; name it in report.

### Structure
- File > ~600 lines or function > ~60 lines → split by meaning. Submodule beside its code (`mod x;` + `x.rs`, tests in `x/tests.rs`).
- Pattern in 3+ places → shared helper.
- rustfmt via `rustfmt.toml` (`max_width = 120`, `use_small_heuristics = "Max"`), applied by `-Lint`. No hand formatting; don't edit `rustfmt.toml` to fit.
- Fix clippy findings in code. Silence only proven false positives, narrowest item, `#[expect(clippy::lint, reason = "why")]`. Never crate-wide `allow`, reasonless `#[allow(...)]`, raised thresholds. Warning pointing at design smell (too many args, complex type) → fix design (struct, type alias).

### Before finishing
- `-Lint` passes: formatted, zero clippy/compiler warnings in workspace (`vendor/gpui` warnings not ours).
- Re-read diff against this section, fix before reporting:
  - Win32 outside `platform`/`os.rs`;
  - index instead of ID;
  - model change in `render`;
  - `Shelf` field assigned from view, or accessor giving view mutable access;
  - `shelf` importing `view`;
  - blocking I/O on UI thread;
  - new `Shelf` field belonging in separate entity;
  - function/file over size limits;
  - page waiting for background work without same-geometry placeholder.
- User-visible change (feature, setting, shortcut, tile, string meaning) → update `README.md`, `README.ru.md` and `docs/manual.md`; plugin protocol change → also `docs/plugins.md`. Pure internals → no doc changes.

## Tests
Tests catch bugs; not for count or green. User won't remind; you decide by these rules and write/update yourself. "No test needed" is normal.

Write a test only if it can catch a real bug (breakage unnoticed by user or by future edits). Must fail on wrong implementation.
- Bug fix: write test first, confirm it fails on old code, then fix.
- New logic: mentally check a breakage (inverted condition, forgotten save, off-by-one) fails it. No tests that pass with any implementation.
- Security/data-safety checks: mental check insufficient; temporarily break code, confirm failure, restore.

Hunt bugs: empty/boundary values; long, multi-line, Cyrillic strings; spaces and `/` in paths, missing files; external call errors; repeated and racing events; user action order.

By change kind:
- **New/changed behavior** (logic, commands/keys, events, persistence, tiles/actions, parsing): test in module's `tests.rs` (`mod tests;` beside code).
  - Extend existing covering test instead of duplicating.
  - Shelf/UI via `Harness` in `src/shelf/tests.rs`: `open_with`, `send(Event)`, `act`, `keys`, `elapse`, `saved()`, `count(&Call)`, `tile(i)` (tile id by position), `read_cx` (nested entities).
  - Tests mutate state via `Shelf` methods, like view.
- **External effects** (files outside sandbox, clipboard, processes, registry, windows, shell): only via `src/system.rs` ports: `Desktop` (`self.desktop`), `Services` (`self.services`, `&dyn Services` in domain).
  - Tests use `Fake` (`src/testing.rs`).
  - Assert result (saved data, clipboard, shown to user); assert call only when call is the result.
  - Missing method → add to port, `platform::Native`, `Fake`.
- **New screen/page/panel/notable element** (button, toggle, field, menu, list): `.marked("name")` (repeated: `name:{id}`/`name:{i}`); add scene/checks to `src/shelf/tests/visual.rs`.
  - Checks: exists, inside window and container, no neighbor overlap, at min and default window sizes, every language.
  - Changes inside already-marked block: no new checks.
  - Mark what can really overflow/overlap (long translations, narrow window, many items), not every icon.
- **New color or text/background pair**: `Theme` tokens only; add pair to contrast test `src/theme/tests.rs` (body text ≥ 4.5:1).
- **New UI strings**: every `i18n` language (`every_language_defines_every_string`).
- **`vendor/gpui` change**: regression test in `src/gpui_tests.rs`.
- **No test**: spacing/styling without new behavior (visual checks/snapshots catch); behavior-preserving refactors (existing tests stay green).

How:
- Test contract (what user sees/gets), not internals. One behavior per test, descriptive name.
- Full isolation/determinism: no real windows, input, registry, network, user data; files only in temp `Sandbox`; time only via `elapse`, never `sleep`.
- Expected value independent: explicit constant or from requirement, never computed by code under test.
- Check only what requirement specifies; don't pin incidentals (aux field format, ordering, absence of other calls) — such tests fail on correct changes and train expectation-fitting.
- Failing test = found bug by default. Change expectation only if task explicitly changes that behavior; else fix code. Unclear → ask user.
- Forbidden for green: deleting tests/assertions, weakening conditions, widening tolerances; raising timeouts without found cause, `#[ignore]`; fitting expectation to output without understanding it.
- Renamed/removed mark → update everywhere: `visual.rs` and `e2e/`.
- Report:
  - One line: tests added/changed and what they catch, or why none needed.
  - Name risky untested parts (e.g. `Native` impl).
  - List each changed existing expectation separately: before → after, and which task requirement caused it.

E2E suite (`e2e/`, see below):
- Run only with user permission; ordinary tasks don't need it.
- Change touches what suite relies on → update affected scenarios immediately: marks `zone`, `tile:N`, `drag-all`, `pin`, `layout`, `settings`, `actions`, `home`, `title-bar`, `title-buttons`, `item:{id}`; `diagnostics.json` format, launch flags, test profile; panel show/hide, tiles, clipboard behavior.
- New Windows integration (DnD, hotkeys, hot corner, registry, Explorer, clipboard, process launch, preview) → scenario in matching `e2e/src/suites/` suite; offer to run in report.
- Don't update snapshot baselines per feature. Change alters look of empty shelf, list, tiles, settings or tiles page → warn in one line that snapshots will diverge. Update (`-E2E baselines`) only on user request, after reviewing diff maps: every difference intentional.

## Agent UI checks
User works at this computer concurrently. Never touch their real Traydex or profile.
- Automated first: layout, visibility, contrast via visual tests in `-Test`. Mark new pages/elements, add to `src/shelf/tests/visual.rs`.
- Look at screen / check behavior in running app only via probe: `.\build.ps1 -E2E probe [options] "step" ...` (reference: `-E2E probe --help`). No own scripts launching Traydex, finding windows, computing coords, sending input, screenshotting (window ends under another window, cursor misses, coords mismatch layout).
  - One call: builds, starts separate test instance with clean profile in `%TEMP%\traydex-e2e` (auto-show off, pinned, no autostart/registry), fixed window spot, waits until layout matches window size, runs steps in order, closes, cleans up.
  - Profile options: `--notes N`, `--files N`, `--images N`, `--layout list|grid`, `--theme light|dark|system`, `--language ru|en|zh|hi|es|fr`, `--size WxH`, `--animations`, `--measure` (frame log for `fps`, `sweep`), `--timeout S`.
  - Steps: `state`, `marks [prefix]`, `shot [name] [mark]`, `click|rclick|dclick <mark>`, `hover <mark>`, `leave`, `scroll <mark> <notches>`, `key <combo>`, `type <text>`, `wait <ms>`, `until page=X|mark=X|gone=X|shown|hidden`, `sweep <mark> <mark> <seconds>`, `fps`.
  - Address only via marks: exact name (`settings`), N-th on screen by position (`item#0`, `tile#2`), last resort logical client coords (`@120,48`). List via `marks`. Missing mark → add `.marked(...)` in code, don't guess coords.
  - Probe refuses steps whose target is outside window, scrolled out, or marks mismatch layout, with reason.
  - After each step prints page, focused window, cursor position, mark under cursor — read before concluding. Failed step stops probe, saves `failure.png`, exit 1.
  - Screenshots of app window only (`PrintWindow`) in `target\e2e\probe\<time>\`; open PNG with Read. Never screen screenshots.
  - Default: app shows window without activation (`--inactive`, test profile only); clicks, modifier-less keys, text, scroll sent as window messages. User focus/cursor/input untouched; no permission needed.
  - `--input` needed for hover, `sweep`, Ctrl/Alt/Shift/Win combos. SendInput; user's mouse/keyboard blocked until probe ends (stop: Ctrl+Alt+Shift+Q); window foregrounded and topmost. Before each step re-checks focus/occlusion; after each move checks cursor on target. Needs user permission, same rules as E2E suite.
- Forbidden:
  - computer use;
  - input/focus/window-order calls outside `e2e` crate (`SendInput`, `SetCursorPos`, `mouse_event`, `keybd_event`, `SetForegroundWindow`, `SetActiveWindow`, `SetFocus`, `SetWindowPos`);
  - showing test window externally via `ShowWindow` (app doesn't know it's visible, lays out for wrong size, marks mismatch);
  - full-screen screenshots;
  - any messages to user's real Traydex windows.
- Real-system-input behavior (OLE DnD, global hotkey, click outside panel, hot corner, window move/resize) → E2E suite with permission. Uncovered new behavior → add `e2e/` scenario. Suite doesn't check system dialogs or tray menu: name those to user.

## System E2E suite
Runs real Traydex in test profile (`--test-profile`) on user's desktop. Separate crate `e2e/`, not in `traydex.exe`; `-Test` doesn't run it.

`.\build.ps1 -E2E <command>`:

| Command | What |
|---|---|
| `run` | Full run. |
| `run lifecycle dnd` | Selected suites: `lifecycle`, `dnd`, `snapshots`, `input`, `shell`, `files`, `motion` (animation smoothness: hover/window-fade frame rate, on-screen colors). |
| `baselines` | Snapshot baselines. |
| `cleanup` | Cleanup after interrupted run. |
| `list` | Scenario list. |
| `probe ...` | Agent one-off UI check (above). |

- `run`/`baselines` options, anywhere: `--scenario-timeout S`, `--timeout S`, `--countdown S` (e.g. `run motion --countdown 3`).
- Run effects (removed after run and at next run start):
  - test Traydex windows, Explorer (test folders only), topmost helper window;
  - physical mouse/keyboard blocked whole run: low-level hooks swallow movement, clicks, wheel, keys; only test SendInput passes;
  - programmatic mouse/keyboard control, focus switching;
  - clipboard overwritten;
  - a small test picture moved to the user's Recycle Bin and restored from it (`files` suite);
  - temp HKCU keys: `Run\Traydex Test`, `Classes\*\shell\TraydexTest`, `Classes\Directory\shell\TraydexTest`;
  - files in `%TEMP%\traydex-e2e`.
- Stopping: physical input swallowed, doesn't stop; Ctrl+Alt+Shift+Q stops with cleanup, returns input immediately, exit 2; Ctrl+Alt+Del always emergency exit; lock lifts itself when runner exits or after run timeout + margin.
- Exit codes: 0 all passed; 1 failures; 2 preflight failed or stopped; 3 no failures but some scenarios didn't run.
- Real Traydex running → input suites (`dnd`, `input`, `files`, `motion`) don't run (code 2); `lifecycle`, `snapshots`, `shell` available.
- Report: `target\e2e\reports\<time>\report.md` + `report.json`; on failure also window screenshot + test instance log. Baselines: `e2e\baselines\<configuration>\`.
- Snapshots cover whole pages: scrollable pages scrolled screen by screen → `name.png`, `name-2.png`, … Differs from baseline if >0.2% pixels change or any 16×16 square has >12 changed pixels (new tab/button never lost in overall tolerance). Marks in `MASKED` (`e2e/src/suites/snapshots.rs`; the `version` footer) are painted over before comparing, so a version bump never changes baselines.
- Permission: before every run (except `list`, `cleanup`, `probe` without `--input`) list effects to user incl. mouse/keyboard block and stop chord, ask permission. Covers only runs of this suite within current task. User starting suite themselves = consent; runner asks nothing.
