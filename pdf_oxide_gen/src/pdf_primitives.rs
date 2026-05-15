use anyhow::Result;
use pdf_oxide::writer::{PdfWriter, PdfWriterConfig};

#[derive(Clone, Copy, Debug, Default)]
pub struct Color {
    r: f32,
    g: f32,
    b: f32,
}
impl Color {
    pub fn hex(s: &str) -> Self {
        let hex = s.trim_start_matches('#');
        if hex.len() != 6 {
            return Self::default();
        }
        let parse = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0) as f32 / 255.0;
        Self {
            r: parse(0),
            g: parse(2),
            b: parse(4),
        }
    }
}

#[derive(Clone, Copy)]
pub enum Font {
    Helvetica,
    HelveticaBold,
    Courier,
    CourierBold,
}
impl Font {
    pub fn name(self) -> &'static str {
        match self {
            Self::Helvetica => "Helvetica",
            Self::HelveticaBold => "Helvetica-Bold",
            Self::Courier => "Courier",
            Self::CourierBold => "Courier-Bold",
        }
    }
}

#[derive(Clone, Copy)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy)]
pub struct Rect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}
impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
}

#[derive(Clone, Copy)]
pub struct Line {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
}
impl Line {
    pub fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Self { x1, y1, x2, y2 }
    }
}

#[derive(Clone, Copy)]
pub struct Circle {
    cx: f32,
    cy: f32,
    radius: f32,
}
impl Circle {
    pub fn new(cx: f32, cy: f32, radius: f32) -> Self {
        Self { cx, cy, radius }
    }
}

pub enum Op {
    FillRect(Rect, Color),
    Text {
        x: f32,
        y: f32,
        text: String,
        font: Font,
        size: f32,
        color: Color,
    },
    Line(Line, Color, f32),
}

pub struct ContentBuilder {
    pub ops: Vec<Op>,
    fill_color: Color,
    stroke_color: Color,
    line_width: f32,
    font: Font,
    font_size: f32,
}
impl ContentBuilder {
    pub fn new() -> Self {
        Self {
            ops: Vec::new(),
            fill_color: Color::default(),
            stroke_color: Color::default(),
            line_width: 1.0,
            font: Font::Helvetica,
            font_size: 12.0,
        }
    }
    pub fn set_fill_color(&mut self, color: Color) {
        self.fill_color = color;
    }
    pub fn fill_rect(&mut self, rect: Rect) {
        self.ops.push(Op::FillRect(rect, self.fill_color));
    }
    pub fn set_font(&mut self, font: Font, size: f32) {
        self.font = font;
        self.font_size = size;
    }
    pub fn draw_string(&mut self, x: f32, y: f32, text: &str) {
        self.ops.push(Op::Text {
            x,
            y,
            text: text.to_string(),
            font: self.font,
            size: self.font_size,
            color: self.fill_color,
        });
    }
    pub fn draw_text_centered(&mut self, x: f32, y: f32, text: &str) {
        let approx_w = text.len() as f32 * self.font_size * 0.5;
        self.draw_string(x - approx_w / 2.0, y, text);
    }
    pub fn draw_text_right(&mut self, x: f32, y: f32, text: &str) {
        let approx_w = text.len() as f32 * self.font_size * 0.5;
        self.draw_string(x - approx_w, y, text);
    }
    pub fn set_stroke_color(&mut self, color: Color) {
        self.stroke_color = color;
    }
    pub fn set_line_width(&mut self, width: f32) {
        self.line_width = width;
    }
    pub fn draw_line(&mut self, line: Line) {
        self.ops.push(Op::Line(line, self.stroke_color, self.line_width));
    }
    pub fn fill_polygon(&mut self, _pts: &[(f32, f32)]) {}
    pub fn fill_circle(&mut self, c: Circle) {
        // Simple approximation: draw a filled square at circle bounds.
        self.fill_rect(Rect::new(
            c.cx - c.radius,
            c.cy - c.radius,
            c.radius * 2.0,
            c.radius * 2.0,
        ));
    }
    pub fn add_internal_link(&mut self, _dest: &str, _rect: Rect) {}
}

pub struct Page {
    pub width: f32,
    pub height: f32,
    pub named_destination: Option<String>,
    pub content: ContentBuilder,
}
impl Page {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            named_destination: None,
            content: ContentBuilder::new(),
        }
    }
    pub fn set_named_destination(&mut self, dest: &str) {
        self.named_destination = Some(dest.to_string());
    }
    pub fn content_builder(&self) -> ContentBuilder {
        ContentBuilder::new()
    }
    pub fn set_content(&mut self, b: ContentBuilder) {
        self.content = b;
    }
}

pub struct Document {
    pages: Vec<Page>,
    compressed: bool,
}
impl Document {
    pub fn new() -> Self {
        Self {
            pages: Vec::new(),
            compressed: false,
        }
    }
    pub fn set_compression(&mut self, compressed: bool) {
        self.compressed = compressed;
    }
    pub fn add_page(&mut self, page: Page) {
        self.pages.push(page);
    }
    pub fn save_to_bytes(self) -> Result<Vec<u8>> {
        let mut writer = if self.compressed {
            PdfWriter::with_config(PdfWriterConfig::default().with_compress(true))
        } else {
            PdfWriter::new()
        };
        for p in self.pages {
            let mut page = writer.add_page(p.width, p.height);
            for op in p.content.ops {
                match op {
                    Op::FillRect(r, c) => {
                        page.fill_rect_colored(r.x, r.y, r.width, r.height, c.r, c.g, c.b);
                    }
                    Op::Text {
                        x,
                        y,
                        text,
                        font,
                        size,
                        color,
                    } => {
                        page.set_fill_color(color.r, color.g, color.b);
                        page.add_text(&text, x, y, font.name(), size);
                    }
                    Op::Line(l, c, lw) => {
                        let dx = (l.x2 - l.x1).abs();
                        let dy = (l.y2 - l.y1).abs();
                        if dy < 0.01 {
                            page.draw_hline_colored(l.x1, l.y1, dx.max(0.1), lw, c.r, c.g, c.b);
                        } else {
                            // Fallback approximation for non-horizontal line.
                            page.fill_rect_colored(
                                l.x1.min(l.x2),
                                l.y1.min(l.y2),
                                dx.max(1.0),
                                dy.max(lw),
                                c.r,
                                c.g,
                                c.b,
                            );
                        }
                    }
                }
            }
            page.finish();
        }
        Ok(writer.finish()?)
    }
}

// â”€â”€ Page geometry (points, A4) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
pub const PAGE_W: f32 = 595.28;
pub const PAGE_H: f32 = 841.89;
pub const HEADER_H: f32 = 90.0;
pub const FOOTER_H: f32 = 44.0;
pub const SIDEBAR_W: f32 = 92.0;
pub const CX: f32 = 107.0;
pub const CONTENT_TOP: f32 = HEADER_H + 8.0;
pub const CONTENT_BOTTOM_MARGIN: f32 = FOOTER_H + 10.0;

// â”€â”€ Brand palette â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
pub fn c_navy()    -> Color { Color::hex("#1E3A5F") }
pub fn c_dnavy()   -> Color { Color::hex("#152B47") }
pub fn c_blue()    -> Color { Color::hex("#0052CC") }
pub fn c_teal()    -> Color { Color::hex("#00B4A0") }
pub fn c_teal2()   -> Color { Color::hex("#008C7A") }
pub fn c_gold()    -> Color { Color::hex("#F0A500") }
pub fn c_ltblue()  -> Color { Color::hex("#EAF2FF") }
pub fn c_sidebar() -> Color { Color::hex("#EEF3FA") }
pub fn c_lblue()   -> Color { Color::hex("#7FB3D3") }
pub fn c_dgray()   -> Color { Color::hex("#2C3E50") }
pub fn c_mgray()   -> Color { Color::hex("#5A6A7A") }
pub fn c_cdcef()   -> Color { Color::hex("#C5DCEF") }
pub fn c_white()   -> Color { Color::hex("#FFFFFF") }
pub fn c_3a8fd4()  -> Color { Color::hex("#3A8FD4") }
pub fn c_2e6db4()  -> Color { Color::hex("#2E6DB4") }
pub fn c_5ab0e8()  -> Color { Color::hex("#5AB0E8") }
pub fn c_7dc8f7()  -> Color { Color::hex("#7DC8F7") }
pub fn c_fb()      -> Color { Color::hex("#1877F2") }
pub fn c_li()      -> Color { Color::hex("#0A66C2") }
pub fn c_tw()      -> Color { Color::hex("#1DA1F2") }
pub fn c_yt()      -> Color { Color::hex("#FF0000") }
pub fn c_wa()      -> Color { Color::hex("#25D366") }
pub fn c_1a3355()  -> Color { Color::hex("#1A3355") }
pub fn c_19304f()  -> Color { Color::hex("#19304F") }
pub fn c_172e4c()  -> Color { Color::hex("#172E4C") }
