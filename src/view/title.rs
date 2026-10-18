use super::*;

impl Shelf {
    pub(super) fn title_bar(&self, t: Theme, cx: &mut Context<Self>) -> Div {
        let caption = Rc::new(Cell::new(Caption { height: TITLE_HEIGHT, ..Caption::default() }));
        let record_caption = self.caption_sink();
        div()
            .marked("title-bar")
            .h(rem(TITLE_HEIGHT))
            .flex_shrink_0()
            .flex()
            .items_center()
            .bg(t.surface)
            .border_b_1()
            .border_color(t.border)
            .child(self.title_home(t, caption.clone(), cx))
            .child(self.title_buttons(t, caption.clone(), cx))
            .child(tiles::probe(move |_| record_caption(caption.get())))
    }

    fn crumb(&self) -> Option<&'static str> {
        match self.page() {
            Page::Shelf => None,
            Page::Actions | Page::Edit => Some(tr().actions),
            Page::Settings => Some(tr().settings),
            Page::Preview => Some(tr().preview),
            Page::Share => Some(tr().add_phone),
            Page::Hashes => Some(tr().add_hash),
            Page::Qr => Some(tr().add_qr),
            Page::Rename => Some(tr().add_rename),
            Page::Media => Some(tr().add_ytdlp),
            Page::Password => Some(tr().add_unpack),
        }
    }

    fn title_home(&self, t: Theme, leading: Rc<Cell<Caption>>, cx: &mut Context<Self>) -> Div {
        let crumb = self.crumb();
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .pl(rem(space::PAGE))
            .flex()
            .items_center()
            .gap(rem(space::S))
            .child(
                div()
                    .id("home")
                    .marked("home")
                    .relative()
                    .h_full()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(rem(space::S))
                    .cursor_pointer()
                    .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.toggle_page(Page::Shelf, cx)))
                    .child(tiles::probe(move |bounds| {
                        leading.set(Caption { leading: (bounds.left().into(), bounds.right().into()), ..leading.get() })
                    }))
                    .child(
                        div()
                            .relative()
                            .size(rem(glyph::M))
                            .flex_shrink_0()
                            .child(icon("brand", t.brand, glyph::M).absolute().top_0().left_0())
                            .child(icon("brand-arrow", t.surface, glyph::M).absolute().top_0().left_0()),
                    )
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Traydex")),
            )
            .child(
                div()
                    .id("caption")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(rem(space::S))
                    .when_some(crumb, |d, crumb| {
                        d.child(div().text_color(t.faint).child("/")).child(div().text_color(t.muted).child(crumb))
                    })
                    .tooltip(hint(
                        t,
                        Tip::keys(tr().default_size, &[tr().double_click])
                            .line(Tip::keys(tr().fit_size, &[tr().double_right_click])),
                    )),
            )
    }

    fn title_buttons(&self, t: Theme, trailing: Rc<Cell<Caption>>, cx: &mut Context<Self>) -> Div {
        let page = self.page();
        let settings = page == Page::Settings;
        let actions = matches!(page, Page::Actions | Page::Edit);
        let pinned = self.store().settings().pinned;
        div()
            .marked("title-buttons")
            .relative()
            .h_full()
            .pr(rem(space::PAGE))
            .flex()
            .items_center()
            .gap(rem(BUTTON_GAP))
            .child(tiles::probe(move |bounds| {
                trailing.set(Caption { trailing: bounds.size.width.into(), ..trailing.get() })
            }))
            .when(self.jobs(cx).last().is_some(), |d| {
                d.child(
                    icon_button(t, "jobs-stop", "stop", tr().stop_jobs, false)
                        .marked("jobs-stop")
                        .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.stop_jobs(cx))),
                )
            })
            .when_some(self.job_indicator(t, cx), |d, indicator| d.child(indicator))
            .when_some(self.alert_indicator(t, cx), |d, indicator| d.child(indicator))
            .when_some(self.toast_indicator(t, cx), |d, indicator| d.child(indicator))
            .child(
                icon_button(
                    t,
                    "pin",
                    "pin",
                    Tip::keys(if pinned { tr().pin_on } else { tr().pin_off }, &self.keys(Command::Pin)),
                    pinned,
                )
                .marked("pin")
                .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.toggle_pinned(cx))),
            )
            .child(
                icon_button(t, "actions", "zap", Tip::keys(tr().actions, &self.keys(Command::Actions)), actions)
                    .marked("actions")
                    .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.toggle_page(Page::Actions, cx))),
            )
            .child(
                icon_button(
                    t,
                    "settings",
                    "settings",
                    Tip::keys(tr().settings, &self.keys(Command::Settings)),
                    settings,
                )
                .marked("settings")
                .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.toggle_page(Page::Settings, cx))),
            )
            .child(
                icon_button(t, "hide", "close", Tip::keys(tr().hide_hint, &[tr().escape_key]), false)
                    .marked("hide")
                    .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.hide("close button", cx))),
            )
    }

    pub(super) fn job_indicator(&self, t: Theme, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let jobs = self.jobs(cx);
        jobs.last()?;
        let ring = |path: SharedString, color: Rgba| {
            svg().path(path).size(rem(glyph::M)).absolute().top_0().left_0().text_color(color)
        };
        let arc = match jobs.fraction() {
            Some(fraction) => {
                let percent = (fraction.clamp(0., 1.) * 100.).round() as u32;
                ring(format!("ring-{percent}").into(), t.accent).into_any_element()
            }
            None => ring("ring-25".into(), t.accent)
                .with_animation("job-spin", Animation::new(timing::SPIN).repeat(), |svg, delta| {
                    svg.with_transformation(Transformation::rotate(percentage(delta)))
                })
                .into_any_element(),
        };
        let navigable = jobs.iter().any(|job| job.place().is_some());
        let jobs = self.jobs_entity();
        Some(
            div()
                .id("jobs")
                .marked("jobs")
                .size(rem(BAR_BUTTON))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(rem(radius::SMALL))
                .when(navigable, |d| {
                    d.cursor_pointer()
                        .hover(move |s| s.bg(t.hover))
                        .on_click(cx.safe_listener(|shelf, _, window, cx| shelf.open_job(window, cx)))
                })
                .child(
                    div().relative().size(rem(glyph::M)).child(ring("ring-track".into(), t.border_strong)).child(arc),
                )
                .tooltip(move |_, cx| {
                    let jobs = jobs.clone();
                    cx.new(|cx| JobsHint { _observer: cx.observe(&jobs, |_, _, cx| cx.notify()), jobs, theme: t })
                        .into()
                }),
        )
    }

    fn notice_indicator(
        &self,
        t: Theme,
        notice: Notice,
        name: &'static str,
        glyph: Svg,
        text: SharedString,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id(name)
            .marked(name)
            .size(rem(BAR_BUTTON))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(rem(radius::SMALL))
            .cursor_pointer()
            .hover(move |s| s.bg(t.hover))
            .child(glyph)
            .tooltip(hint(t, text))
            .on_hover(cx.safe_listener(move |shelf, hovered: &bool, _, cx| shelf.hold_notice(notice, *hovered, cx)))
            .on_click(cx.safe_listener(move |shelf, _, _, cx| shelf.dismiss_notice(notice, cx)))
    }

    fn toast_indicator(&self, t: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.notice_place() == Place::Bottom {
            return None;
        }
        let toast = self.notices(cx).toast()?.clone();
        Some(
            self.notice_indicator(
                t,
                Notice::Toast,
                "toast-icon",
                icon("check", t.success, glyph::M),
                toast.clone(),
                cx,
            )
            .enter(toast, Motion::Fade, self.animated()),
        )
    }

    pub(super) fn alert_indicator(&self, t: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.notice_place() == Place::Bottom {
            return None;
        }
        let alert = self.notices(cx).alert()?.clone();
        Some(
            self.notice_indicator(t, Notice::Alert, "alert", icon("alert", t.danger, glyph::M), alert.clone(), cx)
                .enter(alert, Motion::Fade, self.animated()),
        )
    }
}

struct JobsHint {
    jobs: Entity<Jobs>,
    theme: Theme,
    _observer: Subscription,
}

impl Render for JobsHint {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lines: Vec<Tip> = self
            .jobs
            .read(cx)
            .iter()
            .flat_map(|job| {
                let label = Tip::plain(job.label.clone());
                let label = match job.percent() {
                    Some(percent) => label.or(&[percent]),
                    None => label,
                };
                std::iter::once(label).chain(job.subject.clone().map(Tip::muted))
            })
            .collect();
        let mut lines = lines.into_iter();
        let first = lines.next().unwrap_or_else(|| Tip::plain(""));
        hint_card(self.theme, &lines.fold(first, Tip::line), window)
    }
}
