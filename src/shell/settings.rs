//! Оверлей настроек шелла: nav-колонка +5 страниц (мокап `shell.html`).

use eframe::egui::{
    self, Align2, Color32, FontFamily, FontId, Id, Order, Pos2, Rect, Sense, Stroke, StrokeKind,
    Ui, Vec2,
};

use crate::i18n::tr;
use crate::theme;
use crate::ui::icons;

use super::config::{pv, ACCENTS};
use super::Shell;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Look,
    Account,
    Privacy,
    Launch,
    About,
}

impl Page {
    fn title(self, lang: crate::i18n::Lang) -> &'static str {
        match self {
            Page::Look => tr(lang, "Shell_NavLook"),
            Page::Account => tr(lang, "Shell_NavAccount"),
            Page::Privacy => tr(lang, "Shell_NavPrivacy"),
            Page::Launch => tr(lang, "Shell_NavLaunch"),
            Page::About => tr(lang, "Shell_NavAbout"),
        }
    }
}

const NAV_W: f32 = 220.0;

pub fn overlay(shell: &mut Shell, ctx: &egui::Context) {
    let screen = ctx.content_rect();
    let accent = shell.accent();
    let lang = shell.lang;

    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        shell.settings_open = false;
        return;
    }

    egui::Area::new(Id::unique("shell_settings"))
        .order(Order::Foreground)
        .fixed_pos(Pos2::ZERO)
        .show(ctx, |ui| {
            // Затемнение + клик по фону закрывает.
            let p = ui.painter().clone();
            p.rect_filled(
                screen,
                0.0,
                Color32::from_rgba_unmultiplied(5, 5, 6, 184),
            );
            let backdrop = ui.allocate_rect(screen, Sense::click());
            if backdrop.clicked() {
                shell.settings_open = false;
                return;
            }

            // Окно настроек.
            let size = Vec2::new(
                (screen.width() * 0.96).min(980.0),
                (screen.height() * 0.90).min(680.0),
            );
            let win = Rect::from_center_size(screen.center(), size);
            p.rect_filled(win.translate(Vec2::new(0.0, 14.0)), 12.0, Color32::from_black_alpha(150));
            p.rect_filled(win, 12.0, theme::SIDEBAR);
            p.rect_stroke(win, 12.0, Stroke::new(1.0, theme::LINE), StrokeKind::Inside);
            // Клик внутри окна не «проваливается» на фон.
            let _wresp = ui.allocate_rect(win, Sense::click());

            // Nav-колонка.
            let nav = Rect::from_min_size(win.min, Vec2::new(NAV_W, win.height()));
            p.rect_filled(
                Rect::from_min_max(nav.min, Pos2::new(nav.right() - 1.0, nav.bottom())),
                12.0,
                theme::PANEL,
            );
            p.line_segment(
                [
                    Pos2::new(nav.right() - 1.0, nav.top() + 12.0),
                    Pos2::new(nav.right() - 1.0, nav.bottom() - 12.0),
                ],
                Stroke::new(1.0, theme::LINE),
            );
            p.text(
                nav.min + Vec2::new(18.0, 20.0),
                Align2::LEFT_TOP,
                tr(lang, "Shell_Set").to_uppercase(),
                FontId::new(10.0, FontFamily::Proportional),
                theme::FG_DIMMER,
            );

            let items: [(Page, fn(&Ui, Rect, Color32)); 5] = [
                (Page::Look, icons::star),
                (Page::Account, icons::user),
                (Page::Privacy, icons::shield),
                (Page::Launch, icons::play),
                (Page::About, icons::info),
            ];
            let mut y = nav.top() + 44.0;
            for (page, icon_fn) in items {
                let r = Rect::from_min_size(Pos2::new(nav.left() + 12.0, y), Vec2::new(NAV_W - 24.0, 40.0));
                let resp = ui.interact(r, Id::unique(("set_nav", page as u8)), Sense::click());
                let active = shell.page == page;
                let hov = resp.hovered();
                let fill = if active {
                    Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 26)
                } else if hov {
                    theme::LINE_SOFT
                } else {
                    Color32::TRANSPARENT
                };
                p.rect_filled(r, 6.0, fill);
                if active {
                    p.rect_stroke(
                        r,
                        6.0,
                        Stroke::new(
                            1.0,
                            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 76),
                        ),
                        StrokeKind::Inside,
                    );
                }
                icon_fn(
                    ui,
                    Rect::from_center_size(
                        Pos2::new(r.left() + 20.0, r.center().y),
                        Vec2::splat(15.0),
                    ),
                    if active { accent } else { theme::FG_DIMMER },
                );
                p.text(
                    Pos2::new(r.left() + 38.0, r.center().y),
                    Align2::LEFT_CENTER,
                    page.title(lang),
                    FontId::new(13.0, FontFamily::Proportional),
                    if active { theme::FG } else { theme::FG_DIM },
                );
                if resp.clicked() {
                    shell.page = page;
                }
                y += 44.0;
            }

            // Правая часть: шапка + тело страницы.
            let main = Rect::from_min_max(
                Pos2::new(nav.right(), win.top()),
                win.max,
            );
            p.line_segment(
                [Pos2::new(main.left(), main.top()), Pos2::new(main.left(), main.bottom())],
                Stroke::new(1.0, theme::LINE),
            );
            let head = Rect::from_min_size(main.min, Vec2::new(main.width(), 54.0));
            p.line_segment(
                [head.left_bottom(), head.right_bottom()],
                Stroke::new(1.0, theme::LINE),
            );
            p.text(
                Pos2::new(head.left() + 22.0, head.center().y),
                Align2::LEFT_CENTER,
                shell.page.title(lang),
                theme::bold(14.0),
                theme::FG,
            );
            // Кнопка закрытия.
            let close = Rect::from_min_size(
                Pos2::new(head.right() - 34.0 - 12.0, head.top() + 10.0),
                Vec2::splat(34.0),
            );
            let cre = ui.interact(close, Id::unique("set_close"), Sense::click());
            let ch = cre.hovered();
            p.rect_filled(close, 6.0, if ch { theme::LINE_SOFT } else { Color32::TRANSPARENT });
            p.text(
                close.center(),
                Align2::CENTER_CENTER,
                "✕",
                FontId::new(13.0, FontFamily::Proportional),
                if ch { theme::FG } else { theme::FG_DIM },
            );
            if cre.clicked() {
                shell.settings_open = false;
                return;
            }

            // Тело страницы со скроллом.
            let body = Rect::from_min_max(
                Pos2::new(main.left() + 22.0, main.top() + 54.0 + 18.0),
                Pos2::new(main.right() - 22.0, main.bottom() - 22.0),
            );
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(body),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("set_body")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            match shell.page {
                                Page::Look => page_look(shell, ui, accent),
                                Page::Account => page_account(shell, ui, accent),
                                Page::Privacy => page_privacy(shell, ui, accent),
                                Page::Launch => page_launch(shell, ui, accent),
                                Page::About => page_about(shell, ui, accent),
                            }
                        });
                },
            );
        });
}

// ------------------------------------------------------------- виджеты

/// Заголовок группы (h4 в мокапе).
fn group(ui: &mut Ui, title: &str) {
    ui.add_space(18.0);
    ui.label(
        egui::RichText::new(title.to_uppercase())
            .size(10.5)
            .family(FontFamily::Name("strong".into()))
            .color(theme::FG_DIMMER),
    );
    ui.add_space(9.0);
}

/// Строка настроек: рамка + подпись слева; control рисует вызывающий справа.
fn row(ui: &mut Ui, label: &str, sub: Option<&str>) -> Rect {
    let w = ui.available_width();
    let h = if sub.is_some() { 58.0 } else { 48.0 };
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, 8.0, theme::PANEL);
    p.rect_stroke(r, 8.0, Stroke::new(1.0, theme::LINE), StrokeKind::Inside);
    let x = r.left() + 16.0;
    if let Some(s) = sub {
        p.text(
            Pos2::new(x, r.top() + 13.0),
            Align2::LEFT_TOP,
            label,
            theme::bold(13.5),
            theme::FG,
        );
        p.text(
            Pos2::new(x, r.top() + 33.0),
            Align2::LEFT_TOP,
            s,
            FontId::new(11.5, FontFamily::Proportional),
            theme::FG_DIMMER,
        );
    } else {
        p.text(
            Pos2::new(x, r.center().y),
            Align2::LEFT_CENTER,
            label,
            theme::bold(13.5),
            theme::FG,
        );
    }
    ui.add_space(8.0);
    r
}

/// Правый край строки (для размещения control).
fn right_at(row: Rect, w: f32, h: f32) -> Rect {
    Rect::from_min_size(
        Pos2::new(row.right() - 16.0 - w, row.center().y - h * 0.5),
        Vec2::new(w, h),
    )
}

/// Тумблер 38×21 (как в мокапе).
fn toggle(ui: &mut Ui, r: Rect, on: bool, accent: Color32) -> bool {
    let resp = ui.interact(r, ui.scope_id().with(("toggle", r.min.x as u32)), Sense::click());
    let p = ui.painter();
    let track = if on {
        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 90)
    } else {
        theme::LINE
    };
    p.rect_filled(r, 999.0, track);
    let knob_x = if on { r.right() - 10.5 } else { r.left() + 10.5 };
    p.circle_filled(
        Pos2::new(knob_x, r.center().y),
        7.5,
        if on { accent } else { theme::FG_DIM },
    );
    resp.clicked()
}

/// Сегментированный контроль; возвращает новый индекс (или None).
fn seg(ui: &mut Ui, anchor_top: f32, row: Rect, items: &[&str], sel: usize, accent: Color32) -> Option<usize> {
    let p = ui.painter().clone();
    let mut widths = Vec::with_capacity(items.len());
    let mut total = 0.0;
    for it in items {
        let g = p.layout_no_wrap(
            (*it).to_string(),
            FontId::new(12.0, FontFamily::Proportional),
            theme::FG_DIM,
        );
        let w = g.size().x + 32.0;
        widths.push(w);
        total += w;
    }
    let h = 32.0;
    let x0 = row.right() - 16.0 - total;
    let mut x = x0;
    let mut out = None;
    for (i, it) in items.iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(x, anchor_top), Vec2::new(widths[i], h));
        let resp = ui.interact(
            r,
            ui.scope_id().with(("seg", row.min.y as u32, i)),
            Sense::click(),
        );
        let active = i == sel;
        let fill = if active {
            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 36)
        } else if resp.hovered() {
            theme::LINE_SOFT
        } else {
            Color32::TRANSPARENT
        };
        p.rect_filled(r, 0.0, fill);
        if i > 0 {
            p.line_segment(
                [Pos2::new(r.left(), r.top() + 4.0), Pos2::new(r.left(), r.bottom() - 4.0)],
                Stroke::new(1.0, theme::LINE),
            );
        }
        let g = p.layout_no_wrap(
            (*it).to_string(),
            FontId::new(12.0, FontFamily::Proportional),
            if active { accent } else { theme::FG_DIM },
        );
        p.galley(
            Pos2::new(
                r.center().x - g.size().x * 0.5,
                r.center().y - g.size().y * 0.5,
            ),
            g,
            if active { accent } else { theme::FG_DIM },
        );
        if resp.clicked() {
            out = Some(i);
        }
        x += widths[i];
    }
    // Рамка вокруг всего сегмента.
    p.rect_stroke(
        Rect::from_min_size(Pos2::new(x0, anchor_top), Vec2::new(total, h)),
        0.0,
        Stroke::new(1.0, theme::LINE),
        StrokeKind::Inside,
    );
    out
}

/// Выпадающий список (180×32).
fn combo(ui: &mut Ui, r: Rect, id_salt: impl egui::AsIdSalt, items: &[&str], sel: &mut usize) {
    let text = items.get(*sel).copied().unwrap_or("");
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(r),
        |ui| {
            egui::ComboBox::from_id_salt(id_salt)
                .selected_text(text)
                .width(r.width())
                .show_ui(ui, |ui| {
                    for (i, it) in items.iter().enumerate() {
                        ui.selectable_value(sel, i, *it);
                    }
                });
        },
    );
}

/// Кнопка-пилюля; true — клик.
fn button(ui: &mut Ui, r: Rect, text: &str, kind: BtnKind, accent: Color32) -> bool {
    let resp = ui.interact(
        r,
        ui.scope_id().with(("btn", text, r.min.x as u32, r.min.y as u32)),
        Sense::click(),
    );
    let p = ui.painter();
    let hov = resp.hovered();
    let (fg, bg, border) = match kind {
        BtnKind::Plain => (
            theme::FG,
            if hov { theme::LINE_SOFT } else { Color32::from_rgb(0x17, 0x17, 0x1A) },
            theme::LINE,
        ),
        BtnKind::Accent => {
            if hov {
                (theme::BG, accent, accent)
            } else {
                (
                    accent,
                    Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 20),
                    Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 100),
                )
            }
        }
        BtnKind::Danger => (
            theme::BAD,
            if hov {
                Color32::from_rgba_unmultiplied(0xFF, 0x80, 0x87, 30)
            } else {
                Color32::TRANSPARENT
            },
            Color32::from_rgba_unmultiplied(0xFF, 0x80, 0x87, 90),
        ),
    };
    p.rect_filled(r, 6.0, bg);
    p.rect_stroke(r, 6.0, Stroke::new(1.0, border), StrokeKind::Inside);
    let g = p.layout_no_wrap(
        text.to_string(),
        FontId::new(11.5, FontFamily::Proportional),
        fg,
    );
    p.galley(
        Pos2::new(r.center().x - g.size().x * 0.5, r.center().y - g.size().y * 0.5),
        g,
        fg,
    );
    resp.clicked()
}

#[derive(Clone, Copy)]
enum BtnKind {
    Plain,
    Accent,
    Danger,
}

/// Значение-текст справа (моно, как в мокапе).
fn value(ui: &mut Ui, row: Rect, text: &str) {
    let p = ui.painter();
    let g = p.layout_no_wrap(
        text.to_string(),
        FontId::new(13.0, FontFamily::Monospace),
        theme::FG_DIM,
    );
    p.galley(
        Pos2::new(
            row.right() - 16.0 - g.size().x,
            row.center().y - g.size().y * 0.5,
        ),
        g,
        theme::FG_DIM,
    );
}

/// Строка-слайдер: подпись + значение сверху, ползунок снизу.
fn slider_row(
    ui: &mut Ui,
    label: &str,
    val: &mut i32,
    range: std::ops::RangeInclusive<i32>,
    suffix: &str,
    accent: Color32,
) -> bool {
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, 52.0), Sense::hover());
    let p = ui.painter();
    p.text(
        Pos2::new(r.left(), r.top() + 6.0),
        Align2::LEFT_TOP,
        label,
        FontId::new(12.5, FontFamily::Proportional),
        theme::FG_DIM,
    );
    let vs = format!("{val}{suffix}");
    let g = p.layout_no_wrap(vs, theme::bold(12.0), theme::FG);
    p.galley(
        Pos2::new(r.right() - g.size().x, r.top() + 5.0),
        g,
        theme::FG,
    );
    let srect = Rect::from_min_max(
        Pos2::new(r.left(), r.bottom() - 22.0),
        Pos2::new(r.right(), r.bottom()),
    );
    let scope = ui.scope_builder(
        egui::UiBuilder::new().max_rect(srect),
        |ui| {
            ui.spacing_mut().slider_width = srect.width();
            ui.add(egui::Slider::new(val, range).show_value(false))
                .changed()
        },
    );
    // Акцентный «хвост» ползунка — через visuals скоупа уже не вернуть;
    // перекрашиваем трек после отрисовки нечего, оставляем дефолт.
    let _ = accent;
    scope.inner
}

// -------------------------------------------------------------- страницы

fn page_look(shell: &mut Shell, ui: &mut Ui, accent: Color32) {
    let lang = shell.lang;

    group(ui, tr(lang, "Shell_Bg"));
    // Drop-зона.
    let w = ui.available_width();
    let (dz, _) = ui.allocate_exact_size(Vec2::new(w, 74.0), Sense::click());
    let p = ui.painter();
    let dz_resp = ui.interact(dz, ui.scope_id().with("bg_drop"), Sense::click());
    let hov = dz_resp.hovered();
    p.rect_filled(dz, 8.0, theme::PANEL);
    let dash_color = if hov {
        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 130)
    } else {
        theme::LINE
    };
    let s = Stroke::new(1.0, dash_color);
    let corners = [
        [dz.left_top(), dz.right_top()],
        [dz.right_top(), dz.right_bottom()],
        [dz.right_bottom(), dz.left_bottom()],
        [dz.left_bottom(), dz.left_top()],
    ];
    for [a, b] in corners {
        p.add(egui::epaint::Shape::dashed_line(&[a, b], s, 6.0, 5.0));
    }
    let hint = if shell.cfg.bg_path.is_empty() {
        tr(lang, "Shell_BgDrop").to_string()
    } else {
        shell.cfg.bg_path.clone()
    };
    let g = p.layout_no_wrap(hint, FontId::new(12.5, FontFamily::Proportional), theme::FG_DIMMER);
    p.galley(
        Pos2::new(
            dz.center().x - g.size().x * 0.5,
            dz.center().y - g.size().y * 0.5,
        ),
        g,
        theme::FG_DIMMER,
    );
    if dz_resp.clicked() {
        #[cfg(windows)]
        {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"])
                .pick_file()
            {
                shell.cfg.bg_path = path.display().to_string();
                shell.load_bg();
            }
        }
        #[cfg(not(windows))]
        {
            shell.toast("Перетащите файл на окно");
        }
    }
    if !shell.cfg.bg_path.is_empty() {
        ui.add_space(6.0);
        let (br, _) = ui.allocate_exact_size(Vec2::new(140.0, 30.0), Sense::hover());
        if button(
            ui,
            br,
            tr(lang, "Shell_BgClear"),
            BtnKind::Plain,
            accent,
        ) {
            shell.cfg.bg_path.clear();
            shell.load_bg();
        }
    }

    group(ui, tr(lang, "Shell_BgProc"));
    let mut blur = shell.cfg.bg_blur;
    if slider_row(ui, tr(lang, "Shell_Blur"), &mut blur, 0..=30, " px", accent) {
        shell.cfg.bg_blur = blur;
        shell.schedule_blur();
    }
    let mut dim = shell.cfg.bg_dim;
    if slider_row(ui, tr(lang, "Shell_Dim"), &mut dim, 0..=90, "%", accent) {
        shell.cfg.bg_dim = dim;
    }
    let mut op = shell.cfg.card_opacity;
    if slider_row(ui, tr(lang, "Shell_CardOp"), &mut op, 40..=100, "%", accent) {
        shell.cfg.card_opacity = op;
    }

    group(ui, tr(lang, "Shell_Accent"));
    // Линейка свотчей.
    let sw = 30.0;
    let gap = 10.0;
    let total = ACCENTS.len() as f32 * sw + (ACCENTS.len() - 1) as f32 * gap;
    let (line, _) = ui.allocate_exact_size(Vec2::new(total.max(ui.available_width()), 34.0), Sense::hover());
    let p = ui.painter();
    for (i, hex) in ACCENTS.iter().enumerate() {
        let c = super::config::parse_hex(hex).unwrap_or(theme::GOOD);
        let r = Rect::from_min_size(
            Pos2::new(line.left() + i as f32 * (sw + gap), line.top()),
            Vec2::splat(sw),
        );
        let resp = ui.interact(r, ui.scope_id().with(("swatch", i)), Sense::click());
        let active = shell.cfg.accent.eq_ignore_ascii_case(hex);
        if resp.hovered() && !active {
            p.circle_filled(r.center(), sw * 0.5 + 2.0, theme::LINE_SOFT);
        }
        p.circle_filled(r.center(), sw * 0.5, c);
        if active {
            p.circle_stroke(r.center(), sw * 0.5 + 1.5, Stroke::new(2.0, theme::FG));
            p.circle_stroke(r.center(), sw * 0.5 + 4.0, Stroke::new(1.0, theme::SIDEBAR));
        }
        if resp.clicked() {
            shell.cfg.accent = hex.to_string();
        }
    }
}

fn page_account(shell: &mut Shell, ui: &mut Ui, accent: Color32) {
    let lang = shell.lang;
    let soon = tr(lang, "Shell_AccSoon");

    group(ui, tr(lang, "Shell_AccInfo"));

    // Отображаемое имя.
    let r = row(ui, tr(lang, "Shell_AccDisplay"), Some(tr(lang, "Shell_AccDisplaySub")));
    value(ui, r, "—");
    if button(ui, right_at(r, 74.0, 30.0).translate(Vec2::new(-66.0, 0.0)), tr(lang, "Shell_Edit"), BtnKind::Plain, accent)
    {
        shell.toast(soon);
    }

    // Логин.
    let r = row(ui, tr(lang, "Shell_AccUsername"), Some(tr(lang, "Shell_AccUsernameSub")));
    value(ui, r, "—");
    if button(ui, right_at(r, 74.0, 30.0).translate(Vec2::new(-66.0, 0.0)), tr(lang, "Shell_Edit"), BtnKind::Plain, accent)
    {
        shell.toast(soon);
    }

    // Телефон.
    let r = row(ui, tr(lang, "Shell_AccPhone"), Some(tr(lang, "Shell_AccNotLinked")));
    if button(ui, right_at(r, 64.0, 30.0), tr(lang, "Shell_Add"), BtnKind::Plain, accent) {
        shell.toast(soon);
    }

    // Почта.
    let r = row(ui, tr(lang, "Shell_AccEmail"), None);
    value(ui, r, "—");
    if button(ui, right_at(r, 74.0, 30.0).translate(Vec2::new(-66.0, 0.0)), tr(lang, "Shell_Edit"), BtnKind::Plain, accent)
    {
        shell.toast(soon);
    }

    // Пароль.
    let r = row(ui, tr(lang, "Shell_AccPass"), Some(tr(lang, "Shell_AccPassSub")));
    value(ui, r, "••••••••");
    if button(ui, right_at(r, 74.0, 30.0).translate(Vec2::new(-86.0, 0.0)), tr(lang, "Shell_Edit"), BtnKind::Plain, accent)
    {
        shell.toast(soon);
    }

    // Возраст / день рождения — только значения.
    let r = row(ui, tr(lang, "Shell_AccAge"), None);
    value(ui, r, "—");
    let r = row(ui, tr(lang, "Shell_AccBirthday"), None);
    value(ui, r, "—");

    // Пол — сегмент.
    let r = row(ui, tr(lang, "Shell_AccGender"), None);
    let sel = shell.cfg.acc_gender.min(1);
    let top = r.center().y - 16.0;
    if let Some(n) = seg(ui, top, r, &[tr(lang, "Shell_Male"), tr(lang, "Shell_Female")], sel, accent)
    {
        shell.cfg.acc_gender = n;
    }

    // Язык.
    let r = row(ui, tr(lang, "Shell_AccLang"), None);
    let mut lang_sel = if shell.lang == crate::i18n::Lang::En { 1 } else { 0 };
    combo(
        ui,
        right_at(r, 180.0, 32.0),
        "set_lang",
        &[tr(lang, "Shell_Ru"), tr(lang, "Shell_En")],
        &mut lang_sel,
    );
    let want = if lang_sel == 1 { crate::i18n::Lang::En } else { crate::i18n::Lang::Ru };
    if want != shell.lang {
        shell.lang = want;
        shell.cfg.lang = want.code().to_string();
    }
}

fn page_privacy(shell: &mut Shell, ui: &mut Ui, accent: Color32) {
    let lang = shell.lang;

    ui.label(
        egui::RichText::new(tr(lang, "Shell_PvHint"))
            .size(11.5)
            .color(theme::FG_DIMMER),
    );
    ui.add_space(6.0);

    group(ui, "Content Restrictions");

    let r = row(
        ui,
        "Content maturity",
        Some(tr(lang, "Shell_PvMaturitySub")),
    );
    let top = r.center().y - 16.0;
    if let Some(n) = seg(
        ui,
        top,
        r,
        &pv::MATURITY,
        shell.cfg.privacy.maturity.min(2),
        accent,
    ) {
        shell.cfg.privacy.maturity = n;
    }

    // Заблокированные плейсы.
    let r = row(ui, "Blocked games", None);
    if button(ui, right_at(r, 64.0, 30.0), tr(lang, "Shell_Add"), BtnKind::Plain, accent) {
        shell.toast("Фаза B");
    }
    if shell.cfg.blocked_places.is_empty() {
        dashed_box(ui, tr(lang, "Shell_PvEmpty"));
    } else {
        for id in shell.cfg.blocked_places.clone() {
            list_item(ui, &id, "—", tr(lang, "Shell_Add"), accent, true);
        }
    }

    // Разрешённые плейсы.
    let r = row(ui, "Allowed games", None);
    if button(ui, right_at(r, 64.0, 30.0), tr(lang, "Shell_Add"), BtnKind::Plain, accent) {
        shell.toast("Фаза B");
    }
    if shell.cfg.allowed_places.is_empty() {
        dashed_box(ui, tr(lang, "Shell_PvAllowedEmpty"));
    } else {
        for id in shell.cfg.allowed_places.clone() {
            list_item(ui, &id, "—", tr(lang, "Shell_Add"), accent, true);
        }
    }

    let r = row(ui, "Preview games", Some(tr(lang, "Shell_PvPreviewSub")));
    combo(
        ui,
        right_at(r, 180.0, 32.0),
        "set_preview",
        &pv::PREVIEW,
        &mut shell.cfg.privacy.preview,
    );

    group(ui, "Communication");

    let r = row(ui, "Chat and party with friends", Some(tr(lang, "Shell_PvChatSub")));
    if toggle(ui, right_at(r, 38.0, 21.0), shell.cfg.privacy.chat, accent) {
        shell.cfg.privacy.chat = !shell.cfg.privacy.chat;
    }
    let r = row(ui, "Camera Input", Some(tr(lang, "Shell_PvCameraSub")));
    if toggle(ui, right_at(r, 38.0, 21.0), shell.cfg.privacy.camera, accent) {
        shell.cfg.privacy.camera = !shell.cfg.privacy.camera;
    }
    let r = row(ui, "Live collaboration in Studio", Some(tr(lang, "Shell_PvStudioSub")));
    if toggle(ui, right_at(r, 38.0, 21.0), shell.cfg.privacy.studio, accent) {
        shell.cfg.privacy.studio = !shell.cfg.privacy.studio;
    }

    group(ui, "Visibility & Private Servers");

    let r = row(ui, "Online status", Some(tr(lang, "Shell_PvOnlineSub")));
    combo(
        ui,
        right_at(r, 180.0, 32.0),
        "set_online",
        &pv::VISIBILITY,
        &mut shell.cfg.privacy.online,
    );
    let r = row(ui, "Show current game", Some(tr(lang, "Shell_PvShowGameSub")));
    combo(
        ui,
        right_at(r, 180.0, 32.0),
        "set_showgame",
        &pv::VISIBILITY,
        &mut shell.cfg.privacy.show_game,
    );
    let r = row(ui, "Share activity updates", Some(tr(lang, "Shell_PvActivitySub")));
    if toggle(ui, right_at(r, 38.0, 21.0), shell.cfg.privacy.activity, accent) {
        shell.cfg.privacy.activity = !shell.cfg.privacy.activity;
    }
    let r = row(ui, "Private servers", Some(tr(lang, "Shell_PvServersSub")));
    combo(
        ui,
        right_at(r, 180.0, 32.0),
        "set_servers",
        &pv::SERVERS,
        &mut shell.cfg.privacy.servers,
    );

    group(ui, "Friends & Contacts");

    let r = row(ui, "Search & sync contacts", Some(tr(lang, "Shell_PvContactsSub")));
    if toggle(ui, right_at(r, 38.0, 21.0), shell.cfg.privacy.contacts, accent) {
        shell.cfg.privacy.contacts = !shell.cfg.privacy.contacts;
    }

    group(ui, "Trading & Inventory");

    let r = row(ui, "Trading & inventory", Some(tr(lang, "Shell_PvTradingSub")));
    combo(
        ui,
        right_at(r, 180.0, 32.0),
        "set_trading",
        &pv::TRADING,
        &mut shell.cfg.privacy.trading,
    );

    group(ui, "Blocked Users");
    dashed_box(ui, tr(lang, "Shell_PvEmpty"));
}

fn page_launch(shell: &mut Shell, ui: &mut Ui, accent: Color32) {
    let lang = shell.lang;

    group(ui, tr(lang, "Shell_LgProtocol"));

    let r = row(
        ui,
        tr(lang, "Shell_LgDeeplink"),
        Some(tr(lang, "Shell_LgDeeplinkSub")),
    );
    if toggle(ui, right_at(r, 38.0, 21.0), shell.cfg.deeplink_shell, accent) {
        shell.cfg.deeplink_shell = !shell.cfg.deeplink_shell;
    }
    let r = row(ui, tr(lang, "Shell_LgTray"), Some(tr(lang, "Shell_LgTraySub")));
    if toggle(ui, right_at(r, 38.0, 21.0), shell.cfg.minimize_tray, accent) {
        shell.cfg.minimize_tray = !shell.cfg.minimize_tray;
    }
    let r = row(ui, tr(lang, "Shell_LgClose"), Some(tr(lang, "Shell_LgCloseSub")));
    if toggle(ui, right_at(r, 38.0, 21.0), shell.cfg.close_on_exit, accent) {
        shell.cfg.close_on_exit = !shell.cfg.close_on_exit;
    }

    group(ui, tr(lang, "Shell_LgClient"));

    let r = row(
        ui,
        tr(lang, "Shell_LgChannel"),
        Some(tr(lang, "Shell_LgChannelSub")),
    );
    let cur = shell.lc.channel.clone();
    let mut sel = match cur.as_str() {
        "BETA" => 1,
        "ZNext" => 2,
        _ => 0,
    };
    combo(
        ui,
        right_at(r, 180.0, 32.0),
        "set_channel",
        &["LIVE", "BETA", "ZNext"],
        &mut sel,
    );
    let want = ["LIVE", "BETA", "ZNext"][sel];
    if shell.lc.channel != want {
        shell.lc.channel = want.to_string();
        shell.lc.save();
    }
}

fn page_about(shell: &mut Shell, ui: &mut Ui, accent: Color32) {
    let lang = shell.lang;

    group(ui, tr(lang, "Shell_AbInfo"));

    let r = row(ui, tr(lang, "Shell_AbVersion"), None);
    value(ui, r, concat!(env!("CARGO_PKG_VERSION"), "-shell"));
    let r = row(ui, tr(lang, "Shell_AbRepo"), None);
    value(ui, r, "mesorg-dev/ByteStrap");
    let r = row(ui, tr(lang, "Shell_AbLicense"), None);
    value(ui, r, "GPL-3.0");

    ui.add_space(14.0);
    let (br, _) = ui.allocate_exact_size(Vec2::new(190.0, 34.0), Sense::hover());
    if button(ui, br, tr(lang, "Shell_AbOpen"), BtnKind::Accent, accent) {
        ui.ctx().open_url(egui::OpenUrl::new_tab(
            "https://github.com/mesorg-dev/ByteStrap",
        ));
    }
}

fn dashed_box(ui: &mut Ui, text: &str) {
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, 52.0), Sense::hover());
    let p = ui.painter();
    let s = Stroke::new(1.0, theme::LINE);
    let corners = [
        [r.left_top(), r.right_top()],
        [r.right_top(), r.right_bottom()],
        [r.right_bottom(), r.left_bottom()],
        [r.left_bottom(), r.left_top()],
    ];
    for [a, b] in corners {
        p.add(egui::epaint::Shape::dashed_line(&[a, b], s, 6.0, 5.0));
    }
    p.text(
        r.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::new(12.5, FontFamily::Proportional),
        theme::FG_DIMMER,
    );
    ui.add_space(8.0);
}

fn list_item(ui: &mut Ui, title: &str, sub: &str, action: &str, accent: Color32, danger: bool) {
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, 52.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, 8.0, theme::PANEL);
    p.rect_stroke(r, 8.0, Stroke::new(1.0, theme::LINE), StrokeKind::Inside);
    p.text(
        Pos2::new(r.left() + 16.0, r.top() + 10.0),
        Align2::LEFT_TOP,
        title,
        theme::bold(13.0),
        theme::FG,
    );
    p.text(
        Pos2::new(r.left() + 16.0, r.top() + 30.0),
        Align2::LEFT_TOP,
        sub,
        FontId::new(11.5, FontFamily::Monospace),
        theme::FG_DIMMER,
    );
    let kind = if danger { BtnKind::Danger } else { BtnKind::Plain };
    if button(ui, right_at(r, 84.0, 30.0), action, kind, accent) {
        // Фаза B: реальные списки блокировок.
    }
    ui.add_space(6.0);
}
