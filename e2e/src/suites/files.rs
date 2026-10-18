use super::dnd::{center, drag_into};
use super::{Session, Spec, Suite, none, pattern, same_file, spec, write};
use crate::helper::{self, Payload};
use crate::{Ctx, app, expect_eq, mismatch, pause, poll, skip, wait, win};
use anyhow::{Context, Result};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

pub const SUITE: Suite = Suite { id: "files", title: "Files, preview and processes", specs, run };

fn specs() -> Vec<Spec> {
    vec![
        spec("copy-tile", "The \"copy to folder\" tile (SHFileOperation): copies in place, sources intact", true),
        spec("preview-unlocks", "After a preview (Space) the file isn't locked and can be deleted", false),
        spec("run-tile", "The run-program tile passes paths with spaces and Cyrillic unchanged", true),
        spec("ffmpeg-gif", "The GIF tile creates an animation with ffmpeg", true),
        spec("hash-tile", "The hash tile puts SHA-256 on the clipboard and shows the result page", true),
        spec("ocr-tile", "The text recognition tile (Windows OCR) puts the image text on the clipboard", true),
        spec("rename-tile", "The rename tile: the rule is picked after the drop, files are renamed", true),
        spec("qr-tile", "The QR code tile shows a code for a .url shortcut", true),
    ]
}

fn hidden(ctx: &Ctx, actions: serde_json::Value) -> Result<()> {
    ctx.reset()?;
    ctx.start(app::settings(ctx), vec![], actions, &["--autostart"])?;
    pause(500)
}

fn drop_files(ctx: &Ctx, files: &[PathBuf]) -> Result<()> {
    helper::arm(Payload::Files(files.to_vec()));
    drag_into(ctx, center(ctx.layout.helper), "tile:0")
}

fn clipboard(what: &str, timeout: Duration, ready: impl Fn(&str) -> bool) -> Result<String> {
    wait(what, timeout, || win::clipboard_text().filter(|text| ready(text)))
        .map_err(|_| mismatch(what, "text on the clipboard", win::clipboard_text().unwrap_or_default()))
}

fn listing(folder: &Path) -> Vec<String> {
    fs::read_dir(folder)
        .map(|entries| entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default()
}

fn run(s: &mut Session) {
    s.run("copy-tile", |ctx| {
        let source = ctx.sandbox.folder("copy-src")?;
        let target = ctx.sandbox.folder("copy-dst")?;
        let files = vec![
            write(&source.join("один файл с пробелами.txt"), "копирование плиткой".as_bytes())?,
            write(&source.join("data.bin"), &pattern(7, 65536))?,
        ];
        let originals: Vec<Vec<u8>> = files.iter().map(fs::read).collect::<Result<_, _>>()?;
        hidden(ctx, json!([{ "name": "E2E copy", "type": "copy", "folder": target }]))?;
        drop_files(ctx, &files)?;
        wait("copies appeared in the target folder", Duration::from_secs(10), || {
            files.iter().all(|file| same_file(file, &target.join(file.file_name().unwrap_or_default()))).then_some(())
        })
        .map_err(|_| {
            mismatch(
                "copies in the target folder",
                files
                    .iter()
                    .map(|f| f.file_name().unwrap_or_default().to_string_lossy().into_owned())
                    .collect::<Vec<_>>(),
                listing(&target),
            )
        })?;
        for (file, original) in files.iter().zip(originals) {
            expect_eq(&format!("source unchanged: {}", file.display()), Some(original), fs::read(file).ok())?;
        }
        Ok(())
    });

    s.run("preview-unlocks", |ctx| {
        ctx.reset()?;
        let folder = ctx.sandbox.folder("preview")?;
        let text = write(
            &folder.join("заметка для просмотра.txt"),
            "Предпросмотр текстового файла\r\nвторая строка".as_bytes(),
        )?;
        let picture = folder.join("picture.png");
        image::RgbaImage::from_fn(64, 48, |x, y| image::Rgba([(x * 4) as u8, (y * 5) as u8, 128, 255]))
            .save(&picture)?;
        let items = vec![app::path_item(&text), app::path_item(&picture)];
        let ids: Vec<String> = items.iter().map(|i| i["id"].as_str().unwrap_or_default().to_string()).collect();
        ctx.start(app::settings(ctx), items, none(), &[])?;
        for (id, path) in ids.iter().zip([&text, &picture]) {
            {
                let app = ctx.app()?;
                app.wait_diag("item drawn", Duration::from_secs(5), |d| {
                    d.page == "shelf" && d.has(&format!("item:{id}"))
                })?;
                app.post_click(&format!("item:{id}"))?;
                let panel = app.panel().context("panel not found")?;
                win::post_key(panel, windows::Win32::UI::Input::KeyboardAndMouse::VK_SPACE);
                app.wait_diag(&format!("preview of {} loaded", path.display()), Duration::from_secs(8), |d| {
                    d.page == "preview" && d.preview_loaded
                })?;
                pause(500)?;
                win::post_key(
                    app.panel().context("panel not found")?,
                    windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE,
                );
                app.wait_diag("preview closed", Duration::from_secs(5), |d| d.page == "shelf")?;
            }
            let deleted = poll(Duration::from_secs(3), || fs::remove_file(path).ok())?;
            if deleted.is_none() {
                let error = fs::remove_file(path).err().map(|e| e.to_string()).unwrap_or_default();
                return Err(mismatch(
                    &format!("the file can't be deleted after the preview: {}", path.display()),
                    "deletable",
                    error,
                ));
            }
        }
        Ok(())
    });

    s.run("run-tile", |ctx| {
        if ctx.powershell.is_none() {
            return Err(skip("PowerShell not found"));
        }
        let root = ctx.sandbox.folder("run")?;
        let output = root.join("args.txt");
        let script = root.join("args.ps1");
        let body = format!(
            "\u{feff}[IO.File]::WriteAllLines('{}', [string[]]$args, (New-Object Text.UTF8Encoding $false))\r\n",
            output.display().to_string().replace('\'', "''")
        );
        fs::write(&script, body.as_bytes())?;
        let nested = root.join("Папка с пробелами");
        let files = vec![
            write(&nested.join("Отчёт за 2024 год.txt"), b"one")?,
            write(&nested.join("file with  two spaces.txt"), b"two")?,
        ];
        hidden(ctx, json!([{ "name": "E2E run", "type": "run", "program": script }]))?;
        drop_files(ctx, &files)?;
        let lines = wait("the script got its arguments", Duration::from_secs(20), || {
            let text = fs::read_to_string(&output).ok()?;
            let lines: Vec<String> = text.lines().map(String::from).collect();
            (lines.len() == files.len()).then_some(lines)
        })?;
        let mut expected: Vec<String> = files.iter().map(|f| f.display().to_string()).collect();
        let mut actual = lines;
        expected.sort();
        actual.sort();
        expect_eq("paths received by the program", expected, actual)?;
        Ok(())
    });

    s.run("ffmpeg-gif", |ctx| {
        let ffmpeg = ctx.ffmpeg.clone().ok_or_else(|| skip(format!("ffmpeg {}", crate::NO_FFMPEG)))?;
        let folder = ctx.sandbox.folder("gif")?;
        let video = folder.join("clip.mp4");
        let status = Command::new(&ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=1:size=96x64:rate=10",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&video)
            .status()
            .context("couldn't start ffmpeg")?;
        if !status.success() || !video.is_file() {
            return Err(skip("ffmpeg couldn't prepare the test video"));
        }
        hidden(ctx, json!([{ "type": "gif" }]))?;
        drop_files(ctx, std::slice::from_ref(&video))?;
        let gif = wait("GIF created", Duration::from_secs(60), || {
            fs::read_dir(&folder).ok()?.flatten().map(|e| e.path()).find(|p| {
                p.extension().is_some_and(|e| e.eq_ignore_ascii_case("gif"))
                    && fs::metadata(p).is_ok_and(|m| m.len() > 0)
            })
        })?;
        pause(500)?;
        let header = fs::read(&gif)?;
        expect_eq("GIF header", true, header.starts_with(b"GIF89a") || header.starts_with(b"GIF87a"))?;
        Ok(())
    });

    s.run("hash-tile", |ctx| {
        let folder = ctx.sandbox.folder("hash")?;
        let file = write(&folder.join("установщик abc.bin"), b"abc")?;
        hidden(ctx, json!([{ "type": "hash" }]))?;
        win::clear_clipboard();
        drop_files(ctx, std::slice::from_ref(&file))?;
        let sha256 = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        clipboard("SHA-256 on the clipboard", Duration::from_secs(10), |text| text == sha256)?;
        ctx.app()?.wait_diag("hash page", Duration::from_secs(5), |d| d.page == "hashes" && d.has("hash-card:0"))?;
        Ok(())
    });

    s.run("ocr-tile", |ctx| {
        let powershell = ctx.powershell.clone().ok_or_else(|| skip("PowerShell not found"))?;
        let folder = ctx.sandbox.folder("ocr")?;
        let picture = folder.join("скриншот с текстом.png");
        let script = format!(
            "Add-Type -AssemblyName System.Drawing; $b = New-Object Drawing.Bitmap 900,160; $g = [Drawing.Graphics]::FromImage($b); $g.Clear([Drawing.Color]::White); $g.DrawString('48151623 Traydex', (New-Object Drawing.Font 'Arial',48), [Drawing.Brushes]::Black, 20, 30); $b.Save('{}')",
            picture.display().to_string().replace('\'', "''")
        );
        let status = Command::new(&powershell).args(["-NoProfile", "-Command", &script]).status()?;
        if !status.success() || !picture.is_file() {
            return Err(skip("PowerShell couldn't draw the picture with text"));
        }
        hidden(ctx, json!([{ "type": "ocr" }]))?;
        win::clear_clipboard();
        drop_files(ctx, std::slice::from_ref(&picture))?;
        clipboard("recognized text on the clipboard", Duration::from_secs(20), |text| text.replace(' ', "").contains("48151623"))?;
        Ok(())
    });

    s.run("rename-tile", |ctx| {
        let folder = ctx.sandbox.folder("rename")?;
        let files = vec![write(&folder.join("Привет мир.txt"), b"one")?, write(&folder.join("Ёлка.txt"), b"two")?];
        hidden(ctx, json!([{ "type": "rename" }]))?;
        drop_files(ctx, &files)?;
        let app = ctx.app()?;
        app.wait_diag("rename page", Duration::from_secs(8), |d| d.page == "rename" && d.has("rename-rule:2"))?;
        app.post_click("rename-rule:2")?;
        pause(300)?;
        app.post_click("rename-apply")?;
        let renamed = [folder.join("Privet mir.txt"), folder.join("Yolka.txt")];
        wait("files renamed", Duration::from_secs(8), || renamed.iter().all(|path| path.is_file()).then_some(()))
            .map_err(|_| mismatch("names after renaming", ["Privet mir.txt", "Yolka.txt"], listing(&folder)))?;
        expect_eq("contents preserved", Some(b"one".to_vec()), fs::read(&renamed[0]).ok())?;
        expect_eq("no old names left", false, files.iter().any(|file| file.exists()))?;
        Ok(())
    });

    s.run("qr-tile", |ctx| {
        let folder = ctx.sandbox.folder("qr")?;
        let shortcut = write(&folder.join("Ссылка.url"), b"[InternetShortcut]\r\nURL=https://example.com/traydex\r\n")?;
        hidden(ctx, json!([{ "type": "qr" }]))?;
        drop_files(ctx, std::slice::from_ref(&shortcut))?;
        ctx.app()?.wait_diag("QR code page", Duration::from_secs(8), |d| d.page == "qr" && d.has("qr-code"))?;
        Ok(())
    });
}
