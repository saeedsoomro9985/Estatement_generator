import { spawnSync } from "child_process";
import fs from "fs";
import path from "path";
import os from "os";

const root = process.cwd();
const outputDir = path.join(root, "output");
const binary = [
  path.join(root, "pdf_oxide_gen", "target", "release", "pdf_oxide_gen.exe"),
  path.join(root, "pdf_oxide_gen", "target", "release", "pdf_oxide_gen"),
  path.join(root, "pdf_oxide_gen", "target", "debug", "pdf_oxide_gen.exe"),
  path.join(root, "pdf_oxide_gen", "target", "debug", "pdf_oxide_gen"),
].find(fs.existsSync);

if (!binary) {
  console.error("pdf_oxide_gen binary not found. Run: npm run build:rust");
  process.exit(1);
}

fs.mkdirSync(outputDir, { recursive: true });
const count = Number(process.argv[2] || 2);

const args = [
  String(count),
  "--output-dir", outputDir,
  "--mongo-uri", process.env.MONGODB_URI || "mongodb://localhost:27017",
  "--database", process.env.MONGODB_DATABASE || "EStatements",
  "--collection", process.env.MONGODB_COLLECTION || "Statements",
  "--workers", String(os.cpus().length),
  "--chunk-size", "512",
  "--mode", "batch",
];

console.log("binary:", binary);
console.log("args:", args.join(" "));

const result = spawnSync(binary, args, {
  encoding: "utf8",
  cwd: root,
  maxBuffer: 16 * 1024 * 1024,
});

if (result.stderr) console.error("stderr:\n", result.stderr);
if (result.stdout) console.log("stdout:\n", result.stdout);
console.log("exit:", result.status, "signal:", result.signal);

const pdfs = fs.readdirSync(outputDir).filter((f) => f.startsWith("OXIDE-") && f.endsWith(".pdf"));
console.log("PDF files:", pdfs.length, pdfs.slice(0, 5));
process.exit(result.status === 0 && pdfs.length > 0 ? 0 : 1);
