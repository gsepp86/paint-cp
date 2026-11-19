// Paint CP - programma di disegno in stile Paint moderno, scritto in Rust.
// Pensato per Windows, ma funziona anche su Linux e macOS.

// In release su Windows non apre la finestra del terminale.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CursorIcon, Key, Modifiers, PointerButton, Pos2, Rect,
    Rounding, Sense, Stroke, TextureFilter, TextureHandle, TextureOptions, Vec2,
};
use image::{imageops, DynamicImage, Rgba, RgbaImage};

const MAX_UNDO: usize = 30;
const DEFAULT_SIZE: (u32, u32) = (1024, 600);
const WHITE: [u8; 3] = [255, 255, 255];
const BLACK: [u8; 3] = [0, 0, 0];
const ACCENT: Color32 = Color32::from_rgb(0, 103, 192);

const APP_NAME: &str = "Paint CP";
const APP_VERSION: &str = "1.0";
const APP_DESCRIPTION: &str = "Paint CP è una app semplice per piccole modifiche alle foto in stile Paint di Windows, creato da Peppe CP con l'aiuto della IA.";

const PALETTE: [[u8; 3]; 30] = [
    [0, 0, 0],
    [64, 64, 64],
    [127, 127, 127],
    [195, 195, 195],
    [255, 255, 255],
    [136, 0, 21],
    [237, 28, 36],
    [255, 127, 39],
    [255, 201, 14],
    [255, 242, 0],
    [181, 230, 29],
    [34, 177, 76],
    [0, 162, 232],
    [63, 72, 204],
    [163, 73, 164],
    [255, 174, 201],
    [185, 122, 87],
    [153, 217, 234],
    [112, 146, 190],
    [200, 191, 231],
    [239, 228, 176],
    [0, 128, 128],
    [0, 0, 128],
    [128, 0, 128],
    [255, 0, 255],
    [0, 255, 255],
    [128, 128, 0],
    [255, 128, 128],
    [128, 255, 128],
    [128, 128, 255],
];

// ---------------------------------------------------------------------------
// Tipi
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Select,
    Pencil,
    Brush,
    Eraser,
    Spray,
    Fill,
    Picker,
    Text,
    Line,
    Rectangle,
    Ellipse,
}

const MAIN_TOOLS: [(Tool, &str); 8] = [
    (Tool::Select, "Seleziona (rettangolo)"),
    (Tool::Pencil, "Matita"),
    (Tool::Brush, "Pennello"),
    (Tool::Eraser, "Gomma"),
    (Tool::Spray, "Spray"),
    (Tool::Fill, "Secchiello"),
    (Tool::Picker, "Contagocce"),
    (Tool::Text, "Testo"),
];

const SHAPE_TOOLS: [(Tool, &str); 3] = [
    (Tool::Line, "Linea"),
    (Tool::Rectangle, "Rettangolo"),
    (Tool::Ellipse, "Ellisse"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum ShapeMode {
    Outline,
    Filled,
    Both,
}

/// Un tratto in corso (dal momento in cui premi il mouse a quando lo rilasci).
struct ActiveStroke {
    button: PointerButton,
    start: (i32, i32),
    last: (i32, i32),
    /// Copia della tela all'inizio del tratto: serve per l'anteprima delle forme.
    base: Option<RgbaImage>,
    /// Solo per la selezione: scostamento del punto afferrato dentro la selezione.
    grab: Option<(i32, i32)>,
}

/// Contenuto "sollevato" di una selezione che si sta spostando.
struct Floating {
    content: RgbaImage,
    /// Tela senza il contenuto (con il buco riempito).
    base: RgbaImage,
}

struct Selection {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    floating: Option<Floating>,
}

struct TextEdit {
    pos: (i32, i32),
    content: String,
    base: RgbaImage,
    color: [u8; 3],
    needs_render: bool,
    focus: bool,
}

struct ResizeDialog {
    w: u32,
    h: u32,
    keep_ratio: bool,
    ratio: f32,
}

struct PaintApp {
    canvas: RgbaImage,
    texture: Option<TextureHandle>,
    dirty: bool,
    has_alpha: bool,

    undo: Vec<RgbaImage>,
    redo: Vec<RgbaImage>,

    tool: Tool,
    primary: [u8; 3],
    secondary: [u8; 3],
    size: u32,
    text_size: u32,
    shape_mode: ShapeMode,
    eraser_transparent: bool,
    zoom: f32,
    dark: bool,

    stroke: Option<ActiveStroke>,
    selection: Option<Selection>,
    text_edit: Option<TextEdit>,
    clipboard: Option<RgbaImage>,
    font: Option<FontArc>,
    cursor_px: Option<(i32, i32)>,

    file_path: Option<PathBuf>,
    modified: bool,
    new_dialog: Option<(u32, u32)>,
    resize_dialog: Option<ResizeDialog>,
    about_open: bool,
    status: String,
    last_title: String,
    rng: u64,
}

// ---------------------------------------------------------------------------
// Primitive di disegno (lavorano direttamente sui pixel)
// ---------------------------------------------------------------------------

fn rgba(c: [u8; 3]) -> Rgba<u8> {
    Rgba([c[0], c[1], c[2], 255])
}

fn put(img: &mut RgbaImage, x: i32, y: i32, c: Rgba<u8>) {
    if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
        img.put_pixel(x as u32, y as u32, c);
    }
}

/// Sovrappone `src` (con trasparenza) al pixel di destinazione.
fn blend_pixel(img: &mut RgbaImage, x: i32, y: i32, src: Rgba<u8>) {
    if x < 0 || y < 0 || x >= img.width() as i32 || y >= img.height() as i32 {
        return;
    }
    let sa = src.0[3] as f32 / 255.0;
    if sa <= 0.0 {
        return;
    }
    let dst = img.get_pixel_mut(x as u32, y as u32);
    if sa >= 1.0 {
        *dst = src;
        return;
    }
    let da = dst.0[3] as f32 / 255.0;
    let oa = sa + da * (1.0 - sa);
    for i in 0..3 {
        let v = (src.0[i] as f32 * sa + dst.0[i] as f32 * da * (1.0 - sa)) / oa;
        dst.0[i] = v.round().clamp(0.0, 255.0) as u8;
    }
    dst.0[3] = (oa * 255.0).round() as u8;
}

fn overlay(dst: &mut RgbaImage, src: &RgbaImage, x: i32, y: i32) {
    for sy in 0..src.height() as i32 {
        let dy = y + sy;
        if dy < 0 || dy >= dst.height() as i32 {
            continue;
        }
        for sx in 0..src.width() as i32 {
            let dx = x + sx;
            if dx < 0 || dx >= dst.width() as i32 {
                continue;
            }
            let s = *src.get_pixel(sx as u32, sy as u32);
            blend_pixel(dst, dx, dy, s);
        }
    }
}

fn fill_rect(img: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, c: Rgba<u8>) {
    let (iw, ih) = (img.width() as i32, img.height() as i32);
    for yy in y.max(0)..(y + h).min(ih) {
        for xx in x.max(0)..(x + w).min(iw) {
            img.put_pixel(xx as u32, yy as u32, c);
        }
    }
}

/// Disegna un "timbro" circolare di diametro `size` centrato in (cx, cy).
fn stamp(img: &mut RgbaImage, cx: i32, cy: i32, size: u32, c: Rgba<u8>) {
    if size <= 1 {
        put(img, cx, cy, c);
        return;
    }
    let r = size as f32 / 2.0;
    let ri = r.ceil() as i32;
    for dy in -ri..=ri {
        for dx in -ri..=ri {
            if ((dx * dx + dy * dy) as f32) <= r * r {
                put(img, cx + dx, cy + dy, c);
            }
        }
    }
}

/// Linea di Bresenham con spessore `size`.
fn line(img: &mut RgbaImage, x0: i32, y0: i32, x1: i32, y1: i32, size: u32, c: Rgba<u8>) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let (mut x, mut y) = (x0, y0);
    loop {
        stamp(img, x, y, size, c);
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

fn rect_shape(
    img: &mut RgbaImage,
    a: (i32, i32),
    b: (i32, i32),
    size: u32,
    border: Option<Rgba<u8>>,
    fill: Option<Rgba<u8>>,
) {
    let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
    let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
    if let Some(c) = fill {
        fill_rect(img, x0, y0, x1 - x0 + 1, y1 - y0 + 1, c);
    }
    if let Some(c) = border {
        line(img, x0, y0, x1, y0, size, c);
        line(img, x1, y0, x1, y1, size, c);
        line(img, x1, y1, x0, y1, size, c);
        line(img, x0, y1, x0, y0, size, c);
    }
}

fn ellipse_shape(
    img: &mut RgbaImage,
    a: (i32, i32),
    b: (i32, i32),
    size: u32,
    border: Option<Rgba<u8>>,
    fill: Option<Rgba<u8>>,
) {
    let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
    let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
    let (w, h) = (img.width() as i32, img.height() as i32);
    let cx = (x0 + x1) as f32 / 2.0;
    let cy = (y0 + y1) as f32 / 2.0;
    let rx = (x1 - x0) as f32 / 2.0;
    let ry = (y1 - y0) as f32 / 2.0;

    if let Some(c) = fill {
        for y in y0.max(0)..=y1.min(h - 1) {
            let t = if ry > 0.0 { (y as f32 - cy) / ry } else { 0.0 };
            let half = rx * (1.0 - t * t).max(0.0).sqrt();
            let xs = ((cx - half).round() as i32).max(0);
            let xe = ((cx + half).round() as i32).min(w - 1);
            for x in xs..=xe {
                img.put_pixel(x as u32, y as u32, c);
            }
        }
    }
    if let Some(c) = border {
        let steps = (((rx + ry) * 4.0) as usize).max(32);
        let mut prev = (cx + rx, cy);
        for i in 1..=steps {
            let ang = i as f32 / steps as f32 * std::f32::consts::TAU;
            let p = (cx + rx * ang.cos(), cy + ry * ang.sin());
            line(
                img,
                prev.0.round() as i32,
                prev.1.round() as i32,
                p.0.round() as i32,
                p.1.round() as i32,
                size,
                c,
            );
            prev = p;
        }
    }
}

/// Riempimento a scanline (secchiello): sostituisce l'area contigua dello stesso colore.
fn flood_fill(img: &mut RgbaImage, x: i32, y: i32, c: Rgba<u8>) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    if x < 0 || y < 0 || x >= w || y >= h {
        return;
    }
    let target = *img.get_pixel(x as u32, y as u32);
    if target == c {
        return;
    }
    let mut stack = vec![(x, y)];
    while let Some((sx, sy)) = stack.pop() {
        if *img.get_pixel(sx as u32, sy as u32) != target {
            continue;
        }
        let mut l = sx;
        while l > 0 && *img.get_pixel((l - 1) as u32, sy as u32) == target {
            l -= 1;
        }
        let mut r = sx;
        while r < w - 1 && *img.get_pixel((r + 1) as u32, sy as u32) == target {
            r += 1;
        }
        for xx in l..=r {
            img.put_pixel(xx as u32, sy as u32, c);
        }
        for ny in [sy - 1, sy + 1] {
            if ny < 0 || ny >= h {
                continue;
            }
            let mut inside = false;
            for xx in l..=r {
                let is_target = *img.get_pixel(xx as u32, ny as u32) == target;
                if is_target && !inside {
                    stack.push((xx, ny));
                }
                inside = is_target;
            }
        }
    }
}

/// Piccolo generatore pseudo-casuale (xorshift) per lo spray: niente dipendenze extra.
fn next_rand(state: &mut u64) -> f32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    (x >> 40) as f32 / 16_777_216.0
}

fn spray(img: &mut RgbaImage, rng: &mut u64, c: (i32, i32), size: u32, color: Rgba<u8>) {
    let radius = size as f32;
    let dots = (size * 3).max(8);
    for _ in 0..dots {
        let angle = next_rand(rng) * std::f32::consts::TAU;
        let dist = next_rand(rng).sqrt() * radius;
        put(
            img,
            c.0 + (angle.cos() * dist).round() as i32,
            c.1 + (angle.sin() * dist).round() as i32,
            color,
        );
    }
}

/// Appoggia l'immagine su sfondo bianco (serve per salvare in JPG/BMP).
fn flatten_on_white(img: &mut RgbaImage) {
    for p in img.pixels_mut() {
        let a = p.0[3] as u32;
        if a == 255 {
            continue;
        }
        for i in 0..3 {
            p.0[i] = ((p.0[i] as u32 * a + 255 * (255 - a)) / 255) as u8;
        }
        p.0[3] = 255;
    }
}

/// Disegna il testo (con antialiasing) direttamente sui pixel della tela.
fn draw_text(img: &mut RgbaImage, font: &FontArc, text: &str, pos: (i32, i32), px: f32, color: [u8; 3]) {
    let scale = PxScale::from(px);
    let scaled = font.as_scaled(scale);
    let line_h = scaled.height() + scaled.line_gap();
    let mut cx = 0.0f32;
    let mut cy = scaled.ascent();
    for ch in text.chars() {
        if ch == '\n' {
            cx = 0.0;
            cy += line_h;
            continue;
        }
        if ch == '\r' {
            continue;
        }
        let id = scaled.glyph_id(ch);
        let glyph = id.with_scale_and_position(
            scale,
            ab_glyph::point(pos.0 as f32 + cx, pos.1 as f32 + cy),
        );
        cx += scaled.h_advance(id);
        if let Some(outlined) = font.outline_glyph(glyph) {
            let b = outlined.px_bounds();
            let (bx, by) = (b.min.x.floor() as i32, b.min.y.floor() as i32);
            outlined.draw(|gx, gy, cov| {
                let a = (cov * 255.0).round() as u8;
                if a > 0 {
                    blend_pixel(
                        img,
                        bx + gx as i32,
                        by + gy as i32,
                        Rgba([color[0], color[1], color[2], a]),
                    );
                }
            });
        }
    }
}

/// Cerca un font di sistema; se non c'e' usa quelli integrati in egui.
fn load_font() -> Option<FontArc> {
    let candidates = [
        "C:\\Windows\\Fonts\\segoeui.ttf",
        "C:\\Windows\\Fonts\\arial.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
    ];
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(f) = FontArc::try_from_vec(bytes) {
                return Some(f);
            }
        }
    }
    let defs = egui::FontDefinitions::default();
    for name in ["Ubuntu-Light", "Hack"] {
        if let Some(data) = defs.font_data.get(name) {
            if let Ok(f) = FontArc::try_from_vec(data.font.to_vec()) {
                return Some(f);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Stile e widget personalizzati
// ---------------------------------------------------------------------------

fn setup_style(ctx: &egui::Context, dark: bool) {
    let mut v = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    let r = Rounding::same(6.0);
    v.widgets.noninteractive.rounding = r;
    v.widgets.inactive.rounding = r;
    v.widgets.hovered.rounding = r;
    v.widgets.active.rounding = r;
    v.widgets.open.rounding = r;
    v.window_rounding = Rounding::same(10.0);
    v.menu_rounding = Rounding::same(8.0);
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    if dark {
        v.panel_fill = Color32::from_rgb(32, 32, 32);
        v.window_fill = Color32::from_rgb(44, 44, 44);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(58, 66, 78);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    } else {
        v.panel_fill = Color32::from_rgb(243, 243, 243);
        v.window_fill = Color32::WHITE;
        v.widgets.inactive.weak_bg_fill = Color32::WHITE;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_gray(214));
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(232, 240, 250);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    }
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = vec2(6.0, 6.0);
        s.spacing.button_padding = vec2(10.0, 4.0);
        s.spacing.interact_size.y = 26.0;
    });
}

/// Bordo tratteggiato (le "formiche in marcia" della selezione).
fn dashed_rect(painter: &egui::Painter, r: Rect, base: Color32, dash: Color32) {
    painter.rect_stroke(r, 0.0, Stroke::new(1.0, base));
    let pts = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    let stroke = Stroke::new(1.0, dash);
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = (b - a).length();
        if len <= 0.0 {
            continue;
        }
        let dir = (b - a) / len;
        let mut t = 0.0;
        while t < len {
            let t2 = (t + 4.0).min(len);
            painter.line_segment([a + dir * t, a + dir * t2], stroke);
            t += 8.0;
        }
    }
}

/// Icone degli strumenti disegnate a codice (nessun font di icone necessario).
fn paint_tool_icon(p: &egui::Painter, r: Rect, tool: Tool, col: Color32) {
    let u = r.width() / 24.0;
    let c = r.center();
    let pt = |x: f32, y: f32| c + vec2(x * u, y * u);
    let thin = Stroke::new(1.6 * u, col);
    let medium = Stroke::new(2.0 * u, col);
    match tool {
        Tool::Select => dashed_rect(
            p,
            Rect::from_two_pos(pt(-8.0, -7.0), pt(8.0, 7.0)),
            Color32::TRANSPARENT,
            col,
        ),
        Tool::Pencil => {
            p.line_segment([pt(-4.0, 4.0), pt(6.0, -6.0)], Stroke::new(4.0 * u, col));
            p.line_segment([pt(-8.0, 8.0), pt(-4.0, 4.0)], thin);
        }
        Tool::Brush => {
            p.line_segment([pt(-1.0, 1.0), pt(8.0, -8.0)], Stroke::new(3.0 * u, col));
            p.circle_filled(pt(-5.0, 5.0), 4.0 * u, col);
        }
        Tool::Eraser => {
            p.line_segment([pt(-5.0, 4.0), pt(3.0, -4.0)], Stroke::new(9.0 * u, col));
            p.line_segment([pt(-9.0, 9.0), pt(9.0, 9.0)], thin);
        }
        Tool::Spray => {
            let dots = [
                (-6.0, 6.0),
                (-2.0, 2.0),
                (2.0, -2.0),
                (6.0, -6.0),
                (-6.0, -1.0),
                (1.0, 6.0),
                (6.0, 1.0),
                (-1.0, -6.0),
                (3.0, 3.0),
            ];
            for (x, y) in dots {
                p.circle_filled(pt(x, y), 1.5 * u, col);
            }
        }
        Tool::Fill => {
            p.rect_stroke(Rect::from_two_pos(pt(-8.0, -2.0), pt(2.0, 8.0)), 2.0 * u, thin);
            p.line_segment([pt(-3.0, -2.0), pt(3.0, -8.0)], thin);
            p.circle_filled(pt(7.0, 5.0), 2.2 * u, col);
        }
        Tool::Picker => {
            p.line_segment([pt(-8.0, 8.0), pt(2.0, -2.0)], medium);
            p.circle_stroke(pt(6.0, -6.0), 4.0 * u, medium);
        }
        Tool::Text => {
            p.text(
                c,
                Align2::CENTER_CENTER,
                "A",
                egui::FontId::proportional(20.0 * u),
                col,
            );
        }
        Tool::Line => p.line_segment([pt(-8.0, 8.0), pt(8.0, -8.0)], medium),
        Tool::Rectangle => p.rect_stroke(
            Rect::from_two_pos(pt(-9.0, -6.0), pt(9.0, 6.0)),
            1.0,
            medium,
        ),
        Tool::Ellipse => {
            let mut prev = pt(9.0, 0.0);
            for i in 1..=24 {
                let a = i as f32 / 24.0 * std::f32::consts::TAU;
                let n = pt(9.0 * a.cos(), 6.0 * a.sin());
                p.line_segment([prev, n], medium);
                prev = n;
            }
        }
    }
}

fn tool_button(ui: &mut egui::Ui, selected: bool, tool: Tool, label: &str, size: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let hover_fill = ui.visuals().widgets.hovered.weak_bg_fill;
    let text_col = ui.visuals().text_color();
    if selected || resp.hovered() {
        let fill = if selected {
            Color32::from_rgba_unmultiplied(0, 103, 192, 48)
        } else {
            hover_fill
        };
        ui.painter().rect_filled(rect, 6.0, fill);
        if selected {
            ui.painter().rect_stroke(rect, 6.0, Stroke::new(1.0, ACCENT));
        }
    }
    let col = if selected { ACCENT } else { text_col };
    paint_tool_icon(ui.painter(), rect.shrink(5.0), tool, col);
    resp.on_hover_text(label)
}

fn vsep(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(1.0, 92.0), Sense::hover());
    let col = ui.visuals().widgets.noninteractive.bg_stroke.color;
    ui.painter().rect_filled(r, 0.0, col);
}

/// Gruppo della barra degli strumenti: contenuto + titolo piccolo sotto.
fn group(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        add(ui);
        ui.add_space(2.0);
        ui.label(egui::RichText::new(title).small().weak());
    });
    vsep(ui);
}

// ---------------------------------------------------------------------------
// Applicazione
// ---------------------------------------------------------------------------

impl PaintApp {
    fn new(ctx: &egui::Context) -> Self {
        setup_style(ctx, false);
        Self {
            canvas: RgbaImage::from_pixel(DEFAULT_SIZE.0, DEFAULT_SIZE.1, rgba(WHITE)),
            texture: None,
            dirty: true,
            has_alpha: false,
            undo: Vec::new(),
            redo: Vec::new(),
            tool: Tool::Brush,
            primary: BLACK,
            secondary: WHITE,
            size: 6,
            text_size: 32,
            shape_mode: ShapeMode::Outline,
            eraser_transparent: false,
            zoom: 1.0,
            dark: false,
            stroke: None,
            selection: None,
            text_edit: None,
            clipboard: None,
            font: load_font(),
            cursor_px: None,
            file_path: None,
            modified: false,
            new_dialog: None,
            resize_dialog: None,
            about_open: false,
            status: String::from("Pronto"),
            last_title: String::new(),
            rng: 0x2545_F491_4F6C_DD1D,
        }
    }

    // ----- colori e dimensioni -------------------------------------------

    fn draw_color(&self, button: PointerButton) -> Rgba<u8> {
        if self.tool == Tool::Eraser {
            return if self.eraser_transparent {
                Rgba([0, 0, 0, 0])
            } else {
                rgba(self.secondary)
            };
        }
        match button {
            PointerButton::Secondary => rgba(self.secondary),
            _ => rgba(self.primary),
        }
    }

    fn other_color(&self, button: PointerButton) -> Rgba<u8> {
        match button {
            PointerButton::Secondary => rgba(self.primary),
            _ => rgba(self.secondary),
        }
    }

    fn effective_size(&self) -> u32 {
        if self.tool == Tool::Pencil {
            1
        } else {
            self.size
        }
    }

    fn clamp_to_canvas(&self, p: (i32, i32)) -> (i32, i32) {
        (
            p.0.clamp(0, self.canvas.width() as i32 - 1),
            p.1.clamp(0, self.canvas.height() as i32 - 1),
        )
    }

    // ----- cronologia (annulla / ripeti) ----------------------------------

    fn push_undo(&mut self) {
        self.commit_text();
        if self.undo.len() >= MAX_UNDO {
            self.undo.remove(0);
        }
        self.undo.push(self.canvas.clone());
        self.redo.clear();
        self.modified = true;
    }

    fn do_undo(&mut self) {
        self.commit_text();
        self.stroke = None;
        self.selection = None;
        if let Some(prev) = self.undo.pop() {
            let current = std::mem::replace(&mut self.canvas, prev);
            self.redo.push(current);
            self.dirty = true;
            self.modified = true;
        }
    }

    fn do_redo(&mut self) {
        self.commit_text();
        self.stroke = None;
        self.selection = None;
        if let Some(next) = self.redo.pop() {
            let current = std::mem::replace(&mut self.canvas, next);
            self.undo.push(current);
            self.dirty = true;
            self.modified = true;
        }
    }

    // ----- operazioni sull'immagine ---------------------------------------

    fn new_image(&mut self, w: u32, h: u32) {
        self.push_undo();
        self.selection = None;
        self.canvas = RgbaImage::from_pixel(w.max(1), h.max(1), rgba(WHITE));
        self.has_alpha = false;
        self.file_path = None;
        self.modified = false;
        self.dirty = true;
        self.status = String::from("Nuova immagine");
    }

    fn clear(&mut self) {
        self.push_undo();
        self.selection = None;
        let c = rgba(self.secondary);
        for p in self.canvas.pixels_mut() {
            *p = c;
        }
        self.dirty = true;
    }

    fn transform(&mut self, f: impl FnOnce(&RgbaImage) -> RgbaImage, msg: &str) {
        self.push_undo();
        self.selection = None;
        self.canvas = f(&self.canvas);
        self.dirty = true;
        self.status = msg.to_string();
    }

    fn invert_colors(&mut self) {
        self.push_undo();
        self.selection = None;
        for p in self.canvas.pixels_mut() {
            for i in 0..3 {
                p.0[i] = 255 - p.0[i];
            }
        }
        self.dirty = true;
        self.status = String::from("Colori invertiti");
    }

    fn resize_image(&mut self, w: u32, h: u32) {
        self.push_undo();
        self.selection = None;
        self.canvas = imageops::resize(&self.canvas, w.max(1), h.max(1), imageops::FilterType::Lanczos3);
        self.dirty = true;
        self.status = format!("Ridimensionata a {} x {}", w, h);
    }

    // ----- selezione e appunti ------------------------------------------------

    fn selection_image(&self) -> Option<RgbaImage> {
        let sel = self.selection.as_ref()?;
        match &sel.floating {
            Some(f) => Some(f.content.clone()),
            None => Some(
                imageops::crop_imm(
                    &self.canvas,
                    sel.x as u32,
                    sel.y as u32,
                    sel.w as u32,
                    sel.h as u32,
                )
                .to_image(),
            ),
        }
    }

    fn set_clipboard(&mut self, img: RgbaImage) {
        // Prova anche la clipboard di sistema (cosi' puoi incollare in altri programmi).
        if let Ok(mut cb) = arboard::Clipboard::new() {
            let data = arboard::ImageData {
                width: img.width() as usize,
                height: img.height() as usize,
                bytes: std::borrow::Cow::Borrowed(img.as_raw().as_slice()),
            };
            let _ = cb.set_image(data);
        }
        self.clipboard = Some(img);
    }

    fn copy(&mut self) {
        if let Some(img) = self.selection_image() {
            self.set_clipboard(img);
            self.status = String::from("Copiato");
        }
    }

    fn delete_selection(&mut self) {
        let Some(is_floating) = self.selection.as_ref().map(|s| s.floating.is_some()) else {
            return;
        };
        self.push_undo();
        if is_floating {
            if let Some(Selection {
                floating: Some(f), ..
            }) = &self.selection
            {
                self.canvas.clone_from(&f.base);
            }
        } else if let Some(sel) = &self.selection {
            let c = rgba(self.secondary);
            fill_rect(&mut self.canvas, sel.x, sel.y, sel.w, sel.h, c);
        }
        self.selection = None;
        self.dirty = true;
    }

    fn cut(&mut self) {
        if self.selection.is_some() {
            self.copy();
            self.delete_selection();
            self.status = String::from("Tagliato");
        }
    }

    fn paste(&mut self) {
        let from_system = arboard::Clipboard::new()
            .ok()
            .and_then(|mut cb| cb.get_image().ok())
            .and_then(|d| {
                RgbaImage::from_raw(d.width as u32, d.height as u32, d.bytes.into_owned())
            });
        let Some(img) = from_system.or_else(|| self.clipboard.clone()) else {
            self.status = String::from("Appunti vuoti");
            return;
        };
        self.push_undo();
        let base = self.canvas.clone();
        overlay(&mut self.canvas, &img, 0, 0);
        self.selection = Some(Selection {
            x: 0,
            y: 0,
            w: img.width() as i32,
            h: img.height() as i32,
            floating: Some(Floating { content: img, base }),
        });
        self.tool = Tool::Select;
        self.dirty = true;
        self.status = String::from("Incollato: trascina per spostare");
    }

    fn select_all(&mut self) {
        self.commit_text();
        self.tool = Tool::Select;
        self.selection = Some(Selection {
            x: 0,
            y: 0,
            w: self.canvas.width() as i32,
            h: self.canvas.height() as i32,
            floating: None,
        });
    }

    fn crop_to_selection(&mut self) {
        if let Some(img) = self.selection_image() {
            self.push_undo();
            self.canvas = img;
            self.selection = None;
            self.dirty = true;
            self.status = String::from("Immagine ritagliata");
        }
    }

    /// Solleva il contenuto della selezione al primo movimento e lo sposta.
    fn move_selection(&mut self, p: (i32, i32), grab: (i32, i32)) {
        let needs_lift = matches!(&self.selection, Some(s) if s.floating.is_none());
        if needs_lift {
            self.push_undo();
            if let Some(sel) = &mut self.selection {
                let content = imageops::crop_imm(
                    &self.canvas,
                    sel.x as u32,
                    sel.y as u32,
                    sel.w as u32,
                    sel.h as u32,
                )
                .to_image();
                fill_rect(
                    &mut self.canvas,
                    sel.x,
                    sel.y,
                    sel.w,
                    sel.h,
                    rgba(self.secondary),
                );
                sel.floating = Some(Floating {
                    content,
                    base: self.canvas.clone(),
                });
            }
        }
        if let Some(sel) = &mut self.selection {
            sel.x = p.0 - grab.0;
            sel.y = p.1 - grab.1;
            if let Some(f) = &sel.floating {
                self.canvas.clone_from(&f.base);
                overlay(&mut self.canvas, &f.content, sel.x, sel.y);
            }
        }
    }

    // ----- testo ----------------------------------------------------------------

    fn begin_text(&mut self, button: PointerButton, p: (i32, i32)) {
        if let Some(te) = &mut self.text_edit {
            te.pos = p;
            te.needs_render = true;
            return;
        }
        let c = self.draw_color(button);
        self.text_edit = Some(TextEdit {
            pos: p,
            content: String::new(),
            base: self.canvas.clone(),
            color: [c.0[0], c.0[1], c.0[2]],
            needs_render: false,
            focus: true,
        });
    }

    fn render_text(&mut self) {
        let Some(te) = &self.text_edit else { return };
        if !te.needs_render {
            return;
        }
        let mut img = te.base.clone();
        if let Some(font) = &self.font {
            draw_text(&mut img, font, &te.content, te.pos, self.text_size as f32, te.color);
        }
        self.canvas = img;
        self.dirty = true;
        if let Some(te) = &mut self.text_edit {
            te.needs_render = false;
        }
    }

    fn commit_text(&mut self) {
        self.render_text();
        let Some(te) = self.text_edit.take() else { return };
        if te.content.trim().is_empty() {
            self.canvas = te.base;
            self.dirty = true;
            return;
        }
        // La tela contiene gia' il testo: salviamo lo stato precedente per "Annulla".
        if self.undo.len() >= MAX_UNDO {
            self.undo.remove(0);
        }
        self.undo.push(te.base);
        self.redo.clear();
        self.modified = true;
    }

    fn cancel_text(&mut self) {
        if let Some(te) = self.text_edit.take() {
            self.canvas = te.base;
            self.dirty = true;
        }
    }

    // ----- file -------------------------------------------------------------

    fn open_file(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Immagini", &["png", "jpg", "jpeg", "bmp"])
            .pick_file()
        else {
            return;
        };
        match image::open(&path) {
            Ok(img) => {
                let rgba_img = img.to_rgba8();
                self.push_undo();
                self.selection = None;
                self.has_alpha = rgba_img.pixels().any(|p| p.0[3] < 255);
                self.canvas = rgba_img;
                self.file_path = Some(path);
                self.modified = false;
                self.dirty = true;
                self.status = String::from("Immagine aperta");
            }
            Err(e) => self.status = format!("Impossibile aprire il file: {e}"),
        }
    }

    fn save(&mut self) {
        match self.file_path.clone() {
            Some(p) => self.save_to(p),
            None => self.save_as(),
        }
    }

    fn save_as(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("PNG", &["png"])
            .add_filter("JPEG", &["jpg", "jpeg"])
            .add_filter("Bitmap", &["bmp"])
            .set_file_name("senza_titolo.png");
        if let Some(dir) = self.file_path.as_ref().and_then(|p| p.parent()) {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.save_file() {
            self.save_to(path);
        }
    }

    fn save_to(&mut self, mut path: PathBuf) {
        self.commit_text();
        if path.extension().is_none() {
            path.set_extension("png");
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let result = if matches!(ext.as_str(), "jpg" | "jpeg" | "bmp") {
            // JPEG e BMP non hanno trasparenza: appoggia su bianco.
            let mut flat = self.canvas.clone();
            flatten_on_white(&mut flat);
            DynamicImage::ImageRgba8(flat).to_rgb8().save(&path)
        } else {
            self.canvas.save(&path)
        };
        match result {
            Ok(()) => {
                self.status = format!("Salvato: {}", path.display());
                self.file_path = Some(path);
                self.modified = false;
            }
            Err(e) => self.status = format!("Errore nel salvataggio: {e}"),
        }
    }

    // ----- gestione del tratto ----------------------------------------------

    fn begin_stroke(&mut self, button: PointerButton, p: (i32, i32)) {
        if self.tool != Tool::Select {
            self.selection = None;
        }
        let color = self.draw_color(button);

        match self.tool {
            Tool::Picker => {
                if p.0 >= 0
                    && p.1 >= 0
                    && (p.0 as u32) < self.canvas.width()
                    && (p.1 as u32) < self.canvas.height()
                {
                    let px = self.canvas.get_pixel(p.0 as u32, p.1 as u32).0;
                    let picked = [px[0], px[1], px[2]];
                    if button == PointerButton::Secondary {
                        self.secondary = picked;
                    } else {
                        self.primary = picked;
                    }
                }
                return;
            }
            Tool::Fill => {
                self.push_undo();
                flood_fill(&mut self.canvas, p.0, p.1, color);
                self.dirty = true;
                return;
            }
            Tool::Text => {
                self.begin_text(button, p);
                return;
            }
            Tool::Select => {
                self.begin_select(button, p);
                return;
            }
            _ => {}
        }

        if self.tool == Tool::Eraser && self.eraser_transparent {
            self.has_alpha = true;
        }
        self.push_undo();
        let base = if matches!(self.tool, Tool::Line | Tool::Rectangle | Tool::Ellipse) {
            Some(self.canvas.clone())
        } else {
            None
        };
        self.stroke = Some(ActiveStroke {
            button,
            start: p,
            last: p,
            base,
            grab: None,
        });
        self.continue_stroke(p, true);
    }

    fn begin_select(&mut self, button: PointerButton, p: (i32, i32)) {
        let inside_grab = self.selection.as_ref().and_then(|sel| {
            if p.0 >= sel.x && p.0 < sel.x + sel.w && p.1 >= sel.y && p.1 < sel.y + sel.h {
                Some((p.0 - sel.x, p.1 - sel.y))
            } else {
                None
            }
        });
        if let Some(grab) = inside_grab {
            self.stroke = Some(ActiveStroke {
                button,
                start: p,
                last: p,
                base: None,
                grab: Some(grab),
            });
            return;
        }
        // Nuova selezione: l'eventuale contenuto spostato resta incorporato nella tela.
        let q = self.clamp_to_canvas(p);
        self.selection = Some(Selection {
            x: q.0,
            y: q.1,
            w: 1,
            h: 1,
            floating: None,
        });
        self.stroke = Some(ActiveStroke {
            button,
            start: q,
            last: q,
            base: None,
            grab: None,
        });
    }

    fn continue_stroke(&mut self, p: (i32, i32), force: bool) {
        let (start, last, button) = match &self.stroke {
            Some(s) => (s.start, s.last, s.button),
            None => return,
        };
        if !force && p == last && self.tool != Tool::Spray {
            return;
        }

        let color = self.draw_color(button);
        let other = self.other_color(button);
        let size = self.effective_size();

        match self.tool {
            Tool::Pencil | Tool::Brush | Tool::Eraser => {
                line(&mut self.canvas, last.0, last.1, p.0, p.1, size, color);
            }
            Tool::Spray => {
                spray(&mut self.canvas, &mut self.rng, p, size, color);
            }
            Tool::Line | Tool::Rectangle | Tool::Ellipse => {
                // Ripristina la tela com'era all'inizio e ridisegna la forma aggiornata.
                if let Some(base) = self.stroke.as_ref().and_then(|s| s.base.as_ref()) {
                    self.canvas.clone_from(base);
                }
                if self.tool == Tool::Line {
                    line(&mut self.canvas, start.0, start.1, p.0, p.1, size, color);
                } else {
                    let (border, fill) = match self.shape_mode {
                        ShapeMode::Outline => (Some(color), None),
                        ShapeMode::Filled => (None, Some(color)),
                        ShapeMode::Both => (Some(color), Some(other)),
                    };
                    if self.tool == Tool::Rectangle {
                        rect_shape(&mut self.canvas, start, p, size, border, fill);
                    } else {
                        ellipse_shape(&mut self.canvas, start, p, size, border, fill);
                    }
                }
            }
            Tool::Select => {
                let grab = self.stroke.as_ref().and_then(|s| s.grab);
                if let Some(g) = grab {
                    self.move_selection(p, g);
                } else {
                    let a = self.clamp_to_canvas(start);
                    let b = self.clamp_to_canvas(p);
                    self.selection = Some(Selection {
                        x: a.0.min(b.0),
                        y: a.1.min(b.1),
                        w: (a.0 - b.0).abs() + 1,
                        h: (a.1 - b.1).abs() + 1,
                        floating: None,
                    });
                }
            }
            Tool::Fill | Tool::Picker | Tool::Text => {}
        }

        if let Some(s) = &mut self.stroke {
            s.last = p;
        }
        self.dirty = true;
    }

    fn end_stroke(&mut self) {
        if let Some(s) = self.stroke.take() {
            // Un semplice clic con la selezione la annulla.
            if self.tool == Tool::Select && s.grab.is_none() && s.start == s.last {
                self.selection = None;
            }
        }
    }

    fn process_canvas_input(&mut self, ui: &egui::Ui, response: &egui::Response, rect: Rect) {
        let zoom = self.zoom;
        let pointer = ui.input(|i| i.pointer.latest_pos());
        let px = pointer.map(|p| {
            let v = (p - rect.min) / zoom;
            (v.x.floor() as i32, v.y.floor() as i32)
        });
        let (w, h) = (self.canvas.width() as i32, self.canvas.height() as i32);
        let in_bounds = |p: (i32, i32)| p.0 >= 0 && p.1 >= 0 && p.0 < w && p.1 < h;

        self.cursor_px = match px {
            Some(p) if in_bounds(p) && (response.hovered() || self.stroke.is_some()) => Some(p),
            _ => None,
        };

        if response.hovered() {
            let icon = match self.tool {
                Tool::Text => CursorIcon::Text,
                Tool::Select => CursorIcon::Crosshair,
                _ => CursorIcon::Crosshair,
            };
            ui.ctx().set_cursor_icon(icon);
        }

        // Inizio di un nuovo tratto.
        if self.stroke.is_none() && response.hovered() {
            let (primary, secondary) = ui.input(|i| {
                (
                    i.pointer.button_pressed(PointerButton::Primary),
                    i.pointer.button_pressed(PointerButton::Secondary),
                )
            });
            let button = if primary {
                Some(PointerButton::Primary)
            } else if secondary {
                Some(PointerButton::Secondary)
            } else {
                None
            };
            if let (Some(button), Some(p)) = (button, px) {
                self.begin_stroke(button, p);
            }
        }

        // Tratto in corso.
        if let Some(button) = self.stroke.as_ref().map(|s| s.button) {
            let down = ui.input(|i| i.pointer.button_down(button));
            if let Some(p) = px {
                self.continue_stroke(p, false);
            }
            if down {
                ui.ctx().request_repaint(); // serve per lo spray mentre tieni premuto
            } else {
                self.end_stroke();
            }
        }
    }

    fn upload_texture(&mut self, ctx: &egui::Context) {
        let size = [self.canvas.width() as usize, self.canvas.height() as usize];
        let image = egui::ColorImage::from_rgba_unmultiplied(size, self.canvas.as_raw());
        let options = TextureOptions {
            magnification: TextureFilter::Nearest,
            minification: TextureFilter::Linear,
            ..Default::default()
        };
        if let Some(tex) = self.texture.as_mut() {
            tex.set(image, options);
        } else {
            self.texture = Some(ctx.load_texture("canvas", image, options));
        }
        self.dirty = false;
    }

    // ----- scorciatoie -----------------------------------------------------------

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.wants_keyboard_input();
        let sens = !typing; // non rubare i tasti a un campo di testo attivo
        let (redo_shift, undo, redo_y, save_as, new, open, save, select_all, delete, escape) =
            ctx.input_mut(|i| {
                (
                    sens && i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z),
                    sens && i.consume_key(Modifiers::COMMAND, Key::Z),
                    sens && i.consume_key(Modifiers::COMMAND, Key::Y),
                    i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::S),
                    i.consume_key(Modifiers::COMMAND, Key::N),
                    i.consume_key(Modifiers::COMMAND, Key::O),
                    i.consume_key(Modifiers::COMMAND, Key::S),
                    sens && i.consume_key(Modifiers::COMMAND, Key::A),
                    sens && i.consume_key(Modifiers::NONE, Key::Delete),
                    i.consume_key(Modifiers::NONE, Key::Escape),
                )
            });

        // Copia / taglia / incolla arrivano come eventi dedicati.
        let (copy, cut, paste_evt, v_key) = ctx.input(|i| {
            let (mut c, mut x, mut p) = (false, false, false);
            for e in &i.events {
                match e {
                    egui::Event::Copy => c = true,
                    egui::Event::Cut => x = true,
                    egui::Event::Paste(_) => p = true,
                    _ => {}
                }
            }
            (c, x, p, i.modifiers.command && i.key_pressed(Key::V))
        });

        if redo_shift || redo_y {
            self.do_redo();
        } else if undo {
            self.do_undo();
        }
        if sens {
            if copy {
                self.copy();
            }
            if cut {
                self.cut();
            }
            if paste_evt || v_key {
                self.paste();
            }
        }
        if select_all {
            self.select_all();
        }
        if delete {
            self.delete_selection();
        }
        if escape {
            if self.text_edit.is_some() {
                self.cancel_text();
            } else {
                self.selection = None;
            }
        }
        if new {
            self.new_dialog = Some((self.canvas.width(), self.canvas.height()));
        }
        if open {
            self.open_file();
        }
        if save_as {
            self.save_as();
        } else if save {
            self.save();
        }
    }

    // ----- interfaccia: menu -------------------------------------------------------

    fn rotate_items(&mut self, ui: &mut egui::Ui) {
        if ui.button("Ruota di 90 gradi a destra").clicked() {
            self.transform(|c| imageops::rotate90(c), "Ruotata a destra");
            ui.close_menu();
        }
        if ui.button("Ruota di 90 gradi a sinistra").clicked() {
            self.transform(|c| imageops::rotate270(c), "Ruotata a sinistra");
            ui.close_menu();
        }
        if ui.button("Ruota di 180 gradi").clicked() {
            self.transform(|c| imageops::rotate180(c), "Ruotata di 180 gradi");
            ui.close_menu();
        }
        ui.separator();
        if ui.button("Capovolgi in orizzontale").clicked() {
            self.transform(|c| imageops::flip_horizontal(c), "Capovolta in orizzontale");
            ui.close_menu();
        }
        if ui.button("Capovolgi in verticale").clicked() {
            self.transform(|c| imageops::flip_vertical(c), "Capovolta in verticale");
            ui.close_menu();
        }
    }

    fn open_resize_dialog(&mut self) {
        let (w, h) = (self.canvas.width(), self.canvas.height());
        self.resize_dialog = Some(ResizeDialog {
            w,
            h,
            keep_ratio: true,
            ratio: w as f32 / h.max(1) as f32,
        });
    }

    fn menu_bar(&mut self, ctx: &egui::Context) {
        let frame = egui::Frame::none()
            .inner_margin(egui::Margin::symmetric(8.0, 2.0))
            .fill(ctx.style().visuals.panel_fill);
        egui::TopBottomPanel::top("menu_bar").frame(frame).show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui
                        .add(egui::Button::new("Nuovo...").shortcut_text("Ctrl+N"))
                        .clicked()
                    {
                        self.new_dialog = Some((self.canvas.width(), self.canvas.height()));
                        ui.close_menu();
                    }
                    if ui
                        .add(egui::Button::new("Apri...").shortcut_text("Ctrl+O"))
                        .clicked()
                    {
                        ui.close_menu();
                        self.open_file();
                    }
                    if ui
                        .add(egui::Button::new("Salva").shortcut_text("Ctrl+S"))
                        .clicked()
                    {
                        ui.close_menu();
                        self.save();
                    }
                    if ui
                        .add(egui::Button::new("Salva con nome...").shortcut_text("Ctrl+Maiusc+S"))
                        .clicked()
                    {
                        ui.close_menu();
                        self.save_as();
                    }
                    ui.separator();
                    if ui.button("Esci").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Modifica", |ui| {
                    if ui
                        .add_enabled(
                            !self.undo.is_empty(),
                            egui::Button::new("Annulla").shortcut_text("Ctrl+Z"),
                        )
                        .clicked()
                    {
                        self.do_undo();
                        ui.close_menu();
                    }
                    if ui
                        .add_enabled(
                            !self.redo.is_empty(),
                            egui::Button::new("Ripeti").shortcut_text("Ctrl+Y"),
                        )
                        .clicked()
                    {
                        self.do_redo();
                        ui.close_menu();
                    }
                    ui.separator();
                    let has_sel = self.selection.is_some();
                    if ui
                        .add_enabled(has_sel, egui::Button::new("Taglia").shortcut_text("Ctrl+X"))
                        .clicked()
                    {
                        self.cut();
                        ui.close_menu();
                    }
                    if ui
                        .add_enabled(has_sel, egui::Button::new("Copia").shortcut_text("Ctrl+C"))
                        .clicked()
                    {
                        self.copy();
                        ui.close_menu();
                    }
                    if ui
                        .add(egui::Button::new("Incolla").shortcut_text("Ctrl+V"))
                        .clicked()
                    {
                        self.paste();
                        ui.close_menu();
                    }
                    if ui
                        .add_enabled(has_sel, egui::Button::new("Elimina").shortcut_text("Canc"))
                        .clicked()
                    {
                        self.delete_selection();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui
                        .add(egui::Button::new("Seleziona tutto").shortcut_text("Ctrl+A"))
                        .clicked()
                    {
                        self.select_all();
                        ui.close_menu();
                    }
                    if ui.button("Cancella tutto").clicked() {
                        self.clear();
                        ui.close_menu();
                    }
                });
                ui.menu_button("Immagine", |ui| {
                    if ui
                        .add_enabled(self.selection.is_some(), egui::Button::new("Ritaglia selezione"))
                        .clicked()
                    {
                        self.crop_to_selection();
                        ui.close_menu();
                    }
                    if ui.button("Ridimensiona...").clicked() {
                        self.open_resize_dialog();
                        ui.close_menu();
                    }
                    ui.separator();
                    self.rotate_items(ui);
                    ui.separator();
                    if ui.button("Inverti colori").clicked() {
                        self.invert_colors();
                        ui.close_menu();
                    }
                });
                ui.menu_button("Visualizza", |ui| {
                    if ui.button("Ingrandisci").clicked() {
                        self.zoom = (self.zoom * 1.25).min(16.0);
                    }
                    if ui.button("Riduci").clicked() {
                        self.zoom = (self.zoom / 1.25).max(0.25);
                    }
                    if ui.button("Zoom 100%").clicked() {
                        self.zoom = 1.0;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.checkbox(&mut self.dark, "Tema scuro").changed() {
                        setup_style(ctx, self.dark);
                    }
                });
                ui.menu_button("Info", |ui| {
                    if ui.button(format!("Informazioni su {}", APP_NAME)).clicked() {
                        self.about_open = true;
                        ui.close_menu();
                    }
                });
            });
        });
    }

    // ----- interfaccia: barra degli strumenti -------------------------------------

    fn toolbar(&mut self, ctx: &egui::Context) {
        let dark = self.dark;
        let frame = egui::Frame::none()
            .fill(if dark {
                Color32::from_rgb(40, 40, 40)
            } else {
                Color32::from_rgb(250, 250, 250)
            })
            .inner_margin(egui::Margin::symmetric(12.0, 8.0))
            .stroke(Stroke::new(
                1.0,
                if dark {
                    Color32::from_gray(62)
                } else {
                    Color32::from_gray(222)
                },
            ));

        egui::TopBottomPanel::top("toolbar").frame(frame).show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                // --- Appunti
                group(ui, "Appunti", |ui| {
                    let has_sel = self.selection.is_some();
                    if ui
                        .add(egui::Button::new("Incolla").min_size(vec2(84.0, 24.0)))
                        .clicked()
                    {
                        self.paste();
                    }
                    if ui
                        .add_enabled(has_sel, egui::Button::new("Taglia").min_size(vec2(84.0, 24.0)))
                        .clicked()
                    {
                        self.cut();
                    }
                    if ui
                        .add_enabled(has_sel, egui::Button::new("Copia").min_size(vec2(84.0, 24.0)))
                        .clicked()
                    {
                        self.copy();
                    }
                });

                // --- Immagine
                group(ui, "Immagine", |ui| {
                    let has_sel = self.selection.is_some();
                    if ui
                        .add_enabled(has_sel, egui::Button::new("Ritaglia").min_size(vec2(104.0, 24.0)))
                        .clicked()
                    {
                        self.crop_to_selection();
                    }
                    if ui
                        .add(egui::Button::new("Ridimensiona").min_size(vec2(104.0, 24.0)))
                        .clicked()
                    {
                        self.open_resize_dialog();
                    }
                    ui.menu_button("Ruota / Capovolgi", |ui| {
                        self.rotate_items(ui);
                    });
                });

                // --- Strumenti
                group(ui, "Strumenti", |ui| {
                    egui::Grid::new("tools_grid").spacing([2.0, 2.0]).show(ui, |ui| {
                        for (i, (tool, name)) in MAIN_TOOLS.iter().enumerate() {
                            if tool_button(ui, self.tool == *tool, *tool, name, 34.0).clicked() {
                                self.tool = *tool;
                            }
                            if i % 4 == 3 {
                                ui.end_row();
                            }
                        }
                    });
                });

                // --- Forme
                group(ui, "Forme", |ui| {
                    ui.horizontal(|ui| {
                        for (tool, name) in SHAPE_TOOLS.iter() {
                            if tool_button(ui, self.tool == *tool, *tool, name, 34.0).clicked() {
                                self.tool = *tool;
                            }
                        }
                    });
                    let current = match self.shape_mode {
                        ShapeMode::Outline => "Contorno",
                        ShapeMode::Filled => "Pieno",
                        ShapeMode::Both => "Contorno + pieno",
                    };
                    egui::ComboBox::from_id_source("shape_mode")
                        .width(100.0)
                        .selected_text(current)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.shape_mode, ShapeMode::Outline, "Contorno");
                            ui.selectable_value(&mut self.shape_mode, ShapeMode::Filled, "Pieno");
                            ui.selectable_value(&mut self.shape_mode, ShapeMode::Both, "Contorno + pieno");
                        });
                });

                // --- Dimensione
                group(ui, "Dimensione", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Spessore");
                        ui.add(egui::Slider::new(&mut self.size, 1..=64));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Testo      ");
                        ui.add(egui::Slider::new(&mut self.text_size, 8..=200));
                    });
                    ui.checkbox(&mut self.eraser_transparent, "Gomma trasparente");
                });

                // --- Colori
                group(ui, "Colori", |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.color_edit_button_srgb(&mut self.primary)
                                    .on_hover_text("Colore 1 (clic sinistro)");
                                ui.label(egui::RichText::new("Colore 1").small());
                            });
                            ui.horizontal(|ui| {
                                ui.color_edit_button_srgb(&mut self.secondary)
                                    .on_hover_text("Colore 2 (clic destro)");
                                ui.label(egui::RichText::new("Colore 2").small());
                            });
                            if ui.small_button("Scambia").clicked() {
                                std::mem::swap(&mut self.primary, &mut self.secondary);
                            }
                        });
                        egui::Grid::new("palette").spacing([3.0, 3.0]).show(ui, |ui| {
                            for (i, c) in PALETTE.iter().enumerate() {
                                let resp = ui
                                    .add(
                                        egui::Button::new("")
                                            .fill(Color32::from_rgb(c[0], c[1], c[2]))
                                            .rounding(4.0)
                                            .min_size(Vec2::splat(20.0)),
                                    )
                                    .on_hover_text("Sinistro: colore 1 - Destro: colore 2");
                                if resp.clicked() {
                                    self.primary = *c;
                                }
                                if resp.secondary_clicked() {
                                    self.secondary = *c;
                                }
                                if i % 10 == 9 {
                                    ui.end_row();
                                }
                            }
                        });
                    });
                });
            });
        });
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some((x, y)) = self.cursor_px {
                    ui.label(format!("{x}, {y} px"));
                } else {
                    ui.label("-");
                }
                ui.separator();
                if let Some(s) = &self.selection {
                    ui.label(format!("Selezione: {} x {} px", s.w, s.h));
                    ui.separator();
                }
                ui.label(format!("{} x {} px", self.canvas.width(), self.canvas.height()));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("100%").clicked() {
                        self.zoom = 1.0;
                    }
                    ui.add(
                        egui::Slider::new(&mut self.zoom, 0.25..=16.0)
                            .logarithmic(true)
                            .max_decimals(2)
                            .suffix("x"),
                    );
                    ui.label("Zoom");
                    ui.separator();
                    ui.label(self.status.as_str());
                });
            });
        });
    }

    // ----- interfaccia: tela ---------------------------------------------------------

    fn canvas_panel(&mut self, ctx: &egui::Context) {
        let bg = if self.dark {
            Color32::from_gray(36)
        } else {
            Color32::from_rgb(228, 228, 230)
        };
        let frame = egui::Frame::none()
            .fill(bg)
            .inner_margin(egui::Margin::same(16.0));

        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            // Zoom con Ctrl + rotellina del mouse.
            if ui.rect_contains_pointer(ui.max_rect()) {
                let zoom_delta = ui.input(|i| i.zoom_delta());
                if (zoom_delta - 1.0).abs() > f32::EPSILON {
                    self.zoom = (self.zoom * zoom_delta).clamp(0.25, 16.0);
                }
            }

            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .drag_to_scroll(false)
                .show(ui, |ui| {
                    let size = Vec2::new(self.canvas.width() as f32, self.canvas.height() as f32)
                        * self.zoom;
                    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());

                    self.process_canvas_input(ui, &response, rect);

                    if self.dirty {
                        self.upload_texture(ui.ctx());
                    }

                    // Ombra morbida sotto la tela.
                    ui.painter().rect_filled(
                        rect.translate(vec2(2.0, 3.0)).expand(1.0),
                        3.0,
                        Color32::from_black_alpha(45),
                    );
                    if self.has_alpha {
                        self.paint_checkerboard(ui, rect);
                    }
                    if let Some(tex) = &self.texture {
                        ui.painter().image(
                            tex.id(),
                            rect,
                            Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }

                    self.draw_overlays(ui, &response, rect);
                });
        });
    }

    /// Scacchiera per mostrare le aree trasparenti.
    fn paint_checkerboard(&self, ui: &egui::Ui, rect: Rect) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::WHITE);
        let cell = 8.0;
        let vis = rect.intersect(ui.clip_rect());
        if vis.width() <= 0.0 || vis.height() <= 0.0 {
            return;
        }
        let c0 = ((vis.min.x - rect.min.x) / cell).floor() as i32;
        let c1 = ((vis.max.x - rect.min.x) / cell).ceil() as i32;
        let r0 = ((vis.min.y - rect.min.y) / cell).floor() as i32;
        let r1 = ((vis.max.y - rect.min.y) / cell).ceil() as i32;
        let grey = Color32::from_gray(214);
        for r in r0..r1 {
            for c in c0..c1 {
                if (r + c).rem_euclid(2) == 1 {
                    let min = rect.min + vec2(c as f32 * cell, r as f32 * cell);
                    painter.rect_filled(Rect::from_min_size(min, Vec2::splat(cell)), 0.0, grey);
                }
            }
        }
    }

    /// Cerchio del pennello, selezione tratteggiata e cursore del testo.
    fn draw_overlays(&self, ui: &egui::Ui, response: &egui::Response, rect: Rect) {
        let painter = ui.painter();

        painter.rect_stroke(rect.expand(0.5), 0.0, Stroke::new(1.0, Color32::from_black_alpha(90)));

        if let Some(sel) = &self.selection {
            let min = rect.min + vec2(sel.x as f32, sel.y as f32) * self.zoom;
            let r = Rect::from_min_size(min, vec2(sel.w as f32, sel.h as f32) * self.zoom);
            dashed_rect(painter, r, Color32::WHITE, Color32::BLACK);
        }

        if let Some(te) = &self.text_edit {
            if te.content.is_empty() {
                let top = rect.min + vec2(te.pos.0 as f32, te.pos.1 as f32) * self.zoom;
                painter.line_segment(
                    [top, top + vec2(0.0, self.text_size as f32 * self.zoom)],
                    Stroke::new(1.5, ACCENT),
                );
            }
        }

        if response.hovered() {
            let radius = match self.tool {
                Tool::Brush | Tool::Eraser => self.size as f32 / 2.0,
                Tool::Spray => self.size as f32,
                _ => return,
            };
            if let Some(pos) = ui.input(|i| i.pointer.latest_pos()) {
                let r = (radius * self.zoom).max(2.0);
                painter.circle_stroke(pos, r + 1.0, Stroke::new(1.0, Color32::WHITE));
                painter.circle_stroke(pos, r, Stroke::new(1.0, Color32::BLACK));
            }
        }
    }

    // ----- finestre di dialogo ---------------------------------------------------------

    fn text_dialog(&mut self, ctx: &egui::Context) {
        let mut action: Option<bool> = None;
        let mut changed = false;

        if let Some(te) = &mut self.text_edit {
            egui::Window::new("Testo")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::RIGHT_BOTTOM, vec2(-16.0, -40.0))
                .show(ctx, |ui| {
                    let resp = ui.add(
                        egui::TextEdit::multiline(&mut te.content)
                            .desired_rows(3)
                            .desired_width(260.0)
                            .hint_text("Scrivi qui..."),
                    );
                    if te.focus {
                        resp.request_focus();
                        te.focus = false;
                    }
                    if resp.changed() {
                        changed = true;
                    }
                    let before = self.text_size;
                    ui.horizontal(|ui| {
                        ui.label("Dimensione");
                        ui.add(egui::Slider::new(&mut self.text_size, 8..=200));
                    });
                    if self.text_size != before {
                        changed = true;
                    }
                    ui.label(egui::RichText::new("Clic sulla tela per spostare il testo").small().weak());
                    ui.horizontal(|ui| {
                        if ui.button("Applica").clicked() {
                            action = Some(true);
                        }
                        if ui.button("Annulla").clicked() {
                            action = Some(false);
                        }
                    });
                });
            if changed {
                te.needs_render = true;
            }
        }

        match action {
            Some(true) => self.commit_text(),
            Some(false) => self.cancel_text(),
            None => {}
        }
    }

    fn new_image_dialog(&mut self, ctx: &egui::Context) {
        let mut action: Option<bool> = None;

        if let Some((w, h)) = &mut self.new_dialog {
            egui::Window::new("Nuova immagine")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Larghezza:");
                        ui.add(egui::DragValue::new(w).range(1..=8192).suffix(" px"));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Altezza:    ");
                        ui.add(egui::DragValue::new(h).range(1..=8192).suffix(" px"));
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button("Crea").clicked() {
                            action = Some(true);
                        }
                        if ui.button("Annulla").clicked() {
                            action = Some(false);
                        }
                    });
                });
        }

        if let Some(confirmed) = action {
            if let Some((w, h)) = self.new_dialog.take() {
                if confirmed {
                    self.new_image(w, h);
                }
            }
        }
    }

    fn resize_dialog_ui(&mut self, ctx: &egui::Context) {
        let mut action: Option<bool> = None;

        if let Some(d) = &mut self.resize_dialog {
            egui::Window::new("Ridimensiona immagine")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.checkbox(&mut d.keep_ratio, "Mantieni le proporzioni");
                    let (w0, h0) = (d.w, d.h);
                    ui.horizontal(|ui| {
                        ui.label("Larghezza:");
                        ui.add(egui::DragValue::new(&mut d.w).range(1..=8192).suffix(" px"));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Altezza:    ");
                        ui.add(egui::DragValue::new(&mut d.h).range(1..=8192).suffix(" px"));
                    });
                    if d.keep_ratio {
                        if d.w != w0 {
                            d.h = ((d.w as f32 / d.ratio).round() as u32).clamp(1, 8192);
                        } else if d.h != h0 {
                            d.w = ((d.h as f32 * d.ratio).round() as u32).clamp(1, 8192);
                        }
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button("Applica").clicked() {
                            action = Some(true);
                        }
                        if ui.button("Annulla").clicked() {
                            action = Some(false);
                        }
                    });
                });
        }

        if let Some(confirmed) = action {
            if let Some(d) = self.resize_dialog.take() {
                if confirmed {
                    self.resize_image(d.w, d.h);
                }
            }
        }
    }

    fn about_dialog(&mut self, ctx: &egui::Context) {
        if !self.about_open {
            return;
        }
        let mut open = true;
        let mut close_clicked = false;
        egui::Window::new(format!("Informazioni su {}", APP_NAME))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(380.0);
                ui.add_space(4.0);
                ui.heading(APP_NAME);
                ui.add_space(6.0);
                egui::Grid::new("about_grid").num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
                    ui.label(egui::RichText::new("Nome del programma").weak());
                    ui.label(APP_NAME);
                    ui.end_row();
                    ui.label(egui::RichText::new("Versione").weak());
                    ui.label(APP_VERSION);
                    ui.end_row();
                });
                ui.add_space(8.0);
                ui.label(APP_DESCRIPTION);
                ui.add_space(10.0);
                if ui.button("Chiudi").clicked() {
                    close_clicked = true;
                }
            });
        if !open || close_clicked {
            self.about_open = false;
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let name = self
            .file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("Senza titolo");
        let title = format!(
            "{}{} - {}",
            name,
            if self.modified { "*" } else { "" },
            APP_NAME
        );
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }
}

impl eframe::App for PaintApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_shortcuts(ctx);
        self.menu_bar(ctx);
        self.toolbar(ctx);
        self.status_bar(ctx);

        // Cambiando strumento il testo in corso viene applicato.
        if self.tool != Tool::Text && self.text_edit.is_some() {
            self.commit_text();
        }

        self.text_dialog(ctx);
        self.new_image_dialog(ctx);
        self.resize_dialog_ui(ctx);
        self.about_dialog(ctx);
        self.render_text();
        self.canvas_panel(ctx);
        self.update_title(ctx);
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([1040.0, 520.0])
            .with_title(format!("Senza titolo - {}", APP_NAME)),
        ..Default::default()
    };
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(|cc| Ok(Box::new(PaintApp::new(&cc.egui_ctx)))),
    )
}
