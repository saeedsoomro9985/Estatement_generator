import PDFDocument from 'pdfkit/js/pdfkit.standalone';
import { CustomerData } from './dataService';

// ── Avanza Solutions brand palette ──────────────────────────────────────────
const AV_NAVY    = '#1E3A5F';
const AV_DNAVY   = '#152B47';
const AV_BLUE    = '#0052CC';
const AV_TEAL    = '#00B4A0';
const AV_GOLD    = '#F0A500';
const AV_LTBLUE  = '#EAF2FF';
const AV_SIDEBAR = '#EEF3FA';
const AV_LBLUE   = '#7FB3D3';
const AV_DGRAY   = '#2C3E50';
const AV_MGRAY   = '#5A6A7A';

const HEADER_H   = 90;
const FOOTER_H   = 44;
const SIDEBAR_W  = 92;
const CX         = 107;          // content start X
const PAGE_W     = 595.28;
const PAGE_H     = 841.89;
const COL_W      = PAGE_W - CX - 15;
const CONTENT_TOP = HEADER_H + 8;

// Module-level constants — allocated once, not rebuilt on every page.
const SOCIAL_ICONS: [string, string][] = [
  ['f', '#1877F2'], ['in', '#0A66C2'],
  ['tw', '#1DA1F2'], ['yt', '#FF0000'], ['wa', '#25D366'],
];

const BAR_CHART: [number, number, string][] = [
  [0.04, 0.38, '#2E6DB4'], [0.13, 0.55, '#3A8FD4'],
  [0.22, 0.72, AV_TEAL],  [0.31, 0.50, '#3A8FD4'],
  [0.40, 0.83, AV_TEAL],  [0.49, 0.65, '#5AB0E8'],
  [0.58, 0.90, AV_TEAL],  [0.67, 0.75, '#5AB0E8'],
  [0.76, 0.95, '#7DC8F7'],
];

export const generateStatementPDF = (
  data: CustomerData,
  outputStream?: NodeJS.WritableStream
): Promise<Uint8Array | void> => {
  return new Promise((resolve, reject) => {
    const doc = new PDFDocument({
      size: 'A4',
      margin: 0,
      autoFirstPage: false,
      bufferPages: false,
      compress: true,
    });

    if (outputStream) {
      doc.pipe(outputStream);
      outputStream.on('finish', () => resolve());
      outputStream.on('error', reject);
    } else {
      const chunks: Buffer[] = [];
      doc.on('data', (chunk: Buffer) => chunks.push(chunk));
      doc.on('end', () => resolve(new Uint8Array(Buffer.concat(chunks))));
      doc.on('error', reject);
    }

    let pageCount = 0;

    // ── Drawing helpers ───────────────────────────────────────────────

    const box = (x: number, y: number, w: number, h: number, color: string) =>
      doc.rect(x, y, w, h).fill(color);

    const hline = (x: number, y: number, len: number, color: string, w = 0.5) =>
      doc.moveTo(x, y).lineTo(x + len, y).lineWidth(w).stroke(color);

    // ── Avanza diamond logo mark ──────────────────────────────────────
    const drawDiamondLogo = (x: number, y: number, size: number) => {
      const half = size / 2;
      // Outer teal diamond
      doc.moveTo(x + half, y)
        .lineTo(x + size, y + half)
        .lineTo(x + half, y + size)
        .lineTo(x,        y + half)
        .closePath().fill(AV_TEAL);
      // Inner darker diamond
      const inner = size * 0.38, ih = inner / 2;
      const ix = x + half - ih, iy = y + half - ih;
      doc.moveTo(ix + ih, iy)
        .lineTo(ix + inner, iy + ih)
        .lineTo(ix + ih,    iy + inner)
        .lineTo(ix,         iy + ih)
        .closePath().fill('#008C7A');
      // "A" text
      doc.fillColor('#FFFFFF').font('Helvetica-Bold')
        .fontSize(size * 0.40)
        .text('A', x, y + size * 0.28, { width: size, align: 'center', lineBreak: false });
    };

    // ── Header financial bar-chart image ──────────────────────────────
    const drawHeaderImage = (x: number, y: number, w: number, h: number) => {
      const sw = w / 3;
      box(x,          y, sw, h, AV_DNAVY);
      box(x + sw,     y, sw, h, '#19304F');
      box(x + sw * 2, y, sw, h, '#172E4C');

      const barW = w * 0.075, usableH = h * 0.76;
      // Trend line uses every other bar (indices 0,2,4,6,8).
      const tpx: number[] = [], tpy: number[] = [];
      for (let bi = 0; bi < BAR_CHART.length; bi++) {
        const [bxF, bhF, color] = BAR_CHART[bi];
        const bh = usableH * bhF;
        const bx = x + w * bxF, by = y + h - bh - 6;
        box(bx, by, barW, bh, color);
        box(bx, by, barW, 1.5, '#FFFFFF');
        if (bi % 2 === 0) { tpx.push(bx + barW / 2); tpy.push(by); }
      }
      doc.moveTo(tpx[0], tpy[0]);
      for (let ti = 1; ti < tpx.length; ti++) doc.lineTo(tpx[ti], tpy[ti]);
      doc.lineWidth(1.8).stroke(AV_GOLD);
      doc.circle(tpx[tpx.length - 1], tpy[tpy.length - 1], 3).fill(AV_GOLD);
      doc.fillColor(AV_LBLUE).font('Helvetica-Bold').fontSize(5.5)
        .text('FINANCIAL ANALYTICS', x + 2, y + h - 9,
              { width: w - 4, align: 'center', lineBreak: false });
    };

    // ── Full-width branded header ─────────────────────────────────────
    const drawPageHeader = () => {
      box(0, 0, PAGE_W, HEADER_H, AV_NAVY);
      const logoPanelW = 195;
      box(0, 0, logoPanelW, HEADER_H, AV_DNAVY);

      drawDiamondLogo(10, (HEADER_H - 34) / 2, 34);

      doc.fillColor('#FFFFFF').font('Helvetica-Bold').fontSize(13.5)
        .text('AVANZA SOLUTIONS', 53, 20, { lineBreak: false });
      doc.fillColor(AV_TEAL).font('Helvetica-Bold').fontSize(8)
        .text('PVT. LTD.', 53, 36, { lineBreak: false });
      doc.fillColor(AV_LBLUE).font('Helvetica').fontSize(6)
        .text('www.avanzasolutions.com', 53, 48, { lineBreak: false })
        .text('info@avanzasolutions.com', 53, 57, { lineBreak: false });

      box(logoPanelW, 0, 3, HEADER_H, AV_TEAL);

      // Centre: statement title
      const midX = logoPanelW + 10, midW = 178;
      doc.fillColor('#FFFFFF').font('Helvetica-Bold').fontSize(17)
        .text('BANK STATEMENT', midX, 16, { width: midW, align: 'center', lineBreak: false });
      hline(midX + 10, 37, midW - 20, AV_TEAL, 0.8);

      const custName = data.name ?? '';
      if (custName) {
        doc.fillColor('#FFFFFF').font('Helvetica-Bold').fontSize(8)
          .text(custName, midX, 41, { width: midW, align: 'center', lineBreak: false });
      }
      const period = (data as any).period;
      if (period?.from) {
        doc.fillColor(AV_LBLUE).font('Helvetica').fontSize(6.5)
          .text(`Period: ${period.from} to ${period.to ?? ''}`,
                midX, 54, { width: midW, align: 'center', lineBreak: false });
      }

      // Right chart image
      const imgX = logoPanelW + 3 + midW + 12;
      drawHeaderImage(imgX, 4, PAGE_W - imgX - 2, HEADER_H - 8);

      box(0, HEADER_H - 3, PAGE_W, 3, AV_TEAL);
    };

    // ── Social media icon box ─────────────────────────────────────────
    const drawSocialIcon = (x: number, y: number, label: string, color: string) => {
      const w = 26, h = 19;
      box(x, y, w, h, color);
      box(x, y, w, 2, '#FFFFFF');
      box(x, y + 2, w, h - 2, color);
      doc.fillColor('#FFFFFF').font('Helvetica-Bold').fontSize(7)
        .text(label, x, y + 5, { width: w, align: 'center', lineBreak: false });
    };

    // ── Full-width branded footer ─────────────────────────────────────
    const drawPageFooter = () => {
      const y = PAGE_H - FOOTER_H;
      box(0, y, PAGE_W, FOOTER_H, AV_NAVY);
      box(0, y, PAGE_W, 2, AV_TEAL);

      doc.fillColor('#FFFFFF').font('Helvetica-Bold').fontSize(7)
        .text('AVANZA SOLUTIONS PVT. LTD.', 12, y + 8, { lineBreak: false });
      doc.fillColor(AV_LBLUE).font('Helvetica').fontSize(6)
        .text('Karachi, Pakistan  |  Tel: +92-21-111-282-692', 12, y + 19, { lineBreak: false })
        .text('This is a system-generated statement. No signature required.', 12, y + 29, { lineBreak: false });

      doc.fillColor(AV_LBLUE).font('Helvetica').fontSize(6.5)
        .text(`Page ${pageCount}`, 0, y + 19,
              { width: PAGE_W - 5, align: 'right', lineBreak: false });

      let xi = PAGE_W - SOCIAL_ICONS.length * 30 - 8;
      for (let si = 0; si < SOCIAL_ICONS.length; si++) {
        drawSocialIcon(xi, y + 11, SOCIAL_ICONS[si][0], SOCIAL_ICONS[si][1]);
        xi += 30;
      }
    };

    // Pre-compute sidebar short labels once per document — not per page.
    const accountShorts = data.accounts.slice(0, 8).map(acc => {
      const num = acc.accountNumber ?? '';
      return num.length > 9 ? '...' + num.slice(-9) : num;
    });
    const tdrShorts = data.tdr.slice(0, 5).map(t => {
      const num = t.tdrNumber ?? '';
      return num.length > 9 ? '...' + num.slice(-9) : num;
    });

    // ── Sidebar with clickable navigation ─────────────────────────────
    const drawSidebar = (
      summaryDest: string,
      accountDests: string[],
      tdrDests: string[]
    ) => {
      const sbY = HEADER_H, sbH = PAGE_H - HEADER_H - FOOTER_H;
      box(0, sbY, SIDEBAR_W, sbH, AV_SIDEBAR);
      box(0, sbY, 3, sbH, AV_TEAL);

      doc.fillColor(AV_NAVY).font('Helvetica-Bold').fontSize(6.5)
        .text('NAVIGATE', 8, sbY + 12, { lineBreak: false });
      hline(8, sbY + 23, SIDEBAR_W - 16, AV_TEAL, 0.5);

      // Summary link
      let ny = sbY + 30;
      doc.fillColor(AV_BLUE).font('Helvetica-Bold').fontSize(6.5)
        .text('> Summary', 10, ny, { goTo: summaryDest, lineBreak: false, width: SIDEBAR_W - 14 });
      ny += 13;

      // Account links
      doc.fillColor(AV_MGRAY).font('Helvetica-Bold').fontSize(6)
        .text('ACCOUNTS', 8, ny, { lineBreak: false });
      ny += 10;

      for (let i = 0; i < accountShorts.length; i++) {
        doc.fillColor(AV_BLUE).font('Helvetica').fontSize(6)
          .text(`> ${accountShorts[i]}`, 10, ny,
                { goTo: accountDests[i] ?? summaryDest, lineBreak: false, width: SIDEBAR_W - 14 });
        ny += 10;
      }

      // TDR links
      ny += 4;
      doc.fillColor(AV_MGRAY).font('Helvetica-Bold').fontSize(6)
        .text('TDR', 8, ny, { lineBreak: false });
      ny += 10;

      for (let i = 0; i < tdrShorts.length; i++) {
        doc.fillColor(AV_BLUE).font('Helvetica').fontSize(6)
          .text(`> ${tdrShorts[i]}`, 10, ny,
                { goTo: tdrDests[i] ?? summaryDest, lineBreak: false, width: SIDEBAR_W - 14 });
        ny += 10;
      }

      // Avanza wordmark at bottom of sidebar
      doc.fillColor(AV_TEAL).font('Helvetica-Bold').fontSize(6)
        .text('AVANZA', 8, PAGE_H - FOOTER_H - 16,
              { width: SIDEBAR_W - 8, align: 'center', lineBreak: false });
    };

    // ── Table column header ───────────────────────────────────────────
    const drawTableHeader = (y: number): number => {
      box(CX, y, COL_W, 16, AV_NAVY);
      box(CX, y, 3, 16, AV_TEAL);
      box(CX + COL_W - 3, y, 3, 16, AV_GOLD);
      const hdr = `${'Date'.padEnd(12)} ${'Description'.padEnd(33)} ${'Amount'.padStart(12)} ${'Balance'.padStart(12)}`;
      doc.fillColor('#FFFFFF').font('Courier-Bold').fontSize(7)
        .text(hdr, CX + 5, y + 3.5, { lineBreak: false });
      return y + 16;
    };

    // ── Transaction table renderer (optimised strip batching) ─────────
    const ROW_OPTS = { lineBreak: false } as const;

    const renderTable = (
      transactions: CustomerData['accounts'][0]['transactions'],
      startY: number,
      summaryDest: string,
      accountDests: string[],
      tdrDests: string[]
    ) => {
      const contentBottom = PAGE_H - FOOTER_H - 10;
      let y = drawTableHeader(startY);
      let sectionTop = y;

      // Pre-format all row strings to keep the draw loop as tight as possible.
      const rows = transactions.map(tx =>
        `${(tx.date ?? '').padEnd(12)} ${(tx.description ?? '').substring(0, 32).padEnd(33)} ` +
        `${tx.amount.toFixed(2).padStart(12)} ${tx.balance.toFixed(2).padStart(12)}`
      );

      // Set font/color ONCE before the loop — not on every row.
      doc.fillColor(AV_DGRAY).font('Courier').fontSize(6.8);

      for (let i = 0; i < rows.length; i++) {
        if (y + 12 > contentBottom) {
          if (y > sectionTop) box(CX, sectionTop, 3, y - sectionTop, AV_TEAL);
          box(CX, y, COL_W, 2, AV_NAVY);
          pageCount++;
          doc.addPage();
          drawPageHeader();
          drawPageFooter();
          drawSidebar(summaryDest, accountDests, tdrDests);
          y = drawTableHeader(CONTENT_TOP);
          sectionTop = y;
          // Restore font/color after page chrome resets doc state.
          doc.fillColor(AV_DGRAY).font('Courier').fontSize(6.8);
        }

        // Skip drawing white rows — page background is already white.
        if (i % 2 === 0) box(CX, y, COL_W, 12, AV_LTBLUE);

        doc.text(rows[i], CX + 5, y + 2.5, ROW_OPTS);
        y += 12;
      }

      if (y > sectionTop) box(CX, sectionTop, 3, y - sectionTop, AV_TEAL);
      box(CX, y, COL_W, 2, AV_NAVY);
    };

    // ── Summary table helpers ─────────────────────────────────────────
    const summaryTableHeader = (
      y: number, col1: string, col2: string
    ): number => {
      box(CX, y, COL_W, 17, AV_NAVY);
      box(CX, y, 3, 17, AV_TEAL);
      box(CX + COL_W - 3, y, 3, 17, AV_GOLD);
      doc.fillColor('#FFFFFF').font('Helvetica-Bold').fontSize(7.5)
        .text(col1, CX + 8, y + 4.5, { lineBreak: false })
        .text(col2, CX + 230, y + 4.5,
              { width: COL_W - 245, align: 'right', lineBreak: false });
      return y + 17;
    };

    const summaryTableRow = (
      y: number, col1: string, col2: string,
      idx: number, dest?: string
    ): number => {
      if (idx % 2 === 0) {
        box(CX, y, COL_W, 17, AV_LTBLUE);
        box(CX, y, 3, 17, AV_TEAL);
      } else {
        box(CX, y, 3, 17, '#C5DCEF');
      }
      const opts = dest
        ? { goTo: dest, lineBreak: false, width: 210 }
        : { lineBreak: false };
      doc.fillColor(AV_BLUE).font('Helvetica').fontSize(8)
        .text(col1, CX + 8, y + 4.5, opts);
      doc.fillColor(AV_DGRAY).font('Helvetica-Bold').fontSize(8)
        .text(col2, CX + 230, y + 4.5,
              { width: COL_W - 245, align: 'right', lineBreak: false });
      return y + 17;
    };

    const sectionTitle = (y: number, title: string): number => {
      box(CX, y, 4, 14, AV_NAVY);
      doc.fillColor(AV_NAVY).font('Helvetica-Bold').fontSize(9.5)
        .text(title, CX + 10, y, { lineBreak: false });
      return y + 18;
    };

    // ── Named destinations ────────────────────────────────────────────
    const SUMMARY_DEST   = 'AV_SUMMARY';
    const accountDests   = data.accounts.map((acc) => `AV_ACC_${acc.accountNumber}`);
    const tdrDests       = data.tdr.map((t)   => `AV_TDR_${t.tdrNumber}`);

    // ── Page 1 — Summary ─────────────────────────────────────────────
    pageCount++;
    doc.addPage();
    doc.addNamedDestination(SUMMARY_DEST);
    drawPageHeader();
    drawPageFooter();
    drawSidebar(SUMMARY_DEST, accountDests, tdrDests);

    let y = CONTENT_TOP;

    // Customer card
    box(CX, y, COL_W, 40, AV_LTBLUE);
    box(CX, y, 4, 40, AV_TEAL);
    box(CX + COL_W - 4, y, 4, 40, AV_GOLD);
    doc.fillColor(AV_NAVY).font('Helvetica-Bold').fontSize(12)
      .text('Statement Summary', CX + 10, y + 6, { lineBreak: false });
    doc.fillColor(AV_DGRAY).font('Helvetica').fontSize(8.5)
      .text(data.name ?? '', CX + 10, y + 21, { lineBreak: false });
    doc.fillColor(AV_MGRAY).font('Helvetica').fontSize(7)
      .text(data.address ?? '', CX + 10, y + 31, { width: COL_W - 20, lineBreak: false });
    y += 52;

    // Accounts summary table
    y = sectionTitle(y, 'Accounts');
    y = summaryTableHeader(y, 'Account Number', 'Balance (PKR)');
    for (let i = 0; i < data.accounts.length; i++) {
      const acc = data.accounts[i];
      y = summaryTableRow(y, acc.accountNumber, acc.balance.toFixed(2), i, accountDests[i]);
    }
    box(CX, y, COL_W, 2, AV_NAVY);
    y += 22;

    // TDR summary table
    y = sectionTitle(y, 'Term Deposits (TDR)');
    y = summaryTableHeader(y, 'TDR Number', 'Principal (PKR)');
    for (let i = 0; i < data.tdr.length; i++) {
      const t = data.tdr[i];
      y = summaryTableRow(y, t.tdrNumber, t.principalAmount.toFixed(2), i, tdrDests[i]);
    }
    box(CX, y, COL_W, 2, AV_NAVY);

    // ── Account detail pages ──────────────────────────────────────────
    for (let idx = 0; idx < data.accounts.length; idx++) {
      const acc = data.accounts[idx];
      pageCount++;
      doc.addPage();
      doc.addNamedDestination(accountDests[idx]);
      drawPageHeader();
      drawPageFooter();
      drawSidebar(SUMMARY_DEST, accountDests, tdrDests);

      y = CONTENT_TOP;
      box(CX, y, COL_W, 32, AV_LTBLUE);
      box(CX, y, 4, 32, AV_TEAL);
      doc.fillColor(AV_NAVY).font('Helvetica-Bold').fontSize(10.5)
        .text(`Account: ${acc.accountNumber}`, CX + 10, y + 5, { lineBreak: false });
      doc.fillColor(AV_MGRAY).font('Helvetica').fontSize(7.5)
        .text(
          `Type: ${acc.accountType}   |   Balance: PKR ${acc.balance.toFixed(2)}`,
          CX + 10, y + 19, { lineBreak: false }
        );
      y += 42;

      renderTable(acc.transactions, y, SUMMARY_DEST, accountDests, tdrDests);
    }

    // ── TDR detail pages ─────────────────────────────────────────────
    for (let idx = 0; idx < data.tdr.length; idx++) {
      const t = data.tdr[idx];
      pageCount++;
      doc.addPage();
      doc.addNamedDestination(tdrDests[idx]);
      drawPageHeader();
      drawPageFooter();
      drawSidebar(SUMMARY_DEST, accountDests, tdrDests);

      y = CONTENT_TOP;
      box(CX, y, COL_W, 32, AV_LTBLUE);
      box(CX, y, 4, 32, AV_GOLD);
      doc.fillColor(AV_NAVY).font('Helvetica-Bold').fontSize(10.5)
        .text(`Term Deposit: ${t.tdrNumber}`, CX + 10, y + 5, { lineBreak: false });
      doc.fillColor(AV_MGRAY).font('Helvetica').fontSize(7.5)
        .text(
          `Rate: ${t.interestRate}%   |   Maturity: ${t.maturityDate}   |   Principal: PKR ${t.principalAmount.toFixed(2)}`,
          CX + 10, y + 19, { lineBreak: false }
        );
      y += 42;

      renderTable(t.transactions, y, SUMMARY_DEST, accountDests, tdrDests);
    }

    doc.end();
  });
};
