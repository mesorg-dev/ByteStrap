//! Каркас окна шелла: фон, заголовок, главная с карточками, тосты.

use eframe::egui::{
    self, Align2, Color32, CursorIcon, FontFamily, FontId, Frame, Id, LayerId, Order, Painter,
    Panel, Pos2, Rect, Sense, Stroke, StrokeKind, TextEdit, Vec2,
};

use crate::i18n::{fmt, tr};
use crate::theme;
use crate::ui::icons;

use super::{Card, NetState, Shell};

const HEADER_H: f32 = 54.0;
const MEDIA_H: f32 = 118.0;
const CARD_H: f32 = 202.0;
const GAP: f32 = 16.0;
const MIN_CARD_W: f32 = 218.0;

// ---------------------------------------------------------------- фон

/// Фон окна: картинка (или дефолтный градиент с подсветками) + затемнение.
pub fn paint_background(shell: &Shell, ctx: &egui::Context) {
    let screen = ctx.content_rect();
    let p = ctx.layer_painter(LayerId::new(Order::Background, Id::unique("shell_bg")));

    match &shell.bg_tex {
        Some(tex) => {
            p.image(
                tex.id(),
                screen,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        None => {
            p.rect_filled(screen, 0.0, theme::BG);
            let a = shell.accent();
            radial_glow(
                &p,
                Pos2::new(screen.width() * 0.85, -screen.height() * 0.10),
                760.0,
                [a.r(), a.g(), a.b()],
                0.11,
            );
            radial_glow(
                &p,
                Pos2::new(-screen.width() * 0.10, screen.height() * 1.10),
                620.0,
                [90, 110, 255],
                0.08,
            );
        }
    }

    let dim = (shell.cfg.bg_dim.clamp(0, 90) * 255 / 100) as u8;
    p.rect_filled(screen, 0.0, Color32::from_rgba_unmultiplied(6, 6, 8, dim));
}

/// Мягкая радиальная подсветка: 16 концентрических кругов с низкой альфой.
fn radial_glow(p: &Painter, center: Pos2, radius: f32, rgb: [u8; 3], intensity: f32) {
    const STEPS: usize = 16;
    let step_a = ((intensity / STEPS as f32) * 255.0).round().clamp(1.0, 255.0) as u8;
    for i in (0..STEPS).rev() {
        let t = (i + 1) as f32 / STEPS as f32;
        p.circle_filled(
            center,
            radius * t,
            Color32::from_rgba_unmultiplied(rgb[0], rgb[1], rgb[2], step_a),
        );
    }
}

// ------------------------------------------------------------- заголовок

pub fn header(shell: &mut Shell, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let accent = shell.accent();
    let lang = shell.lang;

    Panel::top("shell_header")
        .exact_size(HEADER_H)
        .frame(Frame::NONE.fill(Color32::from_rgba_unmultiplied(13, 13, 15, 209)))
        .show(ui, |ui| {
            let rect = ui.max_rect();
            let p = ui.painter().clone();
            p.line_segment(
                [rect.left_bottom(), rect.right_bottom()],
                Stroke::new(1.0, theme::LINE),
            );

            // Перетаскивание окна / двойной клик = развёрнуть.
            let drag = ui.interact(
                rect,
                ui.scope_id().with("shell_header_drag"),
                Sense::click_and_drag(),
            );
            if drag.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if drag.double_clicked() {
                let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }
            if drag.hovered() {
                ctx.set_cursor_icon(CursorIcon::Default);
            }

            // Логотип: mark + «ByteStrap» + «SHELL».
            let mark = Rect::from_min_size(rect.min + Vec2::new(18.0, 15.0), Vec2::splat(24.0));
            p.rect_filled(mark, 7.0, accent);
            p.text(
                mark.center(),
                Align2::CENTER_CENTER,
                "B",
                theme::bold(13.0),
                theme::BG,
            );
            let name = p.layout_no_wrap(
                "ByteStrap".to_string(),
                theme::bold(14.0),
                theme::FG,
            );
            let name_pos = Pos2::new(mark.right() + 10.0, rect.center().y - name.size().y * 0.5);
            p.galley(name_pos, name.clone(), theme::FG);
            let shell_word = p.layout_no_wrap(
                tr(lang, "Shell_AbShell").to_uppercase(),
                FontId::new(10.5, FontFamily::Proportional),
                theme::FG_DIMMER,
            );
            p.galley(
                Pos2::new(
                    name_pos.x + name.size().x + 7.0,
                    rect.center().y + 0.5 - shell_word.size().y * 0.5,
                ),
                shell_word,
                theme::FG_DIMMER,
            );

            // Поиск.
            let sx = 232.0;
            let srect = Rect::from_min_size(Pos2::new(sx, 10.0), Vec2::new(380.0, 34.0));
            p.rect_filled(srect, 8.0, theme::PANEL);
            p.rect_stroke(srect, 8.0, Stroke::new(1.0, theme::LINE), StrokeKind::Inside);
            let search_resp = ui.put(
                Rect::from_min_size(srect.min + Vec2::new(34.0, 0.0), Vec2::new(336.0, 34.0)),
                TextEdit::singleline(&mut shell.search)
                    .hint_text(tr(lang, "Shell_Search"))
                    .frame(egui::Frame::NONE)
                    .desired_width(336.0),
            );
            if search_resp.has_focus() {
                p.rect_stroke(
                    srect,
                    8.0,
                    Stroke::new(1.0, accent),
                    StrokeKind::Inside,
                );
                p.rect_stroke(
                    srect.expand(3.0),
                    10.0,
                    Stroke::new(3.0, accent.gamma_multiply(0.14)),
                    StrokeKind::Inside,
                );
            }
            icons::search(
                ui,
                Rect::from_min_size(srect.min + Vec2::new(11.0, 9.0), Vec2::splat(16.0)),
                theme::FG_DIMMER,
            );

            // Оконные кнопки.
            let btn = Vec2::new(46.0, HEADER_H);
            let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
            let buttons: [(egui::ViewportCommand, &str, &str); 3] = [
                (egui::ViewportCommand::Minimized(true), "—", tr(lang, "Win_Min")),
                (
                    egui::ViewportCommand::Maximized(!maximized),
                    if maximized { "❐" } else { "▢" },
                    tr(lang, "Win_Max"),
                ),
                (egui::ViewportCommand::Close, "✕", tr(lang, "Win_Close")),
            ];
            let mut x = rect.right() - btn.x * 3.0;
            for (cmd, glyph, tip) in buttons {
                let r = Rect::from_min_size(Pos2::new(x, rect.top()), btn);
                let resp = ui.interact(r, ui.scope_id().with(("winbtn", glyph)), Sense::click());
                let hov = resp.hovered();
                let fill = if hov && glyph == "✕" {
                    theme::BAD
                } else if hov {
                    theme::LINE_SOFT
                } else {
                    Color32::TRANSPARENT
                };
                p.rect_filled(r, 0.0, fill);
                p.text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    glyph,
                    FontId::new(13.0, FontFamily::Proportional),
                    if hov && glyph == "✕" {
                        theme::FG
                    } else {
                        theme::FG_DIM
                    },
                );
                if hov {
                    resp.clone().on_hover_text(tip);
                }
                if resp.clicked() {
                    ctx.send_viewport_cmd(cmd);
                }
                x += btn.x;
            }

            // Кнопка «Настройки».
            let gear = Rect::from_min_size(
                Pos2::new(rect.right() - 138.0 - 10.0 - 34.0, 10.0),
                Vec2::splat(34.0),
            );
            let gear_resp = ui.interact(
                gear,
                ui.scope_id().with("shell_gear"),
                Sense::click(),
            );
            let gh = gear_resp.hovered();
            p.rect_filled(gear, 6.0, if gh { theme::LINE_SOFT } else { theme::PANEL });
            p.rect_stroke(
                gear,
                6.0,
                Stroke::new(1.0, if gh { theme::FG_DIMMER } else { theme::LINE }),
                StrokeKind::Inside,
            );
            icons::gear(
                ui,
                Rect::from_center_size(gear.center(), Vec2::splat(16.0)),
                if gh { theme::FG } else { theme::FG_DIM },
            );
            if gear_resp.clicked() {
                shell.settings_open = true;
            }

            // Чип пользователя.
            let nick = tr(lang, "Shell_Guest");
            let g_nick = p.layout_no_wrap(nick.to_string(), FontId::new(13.0, FontFamily::Proportional), theme::FG);
            let pill_w = 4.0 + 27.0 + 9.0 + g_nick.size().x + 14.0;
            let user = Rect::from_min_size(
                Pos2::new(gear.left() - 10.0 - pill_w, 10.5),
                Vec2::new(pill_w, 33.0),
            );
            let user_resp = ui.interact(user, ui.scope_id().with("shell_user"), Sense::click());
            let uh = user_resp.hovered();
            p.rect_filled(user, 999.0, theme::PANEL);
            p.rect_stroke(
                user,
                999.0,
                Stroke::new(1.0, if uh { theme::FG_DIMMER } else { theme::LINE }),
                StrokeKind::Inside,
            );
            let av = Rect::from_min_size(user.min + Vec2::new(3.0, 3.0), Vec2::splat(27.0));
            p.circle_filled(av.center(), 13.5, Color32::from_rgb(0x3D, 0x5A, 0x80));
            p.text(
                av.center(),
                Align2::CENTER_CENTER,
                nick.chars().next().unwrap_or('G').to_string(),
                theme::bold(12.0),
                Color32::from_rgb(0xE0, 0xFB, 0xFC),
            );
            p.galley(
                Pos2::new(av.right() + 9.0, user.center().y - g_nick.size().y * 0.5),
                g_nick,
                theme::FG,
            );
            if user_resp.clicked() {
                shell.toast(tr(lang, "Shell_ProfileToast"));
            }

            // Статус сети.
            let (net_text, net_color) = match shell.net {
                NetState::Checking => (tr(lang, "Shell_Checking"), theme::WARN),
                NetState::Online => (tr(lang, "Shell_Online"), accent),
                NetState::Offline => (tr(lang, "Shell_Offline"), theme::BAD),
            };
            let g_net = p.layout_no_wrap(
                net_text.to_string(),
                FontId::new(11.0, FontFamily::Proportional),
                theme::FG_DIM,
            );
            let net_w = 11.0 + 6.0 + 7.0 + g_net.size().x + 11.0;
            let net = Rect::from_min_size(
                Pos2::new(user.left() - 10.0 - net_w, 12.0),
                Vec2::new(net_w, 29.0),
            );
            p.rect_filled(net, 999.0, theme::PANEL);
            p.rect_stroke(net, 999.0, Stroke::new(1.0, theme::LINE), StrokeKind::Inside);
            let pulse = if shell.net == NetState::Online {
                let t = ctx.input(|i| i.time);
                0.55 + 0.45 * (t * 2.9).sin().abs()
            } else {
                1.0
            };
            p.circle_filled(
                net.left_center() + Vec2::new(14.0, 0.0),
                3.0,
                net_color.gamma_multiply(pulse as f32),
            );
            p.galley(
                Pos2::new(net.left() + 24.0, net.center().y - g_net.size().y * 0.5),
                g_net,
                theme::FG_DIM,
            );
        });
}

// --------------------------------------------------------------- главная

pub fn home(shell: &mut Shell, ui: &mut egui::Ui) {
    let accent = shell.accent();
    let lang = shell.lang;

    ui.add_space(24.0);
    section(
        shell,
        ui,
        tr(lang, "Shell_Recent"),
        tr(lang, "Shell_RecentSub"),
        true,
        accent,
    );
    ui.add_space(30.0);
    section(
        shell,
        ui,
        tr(lang, "Shell_Reco"),
        tr(lang, "Shell_RecoSub"),
        false,
        accent,
    );
    ui.add_space(30.0);
}

#[allow(clippy::too_many_arguments)]
fn section(
    shell: &mut Shell,
    ui: &mut egui::Ui,
    title: &str,
    sub: &str,
    is_recent: bool,
    accent: Color32,
) {
    let lang = shell.lang;
    let avail = ui.available_width();

    // Шапка секции: заголовок + подзаголовок + кнопка «Смотреть все».
    let head_h = 46.0;
    let (head, _) = ui.allocate_exact_size(Vec2::new(avail, head_h), Sense::hover());
    let p = ui.painter().clone();
    p.text(
        head.left_top() + Vec2::new(0.0, 8.0),
        Align2::LEFT_TOP,
        title.to_uppercase(),
        theme::bold(12.0),
        theme::FG,
    );
    p.text(
        head.left_top() + Vec2::new(0.0, 26.0),
        Align2::LEFT_TOP,
        sub,
        FontId::new(11.0, FontFamily::Proportional),
        theme::FG_DIMMER,
    );
    let g_see = p.layout_no_wrap(
        tr(lang, "Shell_SeeAll").to_string(),
        FontId::new(12.0, FontFamily::Proportional),
        theme::FG_DIM,
    );
    let see_w = g_see.size().x + 24.0;
    let see = Rect::from_min_size(
        Pos2::new(head.right() - see_w, head.top() + 4.0),
        Vec2::new(see_w, 26.0),
    );
    let see_resp = ui.interact(see, ui.scope_id().with(("see_all", title)), Sense::click());
    let sh = see_resp.hovered();
    p.rect_filled(
        see,
        6.0,
        if sh { theme::PANEL } else { Color32::TRANSPARENT },
    );
    p.rect_stroke(
        see,
        6.0,
        Stroke::new(1.0, theme::LINE),
        StrokeKind::Inside,
    );
    p.galley(
        Pos2::new(
            see.center().x - g_see.size().x * 0.5,
            see.center().y - g_see.size().y * 0.5,
        ),
        g_see,
        if sh { theme::FG } else { theme::FG_DIM },
    );
    if see_resp.clicked() {
        shell.toast(tr(lang, "Shell_SeeAllToast"));
    }
    p.line_segment(
        [head.left_bottom(), head.right_bottom()],
        Stroke::new(1.0, theme::LINE_SOFT),
    );

    // Карточки секции с фильтром поиска.
    let cards: Vec<Card> = if is_recent {
        shell.recent.clone()
    } else {
        shell
            .reco
            .iter()
            .filter(|c| !shell.recent.iter().any(|r| r.id == c.id))
            .cloned()
            .collect()
    };
    let q = shell.search.trim().to_lowercase();
    let visible: Vec<&Card> = cards
        .iter()
        .filter(|c| {
            q.is_empty()
                || c.name.to_lowercase().contains(&q)
                || c.id.contains(&q)
        })
        .collect();

    if visible.is_empty() {
        let msg = if is_recent && cards.is_empty() {
            tr(lang, "Shell_NoRecent")
        } else {
            tr(lang, "Shell_Empty")
        };
        let (r, _) = ui.allocate_exact_size(Vec2::new(avail, 88.0), Sense::hover());
        dashed_rect(&p, r, theme::LINE);
        p.text(
            r.center(),
            Align2::CENTER_CENTER,
            msg,
            FontId::new(13.0, FontFamily::Proportional),
            theme::FG_DIMMER,
        );
        return;
    }

    // Сетка: auto-fill minmax(218px, 1fr) как в мокапе.
    let cols = (((avail + GAP) / (MIN_CARD_W + GAP)).floor() as usize).max(1);
    let cw = (avail - GAP * (cols as f32 - 1.0)) / cols as f32;
    let rows = visible.len().div_ceil(cols);
    let grid_h = rows as f32 * CARD_H + (rows as f32 - 1.0) * GAP;
    let (grid, _) = ui.allocate_exact_size(Vec2::new(avail, grid_h), Sense::hover());

    for (i, card) in visible.iter().enumerate() {
        let col = (i % cols) as f32;
        let row = (i / cols) as f32;
        let r = Rect::from_min_size(
            Pos2::new(
                grid.left() + col * (cw + GAP),
                grid.top() + row * (CARD_H + GAP),
            ),
            Vec2::new(cw, CARD_H),
        );
        card_widget(shell, ui, r, card, accent);
    }
}

/// Карточка места: обложка (или фолбэк-градиент), счётчик, кнопка «Играть».
fn card_widget(shell: &mut Shell, ui: &mut egui::Ui, rect: Rect, card: &Card, accent: Color32) {
    let resp = ui.interact(
        rect,
        ui.scope_id().with(("shell_card", card.id.as_str())),
        Sense::click(),
    );
    let p = ui.painter().clone();
    let hovered = resp.hovered();
    let lift = if hovered { 3.0 } else { 0.0 };
    let r = rect.translate(Vec2::new(0.0, -lift));

    if hovered {
        p.rect_filled(
            r.translate(Vec2::new(0.0, 7.0)),
            8.0,
            Color32::from_black_alpha(120),
        );
    }

    // Тело карточки.
    let fill = Color32::from_rgba_unmultiplied(18, 18, 20, shell.card_alpha());
    p.rect_filled(r, 8.0, fill);
    let border = if hovered {
        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 120)
    } else {
        theme::LINE
    };
    p.rect_stroke(r, 8.0, Stroke::new(1.0, border), StrokeKind::Inside);

    // Медиа-блок.
    let media = Rect::from_min_size(r.min, Vec2::new(r.width(), MEDIA_H));
    let clip = p.with_clip_rect(media);
    match shell.thumbs.get(&card.id) {
        Some(tex) => {
            clip.image(
                tex.id(),
                media,
                cover_uv(tex.size(), media),
                Color32::WHITE,
            );
        }
        None => paint_fallback_media(&clip, media, &card.id, &card.name),
    }

    // Тег «сколько играют» поверх обложки.
    if let Some(n) = card.playing {
        let label = fmt_players(n);
        let g = clip.layout_no_wrap(
            label.clone(),
            FontId::new(10.5, FontFamily::Proportional),
            theme::FG,
        );
        let tw = g.size().x;
        let tag_w = 9.0 + 5.0 + 6.0 + tw + 9.0;
        let tag = Rect::from_min_size(
            Pos2::new(media.right() - 9.0 - tag_w, media.top() + 9.0),
            Vec2::new(tag_w, 22.0),
        );
        clip.rect_filled(tag, 999.0, Color32::from_rgba_unmultiplied(8, 8, 10, 199));
        clip.rect_stroke(
            tag,
            999.0,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 20)),
            StrokeKind::Inside,
        );
        clip.circle_filled(tag.left_center() + Vec2::new(13.0, 0.0), 2.5, accent);
        clip.galley(
            Pos2::new(tag.left() + 21.0, tag.center().y - g.size().y * 0.5),
            g,
            theme::FG,
        );
    }

    // Тело: название + мета-строка.
    let body_top = media.bottom();
    let title_rect = Rect::from_min_max(
        Pos2::new(r.left() + 14.0, body_top + 13.0),
        Pos2::new(r.right() - 14.0, body_top + 30.0),
    );
    paint_ellipsis(
        &p,
        Pos2::new(title_rect.left(), title_rect.center().y),
        &card.name,
        theme::bold(13.5),
        theme::FG,
        title_rect.width(),
    );

    let meta_y = body_top + 48.0;
    let play_text = tr(shell.lang, "Shell_Play");
    let g_play = p.layout_no_wrap(
        play_text.to_uppercase(),
        theme::bold(11.0),
        accent,
    );
    let btn_w = g_play.size().x + 30.0;
    let play_rect = Rect::from_min_size(
        Pos2::new(r.right() - 14.0 - btn_w, meta_y - 14.0),
        Vec2::new(btn_w, 28.0),
    );
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let play_hov = pointer.map(|pt| play_rect.contains(pt)).unwrap_or(false);
    let play_c = Pos2::new(
        play_rect.center().x - g_play.size().x * 0.5,
        play_rect.center().y - g_play.size().y * 0.5,
    );
    if play_hov {
        p.rect_filled(play_rect, 6.0, accent);
        p.galley(play_c, g_play, theme::BG);
    } else {
        p.rect_filled(
            play_rect,
            6.0,
            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 26),
        );
        p.rect_stroke(
            play_rect,
            6.0,
            Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 90),
            ),
            StrokeKind::Inside,
        );
        p.galley(play_c, g_play, accent);
    }

    if let Some(n) = card.playing {
        let line = fmt(shell.lang, "Shell_PlayingFmt", &[&fmt_players(n)]);
        p.text(
            Pos2::new(r.left() + 14.0, meta_y),
            Align2::LEFT_CENTER,
            line,
            FontId::new(11.5, FontFamily::Proportional),
            theme::FG_DIM,
        );
    }

    if resp.clicked() {
        shell.launch(card);
    }
}

/// Фолбэк-обложка без картинки: градиент из hue места + монограмма.
fn paint_fallback_media(p: &Painter, media: Rect, id: &str, name: &str) {
    let hue = hash_hue(id);
    let top = hsl(hue, 0.60, 0.40);
    let bot = Color32::from_rgb(10, 10, 13);
    const BANDS: usize = 12;
    for i in 0..BANDS {
        let t = i as f32 / BANDS as f32;
        let y0 = media.top() + media.height() * t;
        let y1 = media.top() + media.height() * ((i + 1) as f32 / BANDS as f32) + 1.0;
        p.rect_filled(
            Rect::from_min_max(Pos2::new(media.left(), y0), Pos2::new(media.right(), y1)),
            0.0,
            lerp_color(top, bot, (t * t * 1.6).min(1.0)),
        );
    }
    let mono = name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    p.text(
        media.center(),
        Align2::CENTER_CENTER,
        mono,
        theme::bold(44.0),
        Color32::from_white_alpha(33),
    );
}

/// UV-прямоугольник «cover» (обрезка по центру под пропорции rect).
fn cover_uv(img: [usize; 2], rect: Rect) -> Rect {
    let ia = img[0] as f32 / img[1].max(1) as f32;
    let ra = rect.width() / rect.height().max(1.0);
    if ra > ia {
        let f = ia / ra;
        Rect::from_min_max(
            Pos2::new(0.0, (1.0 - f) * 0.5),
            Pos2::new(1.0, (1.0 + f) * 0.5),
        )
    } else {
        let f = ra / ia;
        Rect::from_min_max(
            Pos2::new((1.0 - f) * 0.5, 0.0),
            Pos2::new((1.0 + f) * 0.5, 1.0),
        )
    }
}

/// Текст с многоточием при переполнении.
fn paint_ellipsis(p: &Painter, pos: Pos2, text: &str, font: FontId, color: Color32, max_w: f32) {
    let mut s = text.to_string();
    let g = p.layout_no_wrap(s.clone(), font.clone(), color);
    if g.size().x <= max_w {
        p.galley(pos, g, color);
        return;
    }
    loop {
        if s.chars().count() <= 1 {
            break;
        }
        s.pop();
        let mut t = s.clone();
        t.push('…');
        let g = p.layout_no_wrap(t, font.clone(), color);
        if g.size().x <= max_w {
            p.galley(pos, g, color);
            return;
        }
    }
    let g = p.layout_no_wrap("…".to_string(), font, color);
    p.galley(pos, g, color);
}

fn dashed_rect(p: &Painter, r: Rect, color: Color32) {
    let s = Stroke::new(1.0, color);
    let corners = [
        [r.left_top(), r.right_top()],
        [r.right_top(), r.right_bottom()],
        [r.right_bottom(), r.left_bottom()],
        [r.left_bottom(), r.left_top()],
    ];
    for [a, b] in corners {
        p.add(egui::epaint::Shape::dashed_line(&[a, b], s, 6.0, 5.0));
    }
}

// ----------------------------------------------------------------- тосты

pub fn paint_toast(shell: &Shell, ctx: &egui::Context) {
    let Some((text, _)) = shell.toast.clone() else {
        return;
    };
    let screen = ctx.content_rect();
    let p = ctx.layer_painter(LayerId::new(Order::Foreground, Id::unique("shell_toast")));
    let g = p.layout_no_wrap(
        text.clone(),
        FontId::new(13.0, FontFamily::Proportional),
        theme::FG,
    );
    let w = (g.size().x + 48.0).min(screen.width() - 40.0);
    let rect = Rect::from_center_size(
        Pos2::new(screen.center().x, screen.bottom() - 28.0 - 22.0),
        Vec2::new(w, 44.0),
    );
    p.rect_filled(
        rect.translate(Vec2::new(0.0, 12.0)),
        10.0,
        Color32::from_black_alpha(90),
    );
    p.rect_filled(rect, 10.0, Color32::from_rgb(0x17, 0x17, 0x1A));
    p.rect_stroke(rect, 10.0, Stroke::new(1.0, theme::LINE), StrokeKind::Inside);
    p.galley(
        Pos2::new(
            rect.center().x - g.size().x * 0.5,
            rect.center().y - g.size().y * 0.5,
        ),
        g,
        theme::FG,
    );
}

// ------------------------------------------------------------- resize grips

pub fn resize_grips(_shell: &Shell, ui: &egui::Ui, ctx: &egui::Context) {
    let screen = ui.input(|i| {
        i.viewport()
            .inner_rect
            .unwrap_or_else(|| i.content_rect())
    });
    let m = 6.0;
    let corners: [(Rect, egui::ViewportCommand); 8] = [
        (
            Rect::from_min_size(screen.min, Vec2::splat(m * 2.0)),
            egui::ViewportCommand::BeginResize(egui::ResizeDirection::NorthWest),
        ),
        (
            Rect::from_min_size(
                Pos2::new(screen.right() - m * 2.0, screen.top()),
                Vec2::splat(m * 2.0),
            ),
            egui::ViewportCommand::BeginResize(egui::ResizeDirection::NorthEast),
        ),
        (
            Rect::from_min_size(
                Pos2::new(screen.left(), screen.bottom() - m * 2.0),
                Vec2::splat(m * 2.0),
            ),
            egui::ViewportCommand::BeginResize(egui::ResizeDirection::SouthWest),
        ),
        (
            Rect::from_min_size(screen.max - Vec2::splat(m * 2.0), Vec2::splat(m * 2.0)),
            egui::ViewportCommand::BeginResize(egui::ResizeDirection::SouthEast),
        ),
        (
            Rect::from_min_size(
                Pos2::new(screen.left(), screen.top() + m * 2.0),
                Vec2::new(m, screen.height() - m * 4.0),
            ),
            egui::ViewportCommand::BeginResize(egui::ResizeDirection::West),
        ),
        (
            Rect::from_min_size(
                Pos2::new(screen.right() - m, screen.top() + m * 2.0),
                Vec2::new(m, screen.height() - m * 4.0),
            ),
            egui::ViewportCommand::BeginResize(egui::ResizeDirection::East),
        ),
        (
            Rect::from_min_size(
                Pos2::new(screen.left() + m * 2.0, screen.top()),
                Vec2::new(screen.width() - m * 4.0, m),
            ),
            egui::ViewportCommand::BeginResize(egui::ResizeDirection::North),
        ),
        (
            Rect::from_min_size(
                Pos2::new(screen.left() + m * 2.0, screen.bottom() - m),
                Vec2::new(screen.width() - m * 4.0, m),
            ),
            egui::ViewportCommand::BeginResize(egui::ResizeDirection::South),
        ),
    ];
    for (i, (rect, cmd)) in corners.into_iter().enumerate() {
        let resp = ui.interact(rect, ui.scope_id().with(("resize", i)), Sense::click_and_drag());
        if resp.hovered() {
            ctx.set_cursor_icon(match i {
                0 | 3 => CursorIcon::ResizeNwSe,
                1 | 2 => CursorIcon::ResizeNeSw,
                4 | 5 => CursorIcon::ResizeHorizontal,
                _ => CursorIcon::ResizeVertical,
            });
        }
        if resp.drag_started() {
            ctx.send_viewport_cmd(cmd);
        }
    }
}

// -------------------------------------------------------------- утилиты

/// 420461 → «420K», 2698 → «2.7K», 729 → «729» (как в мокапе).
pub fn fmt_players(n: u64) -> String {
    if n >= 10_000 {
        format!("{}K", ((n as f32 / 1000.0).round() as u64))
    } else if n >= 1000 {
        format!("{:.1}K", n as f32 / 1000.0)
    } else {
        n.to_string()
    }
}

fn hash_hue(s: &str) -> f32 {
    let mut h: u32 = 5381;
    for b in s.bytes() {
        h = h.wrapping_mul(33).wrapping_add(u32::from(b));
    }
    (h % 360) as f32
}

fn hsl(h: f32, s: f32, l: f32) -> Color32 {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h / 60.0).rem_euclid(6.0);
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let q = |v: f32| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(q(r), q(g), q(b))
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn players_format() {
        assert_eq!(fmt_players(729), "729");
        assert_eq!(fmt_players(2698), "2.7K");
        assert_eq!(fmt_players(13_523), "14K");
        assert_eq!(fmt_players(420_461), "420K");
    }

    #[test]
    fn cover_uv_crops_top_bottom() {
        // Обложка 768×432 (1.78) в rect 218×118 (1.85): режем по вертикали.
        let uv = cover_uv([768, 432], Rect::from_min_size(Pos2::ZERO, Vec2::new(218.0, 118.0)));
        assert!(uv.top() > 0.0 && uv.bottom() < 1.0);
        assert_eq!(uv.left(), 0.0);
        assert_eq!(uv.right(), 1.0);
    }

    #[test]
    fn hue_is_stable() {
        assert_eq!(hash_hue("4924922222"), hash_hue("4924922222"));
        assert!(hash_hue("a") < 360.0);
    }
}
