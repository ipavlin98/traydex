use super::{Ended, JobKind, Qr, Renaming, Screen, Shelf};
use crate::actions::{self, Batch, Digest, Lookup, Rule, Single, Slot, Verdict};
use crate::i18n::{fill, tr};
use crate::input::{Changed, Submitted, TextInput};
use crate::store::{self, Item, Kind};
use gpui::*;
use qrcodegen::{QrCode, QrCodeEcc};
use std::fs;
use std::path::PathBuf;

pub const QR_COPY: &str = "qr-copy";
const QR_TITLE: usize = 40;

impl Shelf {
    pub fn copy_text(&mut self, text: String, origin: SharedString, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.flash_copied(origin, cx);
    }

    pub(super) fn open_hashes(&mut self, digests: Vec<Digest>, lookup: Option<Lookup>, cx: &mut Context<Self>) {
        let checking = digests.clone();
        self.show(Screen::Hashes(digests), cx);
        let Some(lookup) = lookup else {
            return;
        };
        let checked = checking.clone();
        let services = self.services.clone();
        self.run_job(
            cx,
            JobKind::Other,
            tr().vt_checking.into(),
            None,
            move |progress| Ok(lookup.verdicts(&checking, progress, &*services)),
            move |shelf, _, ended, cx| match ended {
                Ended::Finished(Ok(verdicts)) => shelf.show_verdicts(&checked, verdicts, cx),
                Ended::Finished(Err(error)) => shelf.fail(format!("{error:#}"), cx),
                Ended::Stopped => {
                    let stopped = checked.iter().map(|_| Verdict::Failed(tr().stopped.into())).collect();
                    shelf.show_verdicts(&checked, stopped, cx);
                }
            },
        );
    }

    fn show_verdicts(&mut self, checked: &[Digest], verdicts: Vec<Verdict>, cx: &mut Context<Self>) {
        let Screen::Hashes(shown) = &mut self.screen else {
            return;
        };
        if !shown.iter().map(|digest| &digest.sha256).eq(checked.iter().map(|digest| &digest.sha256)) {
            return;
        }
        for (digest, verdict) in shown.iter_mut().zip(verdicts) {
            digest.verdict = Some(verdict);
        }
        cx.notify();
    }

    pub fn hash_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tile) = self.tiles.find(&Slot::Single(Single::Hash)) {
            self.open_editor(tile, Some(actions::Field::ApiKey), window, cx);
        }
    }

    pub(super) fn open_qr(&mut self, text: String, cx: &mut Context<Self>) {
        match QrCode::encode_text(&text, QrCodeEcc::Medium) {
            Ok(code) => self.show(Screen::Qr(Qr { text, code, image: None }), cx),
            Err(error) => self.fail(error.to_string(), cx),
        }
    }

    fn with_qr_item(
        &mut self,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, Item, &mut Context<Self>) + 'static,
    ) {
        let Screen::Qr(qr) = &self.screen else {
            return self.fail(tr().qr_needs_text, cx);
        };
        let (text, code, cached) = (qr.text.clone(), qr.code.clone(), qr.image.clone());
        let storage = self.store.storage().clone();
        self.background(
            cx,
            move || {
                if let Some(path) = cached.filter(|path| path.is_file()) {
                    return Ok(path);
                }
                let path = storage.unique("qr", "png");
                if let Err(error) = actions::qr_png(&code).and_then(|png| Ok(fs::write(&path, png)?)) {
                    storage.discard(&path);
                    return Err(error);
                }
                Ok(path)
            },
            move |shelf, written, cx| {
                let path = match written {
                    Ok(path) => path,
                    Err(error) => return shelf.fail(error.to_string(), cx),
                };
                let Screen::Qr(qr) = &mut shelf.screen else {
                    return;
                };
                if qr.text != text {
                    return;
                }
                qr.image = Some(path.clone());
                then(shelf, qr_item(&text, path), cx);
            },
        );
    }

    pub fn copy_qr(&mut self, cx: &mut Context<Self>) {
        self.with_qr_item(cx, |shelf, item, cx| shelf.copy(vec![item], QR_COPY, cx));
    }

    pub fn keep_qr(&mut self, cx: &mut Context<Self>) {
        self.with_qr_item(cx, |shelf, item, cx| {
            let count = shelf.store.add_image(item.path, item.title);
            shelf.added(count, cx);
        });
    }

    pub(super) fn open_rename(&mut self, batch: Batch, cx: &mut Context<Self>) {
        let name = cx.new(|cx| TextInput::new(String::new(), tr().rename_placeholder, false, cx));
        let subscriptions = vec![
            cx.subscribe(&name, |shelf, _, _: &Changed, cx| shelf.replan(cx)),
            cx.subscribe(&name, |shelf, _, _: &Submitted, cx| shelf.rename_files(cx)),
        ];
        let rule = Rule::Date;
        let (plan, ready) = (batch.plan(rule, ""), batch.ready(rule, ""));
        self.show(Screen::Rename(Renaming { batch, rule, name, plan, ready, _subscriptions: subscriptions }), cx);
    }

    fn replan(&mut self, cx: &mut Context<Self>) {
        let Screen::Rename(renaming) = &mut self.screen else {
            return;
        };
        let name = renaming.name.read(cx).text();
        renaming.plan = renaming.batch.plan(renaming.rule, name);
        renaming.ready = renaming.batch.ready(renaming.rule, name);
        cx.notify();
    }

    pub fn set_rule(&mut self, rule: Rule, window: &mut Window, cx: &mut Context<Self>) {
        if let Screen::Rename(renaming) = &mut self.screen {
            renaming.rule = rule;
            if rule == Rule::Name {
                renaming.name.read(cx).focus(window);
            }
            self.replan(cx);
        }
    }

    pub fn rename_files(&mut self, cx: &mut Context<Self>) {
        let Some(plan) = self.renaming().filter(|renaming| renaming.ready).map(|renaming| renaming.plan.clone()) else {
            return;
        };
        self.background(
            cx,
            move || actions::rename_all(plan),
            |shelf, result, cx| match result {
                Ok(done) => {
                    shelf.store.relocate(&done);
                    shelf.navigate(Screen::Shelf, cx);
                    shelf.items_changed(cx);
                    shelf.show_toast(fill(tr().renamed, done.len()), cx);
                }
                Err(error) => shelf.fail(format!("{}: {error:#}", tr().add_rename), cx),
            },
        );
    }
}

fn qr_item(text: &str, path: PathBuf) -> Item {
    let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(QR_TITLE).collect();
    Item {
        id: store::ItemId::new(),
        kind: Kind::Image,
        title: format!("QR · {text}"),
        path,
        text: None,
        managed: true,
        named: false,
        pinned: false,
    }
}
