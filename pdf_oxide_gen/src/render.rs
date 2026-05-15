use anyhow::Result;

use crate::customer::{fmt_money, Customer};
use crate::pdf_primitives::*;
// â”€â”€ PDF renderer â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Render one Customer â†’ Vec<u8> (PDF bytes), matching the Python layout exactly.
pub fn render_pdf(customer: &Customer) -> Result<Vec<u8>> {
    let mut doc = Document::new();
    doc.set_compression(true); // equivalent to setPageCompression(1)

    let col_w = PAGE_W - CX - 15.0;
    let content_bottom = PAGE_H - CONTENT_BOTTOM_MARGIN;

    // Destination names
    let dest_summary = "AV_SUMMARY";
    let acc_dests: Vec<String> = customer
        .accounts
        .iter()
        .map(|a| format!("AV_ACC_{}", a.account_number))
        .collect();
    let tdr_dests: Vec<String> = customer
        .tdr
        .iter()
        .map(|t| format!("AV_TDR_{}", t.tdr_number))
        .collect();

    let period_str = if !customer.period_from.is_empty() || !customer.period_to.is_empty() {
        format!("Period: {} to {}", customer.period_from, customer.period_to)
    } else {
        String::new()
    };

    // Sidebar short labels (mirrors Python pre-compute)
    let acc_shorts: Vec<String> = customer
        .accounts
        .iter()
        .take(8)
        .map(|a| {
            if a.account_number.len() > 9 {
                format!("...{}", &a.account_number[a.account_number.len() - 9..])
            } else {
                a.account_number.clone()
            }
        })
        .collect();
    let tdr_shorts: Vec<String> = customer
        .tdr
        .iter()
        .take(5)
        .map(|t| {
            if t.tdr_number.len() > 9 {
                format!("...{}", &t.tdr_number[t.tdr_number.len() - 9..])
            } else {
                t.tdr_number.clone()
            }
        })
        .collect();

    // â”€â”€ Helpers operating on a ContentBuilder â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    /// Filled rect helper (top-down coords)
    fn filled_rect(b: &mut ContentBuilder, x: f32, y: f32, w: f32, h: f32, color: Color) {
        b.set_fill_color(color);
        b.fill_rect(Rect::new(x, PAGE_H - y - h, w, h));
    }

    /// Text helper (top-down y, baseline placed at y)
    fn draw_text(
        b: &mut ContentBuilder,
        x: f32,
        y: f32,
        text: &str,
        font: Font,
        size: f32,
        color: Color,
        align: TextAlign,
        max_w: f32,
    ) {
        b.set_fill_color(color);
        b.set_font(font, size);
        let ry = PAGE_H - y;
        match align {
            TextAlign::Center => b.draw_text_centered(x + max_w / 2.0, ry, text),
            TextAlign::Right  => b.draw_text_right(x + max_w, ry, text),
            _                 => b.draw_string(x, ry, text),
        }
    }

    /// Horizontal line helper (top-down y)
    fn hline(b: &mut ContentBuilder, x: f32, y: f32, length: f32, color: Color, lw: f32) {
        b.set_stroke_color(color);
        b.set_line_width(lw);
        let ry = PAGE_H - y;
        b.draw_line(Line::new(x, ry, x + length, ry));
    }

    /// Diamond Avanza logo (mirrors _draw_diamond_logo)
    fn draw_diamond(b: &mut ContentBuilder, x: f32, y: f32, size: f32) {
        let half = size / 2.0;
        // Outer teal diamond
        b.set_fill_color(c_teal());
        b.fill_polygon(&[
            (x + half,  PAGE_H - y),
            (x + size,  PAGE_H - (y + half)),
            (x + half,  PAGE_H - (y + size)),
            (x,         PAGE_H - (y + half)),
        ]);
        // Inner teal2 diamond
        let inner = size * 0.38;
        let ih = inner / 2.0;
        let ix = x + half - ih;
        let iy = y + half - ih;
        b.set_fill_color(c_teal2());
        b.fill_polygon(&[
            (ix + ih,    PAGE_H - iy),
            (ix + inner, PAGE_H - (iy + ih)),
            (ix + ih,    PAGE_H - (iy + inner)),
            (ix,         PAGE_H - (iy + ih)),
        ]);
        // White "A"
        b.set_fill_color(c_white());
        b.set_font(Font::HelveticaBold, (size * 0.40) as f32);
        b.draw_text_centered(x + half, PAGE_H - (y + size * 0.28), "A");
    }

    /// Header bar chart image (mirrors _draw_header_image)
    fn draw_header_image(b: &mut ContentBuilder, x: f32, y: f32, w: f32, h: f32) {
        // Background stripes
        filled_rect(b, x, y, w, h, c_dnavy());
        let stripe_colors = [c_1a3355(), c_19304f(), c_172e4c()];
        let sw = w / 3.0;
        for (i, &ref col) in stripe_colors.iter().enumerate() {
            filled_rect(b, x + i as f32 * sw, y, sw, h, col.clone());
        }

        // Bars
        let bars: &[(f32, f32, fn() -> Color)] = &[
            (0.04, 0.38, c_2e6db4), (0.13, 0.55, c_3a8fd4),
            (0.22, 0.72, c_teal),   (0.31, 0.50, c_3a8fd4),
            (0.40, 0.83, c_teal),   (0.49, 0.65, c_5ab0e8),
            (0.58, 0.90, c_teal),   (0.67, 0.75, c_5ab0e8),
            (0.76, 0.95, c_7dc8f7),
        ];
        let bar_w = w * 0.075;
        let usable_h = h * 0.76;
        let mut pts: Vec<(f32, f32)> = Vec::new();

        for &(bx_f, bh_f, color_fn) in bars {
            let bh = usable_h * bh_f;
            let bx = x + w * bx_f;
            let by = y + h - bh - 6.0;
            filled_rect(b, bx, by, bar_w, bh, color_fn());
            filled_rect(b, bx, by, bar_w, 1.5, c_white());
            pts.push((bx + bar_w / 2.0, by));
        }

        // Gold trend line through bars 0,2,4,6,8
        let trend: Vec<(f32, f32)> = [0, 2, 4, 6, 8]
            .iter()
            .map(|&i| pts[i])
            .collect();
        b.set_stroke_color(c_gold());
        b.set_line_width(1.8);
        for i in 0..trend.len() - 1 {
            let (x1, y1) = trend[i];
            let (x2, y2) = trend[i + 1];
            b.draw_line(Line::new(x1, PAGE_H - y1, x2, PAGE_H - y2));
        }
        let (px, py) = trend[trend.len() - 1];
        b.set_fill_color(c_gold());
        b.fill_circle(Circle::new(px, PAGE_H - py, 3.0));

        // "FINANCIAL ANALYTICS" label
        draw_text(b, x + 2.0, y + h - 8.0, "FINANCIAL ANALYTICS",
            Font::HelveticaBold, 5.5, c_lblue(), TextAlign::Center, w - 4.0);
    }

    /// Social icon box (mirrors _draw_social_icon)
    fn draw_social_icon(
        b: &mut ContentBuilder, x: f32, y: f32, label: &str, color: Color,
    ) {
        let (iw, ih) = (26.0_f32, 19.0_f32);
        filled_rect(b, x, y, iw, ih, color.clone());
        filled_rect(b, x, y, iw, 2.0, c_white());
        filled_rect(b, x, y + 2.0, iw, ih - 2.0, color);
        draw_text(b, x, y + 5.0, label,
            Font::HelveticaBold, 7.0, c_white(), TextAlign::Center, iw);
    }

    /// Full static chrome (header, footer, sidebar) â€” mirrors _build_static_form
    fn draw_static_chrome(
        b: &mut ContentBuilder,
        cust_name: &str,
        period_str: &str,
        page_num: usize,
    ) {
        // â”€â”€ Header â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        filled_rect(b, 0.0, 0.0, PAGE_W, HEADER_H, c_navy());
        let logo_panel_w = 195.0;
        filled_rect(b, 0.0, 0.0, logo_panel_w, HEADER_H, c_dnavy());

        draw_diamond(b, 10.0, (HEADER_H - 34.0) / 2.0, 34.0);

        draw_text(b, 53.0, 20.0, "AVANZA SOLUTIONS",
            Font::HelveticaBold, 13.5, c_white(), TextAlign::Left, 0.0);
        draw_text(b, 53.0, 36.0, "PVT. LTD.",
            Font::HelveticaBold, 8.0, c_teal(), TextAlign::Left, 0.0);
        draw_text(b, 53.0, 48.0, "www.avanzasolutions.com",
            Font::Helvetica, 6.0, c_lblue(), TextAlign::Left, 0.0);
        draw_text(b, 53.0, 57.0, "info@avanzasolutions.com",
            Font::Helvetica, 6.0, c_lblue(), TextAlign::Left, 0.0);

        // Teal divider
        filled_rect(b, logo_panel_w, 0.0, 3.0, HEADER_H, c_teal());

        // Centre "BANK STATEMENT"
        let mid_x = logo_panel_w + 10.0;
        let mid_w = 178.0;
        draw_text(b, mid_x, 16.0, "BANK STATEMENT",
            Font::HelveticaBold, 17.0, c_white(), TextAlign::Center, mid_w);
        hline(b, mid_x + 10.0, 37.0, mid_w - 20.0, c_teal(), 0.8);

        // Dynamic customer name + period
        if !cust_name.is_empty() {
            draw_text(b, mid_x, 41.0, cust_name,
                Font::HelveticaBold, 8.0, c_white(), TextAlign::Center, mid_w);
        }
        if !period_str.is_empty() {
            draw_text(b, mid_x, 54.0, period_str,
                Font::Helvetica, 6.5, c_lblue(), TextAlign::Center, mid_w);
        }

        // Right chart panel
        let img_x = logo_panel_w + 3.0 + mid_w + 12.0;
        draw_header_image(b, img_x, 4.0, PAGE_W - img_x - 2.0, HEADER_H - 8.0);

        // Teal bottom strip
        filled_rect(b, 0.0, HEADER_H - 3.0, PAGE_W, 3.0, c_teal());

        // â”€â”€ Footer â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        let fy = PAGE_H - FOOTER_H;
        filled_rect(b, 0.0, fy, PAGE_W, FOOTER_H, c_navy());
        filled_rect(b, 0.0, fy, PAGE_W, 2.0, c_teal());

        draw_text(b, 12.0, fy + 8.0, "AVANZA SOLUTIONS PVT. LTD.",
            Font::HelveticaBold, 7.0, c_white(), TextAlign::Left, 0.0);
        draw_text(b, 12.0, fy + 19.0, "Karachi, Pakistan  |  Tel: +92-21-111-282-692",
            Font::Helvetica, 6.0, c_lblue(), TextAlign::Left, 0.0);
        draw_text(b, 12.0, fy + 29.0,
            "This is a system-generated statement. No signature required.",
            Font::Helvetica, 6.0, c_lblue(), TextAlign::Left, 0.0);

        // Page number
        draw_text(b, 0.0, fy + 19.0,
            &format!("Page {page_num}"),
            Font::Helvetica, 6.5, c_lblue(), TextAlign::Right, PAGE_W - 5.0);

        // Social icons
        let socials: &[(&str, fn() -> Color)] = &[
            ("f", c_fb), ("in", c_li), ("tw", c_tw), ("yt", c_yt), ("wa", c_wa),
        ];
        let mut xi = PAGE_W - socials.len() as f32 * 30.0 - 8.0;
        for &(label, color_fn) in socials {
            draw_social_icon(b, xi, fy + 11.0, label, color_fn());
            xi += 30.0;
        }

        // â”€â”€ Sidebar â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        let sb_y = HEADER_H;
        let sb_h = PAGE_H - HEADER_H - FOOTER_H;
        filled_rect(b, 0.0, sb_y, SIDEBAR_W, sb_h, c_sidebar());
        filled_rect(b, 0.0, sb_y, 3.0, sb_h, c_teal());

        draw_text(b, 8.0, sb_y + 12.0, "NAVIGATE",
            Font::HelveticaBold, 6.5, c_navy(), TextAlign::Left, SIDEBAR_W - 8.0);
        hline(b, 8.0, sb_y + 23.0, SIDEBAR_W - 16.0, c_teal(), 0.5);

        draw_text(b, 8.0, PAGE_H - FOOTER_H - 16.0, "AVANZA",
            Font::HelveticaBold, 6.0, c_teal(), TextAlign::Center, SIDEBAR_W - 8.0);
    }

    /// Sidebar navigation links
    fn draw_sidebar_links(
        b: &mut ContentBuilder,
        acc_shorts: &[String],
        tdr_shorts: &[String],
        acc_dests: &[String],
        tdr_dests: &[String],
    ) {
        let sb_y = HEADER_H;
        let mut ny = sb_y + 30.0;

        draw_text(b, 10.0, ny, "> Summary",
            Font::HelveticaBold, 6.5, c_blue(), TextAlign::Left, SIDEBAR_W - 10.0);
        b.add_internal_link("AV_SUMMARY",
            Rect::new(3.0, PAGE_H - ny + 2.0, SIDEBAR_W - 3.0, 11.0));
        ny += 13.0;

        draw_text(b, 8.0, ny, "ACCOUNTS",
            Font::HelveticaBold, 6.0, c_mgray(), TextAlign::Left, SIDEBAR_W - 8.0);
        ny += 10.0;

        for (i, short) in acc_shorts.iter().enumerate() {
            draw_text(b, 10.0, ny, &format!("> {short}"),
                Font::Helvetica, 6.0, c_blue(), TextAlign::Left, SIDEBAR_W - 10.0);
            if let Some(dest) = acc_dests.get(i) {
                b.add_internal_link(dest,
                    Rect::new(3.0, PAGE_H - ny + 2.0, SIDEBAR_W - 3.0, 11.0));
            }
            ny += 10.0;
        }

        ny += 4.0;
        draw_text(b, 8.0, ny, "TDR",
            Font::HelveticaBold, 6.0, c_mgray(), TextAlign::Left, SIDEBAR_W - 8.0);
        ny += 10.0;

        for (i, short) in tdr_shorts.iter().enumerate() {
            draw_text(b, 10.0, ny, &format!("> {short}"),
                Font::Helvetica, 6.0, c_blue(), TextAlign::Left, SIDEBAR_W - 10.0);
            if let Some(dest) = tdr_dests.get(i) {
                b.add_internal_link(dest,
                    Rect::new(3.0, PAGE_H - ny + 2.0, SIDEBAR_W - 3.0, 11.0));
            }
            ny += 10.0;
        }
    }

    /// Table header row (mirrors _draw_table_header)
    fn draw_table_header(b: &mut ContentBuilder, y: f32, col_w: f32) -> f32 {
        filled_rect(b, CX, y, col_w, 16.0, c_navy());
        filled_rect(b, CX, y, 3.0, 16.0, c_teal());
        filled_rect(b, CX + col_w - 3.0, y, 3.0, 16.0, c_gold());
        let hdr = format!("{:<12} {:<33} {:>12} {:>12}", "Date", "Description", "Amount", "Balance");
        draw_text(b, CX + 5.0, y + 3.5, &hdr,
            Font::CourierBold, 7.0, c_white(), TextAlign::Left, col_w - 10.0);
        y + 16.0
    }

    /// Summary table header
    fn tbl_header(b: &mut ContentBuilder, y: f32, col1: &str, col2: &str, col_w: f32) -> f32 {
        filled_rect(b, CX, y, col_w, 17.0, c_navy());
        filled_rect(b, CX, y, 3.0, 17.0, c_teal());
        filled_rect(b, CX + col_w - 3.0, y, 3.0, 17.0, c_gold());
        draw_text(b, CX + 8.0, y + 4.5, col1,
            Font::HelveticaBold, 7.5, c_white(), TextAlign::Left, 220.0);
        draw_text(b, CX + 230.0, y + 4.5, col2,
            Font::HelveticaBold, 7.5, c_white(), TextAlign::Right, col_w - 245.0);
        y + 17.0
    }

    /// Summary table row
    fn tbl_row(
        b: &mut ContentBuilder, y: f32, col1: &str, col2: &str, idx: usize,
        dest: Option<&str>, col_w: f32,
    ) -> f32 {
        if idx % 2 == 0 {
            filled_rect(b, CX, y, col_w, 17.0, c_ltblue());
            filled_rect(b, CX, y, 3.0, 17.0, c_teal());
        } else {
            filled_rect(b, CX, y, 3.0, 17.0, c_cdcef());
        }
        draw_text(b, CX + 8.0, y + 4.5, col1,
            Font::Helvetica, 8.0, c_blue(), TextAlign::Left, 220.0);
        draw_text(b, CX + 230.0, y + 4.5, col2,
            Font::HelveticaBold, 8.0, c_dgray(), TextAlign::Right, col_w - 245.0);
        if let Some(d) = dest {
            b.add_internal_link(d,
                Rect::new(CX, PAGE_H - y - 17.0, col_w, 17.0));
        }
        y + 17.0
    }

    /// Section title (blue bar + bold text)
    fn section_title(b: &mut ContentBuilder, y: f32, title: &str, _col_w: f32) -> f32 {
        filled_rect(b, CX, y, 4.0, 14.0, c_navy());
        draw_text(b, CX + 10.0, y, title,
            Font::HelveticaBold, 9.5, c_navy(), TextAlign::Left, 300.0);
        y + 18.0
    }

    // â”€â”€ Build pages â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    let mut page_num: usize = 1;

    // Closure to add one full page (chrome + sidebar + bookmark if any)
    let add_page = |doc: &mut Document,
                    page_num: usize,
                    bookmark_dest: Option<&str>,
                    cust_name: &str,
                    period_str: &str,
                    acc_shorts: &[String],
                    tdr_shorts: &[String],
                    acc_dests: &[String],
                    tdr_dests: &[String]| -> ContentBuilder
    {
        let mut page = Page::new(PAGE_W, PAGE_H);
        if let Some(dest) = bookmark_dest {
            page.set_named_destination(dest);
        }
        let mut b = page.content_builder();
        draw_static_chrome(&mut b, cust_name, period_str, page_num);
        draw_sidebar_links(&mut b, acc_shorts, tdr_shorts, acc_dests, tdr_dests);
        b
    };

    // â”€â”€ Page 1 â€” Summary â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    {
        let mut page = Page::new(PAGE_W, PAGE_H);
        page.set_named_destination(dest_summary);
        let mut b = page.content_builder();
        draw_static_chrome(&mut b, &customer.name, &period_str, page_num);
        draw_sidebar_links(&mut b, &acc_shorts, &tdr_shorts, &acc_dests, &tdr_dests);

        let mut y = CONTENT_TOP;
        filled_rect(&mut b, CX, y, col_w, 40.0, c_ltblue());
        filled_rect(&mut b, CX, y, 4.0, 40.0, c_teal());
        filled_rect(&mut b, CX + col_w - 4.0, y, 4.0, 40.0, c_gold());
        draw_text(&mut b, CX + 10.0, y + 6.0, "Statement Summary",
            Font::HelveticaBold, 12.0, c_navy(), TextAlign::Left, col_w - 20.0);
        draw_text(&mut b, CX + 10.0, y + 21.0, &customer.name,
            Font::Helvetica, 8.5, c_dgray(), TextAlign::Left, col_w - 20.0);
        draw_text(&mut b, CX + 10.0, y + 31.0, &customer.address,
            Font::Helvetica, 7.0, c_mgray(), TextAlign::Left, col_w - 20.0);
        y += 52.0;

        y = section_title(&mut b, y, "Accounts", col_w);
        y = tbl_header(&mut b, y, "Account Number", "Balance (PKR)", col_w);
        for (i, acc) in customer.accounts.iter().enumerate() {
            y = tbl_row(&mut b, y, &acc.account_number,
                &fmt_money(acc.balance), i, acc_dests.get(i).map(String::as_str), col_w);
        }
        filled_rect(&mut b, CX, y, col_w, 2.0, c_navy());
        y += 22.0;

        y = section_title(&mut b, y, "Term Deposits (TDR)", col_w);
        y = tbl_header(&mut b, y, "TDR Number", "Principal (PKR)", col_w);
        for (i, td) in customer.tdr.iter().enumerate() {
            y = tbl_row(&mut b, y, &td.tdr_number,
                &fmt_money(td.principal_amount), i,
                tdr_dests.get(i).map(String::as_str), col_w);
        }
        filled_rect(&mut b, CX, y, col_w, 2.0, c_navy());

        page.set_content(b);
        doc.add_page(page);
    }

    // â”€â”€ Account detail pages â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    for (ai, acc) in customer.accounts.iter().enumerate() {
        page_num += 1;
        let mut page = Page::new(PAGE_W, PAGE_H);
        page.set_named_destination(&acc_dests[ai]);
        let mut b = page.content_builder();
        draw_static_chrome(&mut b, &customer.name, &period_str, page_num);
        draw_sidebar_links(&mut b, &acc_shorts, &tdr_shorts, &acc_dests, &tdr_dests);

        let mut y = CONTENT_TOP;
        filled_rect(&mut b, CX, y, col_w, 32.0, c_ltblue());
        filled_rect(&mut b, CX, y, 4.0, 32.0, c_teal());
        draw_text(&mut b, CX + 10.0, y + 5.0,
            &format!("Account: {}", acc.account_number),
            Font::HelveticaBold, 10.5, c_navy(), TextAlign::Left, col_w - 20.0);
        draw_text(&mut b, CX + 10.0, y + 19.0,
            &format!("Type: {}   |   Balance: PKR {}", acc.account_type, fmt_money(acc.balance)),
            Font::Helvetica, 7.5, c_mgray(), TextAlign::Left, col_w - 20.0);
        y += 42.0;

        // Transaction table (with page overflow)
        y = draw_table_header(&mut b, y, col_w);
        let mut section_top = y;

        let rows: Vec<String> = acc.transactions.iter().map(|tx| {
            let desc = if tx.description.len() > 32 {
                tx.description[..32].to_string()
            } else {
                tx.description.clone()
            };
            format!("{:<12} {:<33} {:>12.2} {:>12.2}",
                tx.date, desc, tx.amount, tx.balance)
        }).collect();

        for (i, row) in rows.iter().enumerate() {
            if y + 12.0 > content_bottom {
                // Close current section
                filled_rect(&mut b, CX, section_top, 3.0, y - section_top, c_teal());
                filled_rect(&mut b, CX, y, col_w, 2.0, c_navy());
                page.set_content(b);
                doc.add_page(page);

                page_num += 1;
                page = Page::new(PAGE_W, PAGE_H);
                b = page.content_builder();
                draw_static_chrome(&mut b, &customer.name, &period_str, page_num);
                draw_sidebar_links(&mut b, &acc_shorts, &tdr_shorts, &acc_dests, &tdr_dests);
                y = draw_table_header(&mut b, CONTENT_TOP, col_w);
                section_top = y;
            }
            if i % 2 == 0 {
                filled_rect(&mut b, CX, y, col_w, 12.0, c_ltblue());
            }
            draw_text(&mut b, CX + 5.0, y + 2.5, row,
                Font::Courier, 6.8, c_dgray(), TextAlign::Left, col_w - 10.0);
            y += 12.0;
        }
        filled_rect(&mut b, CX, section_top, 3.0, y - section_top, c_teal());
        filled_rect(&mut b, CX, y, col_w, 2.0, c_navy());
        page.set_content(b);
        doc.add_page(page);
    }

    // â”€â”€ TDR detail pages â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    for (ti, td) in customer.tdr.iter().enumerate() {
        page_num += 1;
        let mut page = Page::new(PAGE_W, PAGE_H);
        page.set_named_destination(&tdr_dests[ti]);
        let mut b = page.content_builder();
        draw_static_chrome(&mut b, &customer.name, &period_str, page_num);
        draw_sidebar_links(&mut b, &acc_shorts, &tdr_shorts, &acc_dests, &tdr_dests);

        let mut y = CONTENT_TOP;
        filled_rect(&mut b, CX, y, col_w, 32.0, c_ltblue());
        filled_rect(&mut b, CX, y, 4.0, 32.0, c_teal());
        filled_rect(&mut b, CX + col_w - 3.0, y, 3.0, 32.0, c_gold());
        draw_text(&mut b, CX + 10.0, y + 5.0,
            &format!("Term Deposit: {}", td.tdr_number),
            Font::HelveticaBold, 10.5, c_navy(), TextAlign::Left, col_w - 20.0);
        draw_text(&mut b, CX + 10.0, y + 19.0,
            &format!("Rate: {}%   |   Maturity: {}   |   Principal: PKR {}",
                td.interest_rate, td.maturity_date, fmt_money(td.principal_amount)),
            Font::Helvetica, 7.5, c_mgray(), TextAlign::Left, col_w - 20.0);
        y += 42.0;

        y = draw_table_header(&mut b, y, col_w);
        let mut section_top = y;

        for (i, tx) in td.transactions.iter().enumerate() {
            if y + 12.0 > content_bottom {
                filled_rect(&mut b, CX, section_top, 3.0, y - section_top, c_teal());
                filled_rect(&mut b, CX, y, col_w, 2.0, c_navy());
                page.set_content(b);
                doc.add_page(page);

                page_num += 1;
                page = Page::new(PAGE_W, PAGE_H);
                b = page.content_builder();
                draw_static_chrome(&mut b, &customer.name, &period_str, page_num);
                draw_sidebar_links(&mut b, &acc_shorts, &tdr_shorts, &acc_dests, &tdr_dests);
                y = draw_table_header(&mut b, CONTENT_TOP, col_w);
                section_top = y;
            }
            if i % 2 == 0 {
                filled_rect(&mut b, CX, y, col_w, 12.0, c_ltblue());
            }
            let desc = if tx.description.len() > 32 { &tx.description[..32] } else { &tx.description };
            let row = format!("{:<12} {:<33} {:>12.2} {:>12.2}",
                tx.date, desc, tx.amount, tx.balance);
            draw_text(&mut b, CX + 5.0, y + 2.5, &row,
                Font::Courier, 6.8, c_dgray(), TextAlign::Left, col_w - 10.0);
            y += 12.0;
        }
        filled_rect(&mut b, CX, section_top, 3.0, y - section_top, c_teal());
        filled_rect(&mut b, CX, y, col_w, 2.0, c_navy());
        page.set_content(b);
        doc.add_page(page);
    }

    Ok(doc.save_to_bytes()?)
}

