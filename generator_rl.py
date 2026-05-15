"""
Statement PDF generator — ReportLab (BSD open-source, C-extension accelerated)
Avanza Solutions Pvt. Ltd. branded template — identical layout to generator.py.

Install:  pip install reportlab
The _rl_accel C extension is included in the standard reportlab wheel and is
loaded automatically; no extra step needed.

Commercial licensing note
─────────────────────────
The open-source 'reportlab' package on PyPI is BSD-licensed and free for any
use including commercial.  ReportLab PLUS (additional features: SVG, barcode,
advanced charts) is a separate paid product — it is NOT required here.
Contact sales@reportlab.com if you later need PLUS features; pricing is quoted
individually and financial-services customers are billed at double standard rates.
"""

from reportlab.pdfgen import canvas as rl_canvas
from reportlab.lib.colors import HexColor
import io, time, json, sys, os, math, argparse
from multiprocessing import Pool, cpu_count

PAGE_W, PAGE_H = 595.28, 841.89   # A4 in points

# ── Brand palette (pre-built HexColor objects — avoids per-call parsing) ──────
C_NAVY    = HexColor('#1E3A5F')
C_DNAVY   = HexColor('#152B47')
C_BLUE    = HexColor('#0052CC')
C_TEAL    = HexColor('#00B4A0')
C_TEAL2   = HexColor('#008C7A')
C_GOLD    = HexColor('#F0A500')
C_LTBLUE  = HexColor('#EAF2FF')
C_SIDEBAR = HexColor('#EEF3FA')
C_LBLUE   = HexColor('#7FB3D3')
C_DGRAY   = HexColor('#2C3E50')
C_MGRAY   = HexColor('#5A6A7A')
C_CDCEF   = HexColor('#C5DCEF')
C_WHITE   = HexColor('#FFFFFF')
C_3A8FD4  = HexColor('#3A8FD4')
C_2E6DB4  = HexColor('#2E6DB4')
C_5AB0E8  = HexColor('#5AB0E8')
C_7DC8F7  = HexColor('#7DC8F7')
C_FB      = HexColor('#1877F2')
C_LI      = HexColor('#0A66C2')
C_TW      = HexColor('#1DA1F2')
C_YT      = HexColor('#FF0000')
C_WA      = HexColor('#25D366')

HEADER_H  = 90
FOOTER_H  = 44
SIDEBAR_W = 92
CX        = 107
CONTENT_TOP           = HEADER_H + 8
CONTENT_BOTTOM_MARGIN = FOOTER_H + 10

# ── Coordinate helpers ────────────────────────────────────────────────────────

def _ry(y: float, h: float = 0) -> float:
    """Top-down y → ReportLab bottom-up y (bottom edge of a box of height h)."""
    return PAGE_H - y - h

def _ry_text(y: float) -> float:
    """Top-down y → ReportLab y for text baseline (approximate: y + font_size)."""
    return PAGE_H - y


# ── Stateful canvas wrapper ───────────────────────────────────────────────────

class _C:
    """Thin stateful wrapper around ReportLab Canvas — skips redundant calls."""
    __slots__ = ('c', '_fc', '_sc', '_lw', '_fn', '_fs')

    def __init__(self, buf: io.BytesIO):
        self.c  = rl_canvas.Canvas(buf, pagesize=(PAGE_W, PAGE_H))
        self.c.setPageCompression(1)
        self._fc = self._sc = self._fn = None
        self._fs = self._lw = None

    # ── State helpers ─────────────────────────────────────────────────
    def fill(self, color):
        if self._fc is not color:
            self.c.setFillColor(color); self._fc = color

    def stroke(self, color):
        if self._sc is not color:
            self.c.setStrokeColor(color); self._sc = color

    def lw(self, w: float):
        if self._lw != w:
            self.c.setLineWidth(w); self._lw = w

    def font(self, name: str, size: float):
        if self._fn != name or self._fs != size:
            self.c.setFont(name, size); self._fn = name; self._fs = size

    def _reset_state(self):
        self._fc = self._sc = self._fn = None
        self._fs = self._lw = None

    # ── Drawing primitives ────────────────────────────────────────────
    def box(self, x: float, y: float, w: float, h: float):
        """Filled rectangle — y is top-down."""
        self.c.rect(x, _ry(y, h), w, h, fill=1, stroke=0)

    def txt(self, x: float, y: float, s: str,
            align: str = 'L', w: float = 0):
        """Text — y is top-down; text is placed so its cap sits at y."""
        ry = _ry_text(y)
        if align == 'C' and w:
            self.c.drawCentredString(x + w / 2, ry, s)
        elif align == 'R' and w:
            self.c.drawRightString(x + w, ry, s)
        else:
            self.c.drawString(x, ry, s)

    def hline(self, x: float, y: float, length: float,
              color, width: float = 0.5):
        self.stroke(color); self.lw(width)
        ry = _ry_text(y)
        self.c.line(x, ry, x + length, ry)

    def poly(self, pts: list, color):
        """Filled polygon — pts are (x, y_top) tuples."""
        self.fill(color)
        p = self.c.beginPath()
        p.moveTo(pts[0][0], _ry_text(pts[0][1]))
        for px, py in pts[1:]:
            p.lineTo(px, _ry_text(py))
        p.close()
        self.c.drawPath(p, fill=1, stroke=0)

    def circle(self, cx: float, cy: float, r: float, color):
        self.fill(color)
        self.c.circle(cx, _ry_text(cy), r, fill=1, stroke=0)

    def show_page(self):
        self.c.showPage()
        self._reset_state()

    def save(self) -> bytes:
        self.c.save()
        return b''   # caller reads from the BytesIO buffer


# ── Form XObject: static chrome (drawn ONCE per PDF, referenced each page) ───

_STATIC_FORM = "avStaticChrome"

def _build_static_form(c: _C):
    """
    Register a Form XObject containing everything that is identical on every
    page for every customer: header background, Avanza logo, company text,
    the bar-chart image, footer background, social icons, sidebar background.

    Dynamic per-page content (customer name, period, page number, account
    links) is drawn as a thin overlay on top after doForm() is called.
    """
    c.c.beginForm(_STATIC_FORM)

    # ── Header background ──────────────────────────────────────────────
    c.fill(C_NAVY);  c.box(0, 0, PAGE_W, HEADER_H)
    logo_panel_w = 195
    c.fill(C_DNAVY); c.box(0, 0, logo_panel_w, HEADER_H)

    _draw_diamond_logo(c, 10, (HEADER_H - 34) / 2, 34)

    c.fill(C_WHITE)
    c.font('Helvetica-Bold', 13.5)
    c.txt(53, 20, 'AVANZA SOLUTIONS')
    c.fill(C_TEAL)
    c.font('Helvetica-Bold', 8)
    c.txt(53, 36, 'PVT. LTD.')
    c.fill(C_LBLUE)
    c.font('Helvetica', 6)
    c.txt(53, 48, 'www.avanzasolutions.com')
    c.txt(53, 57, 'info@avanzasolutions.com')

    c.fill(C_TEAL); c.box(logo_panel_w, 0, 3, HEADER_H)

    mid_x, mid_w = logo_panel_w + 10, 178
    c.fill(C_WHITE)
    c.font('Helvetica-Bold', 17)
    c.txt(mid_x, 16, 'BANK STATEMENT', align='C', w=mid_w)
    c.hline(mid_x + 10, 37, mid_w - 20, C_TEAL, 0.8)

    img_x = logo_panel_w + 3 + mid_w + 12
    _draw_header_image(c, img_x, 4, PAGE_W - img_x - 2, HEADER_H - 8)
    c.fill(C_TEAL); c.box(0, HEADER_H - 3, PAGE_W, 3)

    # ── Footer background ──────────────────────────────────────────────
    fy = PAGE_H - FOOTER_H
    c.fill(C_NAVY);  c.box(0, fy, PAGE_W, FOOTER_H)
    c.fill(C_TEAL);  c.box(0, fy, PAGE_W, 2)

    c.fill(C_WHITE)
    c.font('Helvetica-Bold', 7)
    c.txt(12, fy + 8,  'AVANZA SOLUTIONS PVT. LTD.')
    c.fill(C_LBLUE)
    c.font('Helvetica', 6)
    c.txt(12, fy + 19, 'Karachi, Pakistan  |  Tel: +92-21-111-282-692')
    c.txt(12, fy + 29, 'This is a system-generated statement. No signature required.')

    social = [('f', C_FB), ('in', C_LI), ('tw', C_TW), ('yt', C_YT), ('wa', C_WA)]
    xi = PAGE_W - len(social) * 30 - 8
    for label, color in social:
        _draw_social_icon(c, xi, fy + 11, label, color)
        xi += 30

    # ── Sidebar background ─────────────────────────────────────────────
    sb_y = HEADER_H
    sb_h = PAGE_H - HEADER_H - FOOTER_H
    c.fill(C_SIDEBAR); c.box(0, sb_y, SIDEBAR_W, sb_h)
    c.fill(C_TEAL);    c.box(0, sb_y, 3, sb_h)

    c.fill(C_NAVY)
    c.font('Helvetica-Bold', 6.5)
    c.txt(8, sb_y + 12, 'NAVIGATE', w=SIDEBAR_W - 8)
    c.hline(8, sb_y + 23, SIDEBAR_W - 16, C_TEAL, 0.5)

    c.fill(C_TEAL)
    c.font('Helvetica-Bold', 6)
    c.txt(8, PAGE_H - FOOTER_H - 16, 'AVANZA', align='C', w=SIDEBAR_W - 8)

    c.c.endForm()


# ── Decorative diamond logo ───────────────────────────────────────────────────

def _draw_diamond_logo(c: _C, x: float, y: float, size: float = 34):
    half = size / 2
    c.poly([(x + half, y), (x + size, y + half),
            (x + half, y + size), (x,          y + half)], C_TEAL)
    inner = size * 0.38; ih = inner / 2
    ix = x + half - ih; iy = y + half - ih
    c.poly([(ix + ih, iy), (ix + inner, iy + ih),
            (ix + ih, iy + inner), (ix,           iy + ih)], C_TEAL2)
    c.fill(C_WHITE)
    c.font('Helvetica-Bold', int(size * 0.40))
    c.txt(x, y + size * 0.28, 'A', align='C', w=size)


# ── Header chart image ────────────────────────────────────────────────────────

def _draw_header_image(c: _C, x: float, y: float, w: float, h: float):
    c.fill(C_DNAVY); c.box(x, y, w, h)
    stripes = [HexColor('#1A3355'), HexColor('#19304F'), HexColor('#172E4C')]
    sw = w / 3
    for i, col in enumerate(stripes):
        c.fill(col); c.box(x + i * sw, y, sw, h)

    bars = [
        (0.04, 0.38, C_2E6DB4), (0.13, 0.55, C_3A8FD4),
        (0.22, 0.72, C_TEAL),   (0.31, 0.50, C_3A8FD4),
        (0.40, 0.83, C_TEAL),   (0.49, 0.65, C_5AB0E8),
        (0.58, 0.90, C_TEAL),   (0.67, 0.75, C_5AB0E8),
        (0.76, 0.95, C_7DC8F7),
    ]
    bar_w    = w * 0.075
    usable_h = h * 0.76
    pts = []
    for bx_f, bh_f, color in bars:
        bh = usable_h * bh_f
        bx = x + w * bx_f
        by = y + h - bh - 6
        c.fill(color);  c.box(bx, by, bar_w, bh)
        c.fill(C_WHITE); c.box(bx, by, bar_w, 1.5)
        pts.append((bx + bar_w / 2, by))

    trend_pts = [pts[i] for i in (0, 2, 4, 6, 8)]
    c.stroke(C_GOLD); c.lw(1.8)
    for i in range(len(trend_pts) - 1):
        x1, y1 = trend_pts[i]
        x2, y2 = trend_pts[i + 1]
        c.c.line(x1, _ry_text(y1), x2, _ry_text(y2))
    px, py = trend_pts[-1]
    c.circle(px, py, 3, C_GOLD)

    c.fill(C_LBLUE)
    c.font('Helvetica-Bold', 5.5)
    c.txt(x + 2, y + h - 8, 'FINANCIAL ANALYTICS', align='C', w=w - 4)


# ── Social icon ───────────────────────────────────────────────────────────────

def _draw_social_icon(c: _C, x: float, y: float,
                      label: str, color, iw: float = 26, ih: float = 19):
    c.fill(color);  c.box(x, y, iw, ih)
    c.fill(C_WHITE); c.box(x, y, iw, 2)
    c.fill(color);  c.box(x, y + 2, iw, ih - 2)
    c.fill(C_WHITE)
    c.font('Helvetica-Bold', 7)
    c.txt(x, y + 5, label, align='C', w=iw)


# ── Table header ─────────────────────────────────────────────────────────────

def _draw_table_header(c: _C, y: float, col_w: float) -> float:
    c.fill(C_NAVY); c.box(CX, y, col_w, 16)
    c.fill(C_TEAL); c.box(CX, y, 3, 16)
    c.fill(C_GOLD); c.box(CX + col_w - 3, y, 3, 16)
    c.fill(C_WHITE)
    c.font('Courier-Bold', 7)
    hdr = f"{'Date':<12} {'Description':<33} {'Amount':>12} {'Balance':>12}"
    c.txt(CX + 5, y + 3.5, hdr, w=col_w - 10)
    return y + 16


# ── Core renderer ─────────────────────────────────────────────────────────────

def _render_pdf(data: dict) -> bytes:
    buf    = io.BytesIO()
    c      = _C(buf)
    col_w  = PAGE_W - CX - 15
    cb     = PAGE_H - CONTENT_BOTTOM_MARGIN  # content bottom

    # Build the static chrome form XObject on the first call for this canvas.
    _build_static_form(c)

    # ── Named destinations + link IDs ─────────────────────────────────
    # ReportLab bookmarks are registered with bookmarkPage() on the target page.
    # linkRect() creates a clickable annotation referencing that bookmark.
    DEST_SUMMARY = 'AV_SUMMARY'
    acc_dests = [f"AV_ACC_{acc['accountNumber']}" for acc in data['accounts']]
    tdr_dests = [f"AV_TDR_{t['tdrNumber']}"       for t in data['tdr']]

    period_str = ''
    p = data.get('period', {})
    if p:
        period_str = f"Period: {p.get('from','')} to {p.get('to','')}"

    cust_name = data.get('name', '')

    def page_chrome(pg_num: int):
        """Reference static form then draw dynamic overlay."""
        c.c.doForm(_STATIC_FORM)
        # Customer name + period in header
        mid_x, mid_w = 198, 178
        if cust_name:
            c.fill(C_WHITE); c.font('Helvetica-Bold', 8)
            c.txt(mid_x, 41, cust_name, align='C', w=mid_w)
        if period_str:
            c.fill(C_LBLUE); c.font('Helvetica', 6.5)
            c.txt(mid_x, 54, period_str, align='C', w=mid_w)
        # Page number in footer
        c.fill(C_LBLUE); c.font('Helvetica', 6.5)
        c.txt(0, PAGE_H - FOOTER_H + 19, f'Page {pg_num}',
              align='R', w=PAGE_W - 5)

    # Pre-compute sidebar short labels once per PDF (not once per page).
    acc_shorts = [
        ('...' + acc['accountNumber'][-9:]) if len(acc['accountNumber']) > 9
        else acc['accountNumber']
        for acc in data.get('accounts', [])[:8]
    ]
    tdr_shorts = [
        ('...' + t['tdrNumber'][-9:]) if len(t['tdrNumber']) > 9
        else t['tdrNumber']
        for t in data.get('tdr', [])[:5]
    ]

    def draw_sidebar_links(acc_dests_list, tdr_dests_list):
        """Draw account + TDR link labels and register clickable rects."""
        sb_y = HEADER_H
        ny   = sb_y + 30

        # Summary link text + annotation
        c.fill(C_BLUE); c.font('Helvetica-Bold', 6.5)
        c.txt(10, ny, '> Summary', w=SIDEBAR_W - 10)
        _link(c, 3, ny - 2, SIDEBAR_W - 3, 11, DEST_SUMMARY)
        ny += 13

        c.fill(C_MGRAY); c.font('Helvetica-Bold', 6)
        c.txt(8, ny, 'ACCOUNTS', w=SIDEBAR_W - 8)
        ny += 10

        c.fill(C_BLUE); c.font('Helvetica', 6)
        for i, short in enumerate(acc_shorts):
            c.txt(10, ny, f'> {short}', w=SIDEBAR_W - 10)
            if i < len(acc_dests_list):
                _link(c, 3, ny - 2, SIDEBAR_W - 3, 11, acc_dests_list[i])
            ny += 10

        ny += 4
        c.fill(C_MGRAY); c.font('Helvetica-Bold', 6)
        c.txt(8, ny, 'TDR', w=SIDEBAR_W - 8)
        ny += 10

        c.fill(C_BLUE); c.font('Helvetica', 6)
        for i, short in enumerate(tdr_shorts):
            c.txt(10, ny, f'> {short}', w=SIDEBAR_W - 10)
            if i < len(tdr_dests_list):
                _link(c, 3, ny - 2, SIDEBAR_W - 3, 11, tdr_dests_list[i])
            ny += 10

    # ── Table renderer ────────────────────────────────────────────────
    _TX = CX + 5       # text x position (constant)

    def render_table(transactions: list, start_y: float):
        y           = _draw_table_header(c, start_y, col_w)
        section_top = y
        c.fill(C_DGRAY); c.font('Courier', 6.8)
        # Local refs to avoid attribute lookups in the inner loop.
        draw_str = c.c.drawString

        rows = [
            f"{tx['date']:<12} {tx['description'][:32]:<33}"
            f" {tx['amount']:>12.2f} {tx['balance']:>12.2f}"
            for tx in transactions
        ]

        rl_y = PAGE_H - y - 2.5    # track ReportLab y directly; avoids _ry_text() per row
        for i, row in enumerate(rows):
            if y + 12 > cb:
                c.fill(C_TEAL); c.box(CX, section_top, 3, y - section_top)
                c.fill(C_NAVY); c.box(CX, y, col_w, 2)
                c.show_page()
                pg[0] += 1
                page_chrome(pg[0])
                draw_sidebar_links(acc_dests, tdr_dests)
                y           = _draw_table_header(c, CONTENT_TOP, col_w)
                section_top = y
                c.fill(C_DGRAY); c.font('Courier', 6.8)
                draw_str = c.c.drawString   # canvas may be same object; refresh local ref
                rl_y = PAGE_H - y - 2.5

            if i % 2 == 0:
                c.fill(C_LTBLUE); c.box(CX, y, col_w, 12)
                # Restore text colour — drawString uses the current fill colour.
                c.fill(C_DGRAY)

            draw_str(_TX, rl_y, row)
            y    += 12
            rl_y -= 12

        c.fill(C_TEAL); c.box(CX, section_top, 3, y - section_top)
        c.fill(C_NAVY); c.box(CX, y, col_w, 2)

    # ── Summary table helpers ─────────────────────────────────────────
    def tbl_header(y: float, col1: str, col2: str) -> float:
        c.fill(C_NAVY); c.box(CX, y, col_w, 17)
        c.fill(C_TEAL); c.box(CX, y, 3, 17)
        c.fill(C_GOLD); c.box(CX + col_w - 3, y, 3, 17)
        c.fill(C_WHITE); c.font('Helvetica-Bold', 7.5)
        c.txt(CX + 8, y + 4.5, col1, w=220)
        c.txt(CX + 230, y + 4.5, col2, align='R', w=col_w - 245)
        return y + 17

    def tbl_row(y: float, col1: str, col2: str, idx: int, dest=None) -> float:
        if idx % 2 == 0:
            c.fill(C_LTBLUE); c.box(CX, y, col_w, 17)
            c.fill(C_TEAL);   c.box(CX, y, 3, 17)
        else:
            c.fill(C_CDCEF);  c.box(CX, y, 3, 17)
        c.fill(C_BLUE); c.font('Helvetica', 8)
        c.txt(CX + 8, y + 4.5, col1, w=220)
        c.fill(C_DGRAY); c.font('Helvetica-Bold', 8)
        c.txt(CX + 230, y + 4.5, col2, align='R', w=col_w - 245)
        if dest:
            _link(c, CX, y, col_w, 17, dest)
        return y + 17

    def section_title(y: float, title: str) -> float:
        c.fill(C_NAVY); c.box(CX, y, 4, 14)
        c.fill(C_NAVY); c.font('Helvetica-Bold', 9.5)
        c.txt(CX + 10, y, title, w=300)
        return y + 18

    # ── Page counter (list so nested functions can mutate it) ─────────
    pg = [0]

    # ── Page 1 — Summary ──────────────────────────────────────────────
    # ReportLab starts on page 1 automatically — draw directly without
    # calling showPage() first (which would create a blank preceding page).
    pg[0] = 1
    page_chrome(pg[0])
    c.c.bookmarkPage(DEST_SUMMARY)
    draw_sidebar_links(acc_dests, tdr_dests)

    y = CONTENT_TOP
    c.fill(C_LTBLUE); c.box(CX, y, col_w, 40)
    c.fill(C_TEAL);   c.box(CX, y, 4, 40)
    c.fill(C_GOLD);   c.box(CX + col_w - 4, y, 4, 40)
    c.fill(C_NAVY);   c.font('Helvetica-Bold', 12)
    c.txt(CX + 10, y + 6, 'Statement Summary', w=col_w - 20)
    c.fill(C_DGRAY);  c.font('Helvetica', 8.5)
    c.txt(CX + 10, y + 21, cust_name, w=col_w - 20)
    c.fill(C_MGRAY);  c.font('Helvetica', 7)
    c.txt(CX + 10, y + 31, data.get('address', ''), w=col_w - 20)
    y += 52

    y = section_title(y, 'Accounts')
    y = tbl_header(y, 'Account Number', 'Balance (PKR)')
    for i, acc in enumerate(data['accounts']):
        y = tbl_row(y, acc['accountNumber'],
                    f"{acc['balance']:,.2f}", i, acc_dests[i])
    c.fill(C_NAVY); c.box(CX, y, col_w, 2)
    y += 22

    y = section_title(y, 'Term Deposits (TDR)')
    y = tbl_header(y, 'TDR Number', 'Principal (PKR)')
    for i, t in enumerate(data['tdr']):
        y = tbl_row(y, t['tdrNumber'],
                    f"{t.get('principalAmount', 0):,.2f}", i, tdr_dests[i])
    c.fill(C_NAVY); c.box(CX, y, col_w, 2)

    # ── Account detail pages ──────────────────────────────────────────
    for i, acc in enumerate(data['accounts']):
        c.show_page(); pg[0] += 1
        page_chrome(pg[0])
        c.c.bookmarkPage(acc_dests[i])
        draw_sidebar_links(acc_dests, tdr_dests)

        y = CONTENT_TOP
        c.fill(C_LTBLUE); c.box(CX, y, col_w, 32)
        c.fill(C_TEAL);   c.box(CX, y, 4, 32)
        c.fill(C_NAVY);   c.font('Helvetica-Bold', 10.5)
        c.txt(CX + 10, y + 5,
              f"Account: {acc['accountNumber']}", w=col_w - 20)
        c.fill(C_MGRAY);  c.font('Helvetica', 7.5)
        c.txt(CX + 10, y + 19,
              f"Type: {acc.get('accountType','')}   |   "
              f"Balance: PKR {acc.get('balance', 0):,.2f}", w=col_w - 20)
        y += 42
        render_table(acc['transactions'], y)

    # ── TDR detail pages ─────────────────────────────────────────────
    for i, t in enumerate(data['tdr']):
        c.show_page(); pg[0] += 1
        page_chrome(pg[0])
        c.c.bookmarkPage(tdr_dests[i])
        draw_sidebar_links(acc_dests, tdr_dests)

        y = CONTENT_TOP
        c.fill(C_LTBLUE); c.box(CX, y, col_w, 32)
        c.fill(C_GOLD);   c.box(CX, y, 4, 32)
        c.fill(C_NAVY);   c.font('Helvetica-Bold', 10.5)
        c.txt(CX + 10, y + 5,
              f"Term Deposit: {t['tdrNumber']}", w=col_w - 20)
        c.fill(C_MGRAY);  c.font('Helvetica', 7.5)
        c.txt(CX + 10, y + 19,
              f"Rate: {t.get('interestRate',0)}%   |   "
              f"Maturity: {t.get('maturityDate','')}   |   "
              f"Principal: PKR {t.get('principalAmount',0):,.2f}",
              w=col_w - 20)
        y += 42
        render_table(t['transactions'], y)

    c.c.save()
    return buf.getvalue()


def _link(c: _C, x: float, y: float, w: float, h: float, dest: str):
    """Create an internal PDF link annotation (ReportLab coords)."""
    y1 = _ry(y, h)   # bottom edge
    c.c.linkRect('', dest, (x, y1, x + w, y1 + h), relative=0, Border=(0, 0, 0))


# ── Public API ────────────────────────────────────────────────────────────────

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
            'transactions': [{
                'date':        tx['transactionDate'],
                'description': tx['transactionDetails'],
                'amount':      (float(tx['debitAmountLc'])
                               if float(tx['debitAmountLc']) > 0
                               else float(tx['creditAmountLc'])),
                'balance':     float(tx['balance']),
            } for tx in acc.get('transactions', [])],
        })

    tdr = []
    for td in record.get('termDeposits', []):
        s  = summary_tds.get(td['certNo'], {})
        ft = td['tdrTransactions'][0] if td.get('tdrTransactions') else {}
        tdr.append({
            'tdrNumber':       td['certNo'],
            'principalAmount': float(s.get('openingBalance', 0)),
            'interestRate':    0,
            'maturityDate':    ft.get('maturity', ''),
            'transactions': [{
                'date':        tx['startDate'],
                'description': (f"{tx.get('certificateType','')} - "
                               f"{tx.get('tenure','')} - "
                               f"{tx.get('profitOption','')}"),
                'amount':      float(tx['rupeesAmount']),
                'balance':     float(tx['rupeesAmount']),
            } for tx in td.get('tdrTransactions', [])],
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


def _worker_batch(args):
    chunk, output_dir = args
    for customer in chunk:
        pdf_bytes = _render_pdf(customer)
        with open(os.path.join(output_dir, f"RL-{customer['id']}.pdf"), 'wb') as f:
            f.write(pdf_bytes)
    return len(chunk)


def _worker_single(args):
    customer, output_dir = args
    pdf_bytes = _render_pdf(customer)
    with open(os.path.join(output_dir, f"RL-{customer['id']}.pdf"), 'wb') as f:
        f.write(pdf_bytes)
    return 1


# ── Entry point ───────────────────────────────────────────────────────────────

if __name__ == '__main__':
    parser = argparse.ArgumentParser(
        description='Statement PDF generator — ReportLab / Avanza branding')
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
            chunk_size = args.chunk_size or max(1, math.ceil(count / (workers * 2)))
            chunks     = [customers[i:i + chunk_size]
                          for i in range(0, count, chunk_size)]
            work_args  = [(ch, output_dir) for ch in chunks]
            print(f"[RL-batch] {count} PDFs | {workers} workers | chunk={chunk_size}",
                  file=sys.stderr)
            start = time.time()
            with Pool(processes=min(workers, len(chunks)),
                      maxtasksperchild=4) as pool:
                results = list(pool.imap_unordered(_worker_batch, work_args))
        else:
            work_args = [(c, output_dir) for c in customers]
            print(f"[RL-single] {count} PDFs | {workers} workers", file=sys.stderr)
            start = time.time()
            with Pool(processes=workers, maxtasksperchild=16) as pool:
                results = list(pool.imap_unordered(_worker_single, work_args))

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
        print(f"Done  {total} PDFs | {duration:.2f}s | {total/duration:.2f} PDF/sec",
              file=sys.stderr)
        sys.exit(0)

    # Smoke test
    sample = {
        'id': 'RL-TEST', 'name': 'Test Customer',
        'address': 'Avanza Solutions, Karachi, Pakistan',
        'period': {'from': '01-Jan-2024', 'to': '31-Mar-2024'},
        'accounts': [{
            'accountNumber': 'PK36HABB000000001234',
            'accountType': 'CURRENT', 'balance': 1_250_000.0,
            'transactions': [
                {'date': f'2024-01-{i+1:02d}', 'description': f'Transaction {i+1}',
                 'amount': 1000.0 + i * 50, 'balance': 1_250_000.0 - i * 1000}
                for i in range(30)
            ],
        }],
        'tdr': [{
            'tdrNumber': 'TD-000001', 'principalAmount': 500_000.0,
            'interestRate': 7.5, 'maturityDate': '01-Jan-2025',
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
        _render_pdf(sample)
    t = time.time() - start
    out_path = os.path.join(args.output_dir, 'SAMPLE-rl.pdf')
    with open(out_path, 'wb') as f:
        f.write(_render_pdf(sample))
    print(f"{args.count} PDFs | {t:.2f}s | {args.count/t:.2f} PDF/sec", file=sys.stderr)
    print(f"Sample: {out_path}", file=sys.stderr)
