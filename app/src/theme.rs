//! ScreenCraft's deep-ink palette, fonts, and the small drawing helpers every screen shares.
//! The layout maths mirror the CSS the web UI used, so sizes here are in the same points.

use std::sync::Arc;

use eframe::egui::{
    self, epaint::text::VariationCoords, vec2, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Id, Pos2, Rect,
    Response, Sense, Stroke, StrokeKind, TextStyle, Ui,
};

pub const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}
pub fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a * 255.0) as u8)
}

pub const BG: Color32 = hex(0x090d14);
pub const PANEL: Color32 = hex(0x0f1520);
pub const PANEL2: Color32 = hex(0x141c2a);
pub const RAISE: Color32 = hex(0x182132);
pub const LINE: Color32 = hex(0x1d2738);
pub const LINE2: Color32 = hex(0x2a374d);
pub const TEXT: Color32 = hex(0xe5eaf4);
pub const MUTED: Color32 = hex(0x8693aa);
pub const ACCENT: Color32 = hex(0x3b82c4);
pub const ACCENT2: Color32 = hex(0x6aaee8);
pub const DANGER: Color32 = hex(0xff5566);
pub const OK: Color32 = hex(0x3ddc97);
pub const CLIP: Color32 = hex(0xd9895e);
pub const SOFT: Color32 = hex(0xd5d9ee); // pill text
pub const ICON: Color32 = hex(0xa9b0cc); // icon-button text
pub const LABEL: Color32 = hex(0xc6cae2);

pub fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}
pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}
pub fn cap_font(name: &str, bold_flag: bool, size: f32) -> FontId {
    let fam = match (name, bold_flag) {
        ("Arial Black", _) => "arial-black",
        (_, true) => "helv-bold",
        _ => "helv",
    };
    FontId::new(size, FontFamily::Name(fam.into()))
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let load = |p: &str| std::fs::read(p).ok();
    if let Some(bytes) = load("/System/Library/Fonts/SFNSRounded.ttf").or_else(|| load("/System/Library/Fonts/SFNS.ttf")) {
        let regular = FontData::from_owned(bytes.clone());
        let mut semi = FontData::from_owned(bytes);
        semi.tweak.coords = VariationCoords::new([("wght", 640.0)]);
        fonts.font_data.insert("ui".into(), Arc::new(regular));
        fonts.font_data.insert("ui-semi".into(), Arc::new(semi));
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "ui".into());
        let mut fam = vec!["ui-semi".to_string()];
        fam.extend(fallback.clone());
        fonts.families.insert(FontFamily::Name("semibold".into()), fam);
    } else {
        fonts.families.insert(FontFamily::Name("semibold".into()), fallback.clone());
    }
    // caption faces (the exporter draws the same ones with CoreText)
    let mut caption = |key: &str, path: &str, index: u32| {
        if let Some(bytes) = std::fs::read(path).ok() {
            let mut d = FontData::from_owned(bytes);
            d.index = index;
            fonts.font_data.insert(key.into(), Arc::new(d));
            let mut fam = vec![key.to_string()];
            fam.extend(fallback.clone());
            fonts.families.insert(FontFamily::Name(key.into()), fam);
        } else {
            fonts.families.insert(FontFamily::Name(key.into()), fallback.clone());
        }
    };
    caption("arial-black", "/System/Library/Fonts/Supplemental/Arial Black.ttf", 0);
    caption("helv-bold", "/System/Library/Fonts/Supplemental/Arial Bold.ttf", 0);
    caption("helv", "/System/Library/Fonts/Supplemental/Arial.ttf", 0);
    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = vec2(8.0, 8.0);
        s.animation_time = 0.1;
        s.interaction.selectable_labels = false;
        s.spacing.scroll.floating = true;
        s.spacing.scroll.bar_width = 8.0;
        s.text_styles = [
            (TextStyle::Heading, bold(20.0)),
            (TextStyle::Body, font(13.0)),
            (TextStyle::Button, font(13.0)),
            (TextStyle::Small, font(11.0)),
            (TextStyle::Monospace, mono(12.0)),
        ]
        .into();
        let v = &mut s.visuals;
        v.dark_mode = true;
        v.panel_fill = BG;
        v.window_fill = PANEL;
        v.extreme_bg_color = BG;
        v.text_edit_bg_color = Some(BG);
        v.override_text_color = None;
        v.selection.bg_fill = alpha(ACCENT, 0.35);
        v.selection.stroke = Stroke::new(1.0, ACCENT2);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        v.widgets.inactive.bg_fill = PANEL2;
        v.widgets.hovered.bg_fill = RAISE;
        v.widgets.active.bg_fill = RAISE;
        v.widgets.inactive.corner_radius = CornerRadius::same(7);
        v.widgets.hovered.corner_radius = CornerRadius::same(7);
        v.widgets.active.corner_radius = CornerRadius::same(7);
    });
}

pub fn r(n: u8) -> CornerRadius {
    CornerRadius::same(n)
}

pub fn text_w(ui: &Ui, text: &str, font: FontId) -> f32 {
    ui.painter().layout_no_wrap(text.to_string(), font, Color32::WHITE).size().x
}

/// One-pixel border drawn inside the rect.
pub fn border(ui: &Ui, rect: Rect, rad: u8, color: Color32) {
    ui.painter().rect_stroke(rect, r(rad), Stroke::new(1.0, color), StrokeKind::Inside);
}

pub fn fill(ui: &Ui, rect: Rect, rad: u8, color: Color32) {
    ui.painter().rect_filled(rect, r(rad), color);
}

pub fn label(ui: &Ui, pos: Pos2, align: Align2, text: &str, f: FontId, color: Color32) -> Rect {
    ui.painter().text(pos, align, text, f, color)
}

/// Tiny left-to-right / right-to-left placement helper (a flex row).
pub struct Row {
    pub x: f32,
    pub right: f32,
    pub cy: f32,
    pub gap: f32,
}
impl Row {
    pub fn new(rect: Rect, gap: f32) -> Self {
        Self { x: rect.left(), right: rect.right(), cy: rect.center().y, gap }
    }
    /// Reserve `w` x `h` on the left and advance.
    pub fn left(&mut self, w: f32, h: f32) -> Rect {
        let r = Rect::from_min_size(Pos2::new(self.x, self.cy - h / 2.0), vec2(w, h));
        self.x += w + self.gap;
        r
    }
    /// Reserve `w` x `h` on the right and advance leftwards.
    pub fn right(&mut self, w: f32, h: f32) -> Rect {
        let r = Rect::from_min_size(Pos2::new(self.right - w, self.cy - h / 2.0), vec2(w, h));
        self.right -= w + self.gap;
        r
    }
}

fn hot(ui: &Ui, resp: &Response) -> bool {
    let _ = ui;
    resp.hovered()
}

/// Rounded pill button (`.pill` / `.pill.small`). `on` = accent-tinted.
pub fn pill_size(ui: &Ui, text: &str, small: bool) -> egui::Vec2 {
    let f = font(if small { 12.0 } else { 13.0 });
    let (px, py) = if small { (9.0, 5.0) } else { (11.0, 7.0) };
    vec2(text_w(ui, text, f.clone()) + px * 2.0 + 2.0, if small { 12.0 } else { 13.0 } + py * 2.0 + 4.0)
}
pub fn pill(ui: &mut Ui, id: Id, rect: Rect, text: &str, on: bool, small: bool, enabled: bool) -> Response {
    let resp = ui.interact(rect, id, if enabled { Sense::click() } else { Sense::hover() });
    let hov = enabled && hot(ui, &resp);
    let (bg, bd, fg) = if on {
        (alpha(ACCENT, 0.26), ACCENT, Color32::WHITE)
    } else if hov {
        (RAISE, LINE2, SOFT)
    } else {
        (PANEL2, LINE, SOFT)
    };
    fill(ui, rect, 7, bg);
    border(ui, rect, 7, bd);
    let fg = if enabled { fg } else { alpha(fg, 0.4) };
    label(ui, rect.center(), Align2::CENTER_CENTER, text, font(if small { 12.0 } else { 13.0 }), fg);
    if enabled && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Solid accent button.
pub fn primary(ui: &mut Ui, id: Id, rect: Rect, text: &str, icon: Option<&crate::icons::Icons>, glyph: &str) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    fill(ui, rect, 7, if resp.hovered() { ACCENT2 } else { ACCENT });
    border(ui, rect, 7, alpha(Color32::WHITE, 0.16));
    let f = bold(13.0);
    let w = text_w(ui, text, f.clone());
    let iw = if icon.is_some() { 18.0 + 7.0 } else { 0.0 };
    let x0 = rect.center().x - (w + iw) / 2.0;
    if let Some(ic) = icon {
        ic.paint(ui, glyph, Rect::from_min_size(Pos2::new(x0, rect.center().y - 9.0), vec2(18.0, 18.0)), Color32::WHITE);
    }
    label(ui, Pos2::new(x0 + iw, rect.center().y), Align2::LEFT_CENTER, text, f, Color32::WHITE);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// 32x32 icon button.
pub fn icon_button(ui: &mut Ui, icons: &crate::icons::Icons, id: Id, rect: Rect, glyph: &str, on: bool, tip: &str) -> Response {
    let mut resp = ui.interact(rect, id, Sense::click());
    if resp.hovered() || on {
        fill(ui, rect, 7, if on { alpha(ACCENT, 0.28) } else { RAISE });
    }
    let c = if resp.hovered() || on { Color32::WHITE } else { ICON };
    icons.paint(ui, glyph, Rect::from_center_size(rect.center(), vec2(18.0, 18.0)), c);
    if !tip.is_empty() {
        resp = resp.on_hover_text(tip);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Segmented control. Returns the clicked index.
pub fn seg_size(ui: &Ui, opts: &[&str], pad: f32, f: f32) -> f32 {
    opts.iter().map(|o| text_w(ui, o, font(f)) + pad * 2.0).sum::<f32>() + 2.0
}
pub fn seg(ui: &mut Ui, id: Id, at: Pos2, opts: &[&str], cur: usize, pad: f32, h: f32, f: f32) -> Option<usize> {
    let w = seg_size(ui, opts, pad, f);
    let rect = Rect::from_min_size(at, vec2(w, h));
    border(ui, rect, 8, LINE2);
    let mut x = rect.left() + 1.0;
    let mut clicked = None;
    for (i, o) in opts.iter().enumerate() {
        let ow = text_w(ui, o, font(f)) + pad * 2.0;
        let cell = Rect::from_min_size(Pos2::new(x, rect.top() + 1.0), vec2(ow, h - 2.0));
        let resp = ui.interact(cell, id.with(i), Sense::click());
        let rad = |first: bool, last: bool| CornerRadius { nw: if first { 7 } else { 0 }, sw: if first { 7 } else { 0 }, ne: if last { 7 } else { 0 }, se: if last { 7 } else { 0 } };
        if i == cur {
            ui.painter().rect_filled(cell, rad(i == 0, i + 1 == opts.len()), ACCENT);
        } else if resp.hovered() {
            ui.painter().rect_filled(cell, rad(i == 0, i + 1 == opts.len()), PANEL2);
        }
        if i + 1 < opts.len() {
            ui.painter().vline(cell.right(), cell.y_range(), Stroke::new(1.0, LINE2));
        }
        label(ui, cell.center(), Align2::CENTER_CENTER, o, font(f), if i == cur { Color32::WHITE } else { LABEL });
        if resp.clicked() {
            clicked = Some(i);
        }
        x += ow;
    }
    clicked
}

/// 36x20 toggle. Returns true when clicked.
pub fn toggle(ui: &mut Ui, id: Id, at: Pos2, on: bool) -> bool {
    let rect = Rect::from_min_size(at, vec2(36.0, 20.0));
    let resp = ui.interact(rect, id, Sense::click());
    fill(ui, rect, 6, if on { ACCENT } else { LINE2 });
    let kx = if on { rect.left() + 3.0 + 16.0 } else { rect.left() + 3.0 };
    fill(ui, Rect::from_min_size(Pos2::new(kx, rect.top() + 3.0), vec2(14.0, 14.0)), 4, Color32::WHITE);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

/// Slider with the web UI's look. Returns true while the value changes.
pub fn slider(ui: &mut Ui, id: Id, rect: Rect, v: &mut f32, min: f32, max: f32, step: f32) -> bool {
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    let track = Rect::from_min_max(Pos2::new(rect.left() + 7.0, rect.center().y - 2.0), Pos2::new(rect.right() - 7.0, rect.center().y + 2.0));
    fill(ui, track, 2, LINE2);
    let mut changed = false;
    if resp.dragged() || resp.clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            let t = ((p.x - track.left()) / track.width()).clamp(0.0, 1.0);
            let mut nv = min + t * (max - min);
            if step > 0.0 {
                nv = (nv / step).round() * step;
            }
            let nv = nv.clamp(min, max);
            if (nv - *v).abs() > f32::EPSILON {
                *v = nv;
                changed = true;
            }
        }
    }
    let t = ((*v - min) / (max - min)).clamp(0.0, 1.0);
    let cx = track.left() + t * track.width();
    let thumb = Rect::from_center_size(Pos2::new(cx, rect.center().y), vec2(14.0, 14.0));
    fill(ui, thumb.expand(1.0), 5, if resp.hovered() { Color32::WHITE } else { ACCENT2 });
    fill(ui, thumb.shrink(2.0), 2, if resp.hovered() { Color32::WHITE } else { ACCENT2 });
    border(ui, thumb, 4, BG);
    changed
}

pub fn section_title(ui: &Ui, pos: Pos2, text: &str) {
    label(ui, pos, Align2::LEFT_TOP, &text.to_uppercase(), bold(10.5), MUTED);
}
