use std::ops::Range;
use windows::Win32::{
    Foundation::{POINT, RECT},
    UI::WindowsAndMessaging::*,
};

pub(super) fn region(point: POINT, title: i32, leading: Range<i32>, trailing: i32) -> u32 {
    if point.y < title && !leading.contains(&point.x) && point.x < trailing { HTCAPTION } else { HTCLIENT }
}

pub(super) fn edge_code(left: i32, top: i32, right: i32, bottom: i32, grip: i32, corner: i32) -> Option<u32> {
    let near = |d: i32, limit: i32| d < limit;
    let code = match (near(left, grip), near(right, grip), near(top, grip), near(bottom, grip)) {
        (false, false, false, false) => return None,
        _ if near(left, corner) && near(top, corner) => HTTOPLEFT,
        _ if near(right, corner) && near(top, corner) => HTTOPRIGHT,
        _ if near(left, corner) && near(bottom, corner) => HTBOTTOMLEFT,
        _ if near(right, corner) && near(bottom, corner) => HTBOTTOMRIGHT,
        (_, _, true, _) => HTTOP,
        (_, _, _, true) => HTBOTTOM,
        _ => return None,
    };
    Some(code)
}

pub(super) fn edge(start: i32, end: i32, low: i32, high: i32, threshold: i32) -> i32 {
    if (start - low).abs() <= threshold {
        low - start
    } else if (end - high).abs() <= threshold {
        high - end
    } else {
        0
    }
}

pub(super) fn stretch(
    start: RECT,
    edge: u32,
    (dx, dy): (i32, i32),
    work: impl FnOnce(&RECT) -> Option<RECT>,
    threshold: i32,
    (min_width, min_height): (i32, i32),
) -> RECT {
    let left = matches!(edge, WMSZ_LEFT | WMSZ_TOPLEFT | WMSZ_BOTTOMLEFT);
    let right = matches!(edge, WMSZ_RIGHT | WMSZ_TOPRIGHT | WMSZ_BOTTOMRIGHT);
    let top = matches!(edge, WMSZ_TOP | WMSZ_TOPLEFT | WMSZ_TOPRIGHT);
    let bottom = matches!(edge, WMSZ_BOTTOM | WMSZ_BOTTOMLEFT | WMSZ_BOTTOMRIGHT);
    let mut next = start;
    if left {
        next.left += dx;
    }
    if right {
        next.right += dx;
    }
    if top {
        next.top += dy;
    }
    if bottom {
        next.bottom += dy;
    }
    if let Some(work) = work(&next) {
        let pull = |from: i32, to: i32| if (to - from).abs() <= threshold { to } else { from };
        if left {
            next.left = pull(next.left, work.left);
        }
        if right {
            next.right = pull(next.right, work.right);
        }
        if top {
            next.top = pull(next.top, work.top);
        }
        if bottom {
            next.bottom = pull(next.bottom, work.bottom);
        }
    }
    if next.right - next.left < min_width {
        if left {
            next.left = next.right - min_width;
        } else {
            next.right = next.left + min_width;
        }
    }
    if next.bottom - next.top < min_height {
        if top {
            next.top = next.bottom - min_height;
        } else {
            next.bottom = next.top + min_height;
        }
    }
    next
}

pub(super) fn beside(cursor: POINT, work: RECT, (width, height): (i32, i32), gap: i32) -> (i32, i32) {
    let right = cursor.x + gap;
    let left = cursor.x - gap - width;
    let x = if right + width <= work.right {
        right
    } else if left >= work.left {
        left
    } else if cursor.x - work.left > work.right - cursor.x {
        work.left
    } else {
        (work.right - width).max(work.left)
    };
    let y = (cursor.y - height / 2).min(work.bottom - height).max(work.top);
    (x, y)
}

pub(super) fn anchored(start: i32, end: i32, low: i32, high: i32, size: i32) -> i32 {
    let tolerance = 2;
    let at_low = (start - low).abs() <= tolerance;
    let at_high = (end - high).abs() <= tolerance;
    let keep_high = at_high || (!at_low && (start + end) / 2 > (low + high) / 2);
    if keep_high { end - size } else { start }
}
