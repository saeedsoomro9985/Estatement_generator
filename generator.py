"""
Statement PDF generator — fpdf2 (LGPL, free for commercial use)
Avanza Solutions Pvt. Ltd. branded template — with internal PDF links
"""
from fpdf import FPDF
import time, json, sys, os, math, argparse
from multiprocessing import Pool, cpu_count

PAGE_W, PAGE_H = 595.28, 841.89   # A4 in points

# ── Avanza Solutions brand palette ───────────────────────────────────────────
AV_NAVY    = '#1E3A5F'
AV_DNAVY   = '#152B47'
AV_BLUE    = '#0052CC'
AV_TEAL    = '#00B4A0'
AV_GOLD    = '#F0A500'
AV_LTBLUE  = '#EAF2FF'
AV_SIDEBAR = '#EEF3FA'
AV_LBLUE   = '#7FB3D3'
AV_DGRAY   = '#2C3E50'
AV_MGRAY   = '#5A6A7A'

HEADER_H   = 90
FOOTER_H   = 44
SIDEBAR_W  = 92
CX         = 107          # Content start X
CONTENT_TOP = HEADER_H + 8
CONTENT_BOTTOM_MARGIN = FOOTER_H + 10

# ── Pre-parsed colour cache ───────────────────────────────────────────────────
_RGB_CACHE: dict = {}

def _rgb(h: str):
    if h not in _RGB_CACHE:
        h2 = h.lstrip('#')
        _RGB_CACHE[h] = (int(h2[0:2], 16), int(h2[2:4], 16), int(h2[4:6], 16))
    return _RGB_CACHE[h]


# ── Stateful PDF wrapper ──────────────────────────────────────────────────────

class _PDF(FPDF):
    def __init__(self):
        super().__init__(unit='pt', format='A4')
        self.set_auto_page_break(False)
        self._f:  tuple | None = None
        self._fc: tuple | None = None
        self._tc: tuple | None = None
        self._lw: float | None = None
        self._dc: tuple | None = None

    def font(self, family: str, style: str, size: float):
        k = (family, style, size)
        if self._f != k:
            self.set_font(family, style, size)
            self._f = k

    def fill(self, hex_color: str):
        rgb = _rgb(hex_color)
        if self._fc != rgb:
            self.set_fill_color(*rgb)
            self._fc = rgb

    def ink(self, hex_color: str):
        rgb = _rgb(hex_color)
        if self._tc != rgb:
            self.set_text_color(*rgb)
            self._tc = rgb

    def pen(self, hex_color: str, width: float = 0.5):
        rgb = _rgb(hex_color)
        if self._dc != rgb:
            self.set_draw_color(*rgb)
            self._dc = rgb
        if self._lw != width:
            self.set_line_width(width)
            self._lw = width

    def box(self, x: float, y: float, w: float, h: float):
        self.rect(x, y, w, h, style='F')

    def txt(self, x: float, y: float, text: str,
            w: float = 0, align: str = 'L', line_h: float = 0):
        lh = line_h or self.font_size_pt
        self.set_xy(x, y)
        self.cell(w or PAGE_W - x, lh, str(text), align=align)

    def hline(self, x: float, y: float, length: float,
              color: str, width: float = 0.5):
        self.pen(color, width)
        self.line(x, y, x + length, y)


# ── Avanza diamond logo mark ─────────────────────────────────────────────────

def _draw_diamond_logo(pdf: _PDF, x: float, y: float, size: float = 34):
    half = size / 2
    pdf.fill(AV_TEAL)
    pdf.polygon(
        [(x + half, y), (x + size, y + half),
         (x + half, y + size), (x,        y + half)],
        style='F',
    )
    inner = size * 0.38
    ix = x + half - inner / 2
    iy = y + half - inner / 2
    ih = inner / 2
    pdf.fill('#008C7A')
    pdf.polygon(
        [(ix + ih, iy), (ix + inner, iy + ih),
         (ix + ih, iy + inner), (ix,          iy + ih)],
        style='F',
    )
    pdf.font('Helvetica', 'B', int(size * 0.40))
    pdf.ink('#FFFFFF')
    pdf.set_xy(x, y + size * 0.28)
    pdf.cell(size, size * 0.45, 'A', align='C')


# ── Decorative financial bar-chart "header image" ────────────────────────────

def _draw_header_image(pdf: _PDF, x: float, y: float, w: float, h: float):
    pdf.fill(AV_DNAVY); pdf.box(x, y, w, h)
    stripes = ['#1A3355', '#19304F', '#172E4C']
    sw = w / len(stripes)
    for i, c in enumerate(stripes):
        pdf.fill(c); pdf.box(x + i * sw, y, sw, h)

    bars = [
        (0.04, 0.38, '#2E6DB4'), (0.13, 0.55, '#3A8FD4'),
        (0.22, 0.72, AV_TEAL),   (0.31, 0.50, '#3A8FD4'),
        (0.40, 0.83, AV_TEAL),   (0.49, 0.65, '#5AB0E8'),
        (0.58, 0.90, AV_TEAL),   (0.67, 0.75, '#5AB0E8'),
        (0.76, 0.95, '#7DC8F7'),
    ]
    bar_w    = w * 0.075
    usable_h = h * 0.76
    for bx_f, bh_f, color in bars:
        bh = usable_h * bh_f
        bx = x + w * bx_f
        by = y + h - bh - 6
        pdf.fill(color); pdf.box(bx, by, bar_w, bh)
        pdf.fill('#FFFFFF'); pdf.box(bx, by, bar_w, 1.5)

    pts = [
        (x + w * 0.04 + bar_w / 2, y + h - usable_h * 0.38 - 6),
        (x + w * 0.22 + bar_w / 2, y + h - usable_h * 0.72 - 6),
        (x + w * 0.40 + bar_w / 2, y + h - usable_h * 0.83 - 6),
        (x + w * 0.58 + bar_w / 2, y + h - usable_h * 0.90 - 6),
        (x + w * 0.76 + bar_w / 2, y + h - usable_h * 0.95 - 6),
    ]
    pdf.pen(AV_GOLD, 1.8)
    for i in range(len(pts) - 1):
        pdf.line(pts[i][0], pts[i][1], pts[i + 1][0], pts[i + 1][1])
    lx, ly = pts[-1]
    pdf.set_fill_color(*_rgb(AV_GOLD))
    pdf._fc = None   # invalidate cache after manual set
    pdf.ellipse(lx - 3, ly - 3, 6, 6, style='F')

    pdf.font('Helvetica', 'B', 5.5); pdf.ink(AV_LBLUE)
    pdf.txt(x + 2, y + h - 8, 'FINANCIAL ANALYTICS', w=w - 4, align='C')


# ── Page header ───────────────────────────────────────────────────────────────

def _draw_page_header(pdf: _PDF, customer_name: str = '', period: str = ''):
    pdf.fill(AV_NAVY);  pdf.box(0, 0, PAGE_W, HEADER_H)
    logo_panel_w = 195
    pdf.fill(AV_DNAVY); pdf.box(0, 0, logo_panel_w, HEADER_H)

    _draw_diamond_logo(pdf, 10, (HEADER_H - 34) / 2, 34)

    pdf.font('Helvetica', 'B', 13.5); pdf.ink('#FFFFFF')
    pdf.txt(53, 20, 'AVANZA SOLUTIONS', w=140)
    pdf.font('Helvetica', 'B', 8);    pdf.ink(AV_TEAL)
    pdf.txt(53, 36, 'PVT. LTD.', w=140)
    pdf.font('Helvetica', '', 6);     pdf.ink(AV_LBLUE)
    pdf.txt(53, 48, 'www.avanzasolutions.com', w=140)
    pdf.txt(53, 57, 'info@avanzasolutions.com', w=140)

    pdf.fill(AV_TEAL); pdf.box(logo_panel_w, 0, 3, HEADER_H)

    mid_x = logo_panel_w + 10
    mid_w = 178
    pdf.font('Helvetica', 'B', 17); pdf.ink('#FFFFFF')
    pdf.txt(mid_x, 16, 'BANK STATEMENT', w=mid_w, align='C')
    pdf.hline(mid_x + 10, 37, mid_w - 20, AV_TEAL, 0.8)

    if customer_name:
        pdf.font('Helvetica', 'B', 8); pdf.ink('#FFFFFF')
        pdf.txt(mid_x, 41, customer_name, w=mid_w, align='C')
    if period:
        pdf.font('Helvetica', '', 6.5); pdf.ink(AV_LBLUE)
        pdf.txt(mid_x, 54, period, w=mid_w, align='C')

    img_x = logo_panel_w + 3 + mid_w + 12
    img_w = PAGE_W - img_x - 2
    _draw_header_image(pdf, img_x, 4, img_w, HEADER_H - 8)

    pdf.fill(AV_TEAL); pdf.box(0, HEADER_H - 3, PAGE_W, 3)


# ── Page footer ───────────────────────────────────────────────────────────────

def _draw_social_icon(pdf: _PDF, x: float, y: float,
                      label: str, bg: str, w: float = 26, h: float = 19):
    pdf.fill(bg);      pdf.box(x, y, w, h)
    pdf.fill('#FFFFFF'); pdf.box(x, y, w, 2)
    pdf.fill(bg);      pdf.box(x, y + 2, w, h - 2)
    pdf.font('Helvetica', 'B', 7); pdf.ink('#FFFFFF')
    pdf.set_xy(x, y + 5); pdf.cell(w, 9, label, align='C')


def _draw_page_footer(pdf: _PDF):
    y = PAGE_H - FOOTER_H
    pdf.fill(AV_NAVY);  pdf.box(0, y, PAGE_W, FOOTER_H)
    pdf.fill(AV_TEAL);  pdf.box(0, y, PAGE_W, 2)

    pdf.font('Helvetica', 'B', 7); pdf.ink('#FFFFFF')
    pdf.txt(12, y + 8,  'AVANZA SOLUTIONS PVT. LTD.', w=240)
    pdf.font('Helvetica', '', 6);  pdf.ink(AV_LBLUE)
    pdf.txt(12, y + 19, 'Karachi, Pakistan  |  Tel: +92-21-111-282-692', w=240)
    pdf.txt(12, y + 29, 'This is a system-generated statement. No signature required.', w=280)

    pdf.font('Helvetica', '', 6.5); pdf.ink(AV_LBLUE)
    pdf.txt(0, y + 19, f'Page {pdf.page_no()}', w=PAGE_W - 5, align='R')

    social = [('f', '#1877F2'), ('in', '#0A66C2'),
               ('tw', '#1DA1F2'), ('yt', '#FF0000'), ('wa', '#25D366')]
    xi = PAGE_W - len(social) * 30 - 8
    for label, color in social:
        _draw_social_icon(pdf, xi, y + 11, label, color)
        xi += 30


# ── Sidebar with internal links ───────────────────────────────────────────────

def _draw_sidebar(pdf: _PDF, data: dict, nav_links: dict):
    """Draw Avanza-branded sidebar and register clickable link annotations."""
    sb_y = HEADER_H
    sb_h = PAGE_H - HEADER_H - FOOTER_H

    pdf.fill(AV_SIDEBAR); pdf.box(0, sb_y, SIDEBAR_W, sb_h)
    pdf.fill(AV_TEAL);    pdf.box(0, sb_y, 3, sb_h)

    # Section heading
    pdf.font('Helvetica', 'B', 6.5); pdf.ink(AV_NAVY)
    pdf.txt(8, sb_y + 12, 'NAVIGATE', w=SIDEBAR_W - 8)
    pdf.hline(8, sb_y + 23, SIDEBAR_W - 16, AV_TEAL, 0.5)

    # Summary link
    ny = sb_y + 30
    pdf.font('Helvetica', 'B', 6.5); pdf.ink(AV_BLUE)
    pdf.txt(10, ny, '> Summary', w=SIDEBAR_W - 10)
    if nav_links.get('summary') is not None:
        pdf.link(3, ny - 2, SIDEBAR_W - 3, 11, nav_links['summary'])
    ny += 13

    # Accounts group
    pdf.font('Helvetica', 'B', 6); pdf.ink(AV_MGRAY)
    pdf.txt(8, ny, 'ACCOUNTS', w=SIDEBAR_W - 8)
    ny += 10

    pdf.font('Helvetica', '', 6); pdf.ink(AV_BLUE)
    acc_links = nav_links.get('accounts', [])
    for i, acc in enumerate(data.get('accounts', [])[:8]):
        num = acc['accountNumber']
        short = ('...' + num[-9:]) if len(num) > 9 else num
        pdf.txt(10, ny, f"> {short}", w=SIDEBAR_W - 10)
        if i < len(acc_links):
            pdf.link(3, ny - 2, SIDEBAR_W - 3, 11, acc_links[i])
        ny += 10

    # TDR group
    ny += 4
    pdf.font('Helvetica', 'B', 6); pdf.ink(AV_MGRAY)
    pdf.txt(8, ny, 'TDR', w=SIDEBAR_W - 8)
    ny += 10

    pdf.font('Helvetica', '', 6); pdf.ink(AV_BLUE)
    tdr_links = nav_links.get('tdr', [])
    for i, t in enumerate(data.get('tdr', [])[:5]):
        num = t['tdrNumber']
        short = ('...' + num[-9:]) if len(num) > 9 else num
        pdf.txt(10, ny, f"> {short}", w=SIDEBAR_W - 10)
        if i < len(tdr_links):
            pdf.link(3, ny - 2, SIDEBAR_W - 3, 11, tdr_links[i])
        ny += 10

    # Wordmark
    pdf.font('Helvetica', 'B', 6); pdf.ink(AV_TEAL)
    pdf.txt(8, PAGE_H - FOOTER_H - 16, 'AVANZA', w=SIDEBAR_W - 8, align='C')


# ── Table column header (Avanza branded) ──────────────────────────────────────

def _draw_table_header(pdf: _PDF, y: float) -> float:
    col_w = PAGE_W - CX - 15
    pdf.fill(AV_NAVY); pdf.box(CX, y, col_w, 16)
    pdf.fill(AV_TEAL); pdf.box(CX, y, 3, 16)
    pdf.fill(AV_GOLD); pdf.box(CX + col_w - 3, y, 3, 16)
    pdf.font('Courier', 'B', 7); pdf.ink('#FFFFFF')
    hdr = f"{'Date':<12} {'Description':<33} {'Amount':>12} {'Balance':>12}"
    pdf.txt(CX + 5, y + 3.5, hdr, w=col_w - 10)
    return y + 16


# ── Core renderer ─────────────────────────────────────────────────────────────

def _render_pdf(data: dict) -> bytes:
    pdf = _PDF()
    col_w = PAGE_W - CX - 15
    content_bottom = PAGE_H - CONTENT_BOTTOM_MARGIN

    # Pre-create all internal link IDs and initialise with a placeholder
    # destination (page=1) so they pass fpdf2's validity check before the
    # actual target pages are created.  set_link() is called again when we
    # add_page() for each section, which updates the page to the real one.
    def _make_link() -> int:
        lid = pdf.add_link()
        pdf.set_link(lid, y=0, page=1)   # placeholder — will be updated
        return lid

    summary_link  = _make_link()
    account_links = [_make_link() for _ in data['accounts']]
    tdr_links     = [_make_link() for _ in data['tdr']]
    nav_links = {'summary': summary_link,
                 'accounts': account_links, 'tdr': tdr_links}

    period_str = ''
    p = data.get('period', {})
    if p and (p.get('from') or p.get('to')):
        period_str = f"Period: {p.get('from','')} to {p.get('to','')}"
    elif data.get('id'):
        period_str = f"Customer ID: {data['id']}"

    def page_chrome():
        _draw_page_header(pdf, data.get('name', ''), period_str)
        _draw_page_footer(pdf)
        _draw_sidebar(pdf, data, nav_links)

    # ── Optimised table renderer ──────────────────────────────────────
    # Strip optimisation: instead of N fill+box per row, draw ONE
    # continuous teal strip over the full section height after all rows.

    def render_table(transactions: list, start_y: float):
        y = _draw_table_header(pdf, start_y)
        section_top = y
        pdf.font('Courier', '', 6.8); pdf.ink(AV_DGRAY)

        # Pre-format all row strings — avoids repeated f-string work inside the
        # draw loop and keeps the hot path as tight as possible.
        rows = [
            f"{tx['date']:<12} {tx['description'][:32]:<33}"
            f" {tx['amount']:>12.2f} {tx['balance']:>12.2f}"
            for tx in transactions
        ]

        for i, row in enumerate(rows):
            if y + 12 > content_bottom:
                pdf.fill(AV_TEAL); pdf.box(CX, section_top, 3, y - section_top)
                pdf.fill(AV_NAVY); pdf.box(CX, y, col_w, 2)
                pdf.add_page(); page_chrome()
                y = _draw_table_header(pdf, CONTENT_TOP)
                section_top = y
                pdf.font('Courier', '', 6.8); pdf.ink(AV_DGRAY)

            # Skip drawing white rows — page background is already white.
            # Teal strip is applied once per section after the loop.
            if i % 2 == 0:
                pdf.fill(AV_LTBLUE); pdf.box(CX, y, col_w, 12)

            pdf.txt(CX + 5, y + 2.5, row, w=col_w - 10)
            y += 12

        pdf.fill(AV_TEAL);  pdf.box(CX, section_top, 3, y - section_top)
        pdf.fill(AV_NAVY);  pdf.box(CX, y, col_w, 2)

    # ── Summary table helpers ─────────────────────────────────────────

    def tbl_header(y: float, col1: str, col2: str) -> float:
        pdf.fill(AV_NAVY); pdf.box(CX, y, col_w, 17)
        pdf.fill(AV_TEAL); pdf.box(CX, y, 3, 17)
        pdf.fill(AV_GOLD); pdf.box(CX + col_w - 3, y, 3, 17)
        pdf.font('Helvetica', 'B', 7.5); pdf.ink('#FFFFFF')
        pdf.txt(CX + 8, y + 4.5, col1, w=220)
        pdf.txt(CX + 230, y + 4.5, col2, w=col_w - 245, align='R')
        return y + 17

    def tbl_row(y: float, col1: str, col2: str, idx: int,
                link_id: int | None = None) -> float:
        is_alt = idx % 2 == 0
        if is_alt:
            pdf.fill(AV_LTBLUE); pdf.box(CX, y, col_w, 17)
        pdf.fill(AV_TEAL if is_alt else '#C5DCEF')
        pdf.box(CX, y, 3, 17)
        pdf.font('Helvetica', '', 8); pdf.ink(AV_BLUE)
        pdf.txt(CX + 8, y + 4.5, col1, w=220)
        pdf.font('Helvetica', 'B', 8); pdf.ink(AV_DGRAY)
        pdf.txt(CX + 230, y + 4.5, col2, w=col_w - 245, align='R')
        if link_id is not None:
            pdf.link(CX, y, col_w, 17, link_id)
        return y + 17

    def section_title(y: float, title: str) -> float:
        pdf.fill(AV_NAVY); pdf.box(CX, y, 4, 14)
        pdf.font('Helvetica', 'B', 9.5); pdf.ink(AV_NAVY)
        pdf.txt(CX + 10, y, title, w=300, line_h=14)
        return y + 18

    # ── Page 1 — Summary ──────────────────────────────────────────────
    pdf.add_page()
    pdf.set_link(summary_link)   # destination for sidebar "Summary" link
    page_chrome()

    y = CONTENT_TOP

    # Customer card
    pdf.fill(AV_LTBLUE); pdf.box(CX, y, col_w, 40)
    pdf.fill(AV_TEAL);   pdf.box(CX, y, 4, 40)
    pdf.fill(AV_GOLD);   pdf.box(CX + col_w - 4, y, 4, 40)
    pdf.font('Helvetica', 'B', 12); pdf.ink(AV_NAVY)
    pdf.txt(CX + 10, y + 6, 'Statement Summary', w=col_w - 20)
    pdf.font('Helvetica', '', 8.5); pdf.ink(AV_DGRAY)
    pdf.txt(CX + 10, y + 21, data.get('name', ''), w=col_w - 20)
    pdf.font('Helvetica', '', 7);   pdf.ink(AV_MGRAY)
    pdf.txt(CX + 10, y + 31, data.get('address', ''), w=col_w - 20)
    y += 52

    # Accounts table
    y = section_title(y, 'Accounts')
    y = tbl_header(y, 'Account Number', 'Balance (PKR)')
    for i, acc in enumerate(data['accounts']):
        y = tbl_row(y, acc['accountNumber'],
                    f"{acc['balance']:,.2f}", i, account_links[i])
    pdf.fill(AV_NAVY); pdf.box(CX, y, col_w, 2)
    y += 22

    # TDR table
    y = section_title(y, 'Term Deposits (TDR)')
    y = tbl_header(y, 'TDR Number', 'Principal (PKR)')
    for i, t in enumerate(data['tdr']):
        y = tbl_row(y, t['tdrNumber'],
                    f"{t.get('principalAmount', 0):,.2f}", i, tdr_links[i])
    pdf.fill(AV_NAVY); pdf.box(CX, y, col_w, 2)

    # ── Account detail pages ──────────────────────────────────────────
    for i, acc in enumerate(data['accounts']):
        pdf.add_page()
        pdf.set_link(account_links[i])   # destination
        page_chrome()

        y = CONTENT_TOP
        pdf.fill(AV_LTBLUE); pdf.box(CX, y, col_w, 32)
        pdf.fill(AV_TEAL);   pdf.box(CX, y, 4, 32)
        pdf.font('Helvetica', 'B', 10.5); pdf.ink(AV_NAVY)
        pdf.txt(CX + 10, y + 5, f"Account: {acc['accountNumber']}", w=col_w - 20)
        pdf.font('Helvetica', '', 7.5); pdf.ink(AV_MGRAY)
        pdf.txt(CX + 10, y + 19,
                f"Type: {acc.get('accountType','')}   |   "
                f"Balance: PKR {acc.get('balance', 0):,.2f}", w=col_w - 20)
        y += 42
        render_table(acc['transactions'], y)

    # ── TDR detail pages ─────────────────────────────────────────────
    for i, t in enumerate(data['tdr']):
        pdf.add_page()
        pdf.set_link(tdr_links[i])   # destination
        page_chrome()

        y = CONTENT_TOP
        pdf.fill(AV_LTBLUE); pdf.box(CX, y, col_w, 32)
        pdf.fill(AV_GOLD);   pdf.box(CX, y, 4, 32)
        pdf.font('Helvetica', 'B', 10.5); pdf.ink(AV_NAVY)
        pdf.txt(CX + 10, y + 5, f"Term Deposit: {t['tdrNumber']}", w=col_w - 20)
        pdf.font('Helvetica', '', 7.5); pdf.ink(AV_MGRAY)
        pdf.txt(CX + 10, y + 19,
                f"Rate: {t.get('interestRate',0)}%   |   "
                f"Maturity: {t.get('maturityDate','')}   |   "
                f"Principal: PKR {t.get('principalAmount',0):,.2f}", w=col_w - 20)
        y += 42
        render_table(t['transactions'], y)

    return bytes(pdf.output())


# ── Backward-compat JSON-string wrapper ───────────────────────────────────────

def generate_pdf(data_json: str) -> bytes:
    return _render_pdf(json.loads(data_json))


# ── Raw JSON → internal dict ──────────────────────────────────────────────────

def map_statement(record: dict) -> dict:
    summary_accs = {s['accountNo']: s for s in
                    record.get('summary', {}).get('accounts', [])}
    summary_tds  = {s['certNo']: s for s in
                    record.get('summary', {}).get('termDeposits', [])}

    accounts = []
    for acc in record.get('accounts', []):
        s = summary_accs.get(acc['accountNo'], {})
        accounts.append({
            'accountNumber': acc['accountNo'],
            'accountType':   acc.get('accountType', ''),
            'balance':       float(s.get('closingBalance', 0)),
            'transactions': [
                {
                    'date':        tx['transactionDate'],
                    'description': tx['transactionDetails'],
                    'amount':      (float(tx['debitAmountLc'])
                                   if float(tx['debitAmountLc']) > 0
                                   else float(tx['creditAmountLc'])),
                    'balance':     float(tx['balance']),
                }
                for tx in acc.get('transactions', [])
            ],
        })

    tdr = []
    for td in record.get('termDeposits', []):
        s = summary_tds.get(td['certNo'], {})
        first_tx = td['tdrTransactions'][0] if td.get('tdrTransactions') else {}
        tdr.append({
            'tdrNumber':       td['certNo'],
            'principalAmount': float(s.get('openingBalance', 0)),
            'interestRate':    0,
            'maturityDate':    first_tx.get('maturity', ''),
            'transactions': [
                {
                    'date':        tx['startDate'],
                    'description': (f"{tx.get('certificateType','')} - "
                                   f"{tx.get('tenure','')} - "
                                   f"{tx.get('profitOption','')}"),
                    'amount':      float(tx['rupeesAmount']),
                    'balance':     float(tx['rupeesAmount']),
                }
                for tx in td.get('tdrTransactions', [])
            ],
        })

    cust = record.get('customer', {})
    meta = record.get('meta', {})
    return {
        'id':      cust.get('customerId', ''),
        'name':    cust.get('name', ''),
        'address': cust.get('address', ''),
        'period':  {'from': meta.get('fromDate', ''), 'to': meta.get('toDate', '')},
        'accounts': accounts,
        'tdr':      tdr,
    }


# ── Worker functions ──────────────────────────────────────────────────────────

def _worker_single(args):
    customer, output_dir = args
    pdf_bytes = _render_pdf(customer)
    with open(os.path.join(output_dir, f"PY-{customer['id']}.pdf"), 'wb') as f:
        f.write(pdf_bytes)
    return 1


def _worker_batch(args):
    chunk, output_dir = args
    for customer in chunk:
        pdf_bytes = _render_pdf(customer)
        with open(os.path.join(output_dir, f"PY-{customer['id']}.pdf"), 'wb') as f:
            f.write(pdf_bytes)
    return len(chunk)


# ── Entry point ───────────────────────────────────────────────────────────────

if __name__ == '__main__':
    parser = argparse.ArgumentParser(
        description='Statement PDF generator — fpdf2 / Avanza branding')
    parser.add_argument('count',        nargs='?', type=int, default=10)
    parser.add_argument('--file',       '-f', default=None)
    parser.add_argument('--output-dir', '-o', default='output')
    parser.add_argument('--workers',    '-w', type=int, default=None)
    parser.add_argument('--chunk-size', '-c', type=int, default=None)
    parser.add_argument('--mode', choices=['batch', 'single'], default='batch')
    args = parser.parse_args()

    if args.file:
        if not os.path.exists(args.file):
            print(f"ERROR: file not found: {args.file}", file=sys.stderr)
            sys.exit(1)
        with open(args.file, 'r', encoding='utf-8') as f:
            records = json.load(f)

        count      = min(args.count, len(records))
        workers    = args.workers or cpu_count()
        output_dir = args.output_dir
        os.makedirs(output_dir, exist_ok=True)
        customers  = [map_statement(r) for r in records[:count]]

        if args.mode == 'batch':
            # Use workers*2 chunks so the pool stays busy if any chunk finishes
            # early — better load balancing than one chunk per worker.
            chunk_size = args.chunk_size or max(1, math.ceil(count / (workers * 2)))
            chunks     = [customers[i:i + chunk_size]
                          for i in range(0, count, chunk_size)]
            work_args  = [(ch, output_dir) for ch in chunks]
            print(f"[batch] {count} PDFs | {workers} workers | chunk={chunk_size}",
                  file=sys.stderr)
            start = time.time()
            with Pool(processes=min(workers, len(chunks)),
                      maxtasksperchild=4) as pool:
                results = pool.map(_worker_batch, work_args)
        else:
            work_args = [(c, output_dir) for c in customers]
            print(f"[single] {count} PDFs | {workers} workers", file=sys.stderr)
            start = time.time()
            with Pool(processes=workers, maxtasksperchild=16) as pool:
                results = pool.map(_worker_single, work_args)

        total    = sum(results)
        duration = time.time() - start
        chunk_size_out = (chunk_size if args.mode == 'batch'
                          else math.ceil(count / workers))
        result = {
            'generated':  total,
            'duration':   round(duration, 3),
            'tps':        round(total / duration, 2),
            'workers':    workers,
            'mode':       args.mode,
            'chunk_size': chunk_size_out,
        }
        print(json.dumps(result))
        print(f"Done  {total} PDFs | {duration:.2f}s | {total/duration:.2f} PDF/sec | "
              f"mode={args.mode} workers={workers}", file=sys.stderr)
        sys.exit(0)

    if not sys.stdin.isatty():
        raw = sys.stdin.read()
        if raw:
            sys.stdout.buffer.write(generate_pdf(raw))
        sys.exit(0)

    # Smoke test
    sample = {
        'id':      'TEST-001',
        'name':    'Test Customer',
        'address': 'Avanza Solutions, Karachi, Pakistan',
        'period':  {'from': '01-Jan-2024', 'to': '31-Mar-2024'},
        'accounts': [{
            'accountNumber': 'PK36HABB000000001234',
            'accountType':   'CURRENT',
            'balance':       1_250_000.00,
            'transactions': [
                {'date': f'2024-01-{i+1:02d}', 'description': f'Transaction {i+1}',
                 'amount': 1000.0 + i * 50, 'balance': 1_250_000.0 - i * 1000}
                for i in range(30)
            ],
        }],
        'tdr': [{
            'tdrNumber':       'TD-000001',
            'principalAmount': 500_000.0,
            'interestRate':    7.5,
            'maturityDate':    '01-Jan-2025',
            'transactions': [
                {'date': '2024-01-01',
                 'description': 'Fixed Deposit - 1 Year - Monthly Profit',
                 'amount': 500_000.0, 'balance': 500_000.0}
            ],
        }],
    }
    os.makedirs(args.output_dir, exist_ok=True)
    start = time.time()
    for _ in range(args.count):
        pdf_bytes = _render_pdf(sample)
    t = time.time() - start
    print(f"{args.count} PDFs in {t:.2f}s | {args.count/t:.2f} PDF/sec", file=sys.stderr)
    out_path = os.path.join(args.output_dir, 'SAMPLE-avanza.pdf')
    with open(out_path, 'wb') as f:
        f.write(pdf_bytes)
    print(f"Sample saved: {out_path}", file=sys.stderr)
