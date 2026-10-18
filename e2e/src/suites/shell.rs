use super::{Session, Spec, Suite, none, spec, write};
use crate::sandbox::Sandbox;
use crate::{Ctx, app, expect_eq, poll, wait, win};
use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

pub const SUITE: Suite = Suite { id: "shell", title: "Registry, Explorer and clipboard", specs, run };

fn specs() -> Vec<Spec> {
    vec![
        spec("autostart", "The autostart value points to the test exe with a hidden start", false),
        spec("context-menu", "The context menu item is registered and adds files via the Shell API", false),
        spec(
            "clipboard-file",
            "A file is copied to the clipboard (right click) and stays there after Traydex quits",
            false,
        ),
        spec(
            "clipboard-note",
            "A note is copied to the clipboard as text (right click) and stays there after Traydex quits",
            false,
        ),
    ]
}

fn quoted(ctx: &Ctx, rest: &str) -> String {
    format!("\"{}\" --test-profile \"{}\" {rest}", ctx.sandbox.exe.display(), ctx.sandbox.profile.display())
}

fn copy_item(ctx: &Ctx, id: &str) -> Result<()> {
    let app = ctx.app()?;
    app.wait_diag("item drawn", Duration::from_secs(5), |d| d.has(&format!("item:{id}")))?;
    app.post_click_with(&format!("item:{id}"), true)
}

fn run(s: &mut Session) {
    s.run("autostart", |ctx| {
        ctx.reset()?;
        let mut settings = app::settings(ctx);
        settings["autostart"] = true.into();
        ctx.start(settings, vec![], none(), &["--autostart"])?;
        let expected = quoted(ctx, "--autostart");
        let actual = poll(Duration::from_secs(5), || Sandbox::autostart_value().filter(|value| *value == expected))?;
        expect_eq(
            "value HKCU\\...\\Run \"Traydex Test\"",
            Some(expected.clone()),
            actual.or_else(Sandbox::autostart_value),
        )?;
        Ok(())
    });

    s.run("context-menu", |ctx| {
        ctx.reset()?;
        let folder = ctx.sandbox.folder("shell")?;
        let nested = folder.join("Папка с пробелами");
        let file = write(&nested.join("отчёт 2024 — итог.txt"), "контекстное меню".as_bytes())?;
        let sub = nested.join("вложенная папка");
        fs::create_dir_all(&sub)?;
        let mut settings = app::settings(ctx);
        settings["context_menu"] = true.into();
        ctx.start(settings, vec![], none(), &["--autostart"])?;
        let expected = quoted(ctx, "--send \"%1\"");
        for index in 0..2 {
            let actual =
                poll(Duration::from_secs(5), || Sandbox::menu_command(index).filter(|value| *value == expected))?;
            expect_eq(
                &format!("menu item command #{index}"),
                Some(expected.clone()),
                actual.or_else(|| Sandbox::menu_command(index)),
            )?;
        }
        for path in [&file, &sub] {
            win::shell_verb("TraydexTest", path)
                .with_context(|| format!("the Shell API didn't run the item for {}", path.display()))?;
        }
        let app = ctx.app()?;
        let paths: Vec<PathBuf> =
            wait("the file and the folder are added via the context menu", Duration::from_secs(8), || {
                let saved = app.saved_paths();
                (saved.contains(&file) && saved.contains(&sub)).then_some(saved)
            })?;
        expect_eq("items on the shelf", 2, paths.len())?;
        Ok(())
    });

    s.run("clipboard-file", |ctx| {
        ctx.reset()?;
        let folder = ctx.sandbox.folder("clipboard")?;
        let file = write(&folder.join("буфер обмена.txt"), b"clipboard file")?;
        let item = app::path_item(&file);
        let id = item["id"].as_str().unwrap_or_default().to_string();
        ctx.start(app::settings(ctx), vec![item], none(), &[])?;
        win::clear_clipboard();
        copy_item(ctx, &id)?;
        let copied = poll(Duration::from_secs(4), || win::clipboard_files().filter(|files| *files == [file.clone()]))?;
        expect_eq("files on the clipboard (CF_HDROP)", Some(vec![file.clone()]), copied.or_else(win::clipboard_files))?;
        ctx.quit()?;
        expect_eq("files on the clipboard after Traydex quits", Some(vec![file]), win::clipboard_files())?;
        Ok(())
    });

    s.run("clipboard-note", |ctx| {
        ctx.reset()?;
        let text = "Заметка для буфера — E2E ✓";
        let item = app::note_item(ctx, text)?;
        let id = item["id"].as_str().unwrap_or_default().to_string();
        ctx.start(app::settings(ctx), vec![item], none(), &[])?;
        win::clear_clipboard();
        copy_item(ctx, &id)?;
        let copied = poll(Duration::from_secs(4), || win::clipboard_text().filter(|copied| copied == text))?;
        expect_eq(
            "text on the clipboard (CF_UNICODETEXT)",
            Some(text.to_string()),
            copied.or_else(win::clipboard_text),
        )?;
        ctx.quit()?;
        expect_eq("text on the clipboard after Traydex quits", Some(text.to_string()), win::clipboard_text())?;
        Ok(())
    });
}
