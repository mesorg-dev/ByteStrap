use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Frame, Stroke, TextStyle,
};

pub const BG: Color32 = Color32::from_rgb(0x0A, 0x0A, 0x0B);
pub const SIDEBAR: Color32 = Color32::from_rgb(0x0D, 0x0D, 0x0F);
pub const PANEL: Color32 = Color32::from_rgb(0x12, 0x12, 0x14);
pub const LINE: Color32 = Color32::from_rgb(0x2A, 0x2A, 0x2E);
pub const LINE_SOFT: Color32 = Color32::from_rgb(0x1C, 0x1C, 0x1F);
pub const FG: Color32 = Color32::from_rgb(0xF2, 0xF2, 0xEF);
pub const FG_DIM: Color32 = Color32::from_rgb(0x98, 0x98, 0x9E);
pub const FG_DIMMER: Color32 = Color32::from_rgb(0x5C, 0x5C, 0x62);
pub const GOOD: Color32 = Color32::from_rgb(0x78, 0xDC, 0x82);
pub const BAD: Color32 = Color32::from_rgb(0xFF, 0x80, 0x87);
pub const WARN: Color32 = Color32::from_rgb(0xFF, 0xC8, 0x57);

pub const R_CARD: f32 = 6.0;
pub const R_SOFT: f32 = 2.0;

pub fn card_frame() -> Frame {
    Frame::NONE
        .fill(PANEL)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(R_CARD)
        .inner_margin(egui::Margin::same(16))
}

pub fn apply(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "inter".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Inter-Regular.ttf")).into(),
    );
    fonts.font_data.insert(
        "inter_bold".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Inter-Bold.ttf")).into(),
    );
    let prop = fonts.families.get_mut(&FontFamily::Proportional).unwrap();
    prop.insert(0, "inter".into());
    prop.push("inter_bold".into());

    fonts.font_data.insert(
        "jbmono".into(),
        egui::FontData::from_static(include_bytes!(
            "../assets/fonts/JetBrainsMono-Regular.ttf"
        ))
        .into(),
    );
    fonts.font_data.insert(
        "jbmono_bold".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf"))
            .into(),
    );
    let mono = fonts.families.get_mut(&FontFamily::Monospace).unwrap();
    mono.insert(0, "jbmono".into());
    mono.push("jbmono_bold".into());

    fonts.font_data.insert(
        "unbounded".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Unbounded-Regular.ttf")).into(),
    );
    fonts.font_data.insert(
        "unbounded_bold".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Unbounded-Bold.ttf")).into(),
    );
    let heading = fonts
        .families
        .entry(FontFamily::Name("heading".into()))
        .or_default();
    heading.push("unbounded_bold".into());
    heading.push("unbounded".into());

    ctx.set_fonts(fonts);

    let mut style = egui::Style::default();

    style.text_styles = [
        (
            TextStyle::Small,
            FontId::new(11.5, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(13.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Heading,
            FontId::new(22.0, FontFamily::Name("heading".into())),
        ),
        (
            TextStyle::Monospace,
            FontId::new(12.0, FontFamily::Monospace),
        ),
    ]
    .into();

    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = Color32::from_rgb(0x0E, 0x0E, 0x10);
    v.faint_bg_color = LINE_SOFT;
    v.override_text_color = Some(FG);
    v.selection.bg_fill = GOOD.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, GOOD);
    v.widgets.noninteractive.bg_fill = PANEL;
    v.widgets.noninteractive.weak_bg_fill = PANEL;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, FG_DIM);
    v.widgets.noninteractive.corner_radius = CornerRadius::same(R_SOFT as u8);
    v.widgets.inactive.bg_fill = PANEL;
    v.widgets.inactive.weak_bg_fill = PANEL;
    // Рамка чекбокса/кнопки/инпута в покое: у egui тут default = нет рамки,
    // а fill совпадает с карточкой — квадрат был невидим.
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, FG_DIMMER);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, FG);
    v.widgets.inactive.corner_radius = CornerRadius::same(R_SOFT as u8);
    v.widgets.inactive.expansion = 0.0;
    v.widgets.hovered.bg_fill = LINE_SOFT;
    v.widgets.hovered.weak_bg_fill = LINE_SOFT;
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, FG);
    v.widgets.hovered.corner_radius = CornerRadius::same(R_SOFT as u8);
    v.widgets.hovered.expansion = 0.0;
    v.widgets.active.bg_fill = LINE;
    v.widgets.active.weak_bg_fill = LINE;
    v.widgets.active.fg_stroke = Stroke::new(1.0, FG);
    v.widgets.active.corner_radius = CornerRadius::same(R_SOFT as u8);
    v.widgets.active.expansion = 0.0;
    v.window_corner_radius = CornerRadius::same(R_CARD as u8);
    v.window_stroke = Stroke::new(1.0, LINE);
    v.menu_corner_radius = CornerRadius::same(R_CARD as u8);
    v.slider_trailing_fill = true;
    v.striped = false;
    style.visuals = v;

    style.spacing.item_spacing = egui::vec2(8.0, 7.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.window_margin = egui::Margin::same(10);
    style.spacing.menu_margin = egui::Margin::same(8);
    style.spacing.indent = 16.0;
    // Чекбокс/радио: стандартные 14px слишком мелкие.
    style.spacing.icon_width = 18.0;
    style.spacing.icon_width_inner = 11.0;

    ctx.set_global_style(style);
}
