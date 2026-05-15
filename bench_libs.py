"""Quick benchmark: fpdf2 vs pypdfium2 vs reportlab for our workload."""
import time, io, json

# ── Shared sample data ────────────────────────────────────────────────────────
SAMPLE = {
    "name": "Benchmark Customer",
    "address": "123 Test Street, Karachi",
    "accounts": [
        {
            "accountNumber": "PK12-HABB-000001-1",
            "balance": 150000.0,
            "transactions": [
                {"date": f"2024-01-{i+1:02d}", "description": f"Transaction {i+1}",
                 "amount": 100.0 + i, "balance": 150000.0 - i * 100}
                for i in range(150)
            ],
        }
    ],
    "tdr": [
        {
            "tdrNumber": "TD-000001-1",
            "transactions": [
                {"date": "2024-01-01", "description": "Fixed - 1 Year - Monthly Profit",
                 "amount": 500000.0, "balance": 500000.0}
                for _ in range(50)
            ],
        }
    ],
}

RUNS = 20  # PDFs per library

# ── 1. reportlab ──────────────────────────────────────────────────────────────
def bench_reportlab():
    from reportlab.pdfgen import canvas
    from reportlab.lib.pagesizes import A4
    from reportlab.lib.colors import HexColor

    def render(data):
        buf = io.BytesIO()
        c = canvas.Canvas(buf, pagesize=A4)
        w, h = A4
        cx = 130
        c.setFillColor(HexColor('#F8F8FA'))
        c.rect(0, 0, 100, h, fill=1, stroke=0)
        c.setFont('Helvetica-Bold', 18)
        c.setFillColor(HexColor('#222222'))
        c.drawString(cx, h - 60, "Professional Statement")
        c.setFont('Helvetica', 8)
        c.drawString(cx, h - 80, data['name'])
        ty = h - 130
        c.setFont('Helvetica', 7)
        c.setFillColor(HexColor('#444444'))
        c.rect(cx, ty, 420, 12, fill=1, stroke=0)
        ty -= 12
        c.setFont('Courier', 7)
        c.setFillColor(HexColor('#333333'))
        for i, tx in enumerate(data['accounts'][0]['transactions']):
            if ty < 50:
                c.showPage()
                ty = h - 50
            if i % 2 == 0:
                c.setFillColor(HexColor('#F7F7F7'))
                c.rect(cx, ty - 2, 420, 10, fill=1, stroke=0)
                c.setFillColor(HexColor('#333333'))
            line = f"{tx['date']:<12} {tx['description'][:32]:<33} {tx['amount']:>12.2f} {tx['balance']:>12.2f}"
            c.drawString(cx + 5, ty, line)
            ty -= 10
        c.save()
        return buf.getvalue()

    t = time.perf_counter()
    for _ in range(RUNS):
        render(SAMPLE)
    return time.perf_counter() - t


# ── 2. fpdf2 ──────────────────────────────────────────────────────────────────
def bench_fpdf2():
    from fpdf import FPDF

    PAGE_W, PAGE_H = 595.28, 841.89
    CX = 130

    def hex_rgb(h):
        h = h.lstrip('#')
        return int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16)

    def render(data):
        pdf = FPDF(unit='pt', format='A4')
        pdf.set_auto_page_break(False)

        def fc(h): pdf.set_fill_color(*hex_rgb(h))
        def tc(h): pdf.set_text_color(*hex_rgb(h))
        def frect(x, y, w, h): pdf.rect(x, y, w, h, style='F')

        def txt(x, y, text, font='Helvetica', style='', size=8, w=200, align='L'):
            pdf.set_font(font, style, size)
            pdf.set_xy(x, y)
            pdf.cell(w, size * 1.2, text, align=align)

        pdf.add_page()
        fc('#F8F8FA'); frect(0, 0, 100, PAGE_H)
        tc('#222222')
        txt(CX, 50, "Professional Statement", style='B', size=18, w=400)
        txt(CX, 75, data['name'], size=8, w=300)

        # header bar
        ty = 130
        fc('#444444'); frect(CX, ty, 420, 12)
        tc('#FFFFFF')
        txt(CX + 5, ty, "Date", size=7, w=200)
        ty += 12

        pdf.set_font('Courier', '', 7)
        tc('#333333')
        for i, tx in enumerate(data['accounts'][0]['transactions']):
            if ty > PAGE_H - 50:
                pdf.add_page()
                ty = 50
            if i % 2 == 0:
                fc('#F7F7F7'); frect(CX, ty, 420, 10)
                tc('#333333')
            line = (f"{tx['date']:<12} {tx['description'][:32]:<33}"
                    f" {tx['amount']:>12.2f} {tx['balance']:>12.2f}")
            pdf.set_xy(CX + 5, ty)
            pdf.cell(420, 10, line)
            ty += 10

        return bytes(pdf.output())

    t = time.perf_counter()
    for _ in range(RUNS):
        render(SAMPLE)
    return time.perf_counter() - t


# ── 3. pypdfium2 ─────────────────────────────────────────────────────────────
def bench_pypdfium2():
    import pypdfium2 as pdfium

    PAGE_W, PAGE_H = 595.28, 841.89
    CX = 130

    def hex_color(h):
        h = h.lstrip('#')
        r, g, b = int(h[0:2],16), int(h[2:4],16), int(h[4:6],16)
        # PDFium uses BGRA 32-bit integer
        return (0xFF << 24) | (r << 16) | (g << 8) | b

    def render(data):
        doc = pdfium.PdfDocument.new()

        def add_page():
            page = doc.new_page(PAGE_W, PAGE_H)
            return page

        def fill_rect(page, x, y_from_top, w, h, color):
            path = page.new_obj(pdfium.PdfPathObject)
            path.set_fill_color(color)
            path.set_stroke_color(color)
            # PDFium y is from bottom (same as ReportLab)
            y_rl = PAGE_H - y_from_top - h
            path.insert_segment(pdfium.FPDFPathSegment(x, y_rl, pdfium.raw.FPDF_SEGMENT_MOVETO))
            path.insert_segment(pdfium.FPDFPathSegment(x + w, y_rl, pdfium.raw.FPDF_SEGMENT_LINETO))
            path.insert_segment(pdfium.FPDFPathSegment(x + w, y_rl + h, pdfium.raw.FPDF_SEGMENT_LINETO))
            path.insert_segment(pdfium.FPDFPathSegment(x, y_rl + h, pdfium.raw.FPDF_SEGMENT_LINETO))
            path.close()
            path.set_fill_mode(pdfium.raw.FPDF_FILLMODE_ALTERNATE)
            page.insert_obj(path)
            page.gen_content()

        page = add_page()
        fill_rect(page, 0, 0, 100, PAGE_H, hex_color('#F8F8FA'))

        buf = doc.save()
        return bytes(buf)

    t = time.perf_counter()
    for _ in range(RUNS):
        render(SAMPLE)
    return time.perf_counter() - t


# ── Run ───────────────────────────────────────────────────────────────────────
if __name__ == '__main__':
    print(f"Benchmarking {RUNS} PDFs each (150 account txns + 50 TDR txns per PDF)\n")

    for name, fn in [("reportlab", bench_reportlab),
                     ("fpdf2    ", bench_fpdf2),
                     ("pypdfium2", bench_pypdfium2)]:
        try:
            elapsed = fn()
            tps = RUNS / elapsed
            print(f"  {name}:  {elapsed:.2f}s  |  {tps:.1f} PDF/sec")
        except Exception as e:
            print(f"  {name}:  ERROR — {e}")
