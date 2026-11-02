//! Минималистичные линейные иконки 18×18.

use eframe::egui::{Color32, Pos2, Rect, Stroke, StrokeKind, Ui};

fn stroke(color: Color32) -> Stroke {
    Stroke::new(1.5, color)
}

pub fn home(ui: &Ui, r: Rect, c: Color32) {
    let p = ui.painter();
    let (x0, y0, x1, y1) = (r.left(), r.top(), r.right(), r.bottom());
    let cx = r.center().x;
    let mid = y0 + (y1 - y0) * 0.45;
    p.line_segment([Pos2::new(cx, y0), Pos2::new(x0 + 2.0, mid)], stroke(c));
    p.line_segment([Pos2::new(cx, y0), Pos2::new(x1 - 2.0, mid)], stroke(c));
    p.rect_stroke(
        Rect::from_min_max(Pos2::new(x0 + 3.5, mid), Pos2::new(x1 - 3.5, y1 - 1.0)),
        1.0,
        stroke(c),
        StrokeKind::Inside,
    );
    p.line_segment(
        [Pos2::new(cx, y1 - 1.0), Pos2::new(cx, mid + 4.0)],
        stroke(c),
    );
}

pub fn sliders(ui: &Ui, r: Rect, c: Color32) {
    let p = ui.painter();
    let ys = [r.top() + 3.0, r.center().y, r.bottom() - 3.0];
    for (i, &y) in ys.iter().enumerate() {
        p.line_segment(
            [Pos2::new(r.left() + 2.0, y), Pos2::new(r.right() - 2.0, y)],
            stroke(c),
        );
        let kx = r.left() + 4.0 + i as f32 * ((r.width() - 8.0) / 2.0);
        p.circle_filled(Pos2::new(kx, y), 2.5, c);
        p.circle_stroke(Pos2::new(kx, y), 2.5, stroke(c));
    }
}

pub fn layers(ui: &Ui, r: Rect, c: Color32) {
    let p = ui.painter();
    let cx = r.center().x;
    let cy = r.center().y;
    for i in 0..2 {
        let dy = i as f32 * 5.0;
        let pts = [
            Pos2::new(cx, r.top() + 2.0 + dy),
            Pos2::new(r.right() - 2.0, cy - 1.0 + dy),
            Pos2::new(cx, r.bottom() - 4.0 + dy),
            Pos2::new(r.left() + 2.0, cy - 1.0 + dy),
        ];
        for j in 0..4 {
            p.line_segment([pts[j], pts[(j + 1) % 4]], stroke(c));
        }
    }
}

pub fn info(ui: &Ui, r: Rect, c: Color32) {
    let p = ui.painter();
    p.circle_stroke(r.center(), r.width() / 2.0 - 1.0, stroke(c));
    let cx = r.center().x;
    p.line_segment(
        [Pos2::new(cx, r.top() + 4.5), Pos2::new(cx, r.center().y)],
        stroke(c),
    );
    p.circle_filled(Pos2::new(cx, r.bottom() - 5.0), 1.2, c);
}
