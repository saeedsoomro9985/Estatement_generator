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
    const { totalCount, engine = "node" } = req.body;
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
      const args = [
        String(totalCount),
        "--mongo-uri", mongoUri,
        "--database", mongoDatabase,
        "--collection", mongoCollection,
        "--output-dir", outputDir,
        "--workers", String(numCPU),
        "--mode", "batch",
      ];

      console.log(`[rust-pdf] spawning: ${rustBinaryPath}`);
      console.log(`[rust-pdf] args: ${args.join(" ")}`);
      console.log(`[rust-pdf] timeout: ${RUST_PDF_TIMEOUT_MS}ms`);

      const { spawnSync } = await import("child_process");
      const started = Date.now();

      const result = spawnSync(rustBinaryPath, args, {
        encoding: "utf8",
        maxBuffer: 16 * 1024 * 1024,
        timeout: RUST_PDF_TIMEOUT_MS,
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
      } | null = null;

      try {
        oxideResult = JSON.parse(result.stdout.trim());
      } catch {
        const match = result.stdout?.match(/\{[\s\S]*\}/);
        if (match) {
          try {
            oxideResult = JSON.parse(match[0]);
          } catch (parseErr) {
            console.error("[rust-pdf] could not parse stdout JSON:", parseErr);
          }
        }
      }

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

      console.log(
        `[rust-pdf] ok generated=${oxideResult.generated} duration=${oxideResult.duration}s tps=${oxideResult.tps}`
      );

      return res.json({
        success: true,
        totalGenerated: oxideResult.generated,
        duration: oxideResult.duration,
        tps: oxideResult.tps,
        engine: `Rust · pdf-oxide · ${oxideResult.workers} threads · chunk ${oxideResult.chunk_size}`,
        outputDir,
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
