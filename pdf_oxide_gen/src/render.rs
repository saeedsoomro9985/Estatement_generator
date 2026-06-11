use anyhow::Result;

use crate::customer::{Statement, fmt_money};
use crate::pdf_primitives::*;

const ROW_H: f32 = 16.0;
const HDR_H: f32 = 20.0;
const LABEL_SIZE: f32 = 10.0;
const TABLE_SIZE: f32 = 7.5;
const NAV_SIZE: f32 = 11.0;

pub struct HeaderConfig {
    pub page_type: PageType,
}


/// Count account transaction pages (same pagination rules as the render loop).
fn count_account_pages(stmt: &Statement) -> usize {
    let mut total = 0usize;
    for acc in &stmt.accounts {
        if acc.transactions.is_empty() {
            total += 1;
            continue;
        }
        let mut row_idx = 0usize;
        let mut y = TX_TABLE_TOP + HDR_H;
        let mut pages = 1usize;
        loop {
            while row_idx < acc.transactions.len() && y + ROW_H <= CONTENT_BOTTOM {
                y += ROW_H;
                row_idx += 1;
            }
            if row_idx >= acc.transactions.len() {
                break;
            }
            pages += 1;
            y = TX_TABLE_TOP + HDR_H;
        }
        total += pages;
    }
    total
}

/// Count TDR detail pages (same pagination rules as `render_tdr_pages`).
fn count_tdr_pages(stmt: &Statement) -> usize {
    let mut total = 0usize;
    for td in &stmt.term_deposits {
        if td.certificates.is_empty() {
            continue;
        }
        let mut idx = 0usize;
        while idx < td.certificates.len() {
            let mut y = TDR_TABLE_TOP + HDR_H + TDR_SECTION_BANNER_H;
            let mut pages = 1usize;
            loop {
                while idx < td.certificates.len() {
                    if idx > 0
                        && td.certificates[idx].cert_type_label
                            != td.certificates[idx - 1].cert_type_label
                    {
                        let need = TDR_SECTION_BANNER_H + ROW_H;
                        if y + need > TDR_PAGE_BOTTOM {
                            break;
                        }
                        y += TDR_SECTION_BANNER_H;
                    }
                    if y + ROW_H > TDR_PAGE_BOTTOM {
                        break;
                    }
                    y += ROW_H;
                    idx += 1;
                }
                if idx >= td.certificates.len() {
                    break;
                }
                pages += 1;
                y = TDR_TABLE_TOP + HDR_H + TDR_SECTION_BANNER_H;
            }
            total += pages;
        }
    }
    total
}

pub fn render_pdf(stmt: &Statement) -> Result<Vec<u8>> {
    // Load/rasterize the 3 templates once (parallel); disk cache makes later runs fast.
    // preload_templates()?;

    let mut writer = new_pdf_writer();

    let summary_page = 0usize;
    let accounts_page = 2usize;
    let account_pages = count_account_pages(stmt);
    let tdr_pages = count_tdr_pages(stmt);
    let term_deposits_page = if tdr_pages > 0 {
        accounts_page + account_pages
    } else if account_pages > 0 {
        accounts_page
    } else {
        summary_page
    };
    
    let nav = NavTargets {
        summary: summary_page,
        accounts: accounts_page,
        term_deposits: term_deposits_page,
    };

    let grand_tdr_amount: f64 = stmt
    .term_deposits
    .iter()
    .flat_map(|td| td.certificates.iter())
    .map(|c| c.amount.replace(',', "").parse::<f64>().unwrap_or(0.0))
    .sum();

    let grand_tdr_certificates: usize = stmt
        .term_deposits
        .iter()
        .map(|td| td.certificates.len())
        .sum();

    // ── Page 1: summary template — account + TDR summary tables ──
    {
        let mut b = ContentBuilder::new();

        draw_header(
            &mut b,
            HeaderConfig {
                page_type: PageType::Summary,
            },
        );

        draw_sidebar(&mut b, true, false);
        vline(
            &mut b,
            120.0, // adjust position
            135.0,                          // start Y
            PAGE_H - 40.0,                 // line height
            c_gray_border(),
            0.5,
        );
        draw_customer_block(&mut b, stmt);

        let acc_cols = ["Product", "Account Number", "IBAN", "Currency", "FCY Balance", "Balance"];
        let acc_widths = [72.0, 85.0, 110.0, 52.0, 58.0, 73.0];
        
        draw_text(
            &mut b,
            CX,
            ACC_SUMMARY_TOP - 10.0,
            "Account Summary",
            Font::Bold,
            11.0,
            c_gray_bar(),
            TextAlign::Left,
            200.0,
        );
        let mut y = draw_mbl_table(
            &mut b,
            CX,
            ACC_SUMMARY_TOP,
            &acc_cols,
            &acc_widths,
            stmt.account_summary
                .iter()
                .map(|r| {
                    vec![
                        r.product.as_str(),
                        r.account_number.as_str(),
                        r.iban.as_str(),
                        r.currency.as_str(),
                        r.fcy_balance.as_str(),
                        r.balance.as_str(),
                    ]
                })
                .collect::<Vec<_>>()
                .as_slice(),
        );

        let tdr_cols = [
            "Certificate Type",
            "No Of Certificate",
            "IBAN",
            "Currency",
            "FCY Balance",
            "Balance",
        ];
        let tdr_widths = [58.0, 58.0, 130.0, 42.0, 72.0, 90.0];
        if y + 40.0 > CONTENT_BOTTOM {
            y = TDR_SUMMARY_TOP;
        } else {
            y += 60.0;
        }

        draw_text(
            &mut b,
            CX,
            y - 10.0,
            "Term Deposit Summary",
            Font::Bold,
            11.0,
            c_gray_bar(),
            TextAlign::Left,
            250.0,
        );
        draw_mbl_table(
            &mut b,
            CX,
            y,
            &tdr_cols,
            &tdr_widths,
            stmt.tdr_summary
                .iter()
                .map(|r| {
                    vec![
                        r.certificate_type.as_str(),
                        r.number_of_certificates.as_str(),
                        r.iban.as_str(),
                        r.currency.as_str(),
                        r.fcy_balance.as_str(),
                        r.balance.as_str(),
                    ]
                })
                .collect::<Vec<_>>()
                .as_slice(),
        );

        // draw_text(
        //     &mut b,
        //     CX,
        //     80.0,
        //     &format!(
        //         "Rupees equivalent aggregate balance {}  {}",
        //         stmt.to_date, sum_balances(stmt)
        //     ),
        //     Font::Regular,
        //     LABEL_SIZE,
        //     c_black(),
        //     TextAlign::Left,
        //     TABLE_W,
        // );

        write_content_page(&mut writer, &b, None, nav,ReportType::Standard)?;
    }

    // ── Page 2: summary template + MessageForYou image (DrawAdvert) ──
    {
        let mut b = ContentBuilder::new();
        draw_header(
            &mut b,
            HeaderConfig {
                page_type: PageType::Summary,
            },
        );

        draw_sidebar(&mut b, true, false);
        vline(
            &mut b,
            120.0, // adjust position
            135.0,                          // start Y
            PAGE_H - 40.0,                 // line height
            c_gray_border(),
            0.5,
        );
        draw_customer_block(&mut b, stmt);
        let msg_bytes = load_message_for_you_image()?;
        let img_y = PAGE_H - MSG_IMG_TOP - MSG_IMG_H;
        write_content_page(
            &mut writer,
            &b,
            Some((msg_bytes, MSG_IMG_X, img_y, MSG_IMG_W, MSG_IMG_H)),
            nav,
            ReportType::Standard,
        )?;
    }

    // ── Account transaction pages (Account template) ──
    for acc in &stmt.accounts {
        let mut b = ContentBuilder::new();
        draw_header(
            &mut b,
            HeaderConfig {
                page_type: PageType::Account,
            },
        );
        draw_sidebar(&mut b, false, true);
            vline(
            &mut b,
            120.0, // adjust position
            135.0,                          // start Y
            150.0,                 // line height
            c_gray_border(),
            0.5,
        );
        draw_account_header(&mut b, acc);

        let cols = [
            "Date",
            "Value Date",
            "Doc No",
            "Particular",
            "Debit",
            "Credit",
            "Balance",
        ];
        let tx_x = table_x();
        let tx_w = full_table_width();
        let base = [1.0, 1.0, 1.0, 3.5, 1.0, 1.0, 1.2];
        let sum: f32 = base.iter().sum();

        let widths: Vec<f32> = base.iter()
            .map(|w| (w / sum) * tx_w)
            .collect();
        let mut row_idx = 0usize;
        let mut y = TX_TABLE_TOP;
        let mut first_page = true;
    

        loop {
            if !first_page {
                write_content_page(&mut writer, &b, None, nav,ReportType::Standard)?;
                b = ContentBuilder::new();
                   draw_header(
                        &mut b,
                        HeaderConfig {
                            page_type: PageType::Account,
                        },
                    );
                draw_sidebar(&mut b, false, true);
                  draw_sidebar(&mut b, false, true);
                    vline(
                    &mut b,
                    120.0, // adjust position
                    135.0,                          // start Y
                    150.0,                 // line height
                    c_gray_border(),
                    0.5,
                );
                draw_account_header(&mut b, acc);
            }
            first_page = false;

            draw_mbl_table_header(&mut b, tx_x, y, &cols, &widths, Some(tx_w));
            y += HDR_H;

            while row_idx < acc.transactions.len() && y + ROW_H <= CONTENT_BOTTOM {
                let tx = &acc.transactions[row_idx];
                draw_mbl_data_row(
                    &mut b,
                    tx_x,
                    y,
                    &[
                        tx.date.as_str(),
                        tx.value_date.as_str(),
                        tx.doc_no.as_str(),
                        tx.particular.as_str(),
                        tx.debit.as_str(),
                        tx.credit.as_str(),
                        tx.balance.as_str(),
                    ],
                    &widths,
                    row_idx,
                    Some(tx_w),
                );
                y += ROW_H;
                row_idx += 1;
            }

            if row_idx >= acc.transactions.len() {
                break;
            }
            y = TX_TABLE_TOP;
        }

        let box_y = y + ACCOUNT_BOX_MARGIN;

        if box_y + ACCOUNT_BOX_H <= CONTENT_BOTTOM {
            draw_account_stats_box(
                &mut b,
                box_y,
                acc,
            );

            write_content_page(&mut writer, &b, None, nav,ReportType::Standard)?;
        } else {
            // write last transaction page first
            write_content_page(&mut writer, &b, None, nav,ReportType::Standard)?;

            let mut stats_page = ContentBuilder::new();
            draw_header(
                    &mut stats_page,
                    HeaderConfig {
                        page_type: PageType::Account,
                    },
            );
            draw_sidebar(&mut stats_page, false, true);
            vline(
                &mut stats_page,
                120.0, // adjust position
                135.0,                          // start Y
                140.0,                 // line height
                c_gray_border(),
                0.5,
            );
            draw_account_header(&mut stats_page, acc);

            draw_account_stats_box(
                &mut stats_page,
                TX_TABLE_TOP,
                acc,
            );

            write_content_page(
                &mut writer,
                &stats_page,
                None,
                nav,
                ReportType::Standard,
            )?;
        }
    }

    // ── Term deposit pages (Accounts_TermDeposit template) ──
    for (idx, td) in stmt.term_deposits.iter().enumerate() {
        let is_last_tdr = idx == stmt.term_deposits.len() - 1;

        render_tdr_pages(
            td,
            &mut writer,
            nav,
            is_last_tdr,
            grand_tdr_amount,
            grand_tdr_certificates,
        )?;
    }

    finish_pdf(writer)
}

fn sum_balances(stmt: &Statement) -> String {
    let mut total = 0.0_f64;
    for r in &stmt.account_summary {
        if r.product == "Total" {
            return r.balance.clone();
        }
        if let Ok(v) = r.balance.replace(',', "").parse::<f64>() {
            total += v;
        }
    }
    format!("{total:.2}")
}

// ── Drawing helpers (top-down Y converted to PDF bottom-up) ──

fn td_y(y_top: f32) -> f32 {
    PAGE_H - y_top
}

fn draw_text(
    b: &mut ContentBuilder,
    x: f32,
    y_top: f32,
    text: &str,
    font: Font,
    size: f32,
    color: Color,
    align: TextAlign,
    max_w: f32,
) {
    b.set_fill_color(color);
    b.set_font(font, size);
    let y = td_y(y_top);
    match align {
        TextAlign::Center => b.draw_text_centered(x + max_w / 2.0, y, text),
        TextAlign::Right => b.draw_text_right(x + max_w, y, text),
        _ => b.draw_string(x, y, text),
    }
}

fn filled_rect(b: &mut ContentBuilder, x: f32, y_top: f32, w: f32, h: f32, color: Color) {
    b.set_fill_color(color);
    b.fill_rect(Rect::new(x, td_y(y_top + h), w, h));
}

fn hline(b: &mut ContentBuilder, x: f32, y_top: f32, length: f32, color: Color, lw: f32) {
    b.set_stroke_color(color);
    b.set_line_width(lw);
    let y = td_y(y_top);
    b.draw_line(Line::new(x, y, x + length, y));
}

fn vline(b: &mut ContentBuilder, x: f32, y_top: f32, h: f32, color: Color, lw: f32) {
    b.set_stroke_color(color);
    b.set_line_width(lw);
    b.draw_line(Line::new(x, td_y(y_top), x, td_y(y_top + h)));
}

/// Left sidebar labels (SUMMARY / ACCOUNTS / TERM DEPOSITS) — PdfSharp template positions.
fn draw_sidebar(b: &mut ContentBuilder, on_summary: bool, on_accounts: bool) {
    let items = [
        ("SUMMARY", 140.0, on_summary),
        ("ACCOUNTS", 170.0, on_accounts),
        ("TERM DEPOSITS", 200.0, !on_summary && !on_accounts),
    ];
    for (label, y, active) in items {
        draw_text(
            b,
            SIDEBAR_X,
            y,
            label,
            if active {
                Font::Bold
            } else {
                Font::Regular
            },
            NAV_SIZE,
            c_black(),
            TextAlign::Left,
            80.0,
        );
        draw_text(
            b,
            SIDEBAR_X + 95.0, // same horizontal position for all
            y,
            ">>",
            if active {
                Font::Bold
            } else {
                Font::Regular
            },
            NAV_SIZE,
            c_black(),
            TextAlign::Left,
            20.0,
        );
    }
}

fn draw_customer_block(b: &mut ContentBuilder, stmt: &Statement) {
    let pairs = [
        ("Customer Name", &stmt.customer_name, 125.0, 150.0, 170.0),
        ("From Date", &stmt.from_date, 250.0, 150.0, 170.0),
        ("To Date", &stmt.to_date, 375.0, 150.0, 170.0),
        ("Customer Id", &stmt.customer_id, 125.0, 200.0, 220.0),
    ];
    for (label, value, x_label, y_l, y_v) in pairs {
        draw_text(
            b,
            x_label,
            y_l,
            label,
            Font::Bold,
            LABEL_SIZE,
            c_black(),
            TextAlign::Left,
            120.0,
        );
        draw_text(
            b,
            x_label,
            y_v,
            value,
            Font::Regular,
            LABEL_SIZE,
            c_dark_gray(),
            TextAlign::Left,
            200.0,
        );
    }
    // if !stmt.cif.is_empty() {
    //     draw_text(
    //         b,
    //          250.0,
    //         200.0,
    //         "CIF",
    //         Font::Bold,
    //         LABEL_SIZE,
    //         c_black(),
    //         TextAlign::Left,
    //         40.0,
    //     );
    //     draw_text(
    //         b,
    //         250.0,
    //         220.0,
    //         &stmt.cif,
    //         Font::Regular,
    //         LABEL_SIZE,
    //         c_black(),
    //         TextAlign::Left,
    //         120.0,
    //     );
    // }
}

fn draw_account_header(b: &mut ContentBuilder, acc: &crate::customer::AccountDetail) {
    let rows = [
        ("Title", &acc.title, 125.0, 130.0, 145.0),
        ("Account Type", &acc.account_type, 125.0, 170.0, 185.0),
        ("Account Number", &acc.account_number, 240.0, 170.0, 185.0),
        ("IBAN", &acc.iban, 365.0, 170.0, 185.0),
        ("Currency", &acc.currency, 125.0, 210.0, 225.0),
        ("From Date", &acc.from_date, 240.0, 210.0, 225.0),
        ("To Date", &acc.to_date, 365.0, 210.0, 225.0),
        ("Branch", &acc.branch, 125.0, 250.0, 265.0),
    ];
    for (label, value, x, y_l, y_v) in rows {
        draw_text(
            b,
            x,
            y_l,
            label,
            Font::Bold,
            LABEL_SIZE,
            c_black(),
            TextAlign::Left,
            100.0,
        );
        draw_text(
            b,
            x,
            y_v,
            value,
            Font::Regular,
            LABEL_SIZE,
            c_black(),
            TextAlign::Left,
            200.0,
        );
    }
}

fn draw_account_stats_box(
    b: &mut ContentBuilder,
    y: f32,
    acc: &crate::customer::AccountDetail,
) {
    const ROW_H: f32 = 16.0;

    let x = table_x();
    let w = full_table_width();
    let label_w = w * 0.75;

    let total_credit_count = acc
        .transactions
        .iter()
        .filter(|t| {
            let v = t.credit.replace(',', "").trim().to_string();
            v != "0.00" && v != "0" && v != "-"
        })
        .count();

    let total_debit_count = acc
        .transactions
        .iter()
        .filter(|t| {
            let v = t.debit.replace(',', "").trim().to_string();
            v != "0.00" && v != "0" && v != "-"
        })
        .count();

    let total_credit_turnover: f64 = acc
        .transactions
        .iter()
        .map(|t| {
            t.credit
                .replace(',', "")
                .trim()
                .parse::<f64>()
                .unwrap_or(0.0)
        })
        .sum();

    let total_debit_turnover: f64 = acc
        .transactions
        .iter()
        .map(|t| {
            t.debit
                .replace(',', "")
                .trim()
                .parse::<f64>()
                .unwrap_or(0.0)
        })
        .sum();

    let rows = [
        (
            "Total No. of Credit Transactions",
            total_credit_count.to_string(),
        ),
        (
            "Total No. of Debit Transactions",
            total_debit_count.to_string(),
        ),
        (
            "Total Credit Turnover",
            fmt_money(total_credit_turnover),
        ),
        (
            "Total Debit Turnover",
            fmt_money(total_debit_turnover),
        ),
    ];

    let total_h = ROW_H * rows.len() as f32;

    hline(b, x, y, w, c_gray_bar(), 0.5,);
    // Left border
    vline(b, x, y, total_h, c_gray_bar(), 0.5);

    // Right border
    vline(b, x + w, y, total_h, c_gray_bar(), 0.5);

    let mut row_y = y;

    for (label, value) in rows {
        draw_text(
            b,
            x + 6.0,
            row_y + 10.0,
            label,
            Font::Regular,
            8.0,
            c_black(),
            TextAlign::Left,
            label_w,
        );

        draw_text(
            b,
            x + label_w,
            row_y + 10.0,
            &value,
            Font::Bold,
            8.0,
            c_black(),
            TextAlign::Right,
            w - label_w - 6.0,
        );

        row_y += ROW_H;
    }

    // Bottom border only
    hline(
        b,
        x,
        y + total_h,
        w,
        c_gray_bar(),
        0.5,
    );
}

/// Paginate TDR rows like PdfSharp `AddTDRTableWrapper` (break at y >= 700, new page at y=190).
fn render_tdr_pages(
    td: &crate::customer::TermDepositDetail,
    writer: &mut PdfWriter,
    nav: NavTargets,
    is_last_tdr: bool,
    grand_tdr_amount: f64,
    grand_tdr_certificates: usize,
) -> Result<()> {
    const TOTAL_BOX_H: f32 = 36.0; // 2 rows × 18
    const GRAND_BOX_H: f32 = 36.0;

    let cols = [
        "Certificate No",
        "Profit Option",
        "Start Date",
        "Maturity Date",
        "Tenure",
        "Currency",
        "FCY Balance",
        "Amount",
    ];

    let w = 56.0;
    let widths = [w; 8];

    let mut idx = 0usize;
    let grand_box_y = TDR_PAGE_BOTTOM - GRAND_BOX_H - GRAND_BOX_MARGIN_BOTTOM;

    while idx < td.certificates.len() {
        let mut b = ContentBuilder::new();

        draw_header(
            &mut b,
            HeaderConfig {
                page_type: PageType::TermDeposit,
            },
        );
        draw_sidebar(&mut b, false, false);
        vline(
            &mut b,
            120.0, // adjust position
            135.0,                          // start Y
            PAGE_H - 40.0,                 // line height
            c_gray_border(),
            0.5,
        );
        draw_tdr_header(&mut b, td);

        let mut y = TDR_TABLE_TOP;

        draw_mbl_table_header(&mut b, CX, y, &cols, &widths, None);
        y += HDR_H;

        let cert_type = &td.certificates[idx].cert_type_label;
        draw_tdr_section_banner(&mut b, td, cert_type, y);
        y += TDR_SECTION_BANNER_H;

        while idx < td.certificates.len() {
            if idx > 0
                && td.certificates[idx].cert_type_label
                    != td.certificates[idx - 1].cert_type_label
            {
                let need = TDR_SECTION_BANNER_H + ROW_H;

                if y + need > TDR_PAGE_BOTTOM {
                    break;
                }

                draw_tdr_section_banner(
                    &mut b,
                    td,
                    &td.certificates[idx].cert_type_label,
                    y,
                );

                y += TDR_SECTION_BANNER_H;
            }

            if y + ROW_H > TDR_PAGE_BOTTOM {
                break;
            }

            let cert = &td.certificates[idx];

            draw_mbl_data_row(
                &mut b,
                CX,
                y,
                &[
                    cert.certificate_no.as_str(),
                    cert.profit_option.as_str(),
                    cert.start_date.as_str(),
                    cert.maturity_date.as_str(),
                    cert.tenure.as_str(),
                    cert.currency.as_str(),
                    cert.fcy_balance.as_str(),
                    cert.amount.as_str(),
                ],
                &widths,
                idx,
                None,
            );

            y += ROW_H;
            idx += 1;
        }

        let is_last_page_of_tdr = idx >= td.certificates.len();

        if is_last_page_of_tdr {
            draw_tdr_totals_box(&mut b, y, td);
            y += TOTAL_BOX_H;
        }

        if is_last_tdr && is_last_page_of_tdr {
            if y < grand_box_y - 10.0 {
                draw_grand_tdr_totals_box(
                    &mut b,
                    grand_box_y,
                    grand_tdr_amount,
                    grand_tdr_certificates,
                );

                write_content_page(writer, &b, None, nav,ReportType::Standard)?;
            } else {
                write_content_page(writer, &b, None, nav,ReportType::Standard)?;

                let mut b = ContentBuilder::new();
                draw_header(
                    &mut b,
                    HeaderConfig {
                        page_type: PageType::TermDeposit,
                    },
                );
                draw_sidebar(&mut b, false, false);
                vline(
                    &mut b,
                    120.0, // adjust position
                    135.0,                          // start Y
                    PAGE_H - 40.0,                 // line height
                    c_gray_border(),
                    0.5,
                );
                draw_tdr_header(&mut b, td);

                draw_grand_tdr_totals_box(
                    &mut b,
                    grand_box_y,
                    grand_tdr_amount,
                    grand_tdr_certificates,
                );

                write_content_page(writer, &b, None, nav,ReportType::Standard)?;
            }
        }else {
                write_content_page(writer, &b, None, nav,ReportType::Standard)?;
        }
    }

    Ok(())
}

fn draw_header(
    b: &mut ContentBuilder,
    config: HeaderConfig,
) {
    draw_text(
        b,
        127.0,
        78.0,
        config.page_type.title(),
        Font::Regular,
        26.0,
        c_black(),
        TextAlign::Center,
        300.0,
    );

    hline(
        b,
        20.0,
        115.0,
        PAGE_W - 40.0,
        c_gray_border(),
        0.5,
    );
}

fn draw_tdr_section_banner(
    b: &mut ContentBuilder,
    td: &crate::customer::TermDepositDetail,
    cert_type: &str,
    y: f32,
) {
    const BANNER_ROW_H: f32 = 20.0;
    filled_rect(b, CX, y, TABLE_W, BANNER_ROW_H, c_mbl_mint());
    draw_text(
        b,
        CX + 4.0,
        y + 14.0,
        &format!("{} - {}", td.title, td.cert_no),
        Font::Regular,
        10.0,
        c_black(),
        TextAlign::Left,
        TABLE_W,
    );

    hline(
        b,
        CX,
        y + BANNER_ROW_H,
        TABLE_W,
        c_black(),
        1.0,
    );

    filled_rect(b, CX, y + BANNER_ROW_H, TABLE_W, BANNER_ROW_H, c_mbl_mint());
    draw_text(
        b,
        CX + 4.0,
        y + 32.0,
        cert_type,
        Font::Regular,
        10.0,
        c_black(),
        TextAlign::Left,
        TABLE_W,
    );
}

fn draw_tdr_totals_box(
    b: &mut ContentBuilder,
    y: f32,
    td: &crate::customer::TermDepositDetail,
) {
    const BOX_H: f32 = 18.0;
    const LABEL_W: f32 = 250.0;

    let total_amount: f64 = td
        .certificates
        .iter()
        .map(|c| c.amount.replace(',', "").parse::<f64>().unwrap_or(0.0))
        .sum();

    let total_certs = td.certificates.len();

    let rows = [
        ("Total", fmt_money(total_amount)),
        ("Number Of Certificates", total_certs.to_string()),
    ];

    let total_height = BOX_H * rows.len() as f32;

    // Left border
    vline(b, CX, y, total_height, c_gray_bar(), 0.5);

    // Right border
    vline(b, CX + TABLE_W, y, total_height, c_gray_bar(), 0.5);


    let mut row_y = y;

    for (label, value) in rows {

        draw_text(
            b,
            CX + 4.0,
            row_y + 12.0,
            label,
            Font::Bold,
            10.0,
            c_black(),
            TextAlign::Left,
            LABEL_W - 8.0,
        );

        draw_text(
            b,
            CX + LABEL_W + 4.0,
            row_y + 12.0,
            &value,
            Font::Bold,
            10.0,
            c_black(),
            TextAlign::Right,
            TABLE_W - LABEL_W - 8.0,
        );

        row_y += BOX_H;
    }
    hline(
        b,
        CX,
        y + total_height,
        TABLE_W,
        c_gray_bar(),
        0.5,
    );
}

fn draw_grand_tdr_totals_box(
    b: &mut ContentBuilder,
    y: f32,
    total_amount: f64,
    total_certificates: usize,
) {
    const BOX_H: f32 = 18.0;
    const LABEL_W: f32 = 250.0;

    let rows = [
        ("Grand Total", fmt_money(total_amount)),
        ("Number Of Certificates", total_certificates.to_string()),
    ];

    let total_height = BOX_H * rows.len() as f32;

    hline(b, CX, y, TABLE_W, c_gray_bar(), 0.5,);
    vline(b, CX, y, total_height, c_gray_bar(), 0.5);
    vline(b, CX + TABLE_W, y, total_height, c_gray_bar(), 0.5);

    let mut row_y = y;

    for (label, value) in rows {
        draw_text(
            b,
            CX + 4.0,
            row_y + 12.0,
            label,
            Font::Regular,
            10.0,
            c_black(),
            TextAlign::Left,
            LABEL_W,
        );

        draw_text(
            b,
            CX + LABEL_W,
            row_y + 12.0,
            &value,
            Font::Regular,
            10.0,
            c_black(),
            TextAlign::Right,
            TABLE_W - LABEL_W - 4.0,
        );

        row_y += BOX_H;
    }

    hline(
        b,
        CX,
        y + total_height,
        TABLE_W,
        c_gray_bar(),
        0.5,
    );
}

fn draw_tdr_header(b: &mut ContentBuilder, td: &crate::customer::TermDepositDetail) {
    draw_text(
        b,
        125.0,
        150.0,
        "Title",
        Font::Bold,
        LABEL_SIZE,
        c_black(),
        TextAlign::Left,
        60.0,
    );
    draw_text(
        b,
        125.0,
        170.0,
        &td.title,
        Font::Regular,
        LABEL_SIZE,
        c_black(),
        TextAlign::Left,
        250.0,
    );
    draw_text(
        b,
        240.0,
        150.0,
        "As Of Position",
        Font::Bold,
        LABEL_SIZE,
        c_black(),
        TextAlign::Left,
        100.0,
    );
    draw_text(
        b,
        240.0,
        170.0,
        &td.as_of_date,
        Font::Regular,
        LABEL_SIZE,
        c_black(),
        TextAlign::Left,
        120.0,
    );
}

fn draw_mbl_table_header(
    b: &mut ContentBuilder,
    x: f32,
    y_top: f32,
    headers: &[&str],
    widths: &[f32],
    tx_w: Option<f32>,
) {
    let tx_w = tx_w.unwrap_or(TABLE_W);
    filled_rect(b, x, y_top, tx_w, HDR_H, c_purple());

    const CELL_PADDING: f32 = 5.0;
    const RIGHT_PADDING: f32 = 18.0;

    // Match row alignment
    let aligns = [
        TextAlign::Left,
        TextAlign::Left,
        TextAlign::Left,
        TextAlign::Left,
        TextAlign::Right,
        TextAlign::Right,
        TextAlign::Right,
        TextAlign::Right,  // Amount
    ];

    let mut cx = x;

    for (((hdr, &w), align), _) in headers
        .iter()
        .zip(widths.iter())
        .zip(aligns.iter())
        .zip(0..)
    {
        let effective_width = match align {
            TextAlign::Right => w - RIGHT_PADDING,
            _ => w - (CELL_PADDING * 2.0),
        };

        draw_text(
            b,
            cx + CELL_PADDING,
            y_top + 12.0,
            hdr,
            Font::Bold,
            TABLE_SIZE,
            c_white(),
            *align,
            effective_width,
        );

        vline(b, cx, y_top, HDR_H, c_dark_gray(), 0.5);

        cx += w;
    }

    // Right border
    vline(b, x + tx_w, y_top, HDR_H, c_dark_gray(), 0.5);

    // Top and bottom border
    hline(b, x, y_top, tx_w, c_dark_gray(), 0.5);
    hline(b, x, y_top + HDR_H, tx_w, c_dark_gray(), 0.5);
}

fn draw_mbl_data_row(
    b: &mut ContentBuilder,
    x: f32,
    y_top: f32,
    cells: &[&str],
    widths: &[f32],
    row_idx: usize,
    tx_w: Option<f32>,
) {
     let tx_w = tx_w.unwrap_or(TABLE_W);
    // Explicit alignment per column
    let aligns = [
        TextAlign::Left,   // Date
        TextAlign::Left,   // Value Date
        TextAlign::Left,   // Doc No
        TextAlign::Left,   // Particular
        TextAlign::Right,  // Debit
        TextAlign::Right,  // Credit
        TextAlign::Right,  // Balance
        TextAlign::Right,  // Amount
    ];

    let is_total = cells.iter().any(|v| v.trim().eq_ignore_ascii_case("Total"));

    let font = if is_total {
        Font::Bold
    } else {
        Font::Regular
    };

    const CELL_PADDING: f32 = 5.0;

    let mut cx = x;

    for (((cell, &w), align), idx) in cells
        .iter()
        .zip(widths.iter())
        .zip(aligns.iter())
        .zip(0..)
    {
        let effective_width = match align {
            TextAlign::Right => w - 18.0, // reduce right alignment area
            _ => w - (CELL_PADDING * 2.0),
        };

        draw_text(
            b,
            cx + CELL_PADDING,
            y_top + 10.0,
            cell,
            font,
            TABLE_SIZE,
            c_black(),
            *align,
            effective_width,
        );

        cx += w;
    }

    // Left border
    vline(b, x, y_top, ROW_H, c_gray_bar(), 0.5);
    // Right border
    vline(b, x + tx_w, y_top, ROW_H, c_gray_bar(), 0.5);

    // Bottom border
    hline(b, x, y_top + ROW_H, tx_w, c_gray_bar(), 0.5);
}

fn draw_mbl_table(
    b: &mut ContentBuilder,
    x: f32,
    y_top: f32,
    headers: &[&str],
    widths: &[f32],
    rows: &[Vec<&str>],
) -> f32 {
    draw_mbl_table_header(b, x, y_top, headers, widths, None);
    let mut y = y_top + HDR_H;
    for (i, row) in rows.iter().enumerate() {
        draw_mbl_data_row(b, x, y, row, widths, i, None);
        y += ROW_H;
    }
    y
}
