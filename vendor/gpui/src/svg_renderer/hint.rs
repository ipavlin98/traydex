//! Pixel fitting for icons: straight horizontal and vertical lines get their solid core on whole
//! device pixels, and the rest of the outline moves along between them. Stroke widths stay as they
//! are, so icons keep their weight.
//! Only trees of solid paths that ask for crisp edges (`shape-rendering="crispEdges"`) are fitted,
//! and they stay anti-aliased; anything else, such as a logo built from layers that must stay
//! aligned, is left to resvg.

use resvg::tiny_skia::{
    FillRule, Paint, Path, PathBuilder, PathSegment, Pixmap, Point, Stroke, StrokeDash, Transform,
};

const EPSILON: f32 = 1e-3;

struct Shape<'a> {
    path: Path,
    source: &'a usvg::Path,
    scale: f32,
    width: Option<f32>,
}

/// Draws `tree` into `pixmap`, `scale` device pixels per user unit, supersampled by `supersample`.
/// Returns false without drawing when the tree has something other than solid paths.
pub(super) fn render(tree: &usvg::Tree, scale: f32, supersample: f32, pixmap: &mut Pixmap) -> bool {
    let mut sources = Vec::new();
    if !collect(tree.root(), &mut sources) {
        return false;
    }
    let device = Transform::from_scale(scale, scale);
    let Some(shapes) = sources
        .into_iter()
        .map(|source| {
            let transform = device.pre_concat(source.abs_transform());
            let path = source.data().clone().transform(transform)?;
            let scale = uniform(transform);
            let width = source
                .stroke()
                .map(|stroke| (stroke.width().get() * scale).max(1.));
            Some(Shape {
                path,
                source,
                scale,
                width,
            })
        })
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    let (columns, rows) = stems(&shapes);
    let output = Transform::from_scale(supersample, supersample);
    for shape in &shapes {
        let Some(path) = fitted(&shape.path, &columns, &rows) else {
            continue;
        };
        let stroke = shape
            .source
            .stroke()
            .zip(shape.width)
            .map(|(stroke, width)| Stroke {
                width,
                dash: stroke.dasharray().and_then(|dashes| {
                    StrokeDash::new(
                        dashes.iter().map(|dash| dash * shape.scale).collect(),
                        stroke.dashoffset() * shape.scale,
                    )
                }),
                ..stroke.to_tiny_skia()
            });
        let fill = shape.source.fill().map(|fill| {
            let rule = match fill.rule() {
                usvg::FillRule::NonZero => FillRule::Winding,
                usvg::FillRule::EvenOdd => FillRule::EvenOdd,
            };
            (paint(fill.opacity().get()), rule)
        });
        let stroke = stroke.map(|stroke| {
            let opacity = shape.source.stroke().map_or(1., |stroke| stroke.opacity().get());
            (paint(opacity), stroke)
        });
        let draw_fill = |pixmap: &mut Pixmap| {
            if let Some((paint, rule)) = &fill {
                pixmap.fill_path(&path, paint, *rule, output, None);
            }
        };
        let draw_stroke = |pixmap: &mut Pixmap| {
            if let Some((paint, stroke)) = &stroke {
                pixmap.stroke_path(&path, paint, stroke, output, None);
            }
        };
        match shape.source.paint_order() {
            usvg::PaintOrder::FillAndStroke => {
                draw_fill(pixmap);
                draw_stroke(pixmap);
            }
            usvg::PaintOrder::StrokeAndFill => {
                draw_stroke(pixmap);
                draw_fill(pixmap);
            }
        }
    }
    true
}

fn collect<'a>(group: &'a usvg::Group, sources: &mut Vec<&'a usvg::Path>) -> bool {
    if group.opacity() != usvg::Opacity::ONE
        || group.clip_path().is_some()
        || group.mask().is_some()
        || !group.filters().is_empty()
    {
        return false;
    }
    let solid = |paint: &usvg::Paint| matches!(paint, usvg::Paint::Color(_));
    for node in group.children() {
        match node {
            usvg::Node::Group(group) => {
                if !collect(group, sources) {
                    return false;
                }
            }
            usvg::Node::Path(path) => {
                if path.rendering_mode() != usvg::ShapeRendering::CrispEdges
                    || path.fill().is_some_and(|fill| !solid(fill.paint()))
                    || path.stroke().is_some_and(|stroke| !solid(stroke.paint()))
                {
                    return false;
                }
                if path.is_visible() {
                    sources.push(path);
                }
            }
            usvg::Node::Image(_) | usvg::Node::Text(_) => return false,
        }
    }
    true
}

fn uniform(transform: Transform) -> f32 {
    (transform.sx * transform.sy - transform.kx * transform.ky)
        .abs()
        .sqrt()
}

fn paint(opacity: f32) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, (opacity.clamp(0., 1.) * 255.).round() as u8);
    paint
}

/// Where a line `width` device pixels wide at `at` should go so that its whole pixels are solid: the
/// nearest pixel center when that count is odd, the nearest pixel edge when it's even or for the edge
/// of a fill.
fn snap(at: f32, width: f32) -> f32 {
    if width.floor() % 2. == 1. {
        at.floor() + 0.5
    } else {
        at.round()
    }
}

/// Where `stems`, each a position with its line width, should land. Stems mirrored around the
/// middle of the outermost two, like the walls of a tray around its arrow, land mirrored around one
/// place on the half-pixel grid, so a symmetric icon stays symmetric. A stem on that axis decides
/// whether it is a pixel center or edge; the rest snap on their own.
fn placed(stems: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let (Some(first), Some(last)) = (stems.first(), stems.last()) else {
        return Vec::new();
    };
    let axis = (first.0 + last.0) / 2.;
    let center = match stems.iter().find(|stem| (stem.0 - axis).abs() < EPSILON) {
        Some(&(at, width)) => snap(at, width),
        None => (axis * 2.).round() / 2.,
    };
    let mirrored = |(at, width): (f32, f32)| {
        stems.iter().any(|other| (other.0 + at - 2. * axis).abs() < EPSILON && (other.1 - width).abs() < EPSILON)
    };
    stems
        .iter()
        .map(|&(at, width)| {
            if !mirrored((at, width)) {
                return (at, snap(at, width));
            }
            let near = snap(center - (at - axis).abs(), width);
            (at, if at < axis { near } else { 2. * center - near })
        })
        .collect()
}

/// Positions of straight vertical and horizontal edges with where each should land.
fn stems(shapes: &[Shape]) -> (Vec<(f32, f32)>, Vec<(f32, f32)>) {
    let (mut columns, mut rows) = (Vec::new(), Vec::new());
    for shape in shapes {
        let width = shape.width.unwrap_or(0.);
        let (mut start, mut last) = (Point::zero(), Point::zero());
        let mut edge = |from: Point, to: Point| {
            let (across, along) = ((from.x - to.x).abs(), (from.y - to.y).abs());
            if across < EPSILON && along >= EPSILON {
                columns.push((to.x, width));
            }
            if along < EPSILON && across >= EPSILON {
                rows.push((to.y, width));
            }
        };
        for segment in shape.path.segments() {
            match segment {
                PathSegment::MoveTo(point) => (start, last) = (point, point),
                PathSegment::LineTo(point) => {
                    edge(last, point);
                    last = point;
                }
                PathSegment::QuadTo(_, point) | PathSegment::CubicTo(_, _, point) => last = point,
                PathSegment::Close => {
                    edge(last, start);
                    last = start;
                }
            }
        }
    }
    for stems in [&mut columns, &mut rows] {
        stems.sort_by(|a, b| a.0.total_cmp(&b.0));
        stems.dedup_by(|b, a| (b.0 - a.0).abs() < EPSILON);
    }
    (placed(&columns), placed(&rows))
}

/// Moves `at` the way the stems around it moved: stems themselves land where they snap, points
/// between two stems keep their relative place, points outside shift with the nearest stem.
fn fit(stems: &[(f32, f32)], at: f32) -> f32 {
    let (Some(first), Some(last)) = (stems.first(), stems.last()) else {
        return at;
    };
    if at <= first.0 {
        return at + first.1 - first.0;
    }
    if at >= last.0 {
        return at + last.1 - last.0;
    }
    let next = stems.partition_point(|stem| stem.0 < at);
    let (Some(before), Some(after)) = (stems.get(next.wrapping_sub(1)), stems.get(next)) else {
        return at;
    };
    before.1 + (after.1 - before.1) * (at - before.0) / (after.0 - before.0)
}

fn fitted(path: &Path, columns: &[(f32, f32)], rows: &[(f32, f32)]) -> Option<Path> {
    let point = |point: Point| (fit(columns, point.x), fit(rows, point.y));
    let mut builder = PathBuilder::new();
    for segment in path.segments() {
        match segment {
            PathSegment::MoveTo(to) => {
                let (x, y) = point(to);
                builder.move_to(x, y);
            }
            PathSegment::LineTo(to) => {
                let (x, y) = point(to);
                builder.line_to(x, y);
            }
            PathSegment::QuadTo(control, to) => {
                let ((x1, y1), (x, y)) = (point(control), point(to));
                builder.quad_to(x1, y1, x, y);
            }
            PathSegment::CubicTo(first, second, to) => {
                let ((x1, y1), (x2, y2), (x, y)) = (point(first), point(second), point(to));
                builder.cubic_to(x1, y1, x2, y2, x, y);
            }
            PathSegment::Close => builder.close(),
        }
    }
    builder.finish()
}
