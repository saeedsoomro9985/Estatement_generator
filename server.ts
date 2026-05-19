import express from "express";
import path from "path";
import fs from "fs";
import { fileURLToPath } from "url";
import Piscina from "piscina";
import os from "os";
import { mapStatementToCustomerData, generateCustomerData, CustomerData, RawStatement } from "./src/services/dataService.ts";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// ── Load customer data from JSON file ────────────────────────────────────────

let customers: CustomerData[] = [];

function loadStatementData() {
  const filePath = path.join(process.cwd(), "bank-statements-500.txt");
  if (!fs.existsSync(filePath)) {
    console.warn("⚠️  bank-statements-500.txt not found — stress test will use generated data");
    return;
  }
  console.log("📂 Loading bank-statements-500.txt ...");
  const raw: RawStatement[] = JSON.parse(fs.readFileSync(filePath, "utf-8"));
  customers = raw.map(mapStatementToCustomerData);
  console.log(`✅ Loaded and mapped ${customers.length} customer statements`);
}

loadStatementData();

/** Resolve pdf_oxide_gen binary (release preferred, then debug). */
function findRustBinary(): string | null {
  const candidates = [
    path.join(process.cwd(), "pdf_oxide_gen", "target", "release", "pdf_oxide_gen.exe"),
    path.join(process.cwd(), "pdf_oxide_gen", "target", "release", "pdf_oxide_gen"),
    path.join(process.cwd(), "pdf_oxide_gen", "target", "debug", "pdf_oxide_gen.exe"),
    path.join(process.cwd(), "pdf_oxide_gen", "target", "debug", "pdf_oxide_gen"),
  ];
  return candidates.find(fs.existsSync) ?? null;
}

/** Default max wait for Rust batch (Mongo + PDF). Override with RUST_PDF_TIMEOUT_MS. */
const RUST_PDF_TIMEOUT_MS = Number(process.env.RUST_PDF_TIMEOUT_MS) || 10 * 60 * 1000;

function buildMssqlConnectionString(): string {
  if (process.env.MSSQL_URL?.trim()) return process.env.MSSQL_URL.trim();
  const server = process.env.MSSQL_SERVER ?? "localhost\\SQLEXPRESS02";
  const port = process.env.MSSQL_PORT?.trim();
  const serverWithPort = port ? `${server},${port}` : server;
  const database = process.env.MSSQL_DATABASE ?? "Statements";
  const user = process.env.MSSQL_USER ?? "sa";
  const password = process.env.MSSQL_PASSWORD ?? "Realme5i+123";
  return `Driver={ODBC Driver 17 for SQL Server};Server=localhost,1433;Database=Statements;Uid=sa;Pwd=Realme5i+123;TrustServerCertificate=Yes;Encrypt=no;`;
}

const DEFAULT_MACHINE_ID = process.env.MACHINE_ID ?? "PSG-BSD-SAEED1";
const QUEUE_BATCH_SIZE = Number(process.env.QUEUE_BATCH_SIZE) || 200;
const POLL_INTERVAL_MS = Number(process.env.POLL_INTERVAL_MS) || 200;
const MSSQL_BATCH_SIZE = Number(process.env.MSSQL_BATCH_SIZE) || 100;

/** PDFs written by pdf_oxide_gen use this prefix. */
const OXIDE_PDF_PREFIX = "OXIDE-";

function listOxidePdfs(dir: string): string[] {
  if (!fs.existsSync(dir)) return [];
  return fs
    .readdirSync(dir)
    .filter((f) => f.startsWith(OXIDE_PDF_PREFIX) && f.toLowerCase().endsWith(".pdf"))
    .sort();
}

/** Snapshot filename → mtimeMs for detecting new writes after a Rust run. */
function snapshotOxidePdfs(dir: string): Map<string, number> {
  const snap = new Map<string, number>();
  for (const name of listOxidePdfs(dir)) {
    try {
      snap.set(name, fs.statSync(path.join(dir, name)).mtimeMs);
    } catch {
      /* file removed between list and stat */
    }
  }
  return snap;
}

/** Files that are new or were modified since `before`. */
function newOrUpdatedOxidePdfs(dir: string, before: Map<string, number>): string[] {
  const out: string[] = [];
  for (const name of listOxidePdfs(dir)) {
    try {
      const mtime = fs.statSync(path.join(dir, name)).mtimeMs;
      const prev = before.get(name);
      if (prev === undefined || mtime > prev) {
        out.push(name);
      }
    } catch {
      /* skip */
    }
  }
  return out;
}

function normalizeDirForCompare(p: string): string {
  return path.resolve(p).replace(/^\\\\\?\\/, "").toLowerCase();
}

function logOutputDirState(label: string, dir: string) {
  const exists = fs.existsSync(dir);
  const files = exists ? listOxidePdfs(dir) : [];
  console.log(
    `[rust-pdf] ${label} dir=${dir} exists=${exists} OXIDE-pdf_count=${files.length}`
  );
  if (files.length > 0) {
    const sample = files.slice(0, 5).map((f) => path.join(dir, f));
    console.log(`[rust-pdf] ${label} sample: ${sample.join("; ")}`);
  }
}

// ─────────────────────────────────────────────────────────────────────────────

async function startServer() {
  const app = express();
  const PORT = 3000;

  // Ensure output directory exists for saved PDFs
  const outputDir = path.join(process.cwd(), "output");
  if (!fs.existsSync(outputDir)) {
    fs.mkdirSync(outputDir, { recursive: true });
    console.log(`📁 Created output directory at ${outputDir}`);
  }

  // Initialize Worker Pool for individual PDF generation
  const workerFile = path.resolve(__dirname, "src/services/piscinaWorker.ts");

  const numCPU = os.cpus().length;
  const pool = new Piscina({
    filename: workerFile,
    minThreads: numCPU,
    maxThreads: numCPU,
    // Allow each thread to hold 2 tasks so the next batch starts
    // immediately when one finishes rather than waiting for dispatch.
    concurrentTasksPerWorker: 2,
    idleTimeout: 60000,
  });

  app.use(express.json());

  // API: Total count of loaded customers
  app.get("/api/customers/count", (_req, res) => {
    res.json({ total: customers.length });
  });

  // API: Get a specific customer by zero-based index
  app.get("/api/customers/:index", (req, res) => {
    const idx = parseInt(req.params.index, 10);
    if (isNaN(idx) || idx < 0 || idx >= customers.length) {
      return res.status(404).json({ error: "Customer not found" });
    }
    res.json(customers[idx]);
  });

  // API: Generate all loaded customers to output/
  app.post("/api/generate-all", async (_req, res) => {
    if (customers.length === 0) {
      return res.status(400).json({ success: false, error: "No customer data loaded" });
    }
    const startTime = Date.now();
    try {
      // 2× workers gives the pool 2 chunks per thread so it stays busy
      // while the previous chunk is being written to disk.
      const batchSize = Math.max(1, Math.ceil(customers.length / (numCPU * 2)));
      const tasks = [];
      for (let i = 0; i < customers.length; i += batchSize) {
        const items = customers.slice(i, i + batchSize);
        tasks.push(pool.run({ items, outputDir }));
      }
      const results = await Promise.all(tasks);
      const duration = (Date.now() - startTime) / 1000;
      const totalGenerated = results.flat().length;
      res.json({ success: true, totalGenerated, duration, tps: parseFloat((totalGenerated / duration).toFixed(2)) });
    } catch (error) {
      res.status(500).json({ success: false, error: String(error) });
    }
  });

  // API: Stress Test Endpoint
  app.post("/api/stress-test", async (req, res) => {
    const { totalCount, engine = "node", machineId: bodyMachineId } = req.body;
    console.log(`[stress-test] engine=${engine} totalCount=${totalCount}`);

    if (engine === "rust") {
      const rustBinaryPath = findRustBinary();
      if (!rustBinaryPath) {
        console.error("[rust-pdf] binary not found — run: cd pdf_oxide_gen && cargo build --release");
        return res.status(400).json({
          success: false,
          error:
            "pdf_oxide_gen binary not found. " +
            "Build it first: cd pdf_oxide_gen && cargo build --release",
        });
      }

      const mongoUri = process.env.MONGODB_URI ?? "mongodb://localhost:27017";
      const mongoDatabase = process.env.MONGODB_DATABASE ?? "EStatements";
      const mongoCollection = process.env.MONGODB_COLLECTION ?? "Statements";
      const outputDirAbs = path.resolve(outputDir);
      fs.mkdirSync(outputDirAbs, { recursive: true });
      const channelCap = Math.max(512, QUEUE_BATCH_SIZE * 4);
      const machineId = String(bodyMachineId ?? DEFAULT_MACHINE_ID);
      const mssqlUrl = buildMssqlConnectionString();
      const maxRecords = Number(totalCount) || 0;
      const args = [
        String(maxRecords),
        "--mongo-uri", mongoUri,
        "--database", mongoDatabase,
        "--collection", mongoCollection,
        "--output-dir", outputDirAbs,
        "--workers", String(numCPU),
        "--chunk-size", String(channelCap),
        "--mode", "queue",
        "--machine-id", machineId,
        "--mssql-url", mssqlUrl,
        "--queue-batch-size", String(QUEUE_BATCH_SIZE),
        "--poll-interval-ms", String(POLL_INTERVAL_MS),
        "--sql-batch-size", String(MSSQL_BATCH_SIZE),
      ];

      console.log(`[rust-pdf] machine_id=${machineId} queue_batch=${QUEUE_BATCH_SIZE}`);

      logOutputDirState("BEFORE spawn", outputDirAbs);
      const pdfSnapshotBefore = snapshotOxidePdfs(outputDirAbs);

      console.log(`[rust-pdf] spawning: ${rustBinaryPath}`);
      console.log(`[rust-pdf] cwd: ${process.cwd()}`);
      console.log(`[rust-pdf] expected output: ${outputDirAbs}`);
      console.log(`[rust-pdf] args: ${args.join(" ")}`);
      console.log(`[rust-pdf] timeout: ${RUST_PDF_TIMEOUT_MS}ms`);
      console.log(
        `[rust-pdf] NOTE: PDFs are written here (not pdf_oxide_gen/output): ${outputDirAbs}`
      );

      const { spawnSync } = await import("child_process");
      const started = Date.now();

      const result = spawnSync(rustBinaryPath, args, {
        encoding: "utf8",
        maxBuffer: 16 * 1024 * 1024,
        timeout: RUST_PDF_TIMEOUT_MS,
        cwd: process.cwd(),
      });

      const elapsed = ((Date.now() - started) / 1000).toFixed(2);
      console.log(
        `[rust-pdf] finished in ${elapsed}s | status=${result.status} signal=${result.signal ?? "none"}`
      );

      if (result.stderr?.trim()) {
        console.error(`[rust-pdf] stderr:\n${result.stderr}`);
      }
      if (result.stdout?.trim()) {
        console.log(`[rust-pdf] stdout:\n${result.stdout}`);
      }

      if (result.error) {
        const msg = `Failed to start pdf_oxide_gen: ${result.error.message}`;
        console.error(`[rust-pdf] ${msg}`);
        return res.status(500).json({ success: false, error: msg });
      }

      if (result.signal === "SIGTERM") {
        const msg = `Rust PDF generation timed out after ${RUST_PDF_TIMEOUT_MS / 1000}s. Check MongoDB (${mongoUri}) and server console.`;
        console.error(`[rust-pdf] ${msg}`);
        return res.status(504).json({ success: false, error: msg, stderr: result.stderr?.trim() || undefined });
      }

      if (result.status !== 0) {
        const msg =
          result.stderr?.trim() ||
          `pdf_oxide_gen exited with code ${result.status ?? "unknown"}`;
        console.error(`[rust-pdf] failed: ${msg}`);
        return res.status(500).json({
          success: false,
          error: msg,
          exitCode: result.status,
          stdout: result.stdout?.trim() || undefined,
        });
      }

      let oxideResult: {
        generated: number;
        duration: number;
        tps: number;
        workers: number;
        chunk_size: number;
        mode: string;
        output_dir?: string;
        written?: number;
        produced?: number;
        decoded?: number;
        rendered?: number;
      } | null = null;

      const parseStdoutJson = (stdout: string) => {
        const trimmed = stdout.trim();
        if (!trimmed) return null;
        try {
          return JSON.parse(trimmed);
        } catch {
          const lines = trimmed.split(/\r?\n/).filter((l) => l.trim().length > 0);
          for (let i = lines.length - 1; i >= 0; i--) {
            const line = lines[i].trim();
            if (line.startsWith("{")) {
              try {
                return JSON.parse(line);
              } catch {
                /* try previous line */
              }
            }
          }
          const match = trimmed.match(/\{[\s\S]*\}/);
          if (match) {
            try {
              return JSON.parse(match[0]);
            } catch {
              return null;
            }
          }
          return null;
        }
      };

      oxideResult = parseStdoutJson(result.stdout ?? "");

      if (!oxideResult) {
        const msg = "pdf_oxide_gen succeeded but returned no JSON on stdout";
        console.error(`[rust-pdf] ${msg}`);
        return res.status(500).json({
          success: false,
          error: msg,
          stderr: result.stderr?.trim() || undefined,
          stdout: result.stdout?.trim() || undefined,
        });
      }

      logOutputDirState("AFTER spawn", outputDirAbs);

      const rustReportedDir = oxideResult.output_dir
        ? path.resolve(oxideResult.output_dir)
        : outputDirAbs;
      if (
        normalizeDirForCompare(rustReportedDir) !==
        normalizeDirForCompare(outputDirAbs)
      ) {
        console.warn(
          `[rust-pdf] path mismatch: server expected ${outputDirAbs}, Rust wrote to ${rustReportedDir}`
        );
      }

      const allPdfFiles = listOxidePdfs(outputDirAbs);
      const newPdfFiles = newOrUpdatedOxidePdfs(outputDirAbs, pdfSnapshotBefore);
      const rustWritten = oxideResult.written ?? oxideResult.generated;

      console.log(
        `[rust-pdf] verify | json.generated=${oxideResult.generated} json.written=${rustWritten} ` +
          `json.produced=${oxideResult.produced ?? "?"} decoded=${oxideResult.decoded ?? "?"} ` +
          `rendered=${oxideResult.rendered ?? "?"} | all_on_disk=${allPdfFiles.length} ` +
          `new_this_run=${newPdfFiles.length}`
      );
      if (newPdfFiles.length > 0) {
        console.log(
          `[rust-pdf] new files: ${newPdfFiles
            .slice(0, 10)
            .map((f) => path.join(outputDirAbs, f))
            .join("; ")}${newPdfFiles.length > 10 ? " …" : ""}`
        );
      }

      const stderrSnippet = result.stderr?.trim().slice(-4000);

      if (newPdfFiles.length === 0) {
        const msg =
          `Rust finished but wrote 0 new PDF(s) to ${outputDirAbs}. ` +
          `JSON reported ${rustWritten} written. ` +
          `Look in project root /output (not pdf_oxide_gen/output). ` +
          `Check MongoDB (${mongoUri}) and server console [rust-pdf] logs.`;
        return res.status(500).json({
          success: false,
          error: msg,
          outputDir: outputDirAbs,
          rustOutputDir: rustReportedDir,
          expectedCount: Number(totalCount),
          reportedGenerated: oxideResult.generated,
          reportedWritten: rustWritten,
          allPdfCount: allPdfFiles.length,
          stderr: stderrSnippet,
          stdout: result.stdout?.trim() || undefined,
        });
      }

      if (newPdfFiles.length < rustWritten) {
        console.warn(
          `[rust-pdf] partial write: Rust reported ${rustWritten} written but only ${newPdfFiles.length} new file(s) detected`
        );
      }

      return res.json({
        success: true,
        totalGenerated: oxideResult.generated,
        filesWritten: newPdfFiles.length,
        filesOnDisk: allPdfFiles.length,
        duration: oxideResult.duration,
        tps: oxideResult.tps,
        engine: `Rust · pdf-oxide · ${oxideResult.workers} threads · chunk ${oxideResult.chunk_size}`,
        outputDir: outputDirAbs,
        rustOutputDir: rustReportedDir,
        pdfFiles: newPdfFiles.slice(0, 20).map((f) => path.join(outputDirAbs, f)),
        pipeline: {
          produced: oxideResult.produced,
          decoded: oxideResult.decoded,
          rendered: oxideResult.rendered,
          written: oxideResult.written ?? oxideResult.generated,
        },
        stderrSnippet: stderrSnippet?.slice(-500),
      });
    }

    if (engine === "python" || engine === "python-rl") {
      console.log(`[python-pdf] stress-test start count=${totalCount} engine=${engine}`);
      const useRL = engine === "python-rl";
      const scriptName = useRL ? "generator_rl.py" : "generator.py";
      const pythonPath = path.join(process.cwd(), scriptName);
      const jsonFilePath = path.join(process.cwd(), "bank-statements-500.txt");

      if (!fs.existsSync(pythonPath)) {
        return res.status(400).json({ success: false, error: `${scriptName} not found.` });
      }
      if (!fs.existsSync(jsonFilePath)) {
        return res.status(400).json({ success: false, error: "bank-statements-500.txt not found." });
      }

      const { spawnSync } = await import("child_process");

      const pythonCmd = (() => {
        const winPython = "C:\\Users\\ibrahim.zoaib\\AppData\\Local\\Programs\\Python\\Python313\\python.exe";
        if (fs.existsSync(winPython)) return winPython;
        for (const cmd of ["python3", "python"]) {
          const r = spawnSync(cmd, ["--version"]);
          if (!r.error && r.status === 0) return cmd;
        }
        return "python";
      })();

      const result = spawnSync(
        pythonCmd,
        [
          pythonPath,
          String(totalCount),
          "--file", jsonFilePath,
          "--output-dir", outputDir,
          "--workers", String(numCPU),
          "--mode", "batch",
        ],
        { encoding: "utf8", maxBuffer: 4 * 1024 * 1024 }
      );

      if (result.stderr?.trim()) {
        console.error(`[python-pdf] stderr:\n${result.stderr}`);
      }

      if (result.status !== 0 && !result.stdout?.trim()) {
        const msg = result.stderr?.trim() || "Python script failed";
        console.error(`[python-pdf] failed: ${msg}`);
        return res.status(500).json({ success: false, error: msg, exitCode: result.status });
      }

      let pyResult: { generated: number; duration: number; tps: number; workers: number; chunk_size: number } | null = null;
      try { pyResult = JSON.parse(result.stdout.trim()); } catch { /* ignore */ }

      const libLabel = useRL ? "ReportLab (C-ext)" : "fpdf2";
      return res.json({
        success: true,
        totalGenerated: pyResult?.generated ?? totalCount,
        duration: pyResult?.duration ?? 0,
        tps: pyResult?.tps ?? 0,
        engine: `Python · ${libLabel} · ${pyResult?.workers ?? numCPU} processes · chunk ${pyResult?.chunk_size ?? '?'}`,
        outputDir,
      });
    }

    const startTime = Date.now();
    try {
      const batchSize = Math.max(1, Math.ceil(totalCount / (numCPU * 2)));

      const tasks = [];
      for (let i = 0; i < totalCount; i += batchSize) {
        const items = [];
        for (let j = 0; j < batchSize && i + j < totalCount; j++) {
          const globalIdx = i + j;
          // Cycle through loaded JSON customers; fall back to generated data if none loaded
          const base = customers.length > 0
            ? customers[globalIdx % customers.length]
            : generateCustomerData();
          // Keep actual customer ID so the PDF filename matches the customer record
          items.push({ ...base });
        }
        tasks.push(pool.run({ items, outputDir }));
      }

      const results = await Promise.all(tasks);
      const duration = (Date.now() - startTime) / 1000;
      const totalGenerated = results.flat().length;

      res.json({
        success: true,
        totalGenerated,
        duration,
        tps: parseFloat((totalGenerated / duration).toFixed(2)),
        coresUsed: os.cpus().length,
        engine: "Node.js (Optimized Pool)",
      });
    } catch (error) {
      console.error("Worker Error:", error);
      res.status(500).json({ success: false, error: String(error) });
    }
  });

  // Serve built frontend from dist/ (run `npm run build` first)
  const distPath = path.join(process.cwd(), "dist");
  if (fs.existsSync(distPath)) {
    app.use(express.static(distPath));
    app.get("*", (req, res) => {
      res.sendFile(path.join(distPath, "index.html"));
    });
    console.log(distPath)
    console.log("📦 Serving built frontend from dist/");
  } else {
    app.get("*", (req, res) => {
      res.status(503).send(
        "<h2>Frontend not built</h2><p>Run <code>npm run build</code> first, then restart the server.</p>"
      );
    });
    console.warn("⚠️  dist/ not found — run `npm run build` to build the frontend");
  }

  app.listen(PORT, "0.0.0.0", () => {
    console.log(`🚀 Statement3M running at http://localhost:${PORT}`);
    console.log(`⚡ Multi-threading enabled with ${os.cpus().length} logical cores`);
  });
}

startServer();
