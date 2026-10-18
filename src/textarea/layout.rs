use super::words::{previous, tail};
use super::{LINE_HEIGHT, PADDING_X, PADDING_Y};
use crate::design;
use gpui::*;

pub(super) struct Row {
    pub(super) start: usize,
    pub(super) line: WrappedLine,
    pub(super) breaks: Vec<usize>,
    pub(super) top: Pixels,
}

impl Row {
    pub(super) fn visual(&self, local: usize) -> usize {
        let inner = self.breaks.get(1..self.rows()).unwrap_or_default();
        inner.iter().position(|&b| local < b).unwrap_or(self.rows().saturating_sub(1))
    }

    pub(super) fn x(&self, local: usize) -> Pixels {
        self.x_in(self.visual(local), local)
    }

    pub(super) fn x_in(&self, visual: usize, local: usize) -> Pixels {
        let layout = &self.line.unwrapped_layout;
        layout.x_for_index(local) - layout.x_for_index(self.edge(visual))
    }

    pub(super) fn rows(&self) -> usize {
        self.breaks.len().saturating_sub(1)
    }

    pub(super) fn edge(&self, visual: usize) -> usize {
        self.breaks.get(visual).copied().unwrap_or(self.line.len())
    }
}

pub(super) struct Layout {
    pub(super) version: usize,
    pub(super) rows: Vec<Row>,
    pub(super) bounds: Bounds<Pixels>,
    pub(super) scroll: Pixels,
    pub(super) height: Pixels,
    pub(super) scale: f32,
    pub(super) rem: Pixels,
}

impl Layout {
    pub(super) fn scaled(&self, design: f32) -> Pixels {
        design::rem(design).to_pixels(self.rem)
    }

    pub(super) fn line_height(&self) -> Pixels {
        self.scaled(LINE_HEIGHT)
    }

    pub(super) fn row(&self, offset: usize) -> Option<&Row> {
        let index = self.rows.partition_point(|row| row.start <= offset).saturating_sub(1);
        self.rows.get(index)
    }

    pub(super) fn point(&self, offset: usize) -> Point<Pixels> {
        let Some(row) = self.row(offset) else {
            return Point::default();
        };
        let local = offset - row.start;
        point(row.x(local), row.top + self.line_height() * row.visual(local) as f32)
    }

    pub(super) fn index(&self, position: Point<Pixels>, text: &str) -> usize {
        let line_height = self.line_height();
        let y = position.y.max(px(0.)).min((self.height - px(1.)).max(px(0.)));
        let index = self.rows.partition_point(|row| row.top <= y).saturating_sub(1);
        let Some(row) = self.rows.get(index) else {
            return 0;
        };
        let visual = (((y - row.top) / line_height) as usize).min(row.rows() - 1);
        let layout = &row.line.unwrapped_layout;
        let start = row.edge(visual);
        let local = layout.closest_index_for_x(position.x.max(px(0.)) + layout.x_for_index(start));
        let end = row.edge(visual + 1);
        let last = visual + 1 == row.rows();
        let limit = if last { end } else { previous(tail(text, row.start), end).max(start) };
        row.start + local.clamp(start, limit)
    }

    pub(super) fn origin(&self) -> Point<Pixels> {
        let snap = |value: Pixels| px(design::snap(value.into(), self.scale));
        point(
            snap(self.bounds.left() + self.scaled(PADDING_X)),
            snap(self.bounds.top() + self.scaled(PADDING_Y) - self.scroll),
        )
    }

    pub(super) fn view(&self) -> Pixels {
        (self.bounds.size.height - self.scaled(PADDING_Y * 2.)).max(self.line_height())
    }
}

pub(super) fn breaks(line: &WrappedLine) -> Vec<usize> {
    let runs = line.runs();
    std::iter::once(0)
        .chain(
            line.wrap_boundaries()
                .iter()
                .filter_map(|wrap| runs.get(wrap.run_ix)?.glyphs.get(wrap.glyph_ix).map(|glyph| glyph.index)),
        )
        .chain([line.len()])
        .collect()
}
