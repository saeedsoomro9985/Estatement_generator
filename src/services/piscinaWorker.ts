import { generateStatementPDF } from './pdfService';
import fs from 'fs';
import path from 'path';

/**
 * Piscina worker function utilizing PDFKit (streaming) for higher throughput.
 * Handles batches of work to minimize communication overhead.
 */
export default async ({ items, outputDir }: { items: any[], outputDir: string }) => {
  const results = [];
  for (const data of items) {
    const filePath = path.join(outputDir, `${data.id}.pdf`);
    const writeStream = fs.createWriteStream(filePath, { highWaterMark: 1024 * 1024 });
    await generateStatementPDF(data, writeStream);
    results.push({ id: data.id });
  }
  return results;
};
