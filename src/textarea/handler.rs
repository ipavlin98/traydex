use super::*;

impl EntityInputHandler for TextArea {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        self.edit.text_for_range(&range, adjusted)
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        Some(self.edit.selection_utf16())
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.edit.marked_utf16()
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.edit.unmark();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.edit.target(range.as_ref());
        let edit = if text.chars().any(char::is_whitespace) { Edit::Other } else { Edit::Typing };
        self.replace(range, text, edit, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.edit.target(range.as_ref());
        if self.edit.marked().is_none() {
            self.remember(Edit::Other);
        }
        self.edit.mark(range, &chips::strip(&clean(text)), selected);
        self.version += 1;
        self.reveal = true;
        self.ask(cx);
        cx.emit(Changed);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.layout.as_ref()?;
        let range = self.edit.range_from_utf16(&range);
        let origin = layout.origin();
        let at = layout.point(range.start);
        Some(Bounds::new(origin + at, size(px(1.), layout.line_height())))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.edit.offset_to_utf16(self.index_at(position)))
    }
}
