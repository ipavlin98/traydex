use super::input::outer_corners;
use super::{Session, Spec, Suite, same_file, spec, write};
use crate::explorer::{self, Explorer};
use crate::helper::{self, Payload};
use crate::{Ctx, app, expect_eq, input, mismatch, pause, wait};
use anyhow::{Context, Result};
use serde_json::json;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_A, VK_CONTROL};

pub const SUITE: Suite = Suite { id: "dnd", title: "Drag and drop (OLE)", specs, run };

fn specs() -> Vec<Spec> {
    vec![
        spec(
            "explorer-to-shelf",
            "Files and a folder from Explorer land on the shelf, the panel shows up by itself",
            true,
        ),
        spec("auto-hide", "The panel shows up while dragging and hides after a drop elsewhere", true),
        spec("archive-tile", "Dropping a file on the archive tile: the archive is correct, the source is intact", true),
        spec(
            "plugin-bundle",
            "A .tdx plugin bundle from Explorer gets installed instead of landing on the shelf",
            true,
        ),
        spec("shelf-to-explorer", "Cards from the shelf are copied to Explorer", true),
        spec(
            "shelf-to-tile",
            "Dragging from the shelf opens the tiles, and an item dropped on a tile is processed",
            true,
        ),
        spec("text-source", "Text from a source window with a real DoDragDrop becomes a note", true),
        spec(
            "corner-drag",
            "Dragging into the hot corner area shows the panel even with auto-show off, and the drop works",
            true,
        ),
        spec(
            "right-click-hides",
            "A right click during a drag cancels it, hides even a pinned panel and doesn't reach Windows",
            true,
        ),
    ]
}

const CORNER_DRAG_ZONE: i32 = 440;
const PLUGIN_ID: &str = "e2e-plugin";

fn write_bundle(path: &Path) -> Result<()> {
    let manifest = json!({ "id": PLUGIN_ID, "name": "E2E plugin", "entry": "main.ps1" }).to_string();
    let mut writer = zip::ZipWriter::new(fs::File::create(path)?);
    for (name, data) in [("action.json", manifest.as_str()), ("main.ps1", "exit 0")] {
        writer.start_file(name, zip::write::SimpleFileOptions::default())?;
        writer.write_all(data.as_bytes())?;
    }
    writer.finish()?;
    Ok(())
}

pub(super) fn zip_tile() -> serde_json::Value {
    json!([{ "type": "zip" }])
}

pub(super) fn center(rect: RECT) -> POINT {
    POINT { x: (rect.left + rect.right) / 2, y: (rect.top + rect.bottom) / 2 }
}

fn offset(point: POINT, by: i32) -> POINT {
    POINT { x: point.x + by, y: point.y + by }
}

pub(super) fn drag_into(ctx: &Ctx, from: POINT, target: &str) -> Result<()> {
    input::press(from)?;
    for by in [4, 8, 12, 18, 24] {
        input::move_to(offset(from, by));
        pause(40)?;
    }
    let app = ctx.app()?;
    let mut tick = 0;
    wait("the panel appeared when the drag started", Duration::from_secs(6), || {
        tick += 1;
        input::move_to(offset(from, 24 + tick % 2));
        app.shown().then_some(())
    })?;
    let diag = wait("the panel showed the drop zone", Duration::from_secs(5), || {
        tick += 1;
        input::move_to(offset(from, 24 + tick % 2));
        app.diag().filter(|d| d.dropping && d.has(target))
    })?;
    let point = app.point(&diag, target)?;
    input::glide(point, 24, Duration::from_millis(20))?;
    wait(&format!("cursor over \"{target}\""), Duration::from_secs(4), || {
        tick += 1;
        input::move_to(POINT { x: point.x + tick % 2, y: point.y });
        app.diag().filter(|d| d.over.as_deref() == Some(target))
    })?;
    pause(150)?;
    input::release(point)
}

struct Source {
    folder: PathBuf,
    alpha: PathBuf,
    beta: PathBuf,
    gamma: PathBuf,
}

fn source(ctx: &Ctx) -> Result<Source> {
    let folder = ctx.sandbox.folder("dnd-src")?;
    let alpha = write(&folder.join("alpha.txt"), &super::pattern(1, 3000))?;
    let beta = write(&folder.join("бета файл.txt"), "Проверка перетаскивания — бета".as_bytes())?;
    let gamma = folder.join("gamma");
    write(&gamma.join("inner.txt"), b"inner file")?;
    Ok(Source { folder, alpha, beta, gamma })
}

fn hidden(ctx: &Ctx) -> Result<()> {
    ctx.reset()?;
    ctx.start(app::settings(ctx), vec![], zip_tile(), &["--autostart"])?;
    pause(500)
}

fn run(s: &mut Session) {
    s.run("explorer-to-shelf", |ctx| {
        hidden(ctx)?;
        let src = source(ctx)?;
        let window = explorer::open(&src.folder, ctx.layout.explorer)?;
        let alpha = Explorer::center(window.item("alpha.txt")?);
        input::click(alpha)?;
        pause(300)?;
        input::chord(&[VK_CONTROL, VK_A])?;
        pause(input::double_click_time() + 200)?;
        let alpha = Explorer::center(window.item("alpha.txt")?);
        drag_into(ctx, alpha, "zone")?;
        let app = ctx.app()?;
        wait("three items from Explorer on the shelf", Duration::from_secs(8), || {
            let saved = app.saved_paths();
            [&src.alpha, &src.beta, &src.gamma].iter().all(|p| saved.contains(p)).then_some(())
        })
        .map_err(|_| {
            mismatch("items from Explorer on the shelf", [&src.alpha, &src.beta, &src.gamma], app.saved_paths())
        })?;
        Ok(())
    });

    s.run("auto-hide", |ctx| {
        hidden(ctx)?;
        let src = source(ctx)?;
        let window = explorer::open(&src.folder, ctx.layout.explorer)?;
        let alpha = Explorer::center(window.item("alpha.txt")?);
        input::press(alpha)?;
        for by in [4, 8, 12, 18, 24] {
            input::move_to(offset(alpha, by));
            pause(40)?;
        }
        let app = ctx.app()?;
        let mut tick = 0;
        wait("the panel appeared when the drag started", Duration::from_secs(6), || {
            tick += 1;
            input::move_to(offset(alpha, 24 + tick % 2));
            app.shown().then_some(())
        })?;
        let away = center(ctx.layout.helper);
        input::glide(away, 20, Duration::from_millis(20))?;
        pause(300)?;
        input::release(away)?;
        app.wait_shown(false, "the panel hid after a drop elsewhere", Duration::from_secs(6))?;
        expect_eq("nothing was added to the shelf", Vec::<PathBuf>::new(), app.saved_paths())?;
        Ok(())
    });

    s.run("archive-tile", |ctx| {
        hidden(ctx)?;
        let src = source(ctx)?;
        let original = fs::read(&src.alpha)?;
        let window = explorer::open(&src.folder, ctx.layout.explorer)?;
        let alpha = Explorer::center(window.item("alpha.txt")?);
        drag_into(ctx, alpha, "tile:0")?;
        let archive = src.folder.join("alpha.zip");
        let entries =
            wait("the archive was created next to the source", Duration::from_secs(10), || read_zip(&archive).ok())?;
        expect_eq("archive contents", vec![("alpha.txt".to_string(), original.clone())], entries)?;
        expect_eq("the source file didn't change", original, fs::read(&src.alpha)?)?;
        expect_eq("only the new archive is on the shelf", vec![archive.clone()], ctx.app()?.saved_paths())?;
        expect_eq("archive path on the clipboard", Some(archive.display().to_string()), crate::win::clipboard_text())?;
        Ok(())
    });

    s.run("plugin-bundle", |ctx| {
        hidden(ctx)?;
        let folder = ctx.sandbox.folder("dnd-plugin")?;
        write_bundle(&folder.join("plugin.tdx"))?;
        let window = explorer::open(&folder, ctx.layout.explorer)?;
        let bundle = Explorer::center(window.item("plugin.tdx")?);
        drag_into(ctx, bundle, "zone")?;
        let app = ctx.app()?;
        let manifest = app.data().join("actions").join("custom").join(PLUGIN_ID).join("action.json");
        wait("the plugin was unpacked into the actions folder", Duration::from_secs(8), || {
            manifest.is_file().then_some(())
        })?;
        expect_eq("the plugin bundle didn't land on the shelf", Vec::<PathBuf>::new(), app.saved_paths())?;
        Ok(())
    });

    s.run("shelf-to-explorer", |ctx| {
        ctx.reset()?;
        let src = source(ctx)?;
        let target = ctx.sandbox.folder("dnd-dst")?;
        let window = explorer::open(&target, ctx.layout.explorer)?;
        let drop = window.empty_point()?;
        let items = vec![app::path_item(&src.alpha), app::path_item(&src.beta), app::path_item(&src.gamma)];
        ctx.start(app::settings(ctx), items, zip_tile(), &[])?;
        let traydex = ctx.app()?;
        let diag = traydex.wait_diag("the panel has the \"drag everything\" button", Duration::from_secs(8), |d| {
            d.has("drag-all") && d.items.len() == 3
        })?;
        let grab = traydex.point(&diag, "drag-all")?;
        input::drag(grab, drop, 30)?;
        let copies = [
            (src.alpha.clone(), target.join("alpha.txt")),
            (src.beta.clone(), target.join("бета файл.txt")),
            (src.gamma.join("inner.txt"), target.join("gamma").join("inner.txt")),
        ];
        wait("the files and the folder were copied to Explorer", Duration::from_secs(10), || {
            copies.iter().all(|(from, to)| same_file(from, to)).then_some(())
        })
        .map_err(|_| {
            let present: Vec<String> = fs::read_dir(&target)
                .map(|e| e.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
                .unwrap_or_default();
            mismatch("copies in the target folder", ["alpha.txt", "бета файл.txt", "gamma\\inner.txt"], present)
        })?;
        for path in [&src.alpha, &src.beta, &src.gamma] {
            expect_eq(&format!("source still in place: {}", path.display()), true, path.exists())?;
        }
        Ok(())
    });

    s.run("shelf-to-tile", |ctx| {
        ctx.reset()?;
        let src = source(ctx)?;
        let original = fs::read(&src.alpha)?;
        ctx.start(app::settings(ctx), vec![app::path_item(&src.alpha)], zip_tile(), &[])?;
        let traydex = ctx.app()?;
        let diag = traydex.wait_diag("the shelf has the \"drag everything\" button", Duration::from_secs(8), |d| {
            d.has("drag-all") && d.items.len() == 1
        })?;
        let grab = traydex.point(&diag, "drag-all")?;
        drag_into(ctx, grab, "tile:0")?;
        let archive = src.folder.join("alpha.zip");
        let entries =
            wait("the archive was created next to the source", Duration::from_secs(10), || read_zip(&archive).ok())?;
        expect_eq("archive contents", vec![("alpha.txt".to_string(), original)], entries)?;
        Ok(())
    });

    s.run("text-source", |ctx| {
        hidden(ctx)?;
        let text = "E2E: текст из окна-источника — 42";
        helper::arm(Payload::Text(text.into()));
        drag_into(ctx, center(ctx.layout.helper), "zone")?;
        let app = ctx.app()?;
        let item = wait("the note appeared on the shelf", Duration::from_secs(6), || {
            app.diag().and_then(|d| d.items.into_iter().find(|i| i.kind == "Text"))
        })?;
        let stored =
            item.text.clone().or_else(|| fs::read_to_string(&item.path).ok()).context("note text not found")?;
        expect_eq("note text", text.to_string(), stored)?;
        let effect = wait("the source got the drop result", Duration::from_secs(3), helper::effect)?;
        expect_eq("the drop was accepted as a copy", 1, effect)?;
        Ok(())
    });

    s.run("corner-drag", |ctx| {
        let from = center(ctx.layout.helper);
        let reach = |tip: &POINT| (tip.x - from.x).abs().max((tip.y - from.y).abs());
        let (corner, tip, (dx, dy)) = outer_corners()
            .into_iter()
            .max_by_key(|(_, tip, _)| reach(tip))
            .context("the primary monitor has no outer corner (every corner borders another monitor)")?;
        let zone = CORNER_DRAG_ZONE * ctx.layout.dpi as i32 / 96;
        anyhow::ensure!(
            reach(&tip) > zone,
            "the drag source is in the area near corner {corner}, the screen is too small"
        );
        ctx.reset()?;
        let mut settings = app::settings(ctx);
        settings["automatic"] = json!(false);
        settings["corner"] = corner.into();
        settings["corner_drag"] = json!(true);
        ctx.start(settings, vec![], zip_tile(), &["--autostart"])?;
        pause(500)?;
        let text = "E2E: drag into the hot corner";
        helper::arm(Payload::Text(text.into()));
        input::press(from)?;
        for by in [4, 8, 12, 18, 24] {
            input::move_to(offset(from, by));
            pause(40)?;
        }
        pause(600)?;
        let app = ctx.app()?;
        expect_eq("without auto-show the drag didn't show the panel by itself", false, app.shown())?;
        let inward = 300 * ctx.layout.dpi as i32 / 96;
        let area = POINT { x: tip.x - dx * inward, y: tip.y - dy * inward };
        input::glide(area, 30, Duration::from_millis(15))?;
        let mut tick = 0;
        wait(
            &format!("the area near hot corner {corner} showed the panel during the drag"),
            Duration::from_secs(4),
            || {
                tick += 1;
                input::move_to(POINT { x: area.x + tick % 2, y: area.y });
                app.shown().then_some(())
            },
        )?;
        let diag = wait("the panel showed the drop zone", Duration::from_secs(5), || {
            app.diag().filter(|d| d.dropping && d.has("zone"))
        })?;
        let point = app.point(&diag, "zone")?;
        input::glide(point, 24, Duration::from_millis(20))?;
        wait("cursor over \"zone\"", Duration::from_secs(4), || {
            tick += 1;
            input::move_to(POINT { x: point.x + tick % 2, y: point.y });
            app.diag().filter(|d| d.over.as_deref() == Some("zone"))
        })?;
        pause(150)?;
        input::release(point)?;
        let item = wait("the note appeared on the shelf", Duration::from_secs(6), || {
            app.diag().and_then(|d| d.items.into_iter().find(|i| i.kind == "Text"))
        })?;
        let stored =
            item.text.clone().or_else(|| fs::read_to_string(&item.path).ok()).context("note text not found")?;
        expect_eq("note text", text.to_string(), stored)?;
        Ok(())
    });

    s.run("right-click-hides", |ctx| {
        ctx.reset()?;
        let mut settings = app::settings(ctx);
        settings["pinned"] = json!(true);
        ctx.start(settings, vec![], zip_tile(), &["--autostart"])?;
        pause(500)?;
        helper::arm(Payload::Text("E2E: cancel with the right button".into()));
        let from = center(ctx.layout.helper);
        input::press(from)?;
        for by in [4, 8, 12, 18, 24] {
            input::move_to(offset(from, by));
            pause(40)?;
        }
        let app = ctx.app()?;
        let mut tick = 0;
        wait("the panel appeared when the drag started", Duration::from_secs(6), || {
            tick += 1;
            input::move_to(offset(from, 24 + tick % 2));
            app.shown().then_some(())
        })?;
        input::right_click()?;
        app.wait_shown(false, "the panel hid on the right click", Duration::from_secs(4))?;
        let effect = wait("the drag was cancelled with the left button held", Duration::from_secs(4), helper::effect)?;
        expect_eq("the drag was cancelled without a drop", 0, effect)?;
        input::move_to(offset(from, 24));
        input::release(offset(from, 24))?;
        pause(300)?;
        expect_eq("the right click didn't reach the window under the cursor", 0, helper::right_button_messages())?;
        expect_eq("nothing was added to the shelf", Vec::<PathBuf>::new(), app.saved_paths())?;
        Ok(())
    });
}

pub(super) fn read_zip(path: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    let mut archive = zip::ZipArchive::new(fs::File::open(path)?)?;
    let mut entries = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        entries.push((file.name()?.to_string(), bytes));
    }
    Ok(entries)
}
