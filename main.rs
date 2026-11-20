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

const MAX_UNDO: usize = 20;
/// Limite di memoria della cronologia: oltre questo si scartano i passi piu' vecchi.
const MAX_UNDO_BYTES: usize = 192 * 1024 * 1024;
const DEFAULT_SIZE: (u32, u32) = (1024, 600);
/// Spessore dei righelli attorno al foglio (punti schermo).
const RULER: f32 = 20.0;
/// Spazio libero oltre il foglio per le maniglie di ridimensionamento.
const CANVAS_TAIL: f32 = 12.0;
const WHITE: [u8; 3] = [255, 255, 255];
const BLACK: [u8; 3] = [0, 0, 0];
const ACCENT: Color32 = Color32::from_rgb(0, 103, 192);

const APP_NAME: &str = "Paint CP";
const APP_VERSION: &str = "1.3";
const APP_DESCRIPTION: &str = "Paint CP è una app semplice per piccole modifiche alle foto in stile Paint di Windows, creato da Peppe CP con l'aiuto della IA.";
const APP_DESCRIPTION_EN: &str = "Paint CP is a simple app for small edits to photos in the style of Windows Paint, made by Peppe CP with the help of AI.";

/// Lingua dell'interfaccia, scelta nella sezione Impostazioni.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Lang {
    It,
    En,
}

/// Testo dell'interfaccia nella lingua scelta (italiano o inglese).
fn tr(lang: Lang, it: &'static str, en: &'static str) -> &'static str {
    match lang {
        Lang::It => it,
        Lang::En => en,
    }
}

/// File di configurazione con le impostazioni salvate (per ora la lingua).
fn settings_path() -> Option<PathBuf> {
    let folder = if cfg!(windows) {
        "Paint CP"
    } else {
        "paint-cp"
    };
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    base.map(|b| b.join(folder).join("settings.txt"))
}

/// Impostazioni salvate: lingua, tema e avvio massimizzato.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Settings {
    lang: Lang,
    dark: bool,
    maximized: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            lang: Lang::It,
            dark: false,
            maximized: false,
        }
    }
}

impl Settings {
    /// Legge il file di configurazione; i valori mancanti restano sul default.
    fn load() -> Self {
        let mut settings = Self::default();
        let Some(path) = settings_path() else {
            return settings;
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return settings;
        };
        for line in text.lines() {
            let Some((key, value)) = line.trim().split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "lang" => {
                    settings.lang = if value.eq_ignore_ascii_case("en") {
                        Lang::En
                    } else {
                        Lang::It
                    };
                }
                "theme" => settings.dark = value.eq_ignore_ascii_case("dark"),
                "maximized" => settings.maximized = matches!(value, "1" | "true" | "yes" | "si"),
                _ => {}
            }
        }
        settings
    }

    /// Scrive tutte le impostazioni; un errore di scrittura non blocca l'app.
    fn save(self) {
        let Some(path) = settings_path() else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let lang = match self.lang {
            Lang::It => "it",
            Lang::En => "en",
        };
        let theme = if self.dark { "dark" } else { "light" };
        let maximized = if self.maximized { 1 } else { 0 };
        let _ = std::fs::write(
            path,
            format!("lang={lang}\ntheme={theme}\nmaximized={maximized}\n"),
        );
    }
}

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
    Smudge,
    Eraser,
    Spray,
    Fill,
    Picker,
    Text,
    Line,
    Rectangle,
    Ellipse,
}

const MAIN_TOOLS: [(Tool, &str, &str); 9] = [
    (Tool::Select, "Seleziona (rettangolo)", "Select (rectangle)"),
    (Tool::Pencil, "Matita", "Pencil"),
    (Tool::Brush, "Pennello", "Brush"),
    (
        Tool::Smudge,
        "Fluidifica (sfuma e trascina i colori)",
        "Smudge (blend and drag colors)",
    ),
    (Tool::Eraser, "Gomma", "Eraser"),
    (Tool::Spray, "Spray", "Spray"),
    (Tool::Fill, "Secchiello", "Fill"),
    (Tool::Picker, "Contagocce", "Color picker"),
    (Tool::Text, "Testo", "Text"),
];

const SHAPE_TOOLS: [(Tool, &str, &str); 3] = [
    (Tool::Line, "Linea", "Line"),
    (Tool::Rectangle, "Rettangolo", "Rectangle"),
    (Tool::Ellipse, "Ellisse", "Ellipse"),
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

/// Lato del foglio che si sta trascinando.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ResizeHandle {
    Right,
    Bottom,
    Corner,
}

/// Trascinamento in corso di un margine del foglio (come in Paint).
struct CanvasResize {
    handle: ResizeHandle,
    /// Documento com'era all'inizio del trascinamento.
    original: DocState,
    size0: (u32, u32),
    last: (u32, u32),
}

/// Un livello dell'immagine: pixel, visibilita' e opacita'.
#[derive(Clone)]
struct Layer {
    name: String,
    image: RgbaImage,
    visible: bool,
    opacity: f32,
}

impl Layer {
    fn background(w: u32, h: u32, name: &str) -> Self {
        Self {
            name: name.to_string(),
            image: RgbaImage::from_pixel(w.max(1), h.max(1), rgba(WHITE)),
            visible: true,
            opacity: 1.0,
        }
    }

    fn bytes(&self) -> usize {
        self.image.as_raw().len() + 64
    }
}

/// Stato completo del documento: serve per annulla / ripeti.
#[derive(Clone)]
struct DocState {
    layers: Vec<Layer>,
    active: usize,
}

impl DocState {
    fn bytes(&self) -> usize {
        self.layers.iter().map(Layer::bytes).sum::<usize>() + 32
    }
}

struct PaintApp {
    /// Livelli del documento, dal piu' basso al piu' alto.
    layers: Vec<Layer>,
    /// Indice del livello su cui si sta disegnando.
    active: usize,
    /// Immagine composta pronta per lo schermo (aggiornata solo quando serve).
    display: RgbaImage,
    texture: Option<TextureHandle>,
    dirty: bool,
    has_alpha: bool,

    undo: Vec<DocState>,
    redo: Vec<DocState>,

    tool: Tool,
    primary: [u8; 3],
    secondary: [u8; 3],
    size: u32,
    text_size: u32,
    smudge: f32,
    shape_mode: ShapeMode,
    show_layers: bool,
    eraser_transparent: bool,
    zoom: f32,
    show_ruler: bool,
    dark: bool,
    /// Apri la finestra sempre massimizzata (Impostazioni).
    maximized: bool,
    /// Lingua dell'interfaccia (Impostazioni).
    lang: Lang,

    stroke: Option<ActiveStroke>,
    resize_drag: Option<CanvasResize>,
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
    settings_open: bool,
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

/// Come `overlay`, ma con l'opacita' del livello applicata ai pixel sorgente.
fn overlay_with_opacity(dst: &mut RgbaImage, src: &RgbaImage, opacity: f32) {
    let op = opacity.clamp(0.0, 1.0);
    if op <= 0.0 {
        return;
    }
    for (x, y, p) in src.enumerate_pixels() {
        if p.0[3] == 0 {
            continue;
        }
        let mut s = *p;
        if op < 1.0 {
            s.0[3] = (s.0[3] as f32 * op).round() as u8;
        }
        blend_pixel(dst, x as i32, y as i32, s);
    }
}

/// Cambia la dimensione del foglio senza deformare il disegno: il contenuto resta
/// ancorato in alto a sinistra, la parte nuova e' bianca (o trasparente).
fn resize_canvas_image(img: &RgbaImage, w: u32, h: u32, white: bool) -> RgbaImage {
    let mut out = if white {
        RgbaImage::from_pixel(w, h, rgba(WHITE))
    } else {
        RgbaImage::new(w, h)
    };
    let cw = img.width().min(w);
    let ch = img.height().min(h);
    for y in 0..ch {
        for x in 0..cw {
            out.put_pixel(x, y, *img.get_pixel(x, y));
        }
    }
    out
}

/// Passo delle tacche del righello, scelto perche' le etichette non si accavallino.
fn ruler_step(zoom: f32) -> u32 {
    let min_px = 56.0 / zoom.max(0.01);
    const STEPS: [u32; 14] = [
        1, 2, 5, 10, 20, 25, 50, 100, 200, 250, 500, 1000, 2000, 5000,
    ];
    for s in STEPS {
        if s as f32 >= min_px {
            return s;
        }
    }
    10000
}

/// Icona della finestra, ricavata dal PNG multirisoluzione del progetto.
fn window_icon() -> Option<egui::IconData> {
    let img = image::load_from_memory(include_bytes!("assets/icon.png"))
        .ok()?
        .to_rgba8();
    Some(egui::IconData {
        rgba: img.as_raw().clone(),
        width: img.width(),
        height: img.height(),
    })
}

/// Ritaglia una regione anche se esce dai bordi: le parti fuori restano trasparenti.
fn crop_clamped(img: &RgbaImage, x: i32, y: i32, w: i32, h: i32) -> RgbaImage {
    let (w, h) = (w.max(1), h.max(1));
    let mut out = RgbaImage::new(w as u32, h as u32);
    for sy in 0..h {
        for sx in 0..w {
            let (ix, iy) = (x + sx, y + sy);
            if ix >= 0 && iy >= 0 && (ix as u32) < img.width() && (iy as u32) < img.height() {
                out.put_pixel(sx as u32, sy as u32, *img.get_pixel(ix as u32, iy as u32));
            }
        }
    }
    out
}

/// Fluidifica: trascina e sfuma i colori lungo il tratto (come il dito sul colore fresco).
fn smudge(img: &mut RgbaImage, from: (i32, i32), to: (i32, i32), size: u32, strength: f32) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    if dx == 0 && dy == 0 {
        return;
    }
    let r = (size as f32 / 2.0).max(1.0);
    let ri = r.ceil() as i32;
    let side = (2 * ri + 1) as usize;
    // Campioniamo prima la zona spostata, cosi' il trascinamento non si "mangia" da solo.
    let mut sample: Vec<Rgba<u8>> = Vec::with_capacity(side * side);
    for oy in -ri..=ri {
        for ox in -ri..=ri {
            let (sx, sy) = (to.0 + ox - dx, to.1 + oy - dy);
            if sx >= 0 && sy >= 0 && (sx as u32) < img.width() && (sy as u32) < img.height() {
                sample.push(*img.get_pixel(sx as u32, sy as u32));
            } else {
                sample.push(Rgba([0, 0, 0, 0]));
            }
        }
    }
    for (i, oy) in (-ri..=ri).enumerate() {
        for (j, ox) in (-ri..=ri).enumerate() {
            let dist = ((ox * ox + oy * oy) as f32).sqrt();
            if dist > r {
                continue;
            }
            let (x, y) = (to.0 + ox, to.1 + oy);
            if x < 0 || y < 0 || x >= img.width() as i32 || y >= img.height() as i32 {
                continue;
            }
            let t = strength * (1.0 - dist / r);
            if t <= 0.0 {
                continue;
            }
            let s = sample[i * side + j];
            let dst = *img.get_pixel(x as u32, y as u32);
            let mut out = dst;
            for k in 0..4 {
                let v = dst.0[k] as f32 * (1.0 - t) + s.0[k] as f32 * t;
                out.0[k] = v.round().clamp(0.0, 255.0) as u8;
            }
            img.put_pixel(x as u32, y as u32, out);
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
        for c in p.0[..3].iter_mut() {
            *c = ((*c as u32 * a + 255 * (255 - a)) / 255) as u8;
        }
        p.0[3] = 255;
    }
}

/// Disegna il testo (con antialiasing) direttamente sui pixel della tela.
fn draw_text(
    img: &mut RgbaImage,
    font: &FontArc,
    text: &str,
    pos: (i32, i32),
    px: f32,
    color: [u8; 3],
) {
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
        let glyph = id
            .with_scale_and_position(scale, ab_glyph::point(pos.0 as f32 + cx, pos.1 as f32 + cy));
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
    v.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
    if dark {
        v.panel_fill = Color32::from_rgb(32, 32, 32);
        v.window_fill = Color32::from_rgb(44, 44, 44);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(58, 66, 78);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    } else {
        v.panel_fill = Color32::from_rgb(243, 243, 243);
        v.window_fill = Color32::WHITE;
        v.widgets.inactive.weak_bg_fill = Color32::WHITE;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, Color32::from_gray(214));
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(232, 240, 250);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);
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
    painter.rect_stroke(r, 0.0, Stroke::new(1.0_f32, base));
    let pts = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    let stroke = Stroke::new(1.0_f32, dash);
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
        Tool::Smudge => {
            // Un dito che trascina il colore.
            p.circle_filled(pt(-2.0, 2.0), 4.5 * u, col);
            p.line_segment([pt(0.0, 0.0), pt(7.0, -7.0)], Stroke::new(1.6 * u, col));
            p.circle_stroke(pt(7.0, -7.0), 2.0 * u, thin);
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
            p.rect_stroke(
                Rect::from_two_pos(pt(-8.0, -2.0), pt(2.0, 8.0)),
                2.0 * u,
                thin,
            );
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
        Tool::Line => {
            p.line_segment([pt(-8.0, 8.0), pt(8.0, -8.0)], medium);
        }
        Tool::Rectangle => {
            p.rect_stroke(
                Rect::from_two_pos(pt(-9.0, -6.0), pt(9.0, 6.0)),
                1.0,
                medium,
            );
        }
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

fn tool_button(
    ui: &mut egui::Ui,
    selected: bool,
    tool: Tool,
    label: &str,
    size: f32,
) -> egui::Response {
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
            ui.painter()
                .rect_stroke(rect, 6.0, Stroke::new(1.0_f32, ACCENT));
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
        let settings = Settings::load();
        setup_style(ctx, settings.dark);
        let lang = settings.lang;
        Self {
            layers: vec![Layer::background(
                DEFAULT_SIZE.0,
                DEFAULT_SIZE.1,
                tr(lang, "Sfondo", "Background"),
            )],
            active: 0,
            display: RgbaImage::from_pixel(DEFAULT_SIZE.0, DEFAULT_SIZE.1, rgba(WHITE)),
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
            smudge: 0.5,
            shape_mode: ShapeMode::Outline,
            show_layers: true,
            eraser_transparent: false,
            zoom: 1.0,
            show_ruler: true,
            dark: settings.dark,
            maximized: settings.maximized,
            lang,
            stroke: None,
            resize_drag: None,
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
            settings_open: false,
            status: String::from(tr(lang, "Pronto", "Ready")),
            last_title: String::new(),
            rng: 0x2545_F491_4F6C_DD1D,
        }
    }

    // ----- dimensioni e livello attivo ------------------------------------

    fn width(&self) -> u32 {
        self.layers.first().map(|l| l.image.width()).unwrap_or(1)
    }

    fn height(&self) -> u32 {
        self.layers.first().map(|l| l.image.height()).unwrap_or(1)
    }

    /// Pixel del livello attivo: e' qui che disegnano tutti gli strumenti.
    fn canvas(&self) -> &RgbaImage {
        &self.layers[self.active].image
    }

    fn canvas_mut(&mut self) -> &mut RgbaImage {
        &mut self.layers[self.active].image
    }

    fn set_canvas(&mut self, img: RgbaImage) {
        self.layers[self.active].image = img;
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
            p.0.clamp(0, self.canvas().width() as i32 - 1),
            p.1.clamp(0, self.canvas().height() as i32 - 1),
        )
    }

    // ----- cronologia (annulla / ripeti) ----------------------------------

    fn snapshot(&self) -> DocState {
        DocState {
            layers: self.layers.clone(),
            active: self.active,
        }
    }

    fn restore(&mut self, state: DocState) {
        self.layers = state.layers;
        if self.layers.is_empty() {
            self.layers.push(Layer::background(
                self.width(),
                self.height(),
                tr(self.lang, "Sfondo", "Background"),
            ));
        }
        self.active = state.active.min(self.layers.len() - 1);
        self.stroke = None;
        self.selection = None;
        self.dirty = true;
    }

    /// Scarta i passi piu' vecchi quando la cronologia diventa troppo pesante.
    fn trim_history(&mut self) {
        let mut total: usize = self
            .undo
            .iter()
            .chain(self.redo.iter())
            .map(DocState::bytes)
            .sum();
        while self.undo.len() > 5 && (self.undo.len() > MAX_UNDO || total > MAX_UNDO_BYTES) {
            let removed = self.undo.remove(0);
            total = total.saturating_sub(removed.bytes());
        }
    }

    fn push_undo(&mut self) {
        self.commit_text();
        self.undo.push(self.snapshot());
        self.redo.clear();
        self.trim_history();
        self.modified = true;
    }

    fn do_undo(&mut self) {
        self.commit_text();
        if let Some(prev) = self.undo.pop() {
            let current = self.snapshot();
            self.restore(prev);
            self.redo.push(current);
            self.modified = true;
        }
    }

    fn do_redo(&mut self) {
        self.commit_text();
        if let Some(next) = self.redo.pop() {
            let current = self.snapshot();
            self.restore(next);
            self.undo.push(current);
            self.modified = true;
        }
    }

    // ----- operazioni sull'immagine ---------------------------------------

    fn new_image(&mut self, w: u32, h: u32) {
        self.commit_text();
        self.push_undo();
        self.selection = None;
        self.layers = vec![Layer::background(
            w,
            h,
            tr(self.lang, "Sfondo", "Background"),
        )];
        self.active = 0;
        self.has_alpha = false;
        self.file_path = None;
        self.modified = false;
        self.dirty = true;
        self.status = String::from(tr(self.lang, "Nuova immagine", "New image"));
    }

    fn clear(&mut self) {
        self.push_undo();
        self.selection = None;
        let c = rgba(self.secondary);
        for p in self.canvas_mut().pixels_mut() {
            *p = c;
        }
        self.dirty = true;
        self.status = String::from(tr(self.lang, "Livello svuotato", "Layer cleared"));
    }

    fn transform(&mut self, f: impl Fn(&RgbaImage) -> RgbaImage, msg: &str) {
        self.push_undo();
        self.selection = None;
        for layer in &mut self.layers {
            layer.image = f(&layer.image);
        }
        self.dirty = true;
        self.status = msg.to_string();
    }

    fn invert_colors(&mut self) {
        self.push_undo();
        self.selection = None;
        for p in self.canvas_mut().pixels_mut() {
            for c in p.0[..3].iter_mut() {
                *c = 255 - *c;
            }
        }
        self.dirty = true;
        self.status = String::from(tr(self.lang, "Colori invertiti", "Colors inverted"));
    }

    fn resize_image(&mut self, w: u32, h: u32) {
        let (w, h) = (w.max(1), h.max(1));
        self.push_undo();
        self.selection = None;
        for layer in &mut self.layers {
            layer.image = imageops::resize(&layer.image, w, h, imageops::FilterType::Lanczos3);
        }
        self.dirty = true;
        self.status = format!(
            "{} {} x {}",
            tr(self.lang, "Ridimensionata a", "Resized to"),
            w,
            h
        );
    }

    // ----- livelli ---------------------------------------------------------

    /// Unisce tutti i livelli visibili in una sola immagine.
    fn composite(&self) -> RgbaImage {
        let mut out = RgbaImage::new(self.width(), self.height());
        for layer in &self.layers {
            if !layer.visible {
                continue;
            }
            overlay_with_opacity(&mut out, &layer.image, layer.opacity);
        }
        out
    }

    fn add_layer(&mut self) {
        self.commit_text();
        self.push_undo();
        let (w, h) = (self.width(), self.height());
        let name = format!(
            "{} {}",
            tr(self.lang, "Livello", "Level"),
            self.layers.len() + 1
        );
        self.layers.insert(
            self.active + 1,
            Layer {
                name,
                image: RgbaImage::new(w, h),
                visible: true,
                opacity: 1.0,
            },
        );
        self.active += 1;
        self.selection = None;
        self.dirty = true;
        self.status = String::from(tr(
            self.lang,
            "Nuovo livello trasparente",
            "New transparent layer",
        ));
    }

    fn duplicate_layer(&mut self) {
        self.commit_text();
        self.push_undo();
        let mut copy = self.layers[self.active].clone();
        copy.name = format!("{} {}", copy.name, tr(self.lang, "copia", "copy"));
        self.layers.insert(self.active + 1, copy);
        self.active += 1;
        self.selection = None;
        self.dirty = true;
        self.status = String::from(tr(self.lang, "Livello duplicato", "Layer duplicated"));
    }

    fn delete_layer(&mut self) {
        if self.layers.len() <= 1 {
            self.status = String::from(tr(
                self.lang,
                "Serve almeno un livello",
                "At least one layer is required",
            ));
            return;
        }
        self.commit_text();
        self.push_undo();
        self.layers.remove(self.active);
        self.active = self.active.min(self.layers.len() - 1);
        self.selection = None;
        self.dirty = true;
        self.status = String::from(tr(self.lang, "Livello eliminato", "Layer deleted"));
    }

    fn move_layer(&mut self, up: bool) {
        let other = if up {
            if self.active + 1 >= self.layers.len() {
                return;
            }
            self.active + 1
        } else {
            match self.active.checked_sub(1) {
                Some(i) => i,
                None => return,
            }
        };
        self.commit_text();
        self.push_undo();
        self.layers.swap(self.active, other);
        self.active = other;
        self.selection = None;
        self.dirty = true;
    }

    fn merge_down(&mut self) {
        if self.active == 0 {
            self.status = String::from(tr(
                self.lang,
                "Nessun livello sotto da unire",
                "No layer below to merge into",
            ));
            return;
        }
        self.commit_text();
        self.push_undo();
        let upper = self.layers.remove(self.active);
        self.active -= 1;
        let target = &mut self.layers[self.active];
        overlay_with_opacity(&mut target.image, &upper.image, upper.opacity);
        target.opacity = 1.0;
        self.selection = None;
        self.dirty = true;
        self.status = String::from(tr(
            self.lang,
            "Livello unito a quello sotto",
            "Layer merged into the one below",
        ));
    }

    fn flatten_layers(&mut self) {
        self.commit_text();
        self.push_undo();
        let image = self.composite();
        self.layers = vec![Layer {
            name: String::from(tr(self.lang, "Sfondo", "Background")),
            image,
            visible: true,
            opacity: 1.0,
        }];
        self.active = 0;
        self.selection = None;
        self.dirty = true;
        self.status = String::from(tr(self.lang, "Livelli appiattiti", "Layers flattened"));
    }

    // ----- selezione e appunti ------------------------------------------------

    fn selection_image(&self) -> Option<RgbaImage> {
        let sel = self.selection.as_ref()?;
        match &sel.floating {
            Some(f) => Some(f.content.clone()),
            None => Some(crop_clamped(self.canvas(), sel.x, sel.y, sel.w, sel.h)),
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
            self.status = String::from(tr(self.lang, "Copiato", "Copied"));
        }
    }

    fn delete_selection(&mut self) {
        let Some(is_floating) = self.selection.as_ref().map(|s| s.floating.is_some()) else {
            return;
        };
        self.push_undo();
        if is_floating {
            let base = match &self.selection {
                Some(Selection {
                    floating: Some(f), ..
                }) => Some(f.base.clone()),
                _ => None,
            };
            if let Some(base) = base {
                self.set_canvas(base);
            }
        } else if let Some(sel) = &self.selection {
            let (x, y, w, h) = (sel.x, sel.y, sel.w, sel.h);
            let c = rgba(self.secondary);
            fill_rect(self.canvas_mut(), x, y, w, h, c);
        }
        self.selection = None;
        self.dirty = true;
    }

    fn cut(&mut self) {
        if self.selection.is_some() {
            self.copy();
            self.delete_selection();
            self.status = String::from(tr(self.lang, "Tagliato", "Cut"));
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
            self.status = String::from(tr(self.lang, "Appunti vuoti", "Clipboard is empty"));
            return;
        };
        self.push_undo();
        let base = self.canvas().clone();
        overlay(self.canvas_mut(), &img, 0, 0);
        self.selection = Some(Selection {
            x: 0,
            y: 0,
            w: img.width() as i32,
            h: img.height() as i32,
            floating: Some(Floating { content: img, base }),
        });
        self.tool = Tool::Select;
        self.dirty = true;
        self.status = String::from(tr(
            self.lang,
            "Incollato: trascina per spostare",
            "Pasted: drag to move",
        ));
    }

    fn select_all(&mut self) {
        self.commit_text();
        self.tool = Tool::Select;
        self.selection = Some(Selection {
            x: 0,
            y: 0,
            w: self.canvas().width() as i32,
            h: self.canvas().height() as i32,
            floating: None,
        });
    }

    fn crop_to_selection(&mut self) {
        let Some(sel) = self.selection.as_ref() else {
            self.status = String::from(tr(self.lang, "Nessuna selezione", "No selection"));
            return;
        };
        let (x, y, w, h) = (sel.x, sel.y, sel.w, sel.h);
        self.commit_text();
        self.push_undo();
        for layer in &mut self.layers {
            layer.image = crop_clamped(&layer.image, x, y, w, h);
        }
        self.selection = None;
        self.dirty = true;
        self.status = String::from(tr(self.lang, "Immagine ritagliata", "Image cropped"));
    }

    /// Solleva il contenuto della selezione: viene tolto dal livello e tenuto a parte.
    fn lift_selection(&mut self) {
        let (x, y, w, h) = match self.selection.as_ref() {
            Some(sel) => (sel.x, sel.y, sel.w, sel.h),
            None => return,
        };
        self.push_undo();
        let content = crop_clamped(self.canvas(), x, y, w, h);
        let c = rgba(self.secondary);
        fill_rect(self.canvas_mut(), x, y, w, h, c);
        let base = self.canvas().clone();
        if let Some(sel) = self.selection.as_mut() {
            sel.floating = Some(Floating { content, base });
        }
    }

    /// Sposta la selezione (con il suo contenuto) seguendo il mouse.
    fn move_selection(&mut self, p: (i32, i32), grab: (i32, i32)) {
        if matches!(&self.selection, Some(s) if s.floating.is_none()) {
            self.lift_selection();
        }
        if self.selection.is_none() {
            return;
        }
        let (x, y) = (p.0 - grab.0, p.1 - grab.1);
        if let Some(sel) = self.selection.as_mut() {
            sel.x = x;
            sel.y = y;
        }
        let Some(mut floating) = self.selection.as_mut().and_then(|s| s.floating.take()) else {
            return;
        };
        self.set_canvas(floating.base.clone());
        overlay(self.canvas_mut(), &floating.content, x, y);
        if let Some(sel) = self.selection.as_mut() {
            sel.floating = Some(Floating {
                content: std::mem::take(&mut floating.content),
                base: floating.base,
            });
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
            base: self.canvas().clone(),
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
        if !te.content.is_empty() {
            if let Some(font) = &self.font {
                draw_text(
                    &mut img,
                    font,
                    &te.content,
                    te.pos,
                    self.text_size as f32,
                    te.color,
                );
            }
        }
        self.set_canvas(img);
        self.dirty = true;
        if let Some(te) = &mut self.text_edit {
            te.needs_render = false;
        }
    }

    fn commit_text(&mut self) {
        self.render_text();
        let Some(te) = self.text_edit.take() else {
            return;
        };
        if te.content.trim().is_empty() {
            self.set_canvas(te.base);
            self.dirty = true;
            return;
        }
        // La tela contiene gia' il testo: salviamo lo stato precedente per "Annulla".
        let mut state = self.snapshot();
        if let Some(layer) = state.layers.get_mut(state.active) {
            layer.image = te.base;
        }
        self.undo.push(state);
        self.redo.clear();
        self.trim_history();
        self.modified = true;
    }

    fn cancel_text(&mut self) {
        if let Some(te) = self.text_edit.take() {
            self.set_canvas(te.base);
            self.dirty = true;
        }
    }

    // ----- file -------------------------------------------------------------

    fn open_file(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(
                tr(self.lang, "Immagini", "Images"),
                &["png", "jpg", "jpeg", "bmp"],
            )
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
                let name = path
                    .file_stem()
                    .and_then(|n| n.to_str())
                    .unwrap_or(tr(self.lang, "Sfondo", "Background"))
                    .to_string();
                self.layers = vec![Layer {
                    name,
                    image: rgba_img,
                    visible: true,
                    opacity: 1.0,
                }];
                self.active = 0;
                self.file_path = Some(path);
                self.modified = false;
                self.dirty = true;
                self.status = String::from(tr(self.lang, "Immagine aperta", "Image opened"));
            }
            Err(e) => {
                self.status = format!(
                    "{} {e}",
                    tr(
                        self.lang,
                        "Impossibile aprire il file:",
                        "Could not open the file:"
                    )
                )
            }
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
            .set_file_name(tr(self.lang, "senza_titolo.png", "untitled.png"));
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
        let mut output = self.composite();
        let result = if matches!(ext.as_str(), "jpg" | "jpeg" | "bmp") {
            // JPEG e BMP non hanno trasparenza: appoggia su bianco.
            flatten_on_white(&mut output);
            DynamicImage::ImageRgba8(output).to_rgb8().save(&path)
        } else {
            output.save(&path)
        };
        match result {
            Ok(()) => {
                self.status = format!("{} {}", tr(self.lang, "Salvato:", "Saved:"), path.display());
                self.file_path = Some(path);
                self.modified = false;
            }
            Err(e) => {
                self.status = format!(
                    "{} {e}",
                    tr(self.lang, "Errore nel salvataggio:", "Save error:")
                )
            }
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
                    && (p.0 as u32) < self.canvas().width()
                    && (p.1 as u32) < self.canvas().height()
                {
                    let px = self.canvas().get_pixel(p.0 as u32, p.1 as u32).0;
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
                flood_fill(self.canvas_mut(), p.0, p.1, color);
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
            Some(self.canvas().clone())
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
                line(self.canvas_mut(), last.0, last.1, p.0, p.1, size, color);
            }
            Tool::Smudge => {
                let strength = self.smudge.clamp(0.05, 1.0);
                let (dx, dy) = (p.0 - last.0, p.1 - last.1);
                let step = (size as f32 / 4.0).max(1.0);
                let steps = ((dx.abs().max(dy.abs()) as f32) / step).ceil().max(1.0) as i32;
                let mut prev = last;
                for i in 1..=steps {
                    let t = i as f32 / steps as f32;
                    let cur = (
                        last.0 + (dx as f32 * t).round() as i32,
                        last.1 + (dy as f32 * t).round() as i32,
                    );
                    smudge(self.canvas_mut(), prev, cur, size, strength);
                    prev = cur;
                }
            }
            Tool::Spray => {
                let mut rng = self.rng;
                spray(self.canvas_mut(), &mut rng, p, size, color);
                self.rng = rng;
            }
            Tool::Line | Tool::Rectangle | Tool::Ellipse => {
                // Ripristina la tela com'era all'inizio e ridisegna la forma aggiornata.
                if let Some(base) = self.stroke.as_ref().and_then(|s| s.base.clone()) {
                    self.set_canvas(base);
                }
                if self.tool == Tool::Line {
                    line(self.canvas_mut(), start.0, start.1, p.0, p.1, size, color);
                } else {
                    let (border, fill) = match self.shape_mode {
                        ShapeMode::Outline => (Some(color), None),
                        ShapeMode::Filled => (None, Some(color)),
                        ShapeMode::Both => (Some(color), Some(other)),
                    };
                    if self.tool == Tool::Rectangle {
                        rect_shape(self.canvas_mut(), start, p, size, border, fill);
                    } else {
                        ellipse_shape(self.canvas_mut(), start, p, size, border, fill);
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
        // Mentre si trascina un margine il foglio non deve disegnare.
        if self.resize_drag.is_some() {
            return;
        }
        let zoom = self.zoom;
        let pointer = ui.input(|i| i.pointer.latest_pos());
        let px = pointer.map(|p| {
            let v = (p - rect.min) / zoom;
            (v.x.floor() as i32, v.y.floor() as i32)
        });
        let (w, h) = (self.width() as i32, self.height() as i32);
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
        // Il livello viene composto una sola volta, solo quando qualcosa cambia.
        if self.dirty
            || self.display.width() != self.width()
            || self.display.height() != self.height()
        {
            self.display = self.composite();
            self.has_alpha = self.display.pixels().any(|p| p.0[3] < 255);
        }
        let size = [
            self.display.width() as usize,
            self.display.height() as usize,
        ];
        let image = egui::ColorImage::from_rgba_unmultiplied(size, self.display.as_raw());
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
        let (redo_shift, undo, redo_y, save_as, new, open, save, select_all, delete, escape) = ctx
            .input_mut(|i| {
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

        let new_layer =
            sens && ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::N));
        if new_layer {
            self.add_layer();
        }

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
            self.new_dialog = Some((self.canvas().width(), self.canvas().height()));
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
        if ui
            .button(tr(
                self.lang,
                "Ruota di 90 gradi a destra",
                "Rotate 90° clockwise",
            ))
            .clicked()
        {
            self.transform(
                imageops::rotate90,
                tr(self.lang, "Ruotata a destra", "Rotated 90° clockwise"),
            );
            ui.close_menu();
        }
        if ui
            .button(tr(
                self.lang,
                "Ruota di 90 gradi a sinistra",
                "Rotate 90° counter-clockwise",
            ))
            .clicked()
        {
            self.transform(
                imageops::rotate270,
                tr(
                    self.lang,
                    "Ruotata a sinistra",
                    "Rotated 90° counter-clockwise",
                ),
            );
            ui.close_menu();
        }
        if ui
            .button(tr(self.lang, "Ruota di 180 gradi", "Rotate 180°"))
            .clicked()
        {
            self.transform(
                imageops::rotate180,
                tr(self.lang, "Ruotata di 180 gradi", "Rotated 180°"),
            );
            ui.close_menu();
        }
        ui.separator();
        if ui
            .button(tr(
                self.lang,
                "Capovolgi in orizzontale",
                "Flip horizontally",
            ))
            .clicked()
        {
            self.transform(
                imageops::flip_horizontal,
                tr(
                    self.lang,
                    "Capovolta in orizzontale",
                    "Flipped horizontally",
                ),
            );
            ui.close_menu();
        }
        if ui
            .button(tr(self.lang, "Capovolgi in verticale", "Flip vertically"))
            .clicked()
        {
            self.transform(
                imageops::flip_vertical,
                tr(self.lang, "Capovolta in verticale", "Flipped vertically"),
            );
            ui.close_menu();
        }
    }

    fn open_resize_dialog(&mut self) {
        let (w, h) = (self.canvas().width(), self.canvas().height());
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
        egui::TopBottomPanel::top("menu_bar")
            .frame(frame)
            .show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        if ui
                            .add(
                                egui::Button::new(tr(self.lang, "Nuovo...", "New..."))
                                    .shortcut_text("Ctrl+N"),
                            )
                            .clicked()
                        {
                            self.new_dialog = Some((self.canvas().width(), self.canvas().height()));
                            ui.close_menu();
                        }
                        if ui
                            .add(
                                egui::Button::new(tr(self.lang, "Apri...", "Open..."))
                                    .shortcut_text("Ctrl+O"),
                            )
                            .clicked()
                        {
                            ui.close_menu();
                            self.open_file();
                        }
                        if ui
                            .add(
                                egui::Button::new(tr(self.lang, "Salva", "Save"))
                                    .shortcut_text("Ctrl+S"),
                            )
                            .clicked()
                        {
                            ui.close_menu();
                            self.save();
                        }
                        if ui
                            .add(
                                egui::Button::new(tr(self.lang, "Salva con nome...", "Save as..."))
                                    .shortcut_text(tr(self.lang, "Ctrl+Maiusc+S", "Ctrl+Shift+S")),
                            )
                            .clicked()
                        {
                            ui.close_menu();
                            self.save_as();
                        }
                        ui.separator();
                        if ui.button(tr(self.lang, "Esci", "Exit")).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.menu_button(tr(self.lang, "Modifica", "Edit"), |ui| {
                        if ui
                            .add_enabled(
                                !self.undo.is_empty(),
                                egui::Button::new(tr(self.lang, "Annulla", "Undo"))
                                    .shortcut_text("Ctrl+Z"),
                            )
                            .clicked()
                        {
                            self.do_undo();
                            ui.close_menu();
                        }
                        if ui
                            .add_enabled(
                                !self.redo.is_empty(),
                                egui::Button::new(tr(self.lang, "Ripeti", "Redo"))
                                    .shortcut_text("Ctrl+Y"),
                            )
                            .clicked()
                        {
                            self.do_redo();
                            ui.close_menu();
                        }
                        ui.separator();
                        let has_sel = self.selection.is_some();
                        if ui
                            .add_enabled(
                                has_sel,
                                egui::Button::new(tr(self.lang, "Taglia", "Cut"))
                                    .shortcut_text("Ctrl+X"),
                            )
                            .clicked()
                        {
                            self.cut();
                            ui.close_menu();
                        }
                        if ui
                            .add_enabled(
                                has_sel,
                                egui::Button::new(tr(self.lang, "Copia", "Copy"))
                                    .shortcut_text("Ctrl+C"),
                            )
                            .clicked()
                        {
                            self.copy();
                            ui.close_menu();
                        }
                        if ui
                            .add(
                                egui::Button::new(tr(self.lang, "Incolla", "Paste"))
                                    .shortcut_text("Ctrl+V"),
                            )
                            .clicked()
                        {
                            self.paste();
                            ui.close_menu();
                        }
                        if ui
                            .add_enabled(
                                has_sel,
                                egui::Button::new(tr(self.lang, "Elimina", "Delete"))
                                    .shortcut_text(tr(self.lang, "Canc", "Del")),
                            )
                            .clicked()
                        {
                            self.delete_selection();
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui
                            .add(
                                egui::Button::new(tr(self.lang, "Seleziona tutto", "Select all"))
                                    .shortcut_text("Ctrl+A"),
                            )
                            .clicked()
                        {
                            self.select_all();
                            ui.close_menu();
                        }
                        if ui
                            .button(tr(self.lang, "Cancella tutto", "Clear all"))
                            .clicked()
                        {
                            self.clear();
                            ui.close_menu();
                        }
                    });
                    ui.menu_button(tr(self.lang, "Immagine", "Image"), |ui| {
                        if ui
                            .add_enabled(
                                self.selection.is_some(),
                                egui::Button::new(tr(
                                    self.lang,
                                    "Ritaglia selezione",
                                    "Crop selection",
                                )),
                            )
                            .clicked()
                        {
                            self.crop_to_selection();
                            ui.close_menu();
                        }
                        if ui
                            .button(tr(self.lang, "Ridimensiona...", "Resize..."))
                            .clicked()
                        {
                            self.open_resize_dialog();
                            ui.close_menu();
                        }
                        ui.separator();
                        self.rotate_items(ui);
                        ui.separator();
                        if ui
                            .button(tr(self.lang, "Inverti colori", "Invert colors"))
                            .clicked()
                        {
                            self.invert_colors();
                            ui.close_menu();
                        }
                    });
                    ui.menu_button(tr(self.lang, "Livello", "Layer"), |ui| {
                        if ui
                            .add(
                                egui::Button::new(tr(self.lang, "Nuovo livello", "New layer"))
                                    .shortcut_text(tr(self.lang, "Ctrl+Maiusc+N", "Ctrl+Shift+N")),
                            )
                            .clicked()
                        {
                            self.add_layer();
                            ui.close_menu();
                        }
                        if ui
                            .button(tr(self.lang, "Duplica livello", "Duplicate layer"))
                            .clicked()
                        {
                            self.duplicate_layer();
                            ui.close_menu();
                        }
                        if ui
                            .add_enabled(
                                self.layers.len() > 1,
                                egui::Button::new(tr(self.lang, "Elimina livello", "Delete layer")),
                            )
                            .clicked()
                        {
                            self.delete_layer();
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui
                            .add_enabled(
                                self.active + 1 < self.layers.len(),
                                egui::Button::new(tr(self.lang, "Sposta su", "Move up")),
                            )
                            .clicked()
                        {
                            self.move_layer(true);
                            ui.close_menu();
                        }
                        if ui
                            .add_enabled(
                                self.active > 0,
                                egui::Button::new(tr(self.lang, "Sposta giù", "Move down")),
                            )
                            .clicked()
                        {
                            self.move_layer(false);
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui
                            .add_enabled(
                                self.active > 0,
                                egui::Button::new(tr(
                                    self.lang,
                                    "Unisci al livello sotto",
                                    "Merge down",
                                )),
                            )
                            .clicked()
                        {
                            self.merge_down();
                            ui.close_menu();
                        }
                        if ui
                            .button(tr(self.lang, "Appiattisci tutto", "Flatten image"))
                            .clicked()
                        {
                            self.flatten_layers();
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui
                            .checkbox(
                                &mut self.show_layers,
                                tr(self.lang, "Mostra il pannello livelli", "Show layers panel"),
                            )
                            .clicked()
                        {
                            ui.close_menu();
                        }
                    });
                    ui.menu_button(tr(self.lang, "Visualizza", "View"), |ui| {
                        if ui.button(tr(self.lang, "Ingrandisci", "Zoom in")).clicked() {
                            self.zoom = (self.zoom * 1.25).min(16.0);
                        }
                        if ui.button(tr(self.lang, "Riduci", "Zoom out")).clicked() {
                            self.zoom = (self.zoom / 1.25).max(0.25);
                        }
                        if ui.button(tr(self.lang, "Zoom 100%", "Zoom 100%")).clicked() {
                            self.zoom = 1.0;
                            ui.close_menu();
                        }
                        ui.separator();
                        ui.checkbox(&mut self.show_ruler, tr(self.lang, "Righello", "Ruler"));
                        ui.checkbox(
                            &mut self.show_layers,
                            tr(self.lang, "Pannello livelli", "Layers panel"),
                        );
                    });
                    ui.menu_button(tr(self.lang, "Impostazioni", "Settings"), |ui| {
                        if ui
                            .button(tr(self.lang, "Apri impostazioni...", "Open settings..."))
                            .clicked()
                        {
                            self.settings_open = true;
                            ui.close_menu();
                        }
                        ui.separator();
                        ui.strong(tr(self.lang, "Lingua", "Language"));
                        let lang_before = self.lang;
                        ui.radio_value(&mut self.lang, Lang::It, "Italiano");
                        ui.radio_value(&mut self.lang, Lang::En, "English");
                        ui.separator();
                        ui.strong(tr(self.lang, "Tema", "Theme"));
                        let dark_before = self.dark;
                        ui.radio_value(&mut self.dark, false, tr(self.lang, "Chiaro", "Light"));
                        ui.radio_value(&mut self.dark, true, tr(self.lang, "Scuro", "Dark"));
                        ui.separator();
                        let max_before = self.maximized;
                        ui.checkbox(
                            &mut self.maximized,
                            tr(
                                self.lang,
                                "Apri sempre massimizzato",
                                "Always open maximized",
                            ),
                        );
                        if self.dark != dark_before {
                            setup_style(ctx, self.dark);
                        }
                        if self.lang != lang_before
                            || self.dark != dark_before
                            || self.maximized != max_before
                        {
                            self.save_settings();
                        }
                    });
                    ui.menu_button("Info", |ui| {
                        if ui
                            .button(format!(
                                "{} {}",
                                tr(self.lang, "Informazioni su", "About"),
                                APP_NAME
                            ))
                            .clicked()
                        {
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
                1.0_f32,
                if dark {
                    Color32::from_gray(62)
                } else {
                    Color32::from_gray(222)
                },
            ));

        egui::TopBottomPanel::top("toolbar")
            .frame(frame)
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    // --- Appunti
                    group(ui, tr(self.lang, "Appunti", "Clipboard"), |ui| {
                        let has_sel = self.selection.is_some();
                        if ui
                            .add(
                                egui::Button::new(tr(self.lang, "Incolla", "Paste"))
                                    .min_size(vec2(84.0, 24.0)),
                            )
                            .clicked()
                        {
                            self.paste();
                        }
                        if ui
                            .add_enabled(
                                has_sel,
                                egui::Button::new(tr(self.lang, "Taglia", "Cut"))
                                    .min_size(vec2(84.0, 24.0)),
                            )
                            .clicked()
                        {
                            self.cut();
                        }
                        if ui
                            .add_enabled(
                                has_sel,
                                egui::Button::new(tr(self.lang, "Copia", "Copy"))
                                    .min_size(vec2(84.0, 24.0)),
                            )
                            .clicked()
                        {
                            self.copy();
                        }
                    });

                    // --- Immagine
                    group(ui, tr(self.lang, "Immagine", "Image"), |ui| {
                        let has_sel = self.selection.is_some();
                        if ui
                            .add_enabled(
                                has_sel,
                                egui::Button::new(tr(self.lang, "Ritaglia", "Crop"))
                                    .min_size(vec2(104.0, 24.0)),
                            )
                            .clicked()
                        {
                            self.crop_to_selection();
                        }
                        if ui
                            .add(
                                egui::Button::new(tr(self.lang, "Ridimensiona", "Resize"))
                                    .min_size(vec2(104.0, 24.0)),
                            )
                            .clicked()
                        {
                            self.open_resize_dialog();
                        }
                        ui.menu_button(tr(self.lang, "Ruota / Capovolgi", "Rotate / Flip"), |ui| {
                            self.rotate_items(ui);
                        });
                    });

                    // --- Strumenti
                    group(ui, tr(self.lang, "Strumenti", "Tools"), |ui| {
                        egui::Grid::new("tools_grid")
                            .spacing([2.0, 2.0])
                            .show(ui, |ui| {
                                for (i, (tool, it, en)) in MAIN_TOOLS.iter().enumerate() {
                                    let name = tr(self.lang, it, en);
                                    if tool_button(ui, self.tool == *tool, *tool, name, 34.0)
                                        .clicked()
                                    {
                                        self.tool = *tool;
                                    }
                                    if i % 4 == 3 {
                                        ui.end_row();
                                    }
                                }
                            });
                    });

                    // --- Forme
                    group(ui, tr(self.lang, "Forme", "Shapes"), |ui| {
                        ui.horizontal(|ui| {
                            for (tool, it, en) in SHAPE_TOOLS.iter() {
                                let name = tr(self.lang, it, en);
                                if tool_button(ui, self.tool == *tool, *tool, name, 34.0).clicked()
                                {
                                    self.tool = *tool;
                                }
                            }
                        });
                        let current = match self.shape_mode {
                            ShapeMode::Outline => tr(self.lang, "Contorno", "Outline"),
                            ShapeMode::Filled => tr(self.lang, "Pieno", "Filled"),
                            ShapeMode::Both => tr(self.lang, "Contorno + pieno", "Outline + fill"),
                        };
                        egui::ComboBox::from_id_salt("shape_mode")
                            .width(100.0)
                            .selected_text(current)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.shape_mode,
                                    ShapeMode::Outline,
                                    tr(self.lang, "Contorno", "Outline"),
                                );
                                ui.selectable_value(
                                    &mut self.shape_mode,
                                    ShapeMode::Filled,
                                    tr(self.lang, "Pieno", "Filled"),
                                );
                                ui.selectable_value(
                                    &mut self.shape_mode,
                                    ShapeMode::Both,
                                    tr(self.lang, "Contorno + pieno", "Outline + fill"),
                                );
                            });
                    });

                    // --- Dimensione
                    group(ui, tr(self.lang, "Dimensione", "Size"), |ui| {
                        ui.horizontal(|ui| {
                            ui.label(tr(self.lang, "Spessore", "Width"));
                            ui.add(egui::Slider::new(&mut self.size, 1..=64));
                        });
                        ui.horizontal(|ui| {
                            ui.label(tr(self.lang, "Testo      ", "Text       "));
                            ui.add(egui::Slider::new(&mut self.text_size, 8..=200));
                        });
                        ui.checkbox(
                            &mut self.eraser_transparent,
                            tr(self.lang, "Gomma trasparente", "Transparent eraser"),
                        );
                        ui.horizontal(|ui| {
                            ui.label(tr(self.lang, "Fluidità", "Smudge strength"));
                            ui.add(egui::Slider::new(&mut self.smudge, 0.05..=1.0).max_decimals(2));
                        });
                    });

                    // --- Colori
                    group(ui, tr(self.lang, "Colori", "Colors"), |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.color_edit_button_srgb(&mut self.primary)
                                        .on_hover_text(tr(
                                            self.lang,
                                            "Colore 1 (clic sinistro)",
                                            "Color 1 (left click)",
                                        ));
                                    ui.label(
                                        egui::RichText::new(tr(self.lang, "Colore 1", "Color 1"))
                                            .small(),
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.color_edit_button_srgb(&mut self.secondary)
                                        .on_hover_text(tr(
                                            self.lang,
                                            "Colore 2 (clic destro)",
                                            "Color 2 (right click)",
                                        ));
                                    ui.label(
                                        egui::RichText::new(tr(self.lang, "Colore 2", "Color 2"))
                                            .small(),
                                    );
                                });
                                if ui.small_button(tr(self.lang, "Scambia", "Swap")).clicked() {
                                    std::mem::swap(&mut self.primary, &mut self.secondary);
                                }
                            });
                            egui::Grid::new("palette")
                                .spacing([3.0, 3.0])
                                .show(ui, |ui| {
                                    for (i, c) in PALETTE.iter().enumerate() {
                                        let resp = ui
                                            .add(
                                                egui::Button::new("")
                                                    .fill(Color32::from_rgb(c[0], c[1], c[2]))
                                                    .rounding(4.0)
                                                    .min_size(Vec2::splat(20.0)),
                                            )
                                            .on_hover_text(tr(
                                                self.lang,
                                                "Sinistro: colore 1 - Destro: colore 2",
                                                "Left: color 1 - Right: color 2",
                                            ));
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
                    ui.label(format!(
                        "{} {} x {} px",
                        tr(self.lang, "Selezione:", "Selection:"),
                        s.w,
                        s.h
                    ));
                    ui.separator();
                }
                ui.label(format!("{} x {} px", self.width(), self.height()));
                ui.separator();
                ui.label(format!(
                    "{} {}",
                    tr(self.lang, "Livelli:", "Layers:"),
                    self.layers.len()
                ));

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
                    ui.label(tr(self.lang, "Zoom", "Zoom"));
                    ui.separator();
                    ui.label(self.status.as_str());
                });
            });
        });
    }

    // ----- interfaccia: pannello livelli ------------------------------------------------

    fn select_layer(&mut self, index: usize) {
        if index >= self.layers.len() || index == self.active {
            return;
        }
        self.commit_text();
        self.selection = None;
        self.active = index;
        self.dirty = true;
        self.status = format!(
            "{} {}",
            tr(self.lang, "Livello:", "Layer:"),
            self.layers[index].name
        );
    }

    fn layers_panel(&mut self, ctx: &egui::Context) {
        if !self.show_layers {
            return;
        }
        egui::SidePanel::right("layers_panel")
            .resizable(true)
            .default_width(232.0)
            .width_range(184.0..=360.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.strong(tr(self.lang, "Livelli", "Layers"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(tr(self.lang, "Nascondi", "Hide")).clicked() {
                            self.show_layers = false;
                        }
                    });
                });
                ui.separator();

                // Elenco dal livello più alto al più basso.
                let mut select: Option<usize> = None;
                let mut toggle: Option<usize> = None;
                let count = self.layers.len();
                egui::ScrollArea::vertical()
                    .max_height(280.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for i in (0..count).rev() {
                            let selected = i == self.active;
                            ui.horizontal(|ui| {
                                let mut visible = self.layers[i].visible;
                                if ui
                                    .checkbox(&mut visible, "")
                                    .on_hover_text(tr(
                                        self.lang,
                                        "Mostra o nascondi il livello",
                                        "Show or hide the layer",
                                    ))
                                    .changed()
                                {
                                    toggle = Some(i);
                                }
                                let name = self.layers[i].name.clone();
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 22.0],
                                        egui::SelectableLabel::new(selected, name),
                                    )
                                    .clicked()
                                {
                                    select = Some(i);
                                }
                            });
                        }
                    });

                if let Some(i) = toggle {
                    self.layers[i].visible = !self.layers[i].visible;
                    self.dirty = true;
                }
                if let Some(i) = select {
                    self.select_layer(i);
                }

                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(tr(self.lang, "Opacità", "Opacity"));
                    let mut opacity = self.layers[self.active].opacity;
                    if ui
                        .add(egui::Slider::new(&mut opacity, 0.0..=1.0).max_decimals(2))
                        .changed()
                    {
                        self.layers[self.active].opacity = opacity;
                        self.dirty = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(tr(self.lang, "Nome", "Name"));
                    let mut name = self.layers[self.active].name.clone();
                    if ui.text_edit_singleline(&mut name).changed() {
                        self.layers[self.active].name = name;
                    }
                });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button(tr(self.lang, "Nuovo", "New"))
                        .on_hover_text(tr(
                            self.lang,
                            "Nuovo livello trasparente (Ctrl+Maiusc+N)",
                            "New transparent layer (Ctrl+Shift+N)",
                        ))
                        .clicked()
                    {
                        self.add_layer();
                    }
                    if ui.button(tr(self.lang, "Duplica", "Duplicate")).clicked() {
                        self.duplicate_layer();
                    }
                    if ui.button(tr(self.lang, "Elimina", "Delete")).clicked() {
                        self.delete_layer();
                    }
                    if ui
                        .button(tr(self.lang, "Su", "Up"))
                        .on_hover_text(tr(self.lang, "Sposta su", "Move up"))
                        .clicked()
                    {
                        self.move_layer(true);
                    }
                    if ui
                        .button(tr(self.lang, "Giù", "Down"))
                        .on_hover_text(tr(self.lang, "Sposta giù", "Move down"))
                        .clicked()
                    {
                        self.move_layer(false);
                    }
                    if ui
                        .button(tr(self.lang, "Unisci sotto", "Merge down"))
                        .clicked()
                    {
                        self.merge_down();
                    }
                    if ui.button(tr(self.lang, "Appiattisci", "Flatten")).clicked() {
                        self.flatten_layers();
                    }
                });
            });
    }

    // ----- righello e margini del foglio ---------------------------------------------

    /// Righelli in pixel sopra e a sinistra del foglio.
    fn draw_rulers(&self, ui: &egui::Ui, canvas: Rect) {
        let dark = self.dark;
        let band = if dark {
            Color32::from_rgb(48, 48, 48)
        } else {
            Color32::from_rgb(249, 249, 249)
        };
        let tick = if dark {
            Color32::from_gray(160)
        } else {
            Color32::from_gray(110)
        };
        let weak = if dark {
            Color32::from_gray(92)
        } else {
            Color32::from_gray(178)
        };
        let ink = if dark {
            Color32::from_gray(214)
        } else {
            Color32::from_gray(64)
        };

        let top = Rect::from_min_max(
            pos2(canvas.min.x, canvas.min.y - RULER),
            pos2(canvas.max.x, canvas.min.y),
        );
        let left = Rect::from_min_max(
            pos2(canvas.min.x - RULER, canvas.min.y),
            pos2(canvas.min.x, canvas.max.y),
        );
        let corner =
            Rect::from_min_max(pos2(canvas.min.x - RULER, canvas.min.y - RULER), canvas.min);

        let painter = ui.painter();
        painter.rect_filled(top, 0.0, band);
        painter.rect_filled(left, 0.0, band);
        painter.rect_filled(corner, 0.0, band);
        painter.line_segment(
            [top.left_bottom(), top.right_bottom()],
            Stroke::new(1.0_f32, weak),
        );
        painter.line_segment(
            [left.right_top(), left.right_bottom()],
            Stroke::new(1.0_f32, weak),
        );

        let step = ruler_step(self.zoom);
        let minor = if (step as f32 / 5.0) * self.zoom >= 4.0 {
            (step / 5).max(1)
        } else {
            step
        };
        let font = egui::FontId::proportional(8.5);

        let pt = painter.with_clip_rect(top);
        let mut x = 0;
        while x <= self.width() {
            let px = canvas.min.x + x as f32 * self.zoom;
            if x % step == 0 {
                pt.line_segment(
                    [pos2(px, top.max.y - 7.0), pos2(px, top.max.y)],
                    Stroke::new(1.0_f32, tick),
                );
                pt.text(
                    pos2(px + 2.0, top.min.y + 1.0),
                    Align2::LEFT_TOP,
                    x.to_string(),
                    font.clone(),
                    ink,
                );
            } else {
                pt.line_segment(
                    [pos2(px, top.max.y - 4.0), pos2(px, top.max.y)],
                    Stroke::new(1.0_f32, weak),
                );
            }
            x += minor;
        }

        let pl = painter.with_clip_rect(left);
        let mut y = 0;
        while y <= self.height() {
            let py = canvas.min.y + y as f32 * self.zoom;
            if y % step == 0 {
                pl.line_segment(
                    [pos2(left.max.x - 7.0, py), pos2(left.max.x, py)],
                    Stroke::new(1.0_f32, tick),
                );
                pl.text(
                    pos2(left.min.x + 1.0, py + 1.0),
                    Align2::LEFT_TOP,
                    y.to_string(),
                    font.clone(),
                    ink,
                );
            } else {
                pl.line_segment(
                    [pos2(left.max.x - 4.0, py), pos2(left.max.x, py)],
                    Stroke::new(1.0_f32, weak),
                );
            }
            y += minor;
        }

        // Posizione del cursore evidenziata sui due righelli.
        if let Some((cx, cy)) = self.cursor_px {
            let px = canvas.min.x + cx as f32 * self.zoom;
            let py = canvas.min.y + cy as f32 * self.zoom;
            pt.line_segment(
                [pos2(px, top.min.y), pos2(px, top.max.y)],
                Stroke::new(1.0_f32, ACCENT),
            );
            pl.line_segment(
                [pos2(left.min.x, py), pos2(left.max.x, py)],
                Stroke::new(1.0_f32, ACCENT),
            );
        }
    }

    /// Margini trascinabili: cambiano le dimensioni del foglio come in Paint.
    fn canvas_resize_handles(&mut self, ui: &egui::Ui, canvas: Rect) {
        let grip = 11.0;
        let zones = [
            (
                ResizeHandle::Right,
                Rect::from_min_max(
                    pos2(canvas.max.x - grip / 2.0, canvas.min.y + 10.0),
                    pos2(canvas.max.x + grip / 2.0, canvas.max.y - grip),
                ),
                CursorIcon::ResizeHorizontal,
            ),
            (
                ResizeHandle::Bottom,
                Rect::from_min_max(
                    pos2(canvas.min.x + 10.0, canvas.max.y - grip / 2.0),
                    pos2(canvas.max.x - grip, canvas.max.y + grip / 2.0),
                ),
                CursorIcon::ResizeVertical,
            ),
            (
                ResizeHandle::Corner,
                Rect::from_center_size(canvas.max, Vec2::splat(grip)),
                CursorIcon::ResizeNwSe,
            ),
        ];

        for (handle, zone, cursor) in zones {
            let id = ui.id().with(("canvas_margin", handle as u8));
            let resp = ui.interact(zone, id, Sense::drag());
            if resp.hovered() || self.resize_drag.as_ref().map(|d| d.handle) == Some(handle) {
                ui.ctx().set_cursor_icon(cursor);
            }
            if resp.drag_started() {
                self.begin_canvas_resize(handle);
            }
            if resp.dragged() {
                if let Some(p) = ui.input(|i| i.pointer.latest_pos()) {
                    self.drag_canvas_resize(p, canvas.min);
                }
            }
            if resp.drag_stopped() {
                self.end_canvas_resize();
            }
        }
    }

    fn begin_canvas_resize(&mut self, handle: ResizeHandle) {
        self.commit_text();
        self.selection = None;
        self.push_undo();
        let original = self.undo.last().cloned().unwrap_or_else(|| self.snapshot());
        let size0 = (self.width(), self.height());
        self.resize_drag = Some(CanvasResize {
            handle,
            original,
            size0,
            last: size0,
        });
        self.status = String::from(tr(
            self.lang,
            "Trascina il margine per cambiare il foglio",
            "Drag the edge to resize the canvas",
        ));
    }

    fn drag_canvas_resize(&mut self, pointer: Pos2, origin: Pos2) {
        let Some(drag) = self.resize_drag.as_ref() else {
            return;
        };
        let zoom = self.zoom.max(0.01);
        let px = ((pointer.x - origin.x) / zoom).round() as i32;
        let py = ((pointer.y - origin.y) / zoom).round() as i32;
        let (mut w, mut h) = drag.size0;
        if matches!(drag.handle, ResizeHandle::Right | ResizeHandle::Corner) {
            w = px.clamp(1, 8192) as u32;
        }
        if matches!(drag.handle, ResizeHandle::Bottom | ResizeHandle::Corner) {
            h = py.clamp(1, 8192) as u32;
        }
        if (w, h) == drag.last {
            return;
        }
        // Si riparte sempre dal documento originale del trascinamento,
        // cosi' allargando e restringendo non si perdono pixel.
        let original = drag.original.layers.clone();
        self.layers = original;
        for (i, layer) in self.layers.iter_mut().enumerate() {
            layer.image = resize_canvas_image(&layer.image, w, h, i == 0);
        }
        if let Some(drag) = self.resize_drag.as_mut() {
            drag.last = (w, h);
        }
        self.dirty = true;
        self.status = format!("{} {w} x {h} px", tr(self.lang, "Foglio:", "Canvas:"));
    }

    fn end_canvas_resize(&mut self) {
        let Some(drag) = self.resize_drag.take() else {
            return;
        };
        if drag.last == drag.size0 {
            // Nessuna modifica: togliamo il passo inutile dalla cronologia.
            self.undo.pop();
        } else {
            self.modified = true;
            self.status = format!(
                "{} {} x {} px",
                tr(self.lang, "Foglio:", "Canvas:"),
                drag.last.0,
                drag.last.1
            );
        }
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
                    let canvas_size =
                        Vec2::new(self.width() as f32, self.height() as f32) * self.zoom;
                    let front = if self.show_ruler { RULER } else { 0.0 };
                    let total = canvas_size + vec2(front + CANVAS_TAIL, front + CANVAS_TAIL);
                    let (area, _) = ui.allocate_exact_size(total, Sense::hover());
                    let rect = Rect::from_min_size(area.min + vec2(front, front), canvas_size);
                    let response =
                        ui.interact(rect, ui.id().with("canvas"), Sense::click_and_drag());

                    self.canvas_resize_handles(ui, rect);
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
                    if self.show_ruler {
                        self.draw_rulers(ui, rect);
                    }
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

        painter.rect_stroke(
            rect.expand(0.5),
            0.0,
            Stroke::new(1.0_f32, Color32::from_black_alpha(90)),
        );

        // Maniglie sui margini del foglio (come in Paint).
        let sq = 8.0;
        for c in [
            pos2(rect.max.x, rect.center().y),
            pos2(rect.center().x, rect.max.y),
            rect.max,
        ] {
            let hr = Rect::from_center_size(c, Vec2::splat(sq));
            painter.rect_filled(hr, 1.0, Color32::WHITE);
            painter.rect_stroke(hr, 1.0, Stroke::new(1.0_f32, Color32::from_gray(70)));
        }

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
                    Stroke::new(1.5_f32, ACCENT),
                );
            }
        }

        if response.hovered() {
            let radius = match self.tool {
                Tool::Brush | Tool::Eraser | Tool::Smudge => self.size as f32 / 2.0,
                Tool::Spray => self.size as f32,
                _ => return,
            };
            if let Some(pos) = ui.input(|i| i.pointer.latest_pos()) {
                let r = (radius * self.zoom).max(2.0);
                painter.circle_stroke(pos, r + 1.0, Stroke::new(1.0_f32, Color32::WHITE));
                painter.circle_stroke(pos, r, Stroke::new(1.0_f32, Color32::BLACK));
            }
        }
    }

    // ----- finestre di dialogo ---------------------------------------------------------

    fn text_dialog(&mut self, ctx: &egui::Context) {
        let mut action: Option<bool> = None;
        let mut changed = false;

        if let Some(te) = &mut self.text_edit {
            egui::Window::new(tr(self.lang, "Testo", "Text"))
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::RIGHT_BOTTOM, vec2(-16.0, -40.0))
                .show(ctx, |ui| {
                    let resp = ui.add(
                        egui::TextEdit::multiline(&mut te.content)
                            .desired_rows(3)
                            .desired_width(260.0)
                            .hint_text(tr(self.lang, "Scrivi qui...", "Type here...")),
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
                        ui.label(tr(self.lang, "Dimensione", "Size"));
                        ui.add(egui::Slider::new(&mut self.text_size, 8..=200));
                    });
                    if self.text_size != before {
                        changed = true;
                    }
                    ui.label(
                        egui::RichText::new(tr(
                            self.lang,
                            "Clic sulla tela per spostare il testo",
                            "Click on the canvas to move the text",
                        ))
                        .small()
                        .weak(),
                    );
                    ui.horizontal(|ui| {
                        if ui.button(tr(self.lang, "Applica", "Apply")).clicked() {
                            action = Some(true);
                        }
                        if ui.button(tr(self.lang, "Annulla", "Cancel")).clicked() {
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
            egui::Window::new(tr(self.lang, "Nuova immagine", "New image"))
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(tr(self.lang, "Larghezza:", "Width:"));
                        ui.add(egui::DragValue::new(w).range(1..=8192).suffix(" px"));
                    });
                    ui.horizontal(|ui| {
                        ui.label(tr(self.lang, "Altezza:    ", "Height:    "));
                        ui.add(egui::DragValue::new(h).range(1..=8192).suffix(" px"));
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button(tr(self.lang, "Crea", "Create")).clicked() {
                            action = Some(true);
                        }
                        if ui.button(tr(self.lang, "Annulla", "Cancel")).clicked() {
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
            egui::Window::new(tr(self.lang, "Ridimensiona immagine", "Resize image"))
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.checkbox(
                        &mut d.keep_ratio,
                        tr(self.lang, "Mantieni le proporzioni", "Keep proportions"),
                    );
                    let (w0, h0) = (d.w, d.h);
                    ui.horizontal(|ui| {
                        ui.label(tr(self.lang, "Larghezza:", "Width:"));
                        ui.add(egui::DragValue::new(&mut d.w).range(1..=8192).suffix(" px"));
                    });
                    ui.horizontal(|ui| {
                        ui.label(tr(self.lang, "Altezza:    ", "Height:    "));
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
                        if ui.button(tr(self.lang, "Applica", "Apply")).clicked() {
                            action = Some(true);
                        }
                        if ui.button(tr(self.lang, "Annulla", "Cancel")).clicked() {
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

    /// Salva lingua, tema e avvio massimizzato nelle impostazioni.
    fn save_settings(&self) {
        Settings {
            lang: self.lang,
            dark: self.dark,
            maximized: self.maximized,
        }
        .save();
    }

    /// Finestra Impostazioni: lingua, tema e avvio massimizzato.
    fn settings_dialog(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let mut open = true;
        egui::Window::new(tr(self.lang, "Impostazioni", "Settings"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(360.0);
                ui.add_space(4.0);
                let lang_before = self.lang;
                let dark_before = self.dark;
                let max_before = self.maximized;

                ui.strong(tr(self.lang, "Lingua", "Language"));
                ui.radio_value(&mut self.lang, Lang::It, "Italiano");
                ui.radio_value(&mut self.lang, Lang::En, "English");

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                ui.strong(tr(self.lang, "Tema", "Theme"));
                ui.radio_value(&mut self.dark, false, tr(self.lang, "Chiaro", "Light"));
                ui.radio_value(&mut self.dark, true, tr(self.lang, "Scuro", "Dark"));

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                ui.checkbox(
                    &mut self.maximized,
                    tr(
                        self.lang,
                        "Apri sempre massimizzato",
                        "Always open maximized",
                    ),
                );
                ui.label(
                    egui::RichText::new(tr(
                        self.lang,
                        "L'avvio massimizzato vale dal prossimo avvio.",
                        "Maximized start applies from the next launch.",
                    ))
                    .weak()
                    .small(),
                );

                if self.dark != dark_before {
                    setup_style(ctx, self.dark);
                }
                if self.lang != lang_before
                    || self.dark != dark_before
                    || self.maximized != max_before
                {
                    self.save_settings();
                }
            });
        if !open {
            self.settings_open = false;
        }
    }

    fn about_dialog(&mut self, ctx: &egui::Context) {
        if !self.about_open {
            return;
        }
        let mut open = true;
        let mut close_clicked = false;
        egui::Window::new(format!(
            "{} {}",
            tr(self.lang, "Informazioni su", "About"),
            APP_NAME
        ))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_max_width(400.0);
            ui.add_space(4.0);
            ui.heading(APP_NAME);
            ui.add_space(6.0);
            egui::Grid::new("about_grid")
                .num_columns(2)
                .spacing([14.0, 6.0])
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(tr(self.lang, "Nome del programma", "Program name"))
                            .weak(),
                    );
                    ui.label(APP_NAME);
                    ui.end_row();
                    ui.label(egui::RichText::new(tr(self.lang, "Versione", "Version")).weak());
                    ui.label(APP_VERSION);
                    ui.end_row();
                });
            ui.add_space(8.0);
            ui.label(tr(self.lang, APP_DESCRIPTION, APP_DESCRIPTION_EN));
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);
            ui.label(tr(
                self.lang,
                "Se vuoi contribuire al mio lavoro, visto il tempo che ti farà risparmiare quando stampi tanti file, qui sotto trovi il pulsante per contribuire anche con una piccola donazione simbolica: una app così utile, gratis per sempre, non la trovi ovunque. Apprezzo il gesto ;-)",
                "If you want to support my work, given the time it will save you when printing lots of files, below you'll find a button to contribute even with a small symbolic donation: an app this useful, free forever, isn't easy to find. I appreciate it ;-)",
            ));
            ui.add_space(8.0);
            let donate = egui::Button::new(
                egui::RichText::new(tr(self.lang, "Dona via PayPal", "Donate via PayPal"))
                    .color(Color32::WHITE)
                    .strong(),
            )
            .fill(Color32::from_rgb(0, 112, 186));
            if ui
                .add(donate)
                .on_hover_text("https://paypal.me/PeppeCP")
                .clicked()
            {
                ctx.open_url(egui::OpenUrl::new_tab("https://paypal.me/PeppeCP"));
            }
            ui.add_space(10.0);
            if ui.button(tr(self.lang, "Chiudi", "Close")).clicked() {
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
            .unwrap_or(tr(self.lang, "Senza titolo", "Untitled"));
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
        self.settings_dialog(ctx);
        self.about_dialog(ctx);
        self.render_text();
        self.layers_panel(ctx);
        self.canvas_panel(ctx);
        self.update_title(ctx);
    }
}

fn main() -> eframe::Result<()> {
    let settings = Settings::load();
    let mut viewport = if settings.maximized {
        egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([1040.0, 520.0])
            .with_maximized(true)
            .with_position(egui::pos2(-10000.0, -10000.0))
            .with_title(format!("Senza titolo - {}", APP_NAME))
    } else {
        egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([1040.0, 520.0])
            .with_title(format!("Senza titolo - {}", APP_NAME))
    };
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(|cc| Ok(Box::new(PaintApp::new(&cc.egui_ctx)))),
    )
}
