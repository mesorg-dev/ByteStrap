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

/// Шестерёнка (настройки).
pub fn gear(ui: &Ui, r: Rect, c: Color32) {
    let p = ui.painter();
    let cx = r.center().x;
    let cy = r.center().y;
    let outer = r.width() / 2.0 - 1.5;
    let inner = outer * 0.55;
    p.circle_stroke(r.center(), inner, stroke(c));
    for i in 0..8 {
        let a = std::f32::consts::FRAC_PI_4 * i as f32;
        let dx = a.cos();
        let dy = a.sin();
        p.line_segment(
            [
                Pos2::new(cx + dx * (inner + 1.0), cy + dy * (inner + 1.0)),
                Pos2::new(cx + dx * outer, cy + dy * outer),
            ],
            stroke(c),
        );
    }
}

/// Гаечный ключ (моды).
pub fn wrench(ui: &Ui, r: Rect, c: Color32) {
    let p = ui.painter();
    let (x0, y0, x1, y1) = (r.left(), r.top(), r.right(), r.bottom());
    // Рукоять (диагональ слева-снизу направо-вверх)
    p.line_segment(
        [Pos2::new(x0 + 3.0, y1 - 3.0), Pos2::new(x1 - 7.0, y0 + 7.0)],
        stroke(c),
    );
    // Головка: «C»-образный хомут в правом верхнем углу
    let head = Pos2::new(x1 - 4.5, y0 + 4.5);
    p.circle_stroke(head, 4.5, stroke(c));
    // Прорезь головки
    p.line_segment(
        [Pos2::new(head.x - 1.5, head.y - 6.5), Pos2::new(head.x + 2.5, head.y - 2.5)],
        Stroke::new(3.0, crate::theme::BG),
    );
}

/// Звезда (внешний вид).
pub fn star(ui: &Ui, r: Rect, c: Color32) {
    let p = ui.painter();
    let cx = r.center().x;
    let cy = r.center().y;
    let outer = r.width() / 2.0 - 1.0;
    let inner = outer * 0.45;
    let mut pts = Vec::with_capacity(10);
    for i in 0..10 {
        let rad = if i % 2 == 0 { outer } else { inner };
        let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / 5.0;
        pts.push(Pos2::new(cx + a.cos() * rad, cy + a.sin() * rad));
    }
    for i in 0..10 {
        p.line_segment([pts[i], pts[(i + 1) % 10]], stroke(c));
    }
}

/// Человек (аккаунт).
pub fn user(ui: &Ui, r: Rect, c: Color32) {
    let p = ui.painter();
    let cx = r.center().x;
    let head = Pos2::new(cx, r.top() + 4.5);
    p.circle_stroke(head, 3.5, stroke(c));
    let top = r.top() + 10.5;
    let pts = [
        Pos2::new(r.left() + 3.0, r.bottom() - 1.0),
        Pos2::new(r.left() + 4.5, top + 2.5),
        Pos2::new(cx, top),
        Pos2::new(r.right() - 4.5, top + 2.5),
        Pos2::new(r.right() - 3.0, r.bottom() - 1.0),
    ];
    for i in 0..4 {
        p.line_segment([pts[i], pts[i + 1]], stroke(c));
    }
}

/// Щит (конфиденциальность).
pub fn shield(ui: &Ui, r: Rect, c: Color32) {
    let p = ui.painter();
    let cx = r.center().x;
    let pts = [
        Pos2::new(cx, r.top() + 1.0),
        Pos2::new(r.right() - 2.5, r.top() + 4.5),
        Pos2::new(r.right() - 2.5, r.top() + 9.0),
        Pos2::new(cx, r.bottom() - 1.0),
        Pos2::new(r.left() + 2.5, r.top() + 9.0),
        Pos2::new(r.left() + 2.5, r.top() + 4.5),
    ];
    for i in 0..6 {
        p.line_segment([pts[i], pts[(i + 1) % 6]], stroke(c));
    }
    p.line_segment(
        [Pos2::new(cx - 2.5, r.center().y), Pos2::new(cx - 0.5, r.center().y + 2.5)],
        stroke(c),
    );
    p.line_segment(
        [Pos2::new(cx - 0.5, r.center().y + 2.5), Pos2::new(cx + 3.0, r.center().y - 2.5)],
        stroke(c),
    );
}

/// Треугольник- play (запуск).
pub fn play(ui: &Ui, r: Rect, c: Color32) {
    let p = ui.painter();
    let pts = [
        Pos2::new(r.left() + 3.5, r.top() + 2.5),
        Pos2::new(r.right() - 2.5, r.center().y),
        Pos2::new(r.left() + 3.5, r.bottom() - 2.5),
    ];
    p.line_segment([pts[0], pts[1]], stroke(c));
    p.line_segment([pts[1], pts[2]], stroke(c));
    p.line_segment([pts[2], pts[0]], stroke(c));
}

/// Лупа (поиск).
pub fn search(ui: &Ui, r: Rect, c: Color32) {
    let p = ui.painter();
    let center = Pos2::new(r.left() + r.width() * 0.42, r.top() + r.height() * 0.42);
    let rad = r.width() * 0.32;
    p.circle_stroke(center, rad, stroke(c));
    p.line_segment(
        [
            Pos2::new(center.x + rad * 0.7, center.y + rad * 0.7),
            Pos2::new(r.right() - 1.0, r.bottom() - 1.0),
        ],
        stroke(c),
    );
}
