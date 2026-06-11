use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result};
use image::{ImageFormat, ImageReader};
use pdf_oxide::editor::DocumentEditor;
use pdf_oxide::elements::{ContentElement, ImageContent};
use pdf_oxide::geometry::Rect as GRect;
pub use pdf_oxide::writer::{LinkAnnotation, PdfWriter};
use pdf_oxide::writer::{PageBuilder, PdfWriterConfig};

/// MBL report assets
pub const REPORT_DIR: &str = r"C:\MBL services\Report";

pub const TPL_SUMMARY: &str = "Meezan_Bank_Summary";
pub const TPL_ACCOUNT: &str = "Account";
pub const TPL_TDR: &str = "Accounts_TermDeposit";

pub const IMG_MESSAGE_FOR_YOU: &str = "MessageForYou";

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

/// Clickable sidebar regions
const NAV_LINK_Y_SUMMARY: f32 = 140.0;
const NAV_LINK_Y_ACCOUNTS: f32 = 170.0;
const NAV_LINK_Y_TDR: f32 = 200.0;
const NAV_LINK_W: f32 = 120.0;
const NAV_LINK_H: f32 = 10.0;

pub const FOOTER_X: f32 = 0.0;
pub const FOOTER_Y: f32 = 0.0;
pub const FOOTER_W: f32 = PAGE_W;
pub const FOOTER_H: f32 = 25.0;

pub const IMG_STANDARD_FOOTER: &str = "Footer";
pub const IMG_PREMIUM_FOOTER: &str = "PremiumFooter";

#[derive(Clone, Copy, Debug, Default)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Color {
    pub fn hex(s: &str) -> Self {
        let hex = s.trim_start_matches('#');
        if hex.len() != 6 {
            return Self::default();
        }
        let parse = |i: usize| {
            u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0) as f32 / 255.0
        };
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
pub enum ReportType {
    Standard,
    Premium,
}

#[derive(Clone, Copy)]
pub enum PageType {
    Summary,
    Account,
    TermDeposit,
}

pub struct HeaderAssets {
    pub left_logo: &'static str,
    pub right_logo: Option<&'static str>,
    pub footer: &'static str,
}

impl PageType {
    pub fn title(&self) -> &'static str {
        match self {
            Self::Summary => "SUMMARY",
            Self::Account => "ACCOUNTS",
            Self::TermDeposit => "TERM DEPOSIT",
        }
    }
}

impl ReportType {
    pub fn header_assets(&self) -> HeaderAssets {
        match self {
            ReportType::Standard => HeaderAssets {
                left_logo: "MeezanLogo",
                right_logo: None,
                footer: IMG_STANDARD_FOOTER,
            },
            ReportType::Premium => HeaderAssets {
                left_logo: "MeezanPremiumLogo",
                right_logo: Some("PremiumBadge"),
                footer: IMG_PREMIUM_FOOTER,
            },
        }
    }
}

#[derive(Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
}

#[derive(Clone, Copy)]
pub struct Line {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

impl Line {
    pub fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
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
        let mut writer = PdfWriter::with_config(
            PdfWriterConfig::default().with_compress(true)
        );

        for p in self.pages {
            let mut page = writer.add_page(p.width, p.height);
            apply_builder_to_page(&mut page, &p.content);
            page.finish();
        }

        Ok(writer.finish()?)
    }
}

/// A4 Geometry
pub const PAGE_W: f32 = 595.28;
pub const PAGE_H: f32 = 841.89;
pub const HEADER_HEIGHT: f32 = 70.0;

pub const LEFT_LOGO_X: f32 = 10.0;
pub const LEFT_LOGO_Y: f32 = 38.0;
pub const LEFT_LOGO_W: f32 = 160.0;
pub const LEFT_LOGO_H: f32 = 65.0;

pub const RIGHT_LOGO_X: f32 = PAGE_W - 160.0;
pub const RIGHT_LOGO_Y: f32 = 38.0;
pub const RIGHT_LOGO_W: f32 = 150.0;
pub const RIGHT_LOGO_H: f32 = 65.0;

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


pub fn full_table_width() -> f32 { PAGE_W - 20.0 }
pub fn table_x() -> f32 { 10.0 }

/// Colors
pub fn c_purple() -> Color { Color::hex("#440055") }
pub fn c_gold_text() -> Color { Color::hex("#9A8C27") }
pub fn c_teal() -> Color { Color::hex("#33BAAC") }
pub fn c_black() -> Color { Color::hex("#000000") }
pub fn c_white() -> Color { Color::hex("#FFFFFF") }
pub fn c_dark_gray() -> Color { Color::hex("#404040") }
pub fn c_gray_bar() -> Color {
    Color::hex("#c2c2c2")
}
pub fn c_gray_border() -> Color {
    Color::hex("#dadada")
}
pub fn c_mbl_green() -> Color { Color::hex("#0E836A") }
pub fn c_mbl_mint() -> Color {
    Color::hex("#D5DBDB")
}
// ---------------------------------------------------------------------------
// Raw byte cache — keyed by image name, stores compressed JPEG bytes.
// This was already present; kept as-is.
// ---------------------------------------------------------------------------
static LOGO_CACHE: OnceLock<Mutex<HashMap<String, Arc<Vec<u8>>>>> = OnceLock::new();

fn logo_cache() -> &'static Mutex<HashMap<String, Arc<Vec<u8>>>> {
    LOGO_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

// ---------------------------------------------------------------------------
// PrecompiledImages cache — keyed by report type, stores decoded ImageContent
// objects so they are never re-decoded across pages or across report runs.
//
// Uses Mutex<Option<…>> instead of OnceLock::get_or_try_init because the
// latter is a nightly-only unstable feature (once_cell_try, issue #109737).
// This pattern is fully stable: lock, check, populate if empty, clone out.
//
// ImageContent must be Clone + Send + Sync for this to compile.  If your
// version of pdf_oxide does not satisfy those bounds, remove these statics
// and call PrecompiledImages::prepare() once per report at the call site,
// then pass &precompiled into write_content_page_optimized() for every page.
// ---------------------------------------------------------------------------
static PRECOMPILED_STANDARD: OnceLock<Mutex<Option<PrecompiledImages>>> = OnceLock::new();
static PRECOMPILED_PREMIUM:  OnceLock<Mutex<Option<PrecompiledImages>>> = OnceLock::new();

fn precompiled_slot(
    report_type: ReportType,
) -> &'static Mutex<Option<PrecompiledImages>> {
    match report_type {
        ReportType::Standard => {
            PRECOMPILED_STANDARD.get_or_init(|| Mutex::new(None))
        }
        ReportType::Premium => {
            PRECOMPILED_PREMIUM.get_or_init(|| Mutex::new(None))
        }
    }
}

pub fn resolve_report_asset(name: &str) -> PathBuf {
    let dir = Path::new(REPORT_DIR);
    let candidates = [
        dir.join(name),
        dir.join(format!("{name}.pdf")),
        dir.join(format!("{name}.png")),
        dir.join(format!("{name}.jpg")),
        dir.join(format!("{name}.jpeg")),
    ];
    for p in &candidates {
        if p.exists() { return p.clone(); }
    }
    dir.join(name)
}

/// Compresses image bytes to optimized JPEGs on the fly to aggressively cut file sizes.
fn compress_image_to_jpeg(raw_bytes: &[u8]) -> Result<Vec<u8>> {
    let img = ImageReader::new(Cursor::new(raw_bytes))
        .with_guessed_format()?
        .decode()
        .context("Failed to decode asset image data")?;

    let mut compressed = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut compressed, 75);
    encoder.encode_image(&img).context("Failed to re-encode image to compressed JPEG")?;

    Ok(compressed)
}

pub fn load_message_for_you_image() -> Result<Vec<u8>> {
    let path = resolve_report_asset(IMG_MESSAGE_FOR_YOU);
    let raw = std::fs::read(&path).with_context(|| {
        format!("MessageForYou image not found at {}", path.display())
    })?;
    compress_image_to_jpeg(&raw)
}

pub fn load_logo_image(image_name: &str) -> Result<Arc<Vec<u8>>> {
    {
        let cache = logo_cache().lock().unwrap();
        if let Some(bytes) = cache.get(image_name) {
            return Ok(bytes.clone());
        }
    }

    let path = resolve_report_asset(image_name);
    let raw_bytes = std::fs::read(&path).with_context(|| {
        format!("Logo image not found: {}", path.display())
    })?;

    // Losslessly/lossily down-compress high-res PNG/BMP images into runtime optimized streams
    let optimized_bytes = compress_image_to_jpeg(&raw_bytes)
        .unwrap_or(raw_bytes); // Fallback to raw if decoding fails

    let shared_bytes = Arc::new(optimized_bytes);
    {
        let mut cache = logo_cache().lock().unwrap();
        cache.insert(image_name.to_string(), shared_bytes.clone());
    }
    Ok(shared_bytes)
}

pub fn load_footer_image(image_name: &str) -> Result<Arc<Vec<u8>>> {
    load_logo_image(image_name)
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
            Op::Text { x, y, text, font, size, color } => {
                page.set_fill_color(color.r, color.g, color.b);
                page.add_text(text, *x, *y, font.name(), *size);
            }
            Op::Line(l, c, lw) => {
                let dx = (l.x2 - l.x1).abs();
                let dy = (l.y2 - l.y1).abs();
                if dy < 0.01 {
                    page.draw_hline_colored(l.x1, l.y1, dx.max(0.1), *lw, c.r, c.g, c.b);
                } else if dx < 0.01 {
                    page.fill_rect_colored(l.x1, l.y1.min(l.y2), lw.max(0.1), dy.max(0.1), c.r, c.g, c.b);
                } else {
                    page.fill_rect_colored(l.x1.min(l.x2), l.y1.min(l.y2), dx.max(1.0), dy.max(*lw), c.r, c.g, c.b);
                }
            }
        }
    }
}

pub fn add_sidebar_links_to_page(page: &mut PageBuilder<'_>, nav: NavTargets) {
    page.internal_link(nav_link_rect(NAV_LINK_Y_SUMMARY), nav.summary);
    page.internal_link(nav_link_rect(NAV_LINK_Y_ACCOUNTS), nav.accounts);
    page.internal_link(nav_link_rect(NAV_LINK_Y_TDR), nav.term_deposits);
}

// ---------------------------------------------------------------------------
// PrecompiledImages
//
// Holds decoded ImageContent objects for the header logos and footer.
// Derive Clone so instances can be stored in OnceLock statics and handed out
// by reference via get_or_init / get_cached.
// ---------------------------------------------------------------------------
#[derive(Clone)]
pub struct PrecompiledImages {
    pub left_logo: ImageContent,
    pub right_logo: Option<ImageContent>,
    pub footer: ImageContent,
}

impl PrecompiledImages {
    /// Build from scratch — decodes raw bytes into ImageContent.
    /// Prefer `get_cached` in production code; call this directly only in
    /// tests or when you intentionally need a fresh instance.
    pub fn prepare(report_type: ReportType) -> Result<Self> {
        let assets = report_type.header_assets();

        let left_bytes = load_logo_image(assets.left_logo)?;
        let left_logo = ImageContent::from_bytes(
            GRect::new(LEFT_LOGO_X, PAGE_H - LEFT_LOGO_Y - LEFT_LOGO_H, LEFT_LOGO_W, LEFT_LOGO_H),
            (*left_bytes).clone(),
        )?;

        let right_logo = if let Some(right_logo_name) = assets.right_logo {
            let right_bytes = load_logo_image(right_logo_name)?;
            Some(ImageContent::from_bytes(
                GRect::new(RIGHT_LOGO_X, PAGE_H - RIGHT_LOGO_Y - RIGHT_LOGO_H, RIGHT_LOGO_W, RIGHT_LOGO_H),
                (*right_bytes).clone(),
            )?)
        } else {
            None
        };

        let footer_bytes = load_logo_image(assets.footer)?;
        let footer = ImageContent::from_bytes(
            GRect::new(FOOTER_X, FOOTER_Y, FOOTER_W, FOOTER_H),
            (*footer_bytes).clone(),
        )?;

        Ok(Self { left_logo, right_logo, footer })
    }

    /// Return a process-lifetime cached clone for the given report type.
    ///
    /// The first call per variant pays the decode cost; every subsequent call
    /// (across pages *and* across report generations) clones the cached value
    /// at near-zero cost.
    ///
    /// Uses a stable `Mutex<Option<…>>` pattern to avoid the nightly-only
    /// `OnceLock::get_or_try_init` (once_cell_try, issue #109737).
    pub fn get_cached(report_type: ReportType) -> Result<PrecompiledImages> {
        let slot = precompiled_slot(report_type);
        let mut guard = slot.lock().unwrap();
        if guard.is_none() {
            *guard = Some(Self::prepare(report_type)?);
        }
        // SAFETY: we just guaranteed Some above.
        Ok(guard.as_ref().unwrap().clone())
    }
}

// ---------------------------------------------------------------------------
// Page writing helpers
// ---------------------------------------------------------------------------

/// Core page writer — takes a shared reference to already-decoded images.
/// Call this in a loop over all pages; images are never re-decoded.
pub fn write_content_page_optimized(
    writer: &mut PdfWriter,
    builder: &ContentBuilder,
    image: Option<(Vec<u8>, f32, f32, f32, f32)>,
    nav: NavTargets,
    precompiled: &PrecompiledImages,
) -> Result<()> {
    let mut page = writer.add_page(PAGE_W, PAGE_H);
    apply_builder_to_page(&mut page, builder);

    page.add_element(&ContentElement::Image(precompiled.left_logo.clone()));
    if let Some(ref right_logo) = precompiled.right_logo {
        page.add_element(&ContentElement::Image(right_logo.clone()));
    }

    if let Some((bytes, x, y, w, h)) = image {
        let img = ImageContent::from_bytes(GRect::new(x, y, w, h), bytes)
            .map_err(|e| anyhow::anyhow!("MessageForYou image: {e}"))?;
        page.add_element(&ContentElement::Image(img));
    }

    add_sidebar_links_to_page(&mut page, nav);
    page.add_element(&ContentElement::Image(precompiled.footer.clone()));

    page.finish();
    Ok(())
}

/// Convenience wrapper that looks up (or initialises) the process-wide
/// `PrecompiledImages` cache automatically.
///
/// Drop-in replacement for the old `write_content_page` — same signature,
/// but `PrecompiledImages::prepare()` is now called at most **once per report
/// type per process** instead of once per page.
pub fn write_content_page(
    writer: &mut PdfWriter,
    builder: &ContentBuilder,
    image: Option<(Vec<u8>, f32, f32, f32, f32)>,
    nav: NavTargets,
    report_type: ReportType,
) -> Result<()> {
    let precompiled = PrecompiledImages::get_cached(report_type)?;
    write_content_page_optimized(writer, builder, image, nav, &precompiled)
}

// ---------------------------------------------------------------------------
// PDF finalisation helpers
// ---------------------------------------------------------------------------

pub fn finish_pdf(writer: PdfWriter) -> Result<Vec<u8>> {
    writer
        .finish()
        .map_err(|e| anyhow::anyhow!("Failed to finish PDF: {e}"))
}

pub fn new_pdf_writer() -> PdfWriter {
    PdfWriter::with_config(PdfWriterConfig::default().with_compress(true))
}

pub fn add_sidebar_links_to_document(editor: &mut DocumentEditor, nav: NavTargets) -> Result<()> {
    let page_count = editor.current_page_count();
    for page_index in 0..page_count {
        editor.edit_page(page_index, |page| {
            page.add_annotation(LinkAnnotation::goto_page(nav_link_rect(NAV_LINK_Y_SUMMARY), nav.summary));
            page.add_annotation(LinkAnnotation::goto_page(nav_link_rect(NAV_LINK_Y_ACCOUNTS), nav.accounts));
            page.add_annotation(LinkAnnotation::goto_page(nav_link_rect(NAV_LINK_Y_TDR), nav.term_deposits));
            Ok(())
        })?;
    }
    Ok(())
}

pub fn merge_page_pdfs_with_nav(pages: Vec<Vec<u8>>, nav: NavTargets) -> Result<Vec<u8>> {
    let merged = merge_page_pdfs(pages)?;
    let mut editor = DocumentEditor::from_bytes(merged)?;
    add_sidebar_links_to_document(&mut editor, nav)?;
    editor
        .save_to_bytes()
        .map_err(|e| anyhow::anyhow!("Failed to save PDF with navigation: {e}"))
}

pub fn merge_page_pdfs(pages: Vec<Vec<u8>>) -> Result<Vec<u8>> {
    let (first, rest) = pages.split_first().context("No pages to merge")?;
    let mut editor = DocumentEditor::from_bytes(first.clone())?;
    for page in rest {
        editor.merge_from_bytes(page)?;
    }
    editor
        .save_to_bytes()
        .map_err(|e| anyhow::anyhow!("Failed to merge PDF pages: {e}"))
}