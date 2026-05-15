# Statement3M — Technical Reference

PDF generation engine for Avanza Solutions bank statements.  
Two independent implementations sharing the same data model and visual design:
**Python (fpdf2 + multiprocessing)** and **Node.js (PDFKit + Piscina worker pool)**.

---

## Table of Contents

1. [System Architecture](#1-system-architecture)
2. [Data Model](#2-data-model)
3. [Page Layout](#3-page-layout)
4. [Brand Palette](#4-brand-palette)
5. [Python Engine — `generator.py`](#5-python-engine--generatorpy)
6. [Node.js Engine — `pdfService.ts`](#6-nodejs-engine--pdfservicets)
7. [Internal PDF Links](#7-internal-pdf-links)
8. [Strip-Batching Optimisation](#8-strip-batching-optimisation)
9. [Concurrency Model](#9-concurrency-model)
10. [Express Server — `server.ts`](#10-express-server--serverts)
11. [Engine Comparison](#11-engine-comparison)
12. [Known Constraints & Gotchas](#12-known-constraints--gotchas)

---

## 1. System Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Express Server (server.ts)            │
│                                                         │
│  GET  /api/customers/:index   → serves CustomerData     │
│  POST /api/stress-test        → dispatches to engine    │
│  POST /api/generate-all       → generates all records   │
└────────────────┬────────────────────────┬───────────────┘
                 │                        │
        engine=node               engine=python
                 │                        │
┌────────────────▼──────────┐   ┌─────────▼──────────────┐
│  Piscina Worker Pool      │   │  child_process.spawnSync│
│  (piscinaWorker.ts)       │   │  (generator.py CLI)     │
│                           │   │                         │
│  N worker threads, each   │   │  Python multiprocessing │
│  calls generateStatementPDF│  │  Pool — one process per │
│  (PDFKit streaming)       │   │  CPU core               │
└───────────────────────────┘   └─────────────────────────┘
```

Both engines read from `bank-statements-500.txt` (array of `RawStatement` JSON objects) and write PDF files to the `output/` directory.

The React frontend at `dist/` is served statically. Single-document "Download PDF" runs the Node.js engine entirely **in-browser** via the Vite bundle (PDFKit standalone + node polyfills).

---

## 2. Data Model

### 2.1 Raw JSON (`RawStatement`)

The source file (`bank-statements-500.txt`) is a JSON array of raw bank records:

```jsonc
{
  "_id": "...",
  "statementId": "...",
  "customer": {
    "customerId": "CID-0001",
    "cif": "CIF-12345",
    "name": "Muhammad Ahmed",
    "email": "m.ahmed@email.com",
    "address": "House 5, Karachi"
  },
  "meta": { "fromDate": "01-Jan-2024", "toDate": "31-Mar-2024", "currency": "PKR" },
  "summary": {
    "accounts": [{ "accountNo": "PK36HABB...", "closingBalance": "120000", ... }],
    "termDeposits": [{ "certNo": "TD-001", "openingBalance": "500000", ... }]
  },
  "accounts": [{
    "accountNo": "PK36HABB...", "accountType": "CURRENT", "currency": "PKR",
    "transactions": [{
      "transactionDate": "2024-01-05",
      "transactionDetails": "ATM Withdrawal",
      "docNo": "DOC-001",
      "debitAmountLc": "5000",
      "creditAmountLc": "0",
      "balance": "115000"
    }]
  }],
  "termDeposits": [{
    "certNo": "TD-001",
    "tdrTransactions": [{
      "startDate": "2024-01-01", "maturity": "2025-01-01",
      "tenure": "1 Year", "rupeesAmount": "500000",
      "certificateType": "Fixed", "profitOption": "Monthly"
    }]
  }]
}
```

### 2.2 Internal `CustomerData` (TypeScript / Python dict)

Both engines work with this normalised structure after mapping:

| Field | Type | Description |
|---|---|---|
| `id` | `string` | Customer ID (`customerId` from raw) |
| `name` | `string` | Full name |
| `address` | `string` | Mailing address |
| `period` | `{ from, to }` | Statement period dates |
| `accounts` | `Account[]` | List of bank accounts |
| `tdr` | `TDR[]` | List of term deposits |

**`Account`**

| Field | Type | Description |
|---|---|---|
| `accountNumber` | `string` | IBAN / account number |
| `accountType` | `SAVINGS\|CURRENT\|SALARY\|BUSINESS` | Account classification |
| `balance` | `number` | Closing balance (from `summary.accounts.closingBalance`) |
| `currency` | `string` | Currency code |
| `transactions` | `Transaction[]` | Ledger entries |

**`TDR`**

| Field | Type | Description |
|---|---|---|
| `tdrNumber` | `string` | Certificate number |
| `principalAmount` | `number` | Opening balance from summary |
| `interestRate` | `number` | Annual rate (mapped as 0 if not in raw) |
| `maturityDate` | `string` | Maturity date from first TDR transaction |
| `status` | `ACTIVE\|MATURED` | Derived from `accountStatus` in summary |
| `transactions` | `Transaction[]` | TDR transaction history |

**`Transaction`**

| Field | Type | Description |
|---|---|---|
| `date` | `string` | Transaction date |
| `description` | `string` | Narrative (truncated to 32 chars in PDF) |
| `amount` | `number` | Absolute amount (debit if `debitAmountLc > 0`, else credit) |
| `type` | `DEBIT\|CREDIT` | Direction |
| `balance` | `number` | Running balance after transaction |

### 2.3 Mapping Functions

**TypeScript** — `dataService.ts`
```typescript
mapStatementToCustomerData(record: RawStatement): CustomerData
```

**Python** — `generator.py`
```python
map_statement(record: dict) -> dict
```

Both functions perform identical transformations:
- Resolve account balance from `summary.accounts` by `accountNo`
- Resolve TDR principal from `summary.termDeposits` by `certNo`
- Derive `type` from whichever of `debitAmountLc` / `creditAmountLc` is non-zero
- Truncate TDR description to `"{certificateType} - {tenure} - {profitOption}"`

---

## 3. Page Layout

All coordinates are in PDF points (1 pt = 1/72 inch). A4 = 595.28 × 841.89 pt.

```
┌──────────────────────────────────────────────────────────┐  y=0
│                    HEADER  (90 pt tall)                   │
│  ┌──────────┐  │  BANK STATEMENT   │  [chart image]      │
│  │ AV logo  │  │  Customer name    │                     │
│  └──────────┘  │  Period           │                     │
├──────────────────────────────────────────────────────────┤  y=90
│ SIDEBAR │                                                 │
│  (92pt) │          CONTENT AREA                          │
│         │    x=107, width=473pt                          │
│ NAVIGATE│    (PAGE_W - CX - 15)                          │
│         │                                                 │
│ > Summary                                                │
│ ACCOUNTS│                                                 │
│ > acc1  │                                                 │
│ TDR     │                                                 │
│ > tdr1  │                                                 │
│         │                                                 │
├──────────────────────────────────────────────────────────┤  y=797.89
│                    FOOTER  (44 pt tall)                   │
│  Avanza Solutions PVT. LTD.   Page N   [f][in][tw][yt][wa]│
└──────────────────────────────────────────────────────────┘  y=841.89
```

| Constant | Value | Description |
|---|---|---|
| `PAGE_W` | 595.28 pt | A4 width |
| `PAGE_H` | 841.89 pt | A4 height |
| `HEADER_H` | 90 pt | Header band height |
| `FOOTER_H` | 44 pt | Footer band height |
| `SIDEBAR_W` | 92 pt | Sidebar panel width |
| `CX` | 107 pt | Content area left edge (= `SIDEBAR_W + 15`) |
| `CONTENT_TOP` | 98 pt | First content y (= `HEADER_H + 8`) |
| `COL_W` | 473.28 pt | Transaction table width (= `PAGE_W - CX - 15`) |

---

## 4. Brand Palette

| Name | Hex | Usage |
|---|---|---|
| `AV_NAVY` | `#1E3A5F` | Header/footer background, table headers, section borders |
| `AV_DNAVY` | `#152B47` | Logo panel background, chart panel |
| `AV_BLUE` | `#0052CC` | Sidebar link text, clickable row text |
| `AV_TEAL` | `#00B4A0` | Accent strip (left border of tables), header underline, logo diamond |
| `AV_GOLD` | `#F0A500` | Right border accent on table headers, trend line in chart |
| `AV_LTBLUE` | `#EAF2FF` | Alternating row background, customer card |
| `AV_SIDEBAR` | `#EEF3FA` | Sidebar panel background |
| `AV_LBLUE` | `#7FB3D3` | Secondary text in header/footer |
| `AV_DGRAY` | `#2C3E50` | Transaction row text, body values |
| `AV_MGRAY` | `#5A6A7A` | Section labels, secondary text |

---

## 5. Python Engine — `generator.py`

**Library:** `fpdf2 >= 2.8` (LGPL — free for commercial use)  
**Parallelism:** `multiprocessing.Pool` — one OS process per CPU core  
**Coordinate system:** Origin at top-left; y increases downward

### 5.1 `_PDF` — Stateful Wrapper Class

`_PDF` extends `FPDF` and adds a thin state cache to skip redundant library calls.
Every call to `set_font`, `set_fill_color`, `set_text_color`, and `set_line_width`
goes through the OS PDF driver; skipping duplicates is measurable when rendering
hundreds of PDFs sequentially in the same process.

```python
class _PDF(FPDF):
    _f:  tuple | None   # (family, style, size) — current font key
    _fc: tuple | None   # (r,g,b) — current fill colour
    _tc: tuple | None   # (r,g,b) — current text colour
    _dc: tuple | None   # (r,g,b) — current draw colour
    _lw: float | None   # current line width
```

| Method | Signature | Description |
|---|---|---|
| `font(family, style, size)` | cached | Sets font only when the combination changes |
| `fill(hex)` | cached | Sets fill colour from hex string |
| `ink(hex)` | cached | Sets text colour from hex string |
| `pen(hex, width)` | cached | Sets draw colour + line width |
| `box(x, y, w, h)` | wrapper | Filled rectangle (`rect(style='F')`) |
| `txt(x, y, text, w, align, line_h)` | wrapper | `set_xy` + `cell` in one call |
| `hline(x, y, length, color, width)` | wrapper | Horizontal rule |

The global `_RGB_CACHE` dict caches hex→RGB conversions across all `_PDF` instances
in the same process, so colour parsing happens at most once per unique colour per worker.

### 5.2 Page Chrome Functions

**`_draw_page_header(pdf, customer_name, period)`**

Draws the full-width 90 pt header band. Three zones:

1. **Logo panel** (0–195 pt): Dark navy background (`AV_DNAVY`), diamond logo, company name.
2. **Title panel** (198–376 pt): Navy background, "BANK STATEMENT" title, customer name, period.
3. **Chart image** (376–593 pt): Decorative bar-chart drawn entirely with vector primitives.

The teal 3 pt vertical separator (`AV_TEAL`) at x=195 divides zones 1 and 2.
A 3 pt teal strip at y=87 closes the header.

**`_draw_diamond_logo(pdf, x, y, size=34)`**

Draws a two-tone diamond using `pdf.polygon()`:
- Outer teal diamond (4 vertices: top, right, bottom, left of bounding box)
- Inner dark-teal diamond (38% of outer size, centred)
- White "A" letter, centred in the diamond, at 40% of `size` font size

**`_draw_header_image(pdf, x, y, w, h)`**

Synthetic financial analytics image built from primitives:
- 3-panel dark gradient base
- 9 vertical bars at varying heights and blue/teal shades
- 1.5 pt white highlight cap on each bar
- Gold polyline trend across bars 1, 3, 5, 7, 9
- Gold filled ellipse at the peak (bar 9)
- "FINANCIAL ANALYTICS" label at the bottom

**`_draw_page_footer(pdf)`**

Draws the 44 pt footer band at `y = PAGE_H - FOOTER_H`:
- Full-width navy background with 2 pt teal top border
- Left column: company name, address, disclaimer
- Right: "Page N" in light blue (uses `pdf.page_no()`)
- Far-right: 5 social media icon boxes (f / in / tw / yt / wa) with platform colours

**`_draw_sidebar(pdf, data, nav_links)`**

Draws the left navigation panel and registers clickable link annotations:
- Light-blue background (`AV_SIDEBAR`) with 3 pt teal left accent
- "NAVIGATE" heading + teal rule
- "Summary" link → `nav_links['summary']` (an fpdf2 link ID)
- Up to 8 account links → `nav_links['accounts'][i]`
- Up to 5 TDR links → `nav_links['tdr'][i]`
- "AVANZA" wordmark at the sidebar bottom

Each clickable area is registered with `pdf.link(x, y, w, h, link_id)`. The
coordinates cover the full sidebar width minus the 3 pt accent, making the hit
target the entire row rather than just the text.

### 5.3 Core Renderer — `_render_pdf(data)`

The top-level function that produces a complete PDF as `bytes`.

**Link pre-initialisation (critical)**

fpdf2 requires a link ID to have a page assigned before it can be used in
`pdf.link()`. Since the sidebar is drawn on page 1 before account pages exist,
all link IDs are pre-seeded with `page=1` as a placeholder:

```python
def _make_link() -> int:
    lid = pdf.add_link()
    pdf.set_link(lid, y=0, page=1)   # placeholder
    return lid

summary_link  = _make_link()
account_links = [_make_link() for _ in data['accounts']]
tdr_links     = [_make_link() for _ in data['tdr']]
```

When the actual target page is created, `pdf.set_link(lid)` is called again
(without `page=` argument, which defaults to the current page), updating the
destination:

```python
pdf.add_page()
pdf.set_link(account_links[i])   # now points to this page
```

**Page structure**

| Page | Content |
|---|---|
| 1 | Summary — customer card, accounts table, TDR table |
| 2…N | One account per page (card + transaction table, may span multiple pages) |
| N+1… | One TDR per page (card + transaction table) |

Every page calls `page_chrome()` which draws header, footer, and sidebar.

**`render_table(transactions, start_y)`**

See [Section 8 — Strip-Batching Optimisation](#8-strip-batching-optimisation).

**`tbl_row(y, col1, col2, idx, link_id)`**

Summary table row helper. Covers the row with a `pdf.link()` annotation over
the entire row area (`CX` to `CX + col_w`, height 17 pt) so the full row is
clickable, not just the text.

### 5.4 Public API

| Function | Input | Output |
|---|---|---|
| `_render_pdf(data: dict)` | Internal CustomerData dict | `bytes` (PDF) |
| `generate_pdf(data_json: str)` | JSON string | `bytes` (PDF) — stdin wrapper |
| `map_statement(record: dict)` | Raw RawStatement dict | Internal dict |

### 5.5 CLI Interface

```
python generator.py [count] [options]

Options:
  --file  / -f   Path to bank-statements-500.txt (JSON array)
  --output-dir   Output directory (default: output/)
  --workers  -w  Number of parallel processes (default: cpu_count())
  --chunk-size   PDFs per chunk in batch mode (default: ceil(count/workers))
  --mode         batch | single  (default: batch)
```

**Modes:**

- `batch` — Chunks the customer list, sends one chunk per worker process.
  Each worker iterates its chunk sequentially. Minimises IPC overhead.
- `single` — Sends one customer per worker call (`pool.map`). Higher IPC
  overhead but equal granularity. Useful for very large PDFs.

**Output format (stdout, consumed by server.ts):**

```json
{ "generated": 100, "duration": 4.123, "tps": 24.25, "workers": 8, "mode": "batch", "chunk_size": 13 }
```

Progress/debug information goes to **stderr** only, keeping stdout clean for JSON.

**Stdin mode:** If stdin is not a TTY, reads a single JSON document from stdin
and writes the PDF bytes to stdout. Used for one-off generation via pipe.

---

## 6. Node.js Engine — `pdfService.ts`

**Library:** `pdfkit@0.18.0` — standalone browser-compatible build  
**Import:** `pdfkit/js/pdfkit.standalone` (works in both Node.js and browser)  
**Parallelism:** `piscina` worker thread pool (see Section 9)  
**Coordinate system:** Origin at top-left; y increases downward (same as Python)

### 6.1 Drawing Helpers

```typescript
const box   = (x, y, w, h, color) => doc.rect(x, y, w, h).fill(color);
const hline = (x, y, len, color, w=0.5) =>
  doc.moveTo(x, y).lineTo(x+len, y).lineWidth(w).stroke(color);
```

PDFKit uses a fluent/chainable API — methods return `this`. `fill()` both sets
the fill colour and draws the path. Drawing a filled rectangle therefore requires
`rect().fill()`, not separate colour-set and draw steps.

### 6.2 Diamond Logo

PDFKit has no native polygon API, so the diamond uses the path API:

```typescript
doc.moveTo(x+half, y)
   .lineTo(x+size, y+half)
   .lineTo(x+half, y+size)
   .lineTo(x,      y+half)
   .closePath().fill(AV_TEAL);
```

A second, smaller path draws the inner diamond. The "A" letter is placed with
`doc.text('A', x, y + size*0.28, { width: size, align: 'center' })`.

### 6.3 Header Image

Bar chart bars are drawn with `box()`. The gold trend line uses the path API
(`moveTo` + repeated `lineTo` + `lineWidth().stroke()`). The peak dot uses
`doc.circle(cx, cy, 3).fill(AV_GOLD)`.

### 6.4 Social Icons

Each icon is three rectangles (full box, 2 pt white top stripe, body) plus a
centered text label.

### 6.5 Core Generator — `generateStatementPDF`

```typescript
export const generateStatementPDF = (
  data: CustomerData,
  outputStream?: NodeJS.WritableStream
): Promise<Uint8Array | void>
```

**Streaming mode** (`outputStream` provided — used by Piscina workers):  
PDFKit pipes directly to a file `WriteStream`. The promise resolves on the
stream's `finish` event. Memory usage is bounded — pages are written to disk
as they are rendered.

**Buffer mode** (`outputStream` omitted — used by the browser):  
Chunks are collected in an array and resolved as a `Uint8Array` on the `end`
event. The entire PDF must fit in memory.

**PDFDocument constructor options:**

```typescript
new PDFDocument({
  size: 'A4',
  margin: 0,
  autoFirstPage: false,   // prevents a blank page before the summary
  bufferPages: false,     // stream pages immediately (lower memory)
  compress: true,
})
```

`autoFirstPage: false` is essential. PDFKit creates one page automatically by
default; without this flag, the first explicit `doc.addPage()` would create page 2,
leaving a blank page 1 in every PDF.

### 6.6 Named Destination Links

PDFKit uses named string destinations instead of numeric link IDs:

```typescript
const SUMMARY_DEST  = 'AV_SUMMARY';
const accountDests  = data.accounts.map(a => `AV_ACC_${a.accountNumber}`);
const tdrDests      = data.tdr.map(t => `AV_TDR_${t.tdrNumber}`);
```

**Registering a destination** (on the target page):
```typescript
doc.addNamedDestination(SUMMARY_DEST);
```

**Creating a clickable link** (in sidebar text):
```typescript
doc.text('> Summary', 10, ny, {
  goTo: SUMMARY_DEST,
  lineBreak: false,
  width: SIDEBAR_W - 14,   // REQUIRED — see Section 12
});
```

**Creating a row-level link** (summary table):
```typescript
doc.text(col1, CX + 8, y + 4.5, {
  goTo: dest,
  lineBreak: false,
  width: 210,              // REQUIRED
});
```

No pre-initialisation step is needed — PDFKit resolves named destinations at
`doc.end()` time, so destinations can be declared after the links that point to them.

### 6.7 Page Structure

Identical to the Python engine:

```
Page 1    → Summary (customer card, accounts table, TDR table)
Page 2…N  → Account detail pages (one per account)
Page N+1… → TDR detail pages (one per TDR)
```

Each page calls `drawPageHeader()`, `drawPageFooter()`, `drawSidebar()`.

### 6.8 Table Rendering

See [Section 8 — Strip-Batching Optimisation](#8-strip-batching-optimisation).

---

## 7. Internal PDF Links

Internal links let readers click a sidebar entry or a summary row to jump to
the corresponding detail page. Each engine implements this differently due to
library constraints.

### Python (fpdf2) — Integer Link IDs

```
┌──────────────────────────────────────────────────────────┐
│ Step 1: Pre-create all link IDs                          │
│         pdf.add_link()  →  returns integer ID            │
│         pdf.set_link(id, y=0, page=1)  ← placeholder    │
│                                                           │
│ Step 2: On page 1, register clickable area               │
│         pdf.link(x, y, w, h, link_id)                   │
│         (sidebar, summary rows)                          │
│                                                           │
│ Step 3: When target page is created                       │
│         pdf.add_page()                                    │
│         pdf.set_link(link_id)  ← updates to current page │
└──────────────────────────────────────────────────────────┘
```

The pre-initialisation in Step 1 is mandatory. fpdf2 raises
`ValueError: Cannot insert link X with no page number assigned`
if `pdf.link()` references an ID whose page is still 0.

### Node.js (PDFKit) — Named String Destinations

```
┌──────────────────────────────────────────────────────────┐
│ Step 1: Define destination name strings (any time)       │
│         const dest = 'AV_ACC_PK36HABB...'               │
│                                                           │
│ Step 2: Register destination on target page              │
│         doc.addNamedDestination(dest)                    │
│                                                           │
│ Step 3: Create clickable link anywhere (even before p.2) │
│         doc.text('...', x, y, { goTo: dest, width: N }) │
└──────────────────────────────────────────────────────────┘
```

PDFKit resolves all named destinations when `doc.end()` writes the PDF trailer,
so order does not matter. No pre-initialisation is needed.

---

## 8. Strip-Batching Optimisation

Transaction tables have alternating row backgrounds (light-blue / white) and a
3 pt teal accent strip on the left edge. A naïve approach would draw both a
background rectangle and a 3×12 pt accent strip for every row.

The strip-batching optimisation defers the left accent and instead draws **one
continuous rectangle** covering the entire section height after all rows are
rendered. This reduces `rect()` calls by roughly half per section and is
especially impactful for accounts with 100–300+ transactions.

```
Naïve (per row):
  fill(LTBLUE);  box(CX, y, COL_W, 12)   ← row background
  fill(TEAL);    box(CX, y, 3, 12)        ← accent
  fill(LTBLUE);  box(CX, y+12, COL_W, 12)
  fill(TEAL);    box(CX, y+12, 3, 12)
  ... × N rows

Strip-batched:
  fill(LTBLUE);  box(CX, y,    COL_W, 12)  ← row background only
  fill(WHITE);   box(CX, y+12, COL_W, 12)
  ... × N rows
  fill(TEAL);    box(CX, sectionTop, 3, N*12)  ← ONE accent strip
```

The single teal strip is drawn last; because PDF painters are ordered (later
objects overdraw earlier ones), this correctly covers the row-background edges
and produces a clean continuous accent line.

**Page-break handling:**

When a page break occurs mid-table, the current section is flushed (strip drawn,
bottom border added) before `add_page()`. The new page starts a fresh section:

```python
# Python
if y + 12 > content_bottom:
    pdf.fill(AV_TEAL); pdf.box(CX, section_top, 3, y - section_top)  # flush
    pdf.fill(AV_NAVY); pdf.box(CX, y, col_w, 2)                       # border
    pdf.add_page(); page_chrome()
    y = _draw_table_header(pdf, CONTENT_TOP)
    section_top = y
```

```typescript
// Node.js
if (y + 12 > contentBottom) {
  flushStrip(sectionTop, y);               // flush
  box(CX, y, COL_W, 2, AV_NAVY);          // border
  doc.addPage();
  drawPageHeader(); drawPageFooter(); drawSidebar(...);
  y = drawTableHeader(CONTENT_TOP);
  sectionTop = y;
}
```

---

## 9. Concurrency Model

### Python — `multiprocessing.Pool`

Python's GIL prevents true thread-level parallelism for CPU-bound code.
`multiprocessing.Pool` spawns independent OS processes (one per CPU core),
each with its own Python interpreter.

`_render_pdf` is pure (no shared mutable state), so processes can run completely
independently without locks.

**Batch mode** (default) is preferred for bulk generation:
```
customers = [c0, c1, c2, c3, c4, c5, c6, c7]  # 8 customers, 4 workers
chunks    = [[c0, c1], [c2, c3], [c4, c5], [c6, c7]]
pool.map(_worker_batch, [(chunk, outputDir) for chunk in chunks])
```

Each worker receives a contiguous slice and writes PDFs in a tight loop,
avoiding per-document IPC overhead. **Single mode** sends one customer per
`pool.map` call — higher IPC but useful when individual documents are very large.

### Node.js — `piscina` Worker Thread Pool

Node.js is single-threaded by default. `piscina` creates a pool of worker
threads using the `worker_threads` module. Workers inherit the `tsx` ESM loader
from the parent process, so TypeScript files (`.ts`) are executed directly
without a separate build step.

```typescript
const pool = new Piscina({
  filename: 'src/services/piscinaWorker.ts',
  minThreads: os.cpus().length,
  maxThreads: os.cpus().length,
  idleTimeout: 30000,
});
```

The pool size is fixed at `cpuCount` threads. Each stress-test request is split
into batches of `ceil(totalCount / (cpuCount * 4))` documents and dispatched as
independent tasks:

```typescript
for (let i = 0; i < totalCount; i += batchSize) {
  tasks.push(pool.run({ items: customers.slice(i, i+batchSize), outputDir }));
}
await Promise.all(tasks);
```

`piscinaWorker.ts` processes its batch sequentially (one PDF at a time), streaming
each document directly to a `WriteStream` to minimise memory usage.

### Why Multiprocessing vs Worker Threads?

| | Python `multiprocessing` | Node.js `piscina` |
|---|---|---|
| Isolation | Separate processes | Shared-memory threads |
| GIL | Bypassed (each process has own GIL) | Not applicable |
| IPC cost | Moderate (pickle serialisation) | Low (SharedArrayBuffer / structured clone) |
| Memory | Higher (separate heap per process) | Lower (shared V8 heap) |
| Crash isolation | One crash doesn't kill pool | Thread crash can affect pool |
| Startup | ~200–500 ms per process | ~50–100 ms per thread |

---

## 10. Express Server — `server.ts`

**Runtime:** Node.js + `tsx` (TypeScript execution without compilation)  
**Port:** 3000 (hardcoded)

### Endpoints

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/customers/count` | Returns `{ total: N }` |
| `GET` | `/api/customers/:index` | Returns `CustomerData` at zero-based index |
| `POST` | `/api/stress-test` | Bulk PDF generation (see below) |
| `POST` | `/api/generate-all` | Generate all 500 loaded customers |
| `GET` | `/*` | Serve `dist/index.html` (React SPA) |

### `POST /api/stress-test` Request Body

```jsonc
{
  "totalCount": 100,     // number of PDFs to generate
  "engine": "node"       // "node" | "python"
}
```

**Node path:** Creates Piscina tasks, awaits `Promise.all`, returns throughput metrics.

**Python path:** Calls `generator.py` via `child_process.spawnSync`, reads the
JSON result from stdout, and returns it to the client. Stderr from Python is
captured but not forwarded.

### Response Shape

```jsonc
{
  "success": true,
  "totalGenerated": 100,
  "duration": 4.23,
  "tps": 23.64,
  "engine": "Node.js (Optimized Pool)",
  "coresUsed": 8
}
```

### Data Loading

`bank-statements-500.txt` is loaded synchronously at startup via `fs.readFileSync`
and mapped to `CustomerData[]`. If the file is absent, the server starts anyway
and falls back to generated data via `generateCustomerData()`.

### Static Frontend

`dist/` is served via `express.static`. The React app bundles `pdfService.ts`
via Vite (using `pdfkit/js/pdfkit.standalone` + node polyfills), enabling
single-document PDF generation directly in the browser without a server round-trip.

---

## 11. Engine Comparison

| Aspect | Python (`generator.py`) | Node.js (`pdfService.ts`) |
|---|---|---|
| PDF library | fpdf2 2.8.x (LGPL) | pdfkit 0.18.0 (MIT) |
| Language | Python 3.10+ | TypeScript 5.x |
| Parallelism | `multiprocessing.Pool` | `piscina` worker threads |
| Link mechanism | Integer IDs (`add_link` / `set_link`) | Named strings (`addNamedDestination`) |
| Link pre-init | Required (placeholder page=1) | Not required |
| `goTo` width | N/A | Must pass explicit `width` in `text()` options |
| Font support | Core fonts only (Helvetica, Courier) — Latin-1 only | Same |
| Page creation | `pdf.add_page()` | `doc.addPage()` (with `autoFirstPage: false`) |
| Streaming | Writes bytes at `pdf.output()` | Streams to `WriteStream` as pages render |
| Browser support | No | Yes (via standalone bundle + Vite polyfills) |
| Throughput (observed) | ~20–25 PDF/sec (8 cores) | ~15–20 PDF/sec (8 threads) |
| Output filename prefix | `PY-{id}.pdf` | `{id}.pdf` |

### Identical Design Choices

- Same page dimensions and constants (`PAGE_W`, `PAGE_H`, `HEADER_H`, etc.)
- Same brand palette (hex values match exactly)
- Same 3-zone header layout (logo panel / title panel / chart image)
- Same sidebar layout, link positions, and hit areas
- Same strip-batching optimisation logic
- Same table column widths and row height (12 pt)
- Same font sizes and styles at each position

---

## 12. Known Constraints & Gotchas

### PDFKit `goTo` requires explicit `width`

**Symptom:** `Error: unsupported number: NaN` at `doc.end()`, crashing the worker.

**Cause:** When `doc.text()` includes `{ goTo: dest }`, PDFKit creates a link
annotation using `options.textWidth` as the annotation width. Without an explicit
`width` in the text options, `textWidth` is `undefined`, making the annotation
coordinate `NaN`.

**Fix:** Always include `width` when using `goTo`:
```typescript
doc.text('> Summary', 10, ny, {
  goTo: SUMMARY_DEST,
  lineBreak: false,
  width: SIDEBAR_W - 14,   // ← required
});
```

### fpdf2 Unicode (Latin-1 only for core fonts)

Core PDF fonts (Helvetica, Courier, Times) only support Latin-1 (ISO-8859-1).
Unicode characters like `•` (U+2022) raise `FPDFUnicodeEncodingException`.

**Fix:** Use ASCII equivalents (e.g., `'-'` instead of `'•'`). To support full
Unicode, register a TTF font:
```python
pdf.add_font('DejaVu', '', 'DejaVuSans.ttf', uni=True)
pdf.set_font('DejaVu', '', 8)
```

### fpdf2 link must have page assigned before `pdf.link()` is called

fpdf2 validates link IDs at the point `pdf.link()` is called, not at
`pdf.output()`. Because the sidebar (on page 1) references account pages that
don't yet exist, all link IDs must be pre-seeded with a valid page number.

The current code uses `page=1` as the placeholder. This is valid as long as the
summary page is always page 1 (which it always is).

### `autoFirstPage: false` in PDFKit

Without this constructor option, PDFKit creates page 1 automatically. The first
`doc.addPage()` call then creates page 2, leaving a blank first page in the PDF.

### `bufferPages: false` and named destinations

Named destinations are written to the PDF trailer at `doc.end()`. They work
correctly with `bufferPages: false` (streaming mode) because PDFKit stores them
in memory and writes them at the end regardless of when pages are streamed.

### Python `multiprocessing` on Windows

Python's `multiprocessing` uses `spawn` on Windows (not `fork`). Each worker
process re-imports the module from scratch, which adds ~200–400 ms cold-start
overhead. The batch mode amortises this across the chunk, making it efficient
for production loads. The `if __name__ == '__main__':` guard in `generator.py`
is required to prevent recursive spawning on Windows.

### Transaction description truncation

Both engines truncate `description` to 32 characters for the transaction table
column. This matches the fixed-width Courier column layout. Longer descriptions
are silently clipped; no ellipsis is added.
