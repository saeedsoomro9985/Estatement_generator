use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result};
use pdf_oxide::editor::DocumentEditor;
use pdf_oxide::elements::{ContentElement, ImageContent};
use pdf_oxide::geometry::Rect as GRect;
use pdf_oxide::rendering::{render_page, RenderOptions};
use pdf_oxide::PdfDocument;
pub use pdf_oxide::writer::{LinkAnnotation, PdfWriter};
use pdf_oxide::writer::{PageBuilder, PdfWriterConfig};

/// MBL report assets
pub const REPORT_DIR: &str = r"C:\MBL services\Report";

pub const TPL_SUMMARY: &str = "Meezan_Bank_Summary";
pub const TPL_ACCOUNT: &str = "Account";
pub const TPL_TDR: &str = "Accounts_TermDeposit";

pub const IMG_MESSAGE_FOR_YOU: &str = "MessageForYou";

/// Rasterize vector PDF templates at this DPI (only 3 templates per run, not per page).
const TEMPLATE_RENDER_DPI: u32 = 96;
const TEMPLATE_JPEG_QUALITY: u8 = 82;
const TEMPLATE_CACHE_MAGIC: &[u8; 4] = b"PTC1";

/// MessageForYou placement
pub const MSG_IMG_X: f32 = 125.0;
pub const MSG_IMG_TOP: f32 = 420.0;
pub const MSG_IMG_W: f32 = 16.0 * 72.0 / 2.54;
pub const MSG_IMG_H: f32 = 10.0 * 72.0 / 2.54;

/// Sidebar navigation targets (0-based page indices in the final PDF).
#[derive(Clone, Copy, Debug)]
pub struct NavTargets {
    pub summary: usize,
    pub accounts: usize,
    pub term_deposits: usize,
}

/// Clickable sidebar regions (top-down y, converted to PDF bottom-left inside helper).
const NAV_LINK_Y_SUMMARY: f32 = 140.0;
const NAV_LINK_Y_ACCOUNTS: f32 = 170.0;
const NAV_LINK_Y_TDR: f32 = 200.0;
const NAV_LINK_W: f32 = 120.0;
const NAV_LINK_H: f32 = 18.0;

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

        let parse =
            |i: usize| u8::from_str_radix(&hex[i..i + 2], 16)
                .unwrap_or(0) as f32
                / 255.0;

        Self {
            r: parse(0),
            g: parse(2),
            b: parse(4),
        }
    }
}

#[derive(Clone, Copy)]
pub enum Font {
    Regular,
    Bold,
    Mono,
    MonoBold,
}

impl Font {
    pub fn name(self) -> &'static str {
        match self {
            Font::Regular => "Times-Roman",
            Font::Bold => "Times-Bold",
            Font::Mono => "Courier",
            Font::MonoBold => "Courier-Bold",
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
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
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
    pub fn new(
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    ) -> Self {
        Self { x1, y1, x2, y2 }
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
            font: Font::Regular,
            font_size: 12.0,
        }
    }

    pub fn set_fill_color(
        &mut self,
        color: Color,
    ) {
        self.fill_color = color;
    }

    pub fn fill_rect(
        &mut self,
        rect: Rect,
    ) {
        self.ops.push(Op::FillRect(rect, self.fill_color));
    }

    pub fn set_font(
        &mut self,
        font: Font,
        size: f32,
    ) {
        self.font = font;
        self.font_size = size;
    }

    pub fn draw_string(
        &mut self,
        x: f32,
        y: f32,
        text: &str,
    ) {
        self.ops.push(Op::Text {
            x,
            y,
            text: text.to_string(),
            font: self.font,
            size: self.font_size,
            color: self.fill_color,
        });
    }

    pub fn draw_text_centered(
        &mut self,
        x: f32,
        y: f32,
        text: &str,
    ) {
        let approx_w =
            text.len() as f32 * self.font_size * 0.5;

        self.draw_string(
            x - approx_w / 2.0,
            y,
            text,
        );
    }

    pub fn draw_text_right(
        &mut self,
        x: f32,
        y: f32,
        text: &str,
    ) {
        let approx_w =
            text.len() as f32 * self.font_size * 0.5;

        self.draw_string(
            x - approx_w,
            y,
            text,
        );
    }

    pub fn set_stroke_color(
        &mut self,
        color: Color,
    ) {
        self.stroke_color = color;
    }

    pub fn set_line_width(
        &mut self,
        width: f32,
    ) {
        self.line_width = width;
    }

    pub fn draw_line(
        &mut self,
        line: Line,
    ) {
        self.ops.push(Op::Line(
            line,
            self.stroke_color,
            self.line_width,
        ));
    }

    pub fn add_internal_link(
        &mut self,
        _dest: &str,
        _rect: Rect,
    ) {
    }
}

pub struct Page {
    pub width: f32,
    pub height: f32,
    pub named_destination: Option<String>,
    pub content: ContentBuilder,
}

impl Page {
    pub fn new(
        width: f32,
        height: f32,
    ) -> Self {
        Self {
            width,
            height,
            named_destination: None,
            content: ContentBuilder::new(),
        }
    }

    pub fn set_named_destination(
        &mut self,
        dest: &str,
    ) {
        self.named_destination =
            Some(dest.to_string());
    }

    pub fn content_builder(&self) -> ContentBuilder {
        ContentBuilder::new()
    }

    pub fn set_content(
        &mut self,
        b: ContentBuilder,
    ) {
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

    pub fn set_compression(
        &mut self,
        compressed: bool,
    ) {
        self.compressed = compressed;
    }

    pub fn add_page(
        &mut self,
        page: Page,
    ) {
        self.pages.push(page);
    }

    pub fn save_to_bytes(self) -> Result<Vec<u8>> {
        let mut writer = if self.compressed {
            PdfWriter::with_config(
                PdfWriterConfig::default()
                    .with_compress(true),
            )
        } else {
            PdfWriter::new()
        };

        for p in self.pages {
            let mut page =
                writer.add_page(p.width, p.height);

            for op in p.content.ops {
                match op {
                    Op::FillRect(r, c) => {
                        page.fill_rect_colored(
                            r.x,
                            r.y,
                            r.width,
                            r.height,
                            c.r,
                            c.g,
                            c.b,
                        );
                    }

                    Op::Text {
                        x,
                        y,
                        text,
                        font,
                        size,
                        color,
                    } => {
                        page.set_fill_color(
                            color.r,
                            color.g,
                            color.b,
                        );

                        page.add_text(
                            &text,
                            x,
                            y,
                            font.name(),
                            size,
                        );
                    }

                    Op::Line(l, c, lw) => {
                        let dx =
                            (l.x2 - l.x1).abs();

                        let dy =
                            (l.y2 - l.y1).abs();

                        if dy < 0.01 {
                            page.draw_hline_colored(
                                l.x1,
                                l.y1,
                                dx.max(0.1),
                                lw,
                                c.r,
                                c.g,
                                c.b,
                            );
                        } else if dx < 0.01 {
                            page.fill_rect_colored(
                                l.x1,
                                l.y1.min(l.y2),
                                lw.max(0.1),
                                dy.max(0.1),
                                c.r,
                                c.g,
                                c.b,
                            );
                        } else {
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

/// A4 Geometry
pub const PAGE_W: f32 = 595.28;
pub const PAGE_H: f32 = 841.89;

pub const CX: f32 = 125.0;
pub const SIDEBAR_X: f32 = 10.0;

pub const TABLE_W: f32 = 450.0;

pub const CONTENT_BOTTOM: f32 = 780.0;

pub const TX_TABLE_TOP: f32 = 280.0;

pub const ACC_SUMMARY_TOP: f32 = 270.0;
pub const TDR_SUMMARY_TOP: f32 = 270.0;

pub const TDR_PAGE_BOTTOM: f32 = 780.0;
pub const TDR_TABLE_TOP: f32 = 190.0;

pub const TDR_SECTION_BANNER_H: f32 = 40.0;

pub const TOTAL_BOX_H: f32 = 36.0; // 2 rows × 18
pub const GRAND_BOX_H: f32 = 36.0;
pub const GRAND_BOX_MARGIN_BOTTOM: f32 = 20.0;

pub const ACCOUNT_BOX_MARGIN: f32 = 20.0;
pub const ACCOUNT_BOX_H: f32 = 64.0; // 4 rows × 16


pub fn full_table_width() -> f32 {
    PAGE_W - 20.0 // full usable page width
}

pub fn table_x() -> f32 {
    10.0 // small margin instead of CX (125)
}

/// Colors

pub fn c_purple() -> Color {
    Color::hex("#440055")
}

pub fn c_gold_text() -> Color {
    Color::hex("#9A8C27")
}

pub fn c_teal() -> Color {
    Color::hex("#33BAAC")
}

pub fn c_black() -> Color {
    Color::hex("#000000")
}

pub fn c_white() -> Color {
    Color::hex("#FFFFFF")
}

pub fn c_dark_gray() -> Color {
    Color::hex("#404040")
}

pub fn c_gray_bar() -> Color {
    Color::hex("#bdbdbd")
}

pub fn c_mbl_green() -> Color {
    Color::hex("#0E836A")
}

pub fn c_mbl_mint() -> Color {
    Color::hex("#D5DBDB")
}


/// Resolve report assets

pub fn resolve_report_asset(
    name: &str,
) -> PathBuf {
    let dir = Path::new(REPORT_DIR);

    let candidates = [
        dir.join(name),
        dir.join(format!("{name}.pdf")),
        dir.join(format!("{name}.png")),
        dir.join(format!("{name}.jpg")),
        dir.join(format!("{name}.jpeg")),
    ];

    for p in &candidates {
        if p.exists() {
            return p.clone();
        }
    }

    dir.join(name)
}

pub fn load_message_for_you_image(
) -> Result<Vec<u8>> {
    let path =
        resolve_report_asset(IMG_MESSAGE_FOR_YOU);

    std::fs::read(&path).with_context(|| {
        format!(
            "MessageForYou image not found at {}",
            path.display()
        )
    })
}

#[derive(Clone)]
struct CachedTemplate {
    page_w: f32,
    page_h: f32,
    pixels: Arc<Vec<u8>>,
}

fn template_cache() -> &'static Mutex<HashMap<String, CachedTemplate>> {
    static CACHE: OnceLock<Mutex<HashMap<String, CachedTemplate>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Prefer static images over PDF so we skip rasterization when PNG/JPG templates exist.
fn resolve_template_path(name: &str) -> PathBuf {
    let dir = Path::new(REPORT_DIR);
    let candidates = [
        dir.join(format!("{name}.png")),
        dir.join(format!("{name}.jpg")),
        dir.join(format!("{name}.jpeg")),
        dir.join(format!("{name}.pdf")),
        dir.join(name),
    ];
    for p in &candidates {
        if p.exists() {
            return p.clone();
        }
    }
    dir.join(format!("{name}.pdf"))
}

fn template_disk_cache_dir() -> PathBuf {
    std::env::temp_dir().join("pdf_demo_tpl_cache")
}

fn template_disk_cache_path(source: &Path, dpi: u32) -> PathBuf {
    let meta = std::fs::metadata(source).ok();
    let mtime = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let len = meta.map(|m| m.len()).unwrap_or(0);
    let stem = source
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "template".to_string());
    template_disk_cache_dir().join(format!("{stem}_{mtime}_{len}_{dpi}.ptc1"))
}

fn read_template_disk_cache(path: &Path) -> Result<Option<(f32, f32, Vec<u8>)>> {
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Ok(None),
    };
    let mut magic = [0u8; 4];
    if file.read_exact(&mut magic).is_err() || magic != *TEMPLATE_CACHE_MAGIC {
        return Ok(None);
    }
    let mut wb = [0u8; 4];
    let mut hb = [0u8; 4];
    let mut lb = [0u8; 4];
    if file.read_exact(&mut wb).is_err()
        || file.read_exact(&mut hb).is_err()
        || file.read_exact(&mut lb).is_err()
    {
        return Ok(None);
    }
    let page_w = f32::from_le_bytes(wb);
    let page_h = f32::from_le_bytes(hb);
    let len = u32::from_le_bytes(lb) as usize;
    let mut pixels = vec![0u8; len];
    if file.read_exact(&mut pixels).is_err() {
        return Ok(None);
    }
    Ok(Some((page_w, page_h, pixels)))
}

fn write_template_disk_cache(path: &Path, page_w: f32, page_h: f32, pixels: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::File::create(path)?;
    file.write_all(TEMPLATE_CACHE_MAGIC)?;
    file.write_all(&page_w.to_le_bytes())?;
    file.write_all(&page_h.to_le_bytes())?;
    file.write_all(&(pixels.len() as u32).to_le_bytes())?;
    file.write_all(pixels)?;
    Ok(())
}

/// Load a report template once per name; reused for every page of that type (pdf_oxide dedupes image XObjects).
fn get_cached_template(name: &str) -> Result<CachedTemplate> {
    let mut cache = template_cache()
        .lock()
        .map_err(|e| anyhow::anyhow!("Template cache lock: {e}"))?;

    if let Some(entry) = cache.get(name) {
        return Ok(entry.clone());
    }

    let path = resolve_template_path(name);
    let (page_w, page_h, pixels) = load_template_asset(&path)
        .with_context(|| format!("Template not found: {}", path.display()))?;

    let entry = CachedTemplate {
        page_w,
        page_h,
        pixels: Arc::new(pixels),
    };
    cache.insert(name.to_string(), entry.clone());
    Ok(entry)
}

/// Rasterize/load all three templates in parallel before building pages (avoids serial cold-start).
pub fn preload_templates() -> Result<()> {
    const NAMES: [&str; 3] = [TPL_SUMMARY, TPL_ACCOUNT, TPL_TDR];
    let handles: Vec<_> = NAMES
        .iter()
        .map(|&name| {
            let name = name.to_string();
            std::thread::spawn(move || get_cached_template(&name))
        })
        .collect();
    for handle in handles {
        handle
            .join()
            .map_err(|_| anyhow::anyhow!("Template preload thread panicked"))??;
    }
    Ok(())
}

/// Clear cached templates (e.g. if report files change between runs in a long-lived process).
pub fn clear_template_cache() {
    if let Ok(mut cache) = template_cache().lock() {
        cache.clear();
    }
}

fn load_template_asset(path: &Path) -> Result<(f32, f32, Vec<u8>)> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if ext == "pdf" {
        let cache_path = template_disk_cache_path(path, TEMPLATE_RENDER_DPI);
        if let Some(cached) = read_template_disk_cache(&cache_path)? {
            return Ok(cached);
        }

        let mut doc = PdfDocument::open(path)
            .with_context(|| format!("Failed to open template PDF: {}", path.display()))?;
        let (x0, y0, x1, y1) = doc.get_page_media_box(0)?;
        let w = x1 - x0;
        let h = y1 - y0;
        let opts = RenderOptions::with_dpi(TEMPLATE_RENDER_DPI).as_jpeg(TEMPLATE_JPEG_QUALITY);
        let  rendered = render_page(&mut doc, 0, &opts)
            .with_context(|| format!("Failed to render template: {}", path.display()))?;
        let _ = write_template_disk_cache(&cache_path, w, h, &rendered.data);
        return Ok((w, h, rendered.data));
    }

    if matches!(ext.as_str(), "png" | "jpg" | "jpeg") {
        let bytes = std::fs::read(path)
            .with_context(|| format!("Failed to read template image: {}", path.display()))?;
        let (page_w, page_h) =
            template_page_size_from_image(&bytes).unwrap_or((PAGE_W, PAGE_H));
        return Ok((page_w, page_h, bytes));
    }

    anyhow::bail!(
        "Unsupported template at {} (expected .pdf, .png, or .jpg)",
        path.display()
    )
}

fn template_page_size_from_image(bytes: &[u8]) -> Option<(f32, f32)> {
    use pdf_oxide::writer::ImageData;
    let parsed = ImageData::from_bytes(bytes).ok()?;
    if parsed.width == 0 || parsed.height == 0 {
        return None;
    }
    let w_in = parsed.width as f32 / TEMPLATE_RENDER_DPI as f32;
    let h_in = parsed.height as f32 / TEMPLATE_RENDER_DPI as f32;
    Some((w_in * 72.0, h_in * 72.0))
}

fn nav_link_rect(y_top: f32) -> GRect {
    let y = PAGE_H - y_top - NAV_LINK_H + 8.0;
    GRect::new(SIDEBAR_X, y, NAV_LINK_W, NAV_LINK_H)
}

fn apply_builder_to_page(page: &mut PageBuilder<'_>, builder: &ContentBuilder) {
    for op in &builder.ops {
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
                page.add_text(&text, *x, *y, font.name(), *size);
            }
            Op::Line(l, c, lw) => {
                let dx = (l.x2 - l.x1).abs();
                let dy = (l.y2 - l.y1).abs();
                if dy < 0.01 {
                    page.draw_hline_colored(l.x1, l.y1, dx.max(0.1), *lw, c.r, c.g, c.b);
                } else if dx < 0.01 {
                    page.fill_rect_colored(
                        l.x1,
                        l.y1.min(l.y2),
                        lw.max(0.1),
                        dy.max(0.1),
                        c.r,
                        c.g,
                        c.b,
                    );
                } else {
                    page.fill_rect_colored(
                        l.x1.min(l.x2),
                        l.y1.min(l.y2),
                        dx.max(1.0),
                        dy.max(*lw),
                        c.r,
                        c.g,
                        c.b,
                    );
                }
            }
        }
    }
}

/// Sidebar link annotations on a page being built.
pub fn add_sidebar_links_to_page(page: &mut PageBuilder<'_>, nav: NavTargets) {
    page.internal_link(nav_link_rect(NAV_LINK_Y_SUMMARY), nav.summary);
    page.internal_link(nav_link_rect(NAV_LINK_Y_ACCOUNTS), nav.accounts);
    page.internal_link(nav_link_rect(NAV_LINK_Y_TDR), nav.term_deposits);
}

/// Append one page: cached template background + dynamic overlay + optional image + nav links.
///
/// Uses one shared `PdfWriter` so fonts are registered once. Each of the three templates is
/// loaded/rasterized at most once per process; identical background bytes share one image XObject.
pub fn write_content_page(
    writer: &mut PdfWriter,
    template_name: &str,
    builder: &ContentBuilder,
    image: Option<(Vec<u8>, f32, f32, f32, f32)>,
    nav: NavTargets,
) -> Result<()> {
    let tpl = get_cached_template(template_name)?;
    let mut page = writer.add_page(tpl.page_w, tpl.page_h);

    let bg = ImageContent::from_bytes(
        GRect::new(0.0, 0.0, tpl.page_w, tpl.page_h),
        Arc::clone(&tpl.pixels).as_ref().clone(),
    )
    .map_err(|e| {
        anyhow::anyhow!(
            "Template background {}: {e}",
            resolve_template_path(template_name).display()
        )
    })?;
    page.add_element(&ContentElement::Image(bg));

    apply_builder_to_page(&mut page, builder);

    if let Some((bytes, x, y, w, h)) = image {
        let img = ImageContent::from_bytes(GRect::new(x, y, w, h), bytes)
            .map_err(|e| anyhow::anyhow!("MessageForYou image: {e}"))?;
        page.add_element(&ContentElement::Image(img));
    }

    add_sidebar_links_to_page(&mut page, nav);
    page.finish();
    Ok(())
}

/// Finish one multi-page PDF with compressed streams.
pub fn finish_pdf(writer: PdfWriter) -> Result<Vec<u8>> {
    writer
        .finish()
        .map_err(|e| anyhow::anyhow!("Failed to finish PDF: {e}"))
}

/// New `PdfWriter` with Flate compression enabled (smaller files).
pub fn new_pdf_writer() -> PdfWriter {
    PdfWriter::with_config(PdfWriterConfig::default().with_compress(true))
}

/// Add sidebar link annotations to every page of a merged document.
pub fn add_sidebar_links_to_document(
    editor: &mut DocumentEditor,
    nav: NavTargets,
) -> Result<()> {
    let page_count = editor.current_page_count();
    for page_index in 0..page_count {
        editor.edit_page(page_index, |page| {
            page.add_annotation(LinkAnnotation::goto_page(
                nav_link_rect(NAV_LINK_Y_SUMMARY),
                nav.summary,
            ));
            page.add_annotation(LinkAnnotation::goto_page(
                nav_link_rect(NAV_LINK_Y_ACCOUNTS),
                nav.accounts,
            ));
            page.add_annotation(LinkAnnotation::goto_page(
                nav_link_rect(NAV_LINK_Y_TDR),
                nav.term_deposits,
            ));
            Ok(())
        })?;
    }
    Ok(())
}

/// Merge pages, then add sidebar navigation links (single pass over the final page list).
pub fn merge_page_pdfs_with_nav(
    pages: Vec<Vec<u8>>,
    nav: NavTargets,
) -> Result<Vec<u8>> {
    let merged = merge_page_pdfs(pages)?;
    let mut editor = DocumentEditor::from_bytes(merged)?;
    add_sidebar_links_to_document(&mut editor, nav)?;
    editor
        .save_to_bytes()
        .map_err(|e| anyhow::anyhow!("Failed to save PDF with navigation: {e}"))
}

pub fn merge_page_pdfs(
    pages: Vec<Vec<u8>>,
) -> Result<Vec<u8>> {
    let (first, rest) =
        pages.split_first().context(
            "No pages to merge",
        )?;

    let mut editor =
        DocumentEditor::from_bytes(first.clone())?;

    for page in rest {
        editor.merge_from_bytes(page)?;
    }

    editor
        .save_to_bytes()
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to merge PDF pages: {e}"
            )
        })
}
