use super::*;

const STATUS_MARK: f32 = glyph::S;

impl Shelf {
    fn path_row(
        &self,
        t: Theme,
        caption: &'static str,
        tile: TileId,
        path: String,
        text_width: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        labelled(t, caption).child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(rem(space::M))
                .child(div().w(rem(text_width)).truncate().text_color(t.muted).child(path))
                .child(
                    text_button(t, "change-path", tr().change, Look::Plain)
                        .on_click(cx.safe_listener(move |shelf, _, _, cx| shelf.change_path(tile, cx))),
                ),
        )
    }

    fn engines(&self, t: Theme, tile: TileId, current: &str, cx: &mut Context<Self>) -> Div {
        let custom = engine_name(current).is_none();
        let options = ENGINES.into_iter().map(|url| (url, engine_name(url).unwrap_or_default()));
        let cells: Vec<_> = options
            .chain([("", tr().search_custom)])
            .enumerate()
            .map(|(n, (url, name))| {
                let active = if custom { url.is_empty() } else { url == current.trim() };
                let slot = Action::new(Kind::Search { url: url.into() }).slot();
                let taken = self.tiles().taken(&slot, Some(tile));
                choice(t, ("engine", n), name, active, !taken).when(!active && !taken, |d| {
                    d.on_click(cx.safe_listener(move |shelf, _, window, cx| shelf.set_engine(tile, url, window, cx)))
                })
            })
            .collect();
        labelled(t, tr().search_engine).child(div().flex().flex_col().gap(rem(space::S)).children(grid(
            cells,
            ENGINE_COLUMNS,
            space::S,
        )))
    }

    fn ocr_languages(&self, t: Theme, tile: TileId, current: &str, cx: &mut Context<Self>) -> Div {
        let installed = self.editor().map(|editor| editor.languages.clone()).unwrap_or_default();
        let missing = !current.is_empty() && !installed.iter().any(|(tag, _)| tag == current);
        let choices: Vec<(String, String)> = [(String::new(), tr().ocr_language_auto.to_string())]
            .into_iter()
            .chain(installed)
            .chain(missing.then(|| (current.to_string(), current.to_string())))
            .collect();
        let cells: Vec<_> = choices
            .into_iter()
            .enumerate()
            .map(|(n, (tag, name))| {
                let active = tag == current;
                choice(t, ("ocr-language", n), name, active, true).marked(format!("ocr-language:{n}")).when(
                    !active,
                    |d| {
                        d.on_click(cx.safe_listener(move |shelf, _, _, cx| {
                            let tag = tag.clone();
                            shelf.edit_kind(tile, cx, |kind| {
                                if let Kind::Ocr { language } = kind {
                                    *language = tag;
                                }
                            })
                        }))
                    },
                )
            })
            .collect();
        labelled(t, tr().ocr_language)
            .child(div().marked("ocr-languages").flex().flex_col().gap(rem(space::S)).children(grid(
                cells,
                LANGUAGE_COLUMNS,
                space::S,
            )))
            .child(div().flex().child(
                link(t, "ocr-language-add", tr().ocr_language_add, WINDOWS_LANGUAGES, cx).marked("ocr-language-add"),
            ))
    }

    fn extras(&self, t: Theme, tile: TileId, action: &Action, text_width: f32, cx: &mut Context<Self>) -> Option<Div> {
        let path = || action.path().map(|p| p.display().to_string()).unwrap_or_default();
        Some(match action.kind {
            Kind::Move { .. } | Kind::Copy { .. } => {
                let copy = matches!(action.kind, Kind::Copy { .. });
                div()
                    .flex()
                    .flex_col()
                    .gap(rem(space::XL))
                    .child(self.path_row(t, tr().add_folder, tile, path(), text_width, cx))
                    .child(options(
                        t,
                        tr().mode,
                        vec![
                            segment(t, "mode-move", tr().verb_move, !copy)
                                .on_click(cx.safe_listener(move |shelf, _, _, cx| shelf.set_copy(tile, false, cx))),
                            segment(t, "mode-copy", tr().verb_copy, copy)
                                .on_click(cx.safe_listener(move |shelf, _, _, cx| shelf.set_copy(tile, true, cx))),
                        ],
                    ))
            }
            Kind::Hash { algorithm, .. } => options(
                t,
                tr().hash_copy_label,
                [Algorithm::Sha256, Algorithm::Md5]
                    .into_iter()
                    .map(|choice| {
                        segment(t, choice.label(), choice.label(), algorithm == choice).on_click(cx.safe_listener(
                            move |shelf, _, _, cx| {
                                shelf.edit_kind(tile, cx, |kind| {
                                    if let Kind::Hash { algorithm, .. } = kind {
                                        *algorithm = choice;
                                    }
                                })
                            },
                        ))
                    })
                    .collect(),
            ),
            Kind::Compress { format } => options(
                t,
                tr().compress_format,
                [(Compression::Jpeg, "JPEG (MozJPEG)"), (Compression::Webp, "WebP")]
                    .into_iter()
                    .map(|(choice, text)| {
                        segment(t, choice.label(), text, format == choice).on_click(cx.safe_listener(
                            move |shelf, _, _, cx| {
                                shelf.edit_kind(tile, cx, |kind| {
                                    if let Kind::Compress { format } = kind {
                                        *format = choice;
                                    }
                                })
                            },
                        ))
                    })
                    .collect(),
            ),
            Kind::Run { .. } => self.path_row(t, tr().program_label, tile, path(), text_width, cx),
            Kind::Search { ref url } => self.engines(t, tile, url, cx),
            Kind::Ocr { ref language } => self.ocr_languages(t, tile, language, cx),
            Kind::Imgur { .. } => div().flex().child(link(t, "client-id", tr().get_client_id, IMGUR_REGISTER, cx)),
            Kind::Custom(ref custom) => return Some(self.plugin_settings(t, tile, custom, cx)),
            Kind::Cancel => div()
                .marked("cancel-note")
                .text_size(rem(text::SMALL))
                .line_height(rem(line::SMALL))
                .text_color(t.muted)
                .child(tr().cancel_tile_note),
            _ => return None,
        })
    }

    fn icon_only_row(&self, t: Theme, tile: TileId, on: bool, cx: &mut Context<Self>) -> Div {
        div()
            .marked("icon-only")
            .flex()
            .items_center()
            .justify_between()
            .gap(rem(space::APART))
            .child(div().flex_1().min_w_0().truncate().child(tr().icon_only))
            .child(
                switch(t, "icon-only", on)
                    .on_click(cx.safe_listener(move |shelf, _, _, cx| shelf.set_icon_only(tile, !on, cx))),
            )
    }

    fn setting_input(&self, key: &str) -> Option<Entity<TextInput>> {
        let settings = &self.editor()?.settings;
        settings.iter().find(|(known, _)| known == key).map(|(_, input)| input.clone())
    }

    fn plugin_settings(&self, t: Theme, tile: TileId, custom: &Custom, cx: &mut Context<Self>) -> Div {
        let Some(plugin) = custom.plugin() else {
            return div()
                .marked("plugin-missing")
                .flex()
                .items_start()
                .gap(rem(space::S))
                .text_size(rem(text::SMALL))
                .line_height(rem(line::SMALL))
                .text_color(t.danger)
                .child(icon("alert", t.danger, glyph::S))
                .child(div().flex_1().min_w_0().child(fill(tr().plugin_missing, &custom.id)));
        };
        let about = (!plugin.description().is_empty()).then(|| {
            div()
                .marked("plugin-about")
                .text_size(rem(text::SMALL))
                .line_height(rem(line::SMALL))
                .text_color(t.muted)
                .child(plugin.description().to_string())
        });
        let rows: Vec<Div> = plugin
            .settings()
            .iter()
            .enumerate()
            .map(|(index, setting)| self.setting_row(t, tile, custom, (index, setting), cx))
            .collect();
        div().flex().flex_col().gap(rem(space::XL)).children(about).children(rows)
    }

    fn setting_row(
        &self,
        t: Theme,
        tile: TileId,
        custom: &Custom,
        (index, setting): (usize, &Setting),
        cx: &mut Context<Self>,
    ) -> Div {
        let (mark, key) = (format!("setting:{index}"), setting.key.clone());
        let field = self.setting_input(&setting.key);
        match setting.input {
            Input::Toggle => {
                let on = custom.value(setting).as_bool().unwrap_or_default();
                div()
                    .marked(mark.clone())
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(rem(space::APART))
                    .child(div().flex_1().min_w_0().truncate().child(setting.label.clone()))
                    .child(switch(t, mark, on).on_click(
                        cx.safe_listener(move |shelf, _, _, cx| shelf.set_value(tile, &key, Value::Bool(!on), cx)),
                    ))
            }
            Input::File | Input::Folder => {
                let folders = setting.input == Input::Folder;
                let pick = text_button(t, ("pick-setting", index), tr().change, Look::Plain)
                    .marked(format!("pick-setting:{index}"))
                    .on_click(
                        cx.safe_listener(move |shelf, _, _, cx| shelf.pick_value(tile, key.clone(), folders, cx)),
                    );
                labelled(t, setting.label.clone()).marked(mark).child(
                    div()
                        .flex()
                        .items_center()
                        .gap(rem(space::M))
                        .child(div().flex_1().min_w_0().children(field))
                        .child(pick),
                )
            }
            Input::Text | Input::Secret => labelled(t, setting.label.clone()).marked(mark).children(field),
        }
    }

    fn tool_status(&self, t: Theme, window: &Window, tool: Tool, place: Slot, cx: &mut Context<Self>) -> Div {
        let job = self.tool_job(tool, cx).map(|job| (job.label.clone(), job.percent()));
        let name = format!("tool:{}", tool.name());
        let status = if let Some((label, percent)) = job {
            let column = snap_up(measure(window, "100%", PERCENT_TEXT), crate::design::scale(window));
            notice(
                &name,
                spinner(t.accent, STATUS_MARK),
                label,
                div()
                    .flex()
                    .items_center()
                    .gap(rem(space::S))
                    .child(
                        div()
                            .w(rem(column))
                            .text_right()
                            .text_size(rem(PERCENT_TEXT))
                            .text_color(t.muted)
                            .children(percent),
                    )
                    .child(
                        text_button(t, ("cancel-tool", tool as usize), tr().cancel, Look::Plain)
                            .on_click(cx.safe_listener(move |shelf, _, _, cx| shelf.cancel_install(tool, cx))),
                    ),
            )
        } else {
            match self.presence(tool) {
                Presence::Missing => notice(
                    &name,
                    leading_icon("alert", t.danger, STATUS_MARK),
                    fill(tr().tool_missing, tool.name()),
                    text_button(t, ("install-tool", tool as usize), tr().download, Look::Plain)
                        .on_click(cx.safe_listener(move |shelf, _, _, cx| shelf.install(tool, place.clone(), cx))),
                ),
                Presence::Bundled => notice(
                    &name,
                    leading_icon("check", t.accent_text, STATUS_MARK),
                    fill(tr().tool_ready, tool.name()),
                    row_button(t, ("remove-tool", tool as usize), "trash", fill(tr().tool_remove, tool.name()))
                        .on_click(cx.safe_listener(move |shelf, _, _, cx| shelf.remove_tool(tool, cx))),
                ),
                Presence::System => notice(
                    &name,
                    leading_icon("check", t.accent_text, STATUS_MARK),
                    fill(tr().tool_ready, tool.name()),
                    div(),
                ),
            }
        };
        labelled(t, tool.name()).marked(name).child(div().min_h(rem(size::CONTROL)).flex().items_center().child(status))
    }

    fn plugin_status(&self, t: Theme, action: &Action, cx: &mut Context<Self>) -> Option<Div> {
        let Kind::Custom(custom) = &action.kind else {
            return None;
        };
        let plugin = custom.plugin()?;
        let id = custom.id.clone();
        let uninstall = row_button(t, "uninstall-plugin", "trash", tr().plugin_uninstall)
            .marked("uninstall-plugin")
            .on_click(cx.safe_listener(move |shelf, _, _, cx| shelf.ask_uninstall(&id, cx)));
        Some(div().marked("plugin-status").min_h(rem(size::CONTROL)).flex().items_center().child(notice(
            "plugin-status",
            leading_icon("check", t.accent_text, STATUS_MARK),
            fill(tr().plugin_installed, plugin.name()),
            uninstall,
        )))
    }

    pub(in crate::view) fn editor_page(
        &self,
        t: Theme,
        window: &Window,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((editor, action)) = self.editor().and_then(|editor| Some((editor, self.tiles().get(editor.tile)?)))
        else {
            return self.actions_page(t, width, cx);
        };
        let tile = editor.tile;
        let mut fields: Vec<Div> = editor
            .inputs
            .iter()
            .enumerate()
            .map(|(index, (field, input))| {
                if *field != Field::ApiKey {
                    return labelled(t, field.label()).marked(format!("field:{index}")).child(input.clone());
                }
                div()
                    .marked(format!("field:{index}"))
                    .flex()
                    .flex_col()
                    .p(rem(space::L))
                    .gap(rem(space::M))
                    .rounded(rem(radius::CONTROL))
                    .border_1()
                    .border_color(t.border)
                    .bg(t.bg)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(rem(space::LABEL))
                            .child(icon("shield", t.muted, glyph::S))
                            .child(label(t, field.label())),
                    )
                    .child(input.clone())
                    .child(div().flex().child(link(t, "virustotal-join", tr().virustotal_get_key, VIRUSTOTAL_JOIN, cx)))
            })
            .collect();
        let labels = editor.inputs.iter().position(|(field, _)| *field == Field::Description).map_or(0, |at| at + 1);
        fields.insert(labels.min(fields.len()), self.icon_only_row(t, tile, action.icon_only, cx));
        let extras = self.extras(t, tile, action, width - EDITOR_CHROME, cx);
        let ready = self.ready(action);
        let content = page("editor")
            .child(
                div().flex().child(
                    div()
                        .id("back")
                        .marked("back")
                        .flex()
                        .items_center()
                        .gap(rem(space::X2S))
                        .text_size(rem(text::SMALL))
                        .text_color(t.accent_text)
                        .cursor_pointer()
                        .child(leading_icon("chevron-left", t.accent_text, glyph::S))
                        .child(div().relative().left(rem(-inset("chevron-left", glyph::S))).child(tr().actions))
                        .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.toggle_page(Page::Actions, cx))),
                ),
            )
            .child(heading(
                t,
                "editor",
                action_icon(action),
                div().min_w_0().truncate().child(action.title()),
                div().min_w_0().truncate().when(!ready, |d| d.text_color(t.danger)).child(if ready {
                    action.description().into()
                } else {
                    SharedString::from(tr().needs_setup)
                }),
            ))
            .child(
                group(t)
                    .p(rem(space::GROUP))
                    .gap(rem(space::XL))
                    .children(fields)
                    .children(extras)
                    .children(action.tools().iter().map(|tool| self.tool_status(t, window, *tool, action.slot(), cx)))
                    .children(self.plugin_status(t, action, cx)),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(
                        text_button(t, "save-action", tr().save, Look::Plain)
                            .marked("save-action")
                            .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.toggle_page(Page::Actions, cx))),
                    )
                    .when(!action.cancels(), |d| {
                        d.child(
                            text_button(t, "delete-action", tr().remove_action, Look::Danger)
                                .marked("delete-action")
                                .on_click(cx.safe_listener(move |shelf, _, _, cx| shelf.remove_action(tile, cx))),
                        )
                    }),
            );
        self.scroll_area(t, Page::Edit, "editor-page", content)
    }
}
