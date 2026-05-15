/**
 * @license
 * SPDX-License-Identifier: Apache-2.0
 */

import React, { useState, useEffect, useCallback } from 'react';
import { motion, AnimatePresence } from 'motion/react';
import {
  Download,
  Zap,
  Database,
  FileText,
  CheckCircle2,
  BarChart3,
  Users,
  Activity,
  AlertCircle,
  Play,
  ChevronLeft,
  ChevronRight,
} from 'lucide-react';
import { CustomerData } from './services/dataService';
import { generateStatementPDF } from './services/pdfService';

export default function App() {
  const [currentCustomer, setCurrentCustomer] = useState<CustomerData | null>(null);
  const [customerIndex, setCustomerIndex] = useState(0);
  const [totalCustomers, setTotalCustomers] = useState(0);
  const [isLoadingCustomer, setIsLoadingCustomer] = useState(false);
  const [isGenerating, setIsGenerating] = useState(false);
  const [testMode, setTestMode] = useState(false);
  const [testCount, setTestCount] = useState(100);
  const [testProgress, setTestProgress] = useState(0);
  const [results, setResults] = useState<{ tps: number; totalTime: number; engine: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [activeEngine, setActiveEngine] = useState<'node' | 'python' | 'python-rl' | 'rust'>('node');

  const fetchCustomer = useCallback(async (index: number) => {
    setIsLoadingCustomer(true);
    try {
      const res = await fetch(`/api/customers/${index}`);
      if (!res.ok) throw new Error('Not found');
      const data: CustomerData = await res.json();
      setCurrentCustomer(data);
      setCustomerIndex(index);
    } catch {
      setError(`Failed to load customer at index ${index}`);
    } finally {
      setIsLoadingCustomer(false);
    }
  }, []);

  useEffect(() => {
    fetch('/api/customers/count')
      .then(r => r.json())
      .then(data => {
        setTotalCustomers(data.total);
        fetchCustomer(0);
      })
      .catch(() => setError('Could not reach server — is it running?'));
  }, [fetchCustomer]);

  const goToPrev = () => {
    if (totalCustomers === 0 || isLoadingCustomer) return;
    fetchCustomer((customerIndex - 1 + totalCustomers) % totalCustomers);
  };

  const goToNext = () => {
    if (totalCustomers === 0 || isLoadingCustomer) return;
    fetchCustomer((customerIndex + 1) % totalCustomers);
  };

  const handleDownloadSingle = async () => {
    if (!currentCustomer) return;
    setIsGenerating(true);
    try {
      const pdfData = await generateStatementPDF(currentCustomer) as Uint8Array;
      if (pdfData) {
        const blob = new Blob([pdfData], { type: 'application/pdf' });
        const url = URL.createObjectURL(blob);
        const link = document.createElement('a');
        link.href = url;
        link.download = `Stmt_${currentCustomer.id}.pdf`;
        link.click();
        URL.revokeObjectURL(url);
      }
    } catch (err) {
      console.error(err);
      setError('Failed to generate PDF');
    } finally {
      setIsGenerating(false);
    }
  };

  const runStressTest = async () => {
    setTestMode(true);
    setResults(null);
    setTestProgress(0);
    setError(null);

    try {
      const response = await fetch('/api/stress-test', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ totalCount: testCount, engine: activeEngine }),
      });

      const raw = await response.text();
      let data: { success?: boolean; error?: string; tps?: number; duration?: number; engine?: string };
      try {
        data = JSON.parse(raw);
      } catch {
        throw new Error(raw || `HTTP ${response.status}: ${response.statusText}`);
      }

      if (!response.ok || !data.success) {
        throw new Error(data.error || `HTTP ${response.status}: ${response.statusText}`);
      }

      setResults({ tps: data.tps!, totalTime: data.duration!, engine: data.engine! });
      setTestProgress(testCount);
    } catch (err) {
      console.error(err);
      const message = err instanceof Error ? err.message : 'Stress test failed';
      setError(message);
    } finally {
      setTestMode(false);
    }
  };

  const totalTxCount = currentCustomer
    ? currentCustomer.accounts.reduce((s, a) => s + a.transactions.length, 0) +
      currentCustomer.tdr.reduce((s, t) => s + t.transactions.length, 0)
    : 0;

  return (
    <div className="min-h-screen bg-[#FDFDFB] text-[#141414] font-sans selection:bg-[#E2E2E2]">
      {/* Background Grid */}
      <div
        className="fixed inset-0 pointer-events-none opacity-[0.03]"
        style={{
          backgroundImage:
            'linear-gradient(#141414 1px, transparent 1px), linear-gradient(90deg, #141414 1px, transparent 1px)',
          backgroundSize: '40px 40px',
        }}
      />

      {/* Header */}
      <header className="sticky top-0 z-50 bg-white/80 backdrop-blur-md border-b border-[#141414]/10 px-6 py-4 flex justify-between items-center">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 bg-[#141414] rounded-lg flex items-center justify-center text-white">
            <Activity className="w-6 h-6" />
          </div>
          <div>
            <h1 className="text-xl font-bold tracking-tight">Statement3M</h1>
            <p className="text-[10px] font-mono uppercase tracking-widest opacity-50">Enterprise Transaction Engine</p>
          </div>
        </div>
        <nav className="flex gap-4 items-center">
          {totalCustomers > 0 && (
            <span className="text-xs font-mono opacity-50">{totalCustomers} records loaded</span>
          )}
          <div className="flex items-center gap-2 px-3 py-1 bg-green-50 text-green-700 border border-green-200 rounded-full text-xs font-semibold">
            <div className="w-2 h-2 bg-green-500 rounded-full animate-pulse" />
            Engine Ready
          </div>
        </nav>
      </header>

      <main className="max-w-7xl mx-auto px-6 py-10 grid lg:grid-cols-[1fr_400px] gap-10">
        <div className="space-y-10">
          {/* Hero */}
          <section className="bg-white border border-[#141414]/5 rounded-2xl p-8 shadow-sm">
            <div className="flex flex-col md:flex-row md:items-end justify-between gap-6 mb-8">
              <div>
                <h2 className="text-4xl font-light tracking-tight mb-2">High Efficiency Generation</h2>
                <p className="text-[#141414]/60 max-w-xl italic">
                  Targeted for a throughput of 30 statements per second, facilitating 3 million customer records every
                  24 hours.
                </p>
              </div>
              <div className="flex items-baseline gap-2">
                <span className="text-6xl font-black tabular-nums">3.0</span>
                <span className="text-xl font-bold opacity-30">M/Day</span>
              </div>
            </div>

            <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
              {[
                { label: 'Target Rate', value: '30 s/sec', icon: Zap },
                { label: 'Data Input', value: 'JSON / MongoDB', icon: Database },
                { label: 'Output Format', value: 'PDF/A Std', icon: FileText },
                { label: 'Loaded Records', value: totalCustomers > 0 ? `${totalCustomers}` : '—', icon: Users },
              ].map((stat, i) => (
                <div key={i} className="p-4 bg-[#F8F8F7] rounded-xl border border-[#141414]/5 flex items-center gap-3">
                  <stat.icon className="w-5 h-5 opacity-40" />
                  <div>
                    <p className="text-[10px] font-mono uppercase tracking-tighter opacity-50">{stat.label}</p>
                    <p className="font-bold">{stat.value}</p>
                  </div>
                </div>
              ))}
            </div>
          </section>

          {/* Customer Preview */}
          <section className="space-y-4">
            <div className="flex justify-between items-center px-2">
              <h3 className="font-bold flex items-center gap-2 uppercase text-xs tracking-widest text-[#141414]/60">
                <Users className="w-4 h-4" /> Customer Preview
              </h3>

              {/* Navigation */}
              <div className="flex items-center gap-3">
                <button
                  onClick={goToPrev}
                  disabled={isLoadingCustomer || totalCustomers === 0}
                  className="w-8 h-8 rounded-full border border-[#141414]/20 flex items-center justify-center hover:bg-[#141414] hover:text-white transition-colors disabled:opacity-30"
                >
                  <ChevronLeft className="w-4 h-4" />
                </button>
                <span className="text-xs font-mono tabular-nums opacity-60 min-w-[70px] text-center">
                  {totalCustomers > 0 ? `${customerIndex + 1} / ${totalCustomers}` : '…'}
                </span>
                <button
                  onClick={goToNext}
                  disabled={isLoadingCustomer || totalCustomers === 0}
                  className="w-8 h-8 rounded-full border border-[#141414]/20 flex items-center justify-center hover:bg-[#141414] hover:text-white transition-colors disabled:opacity-30"
                >
                  <ChevronRight className="w-4 h-4" />
                </button>
              </div>
            </div>

            <AnimatePresence mode="wait">
              {currentCustomer && !isLoadingCustomer ? (
                <motion.div
                  key={currentCustomer.id}
                  initial={{ opacity: 0, y: 10 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: -10 }}
                  className="bg-white border-2 border-[#141414] rounded-2xl overflow-hidden shadow-[8px_8px_0px_#141414]"
                >
                  {/* Card Header */}
                  <div className="bg-[#141414] text-white p-6 flex justify-between items-start">
                    <div>
                      <h4 className="text-2xl font-bold">{currentCustomer.name}</h4>
                      <p className="text-white/50 text-xs font-mono mt-1">{currentCustomer.id}</p>
                      {currentCustomer.cif && (
                        <p className="text-white/40 text-[10px] font-mono">CIF: {currentCustomer.cif}</p>
                      )}
                      {currentCustomer.period && (
                        <p className="text-white/40 text-[10px] font-mono mt-1">
                          Period: {currentCustomer.period.from} → {currentCustomer.period.to}
                        </p>
                      )}
                    </div>
                    <button
                      onClick={handleDownloadSingle}
                      disabled={isGenerating}
                      className="bg-white text-[#141414] px-4 py-2 rounded-lg font-bold text-sm flex items-center gap-2 hover:bg-[#F2F2F2] transition-colors disabled:opacity-50"
                    >
                      {isGenerating ? (
                        <div className="w-4 h-4 border-2 border-t-transparent border-[#141414] rounded-full animate-spin" />
                      ) : (
                        <Download className="w-4 h-4" />
                      )}
                      {isGenerating ? 'Generating…' : 'Download PDF'}
                    </button>
                  </div>

                  <div className="p-8 grid md:grid-cols-2 gap-8">
                    {/* Accounts + TDR */}
                    <div className="space-y-6">
                      <div>
                        <p className="text-[10px] font-mono uppercase tracking-widest opacity-40 mb-2">Accounts</p>
                        <ul className="space-y-2">
                          {currentCustomer.accounts.map(acc => (
                            <li
                              key={acc.id}
                              className="flex justify-between items-center p-3 bg-[#F8F8F7] rounded-lg border border-[#141414]/5"
                            >
                              <div className="flex items-center gap-3">
                                <div className="w-2 h-2 rounded-full bg-blue-500" />
                                <div>
                                  <p className="font-semibold text-sm">{acc.accountType}</p>
                                  <p className="text-[10px] font-mono opacity-50">{acc.accountNumber}</p>
                                </div>
                              </div>
                              <div className="text-right">
                                <p className="font-mono text-xs font-bold">
                                  {acc.currency} {acc.balance.toLocaleString('en-PK', { minimumFractionDigits: 2 })}
                                </p>
                                <p className="text-[9px] opacity-40">{acc.transactions.length} txns</p>
                              </div>
                            </li>
                          ))}
                        </ul>
                      </div>

                      {currentCustomer.tdr.length > 0 && (
                        <div className="p-4 bg-orange-50 border border-orange-100 rounded-xl">
                          <div className="flex justify-between items-center mb-2">
                            <p className="text-[10px] font-bold text-orange-800 uppercase tracking-widest">
                              Term Deposits
                            </p>
                            <span className="px-2 py-0.5 bg-orange-200 text-orange-900 rounded text-[9px] font-black uppercase">
                              {currentCustomer.tdr.length} TDR{currentCustomer.tdr.length > 1 ? 's' : ''}
                            </span>
                          </div>
                          {currentCustomer.tdr.map(t => (
                            <div key={t.id} className="mt-2 border-t border-orange-200 pt-2 first:border-0 first:pt-0">
                              <p className="text-sm font-bold text-orange-950 font-mono">{t.tdrNumber}</p>
                              <div className="flex justify-between mt-1">
                                <p className="text-[10px] text-orange-900/60">
                                  Maturity: {new Date(t.maturityDate).toLocaleDateString()}
                                </p>
                                <p className="text-[10px] font-mono text-orange-900/80">
                                  {t.principalAmount.toLocaleString('en-PK', { minimumFractionDigits: 2 })}
                                </p>
                              </div>
                            </div>
                          ))}
                        </div>
                      )}
                    </div>

                    {/* Stats */}
                    <div className="p-6 bg-[#F8F8F7] rounded-2xl border-2 border-dashed border-[#141414]/10 flex flex-col justify-center items-center text-center">
                      <div className="w-16 h-16 bg-white border border-[#141414]/5 rounded-full flex items-center justify-center mb-4 shadow-sm">
                        <BarChart3 className="w-8 h-8 opacity-20" />
                      </div>
                      <h5 className="font-bold text-xs uppercase tracking-widest mb-1 opacity-50">
                        Total Transactions
                      </h5>
                      <p className="text-5xl font-black tracking-tighter tabular-nums">
                        {totalTxCount.toLocaleString()}
                      </p>
                      <p className="text-xs opacity-60 mt-2">
                        {currentCustomer.accounts.length} accounts · {currentCustomer.tdr.length} TDRs
                      </p>
                      {currentCustomer.period && (
                        <p className="text-[10px] font-mono opacity-40 mt-3">
                          {currentCustomer.period.from} to {currentCustomer.period.to}
                        </p>
                      )}
                    </div>
                  </div>
                </motion.div>
              ) : (
                <motion.div
                  key="loading"
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1 }}
                  exit={{ opacity: 0 }}
                  className="bg-white border-2 border-[#141414]/10 rounded-2xl h-64 flex items-center justify-center"
                >
                  <div className="flex flex-col items-center gap-3 opacity-40">
                    <div className="w-8 h-8 border-2 border-t-transparent border-[#141414] rounded-full animate-spin" />
                    <p className="text-sm font-mono">Loading customer…</p>
                  </div>
                </motion.div>
              )}
            </AnimatePresence>
          </section>
        </div>

        {/* Sidebar: Stress Test */}
        <aside className="space-y-6">
          <div className="bg-white border-2 border-[#141414] rounded-2xl p-6 shadow-[4px_4px_0px_#141414]">
            <h3 className="text-lg font-bold flex items-center gap-2 mb-4">
              <Zap className="w-5 h-5 text-yellow-500 fill-yellow-500" /> Stress Test
            </h3>
            <p className="text-sm text-[#141414]/60 mb-6">
              Generate PDFs from the loaded JSON records using the Node.js worker pool or Python engine.
            </p>

            <div className="space-y-4">
              <div>
                <label className="text-[10px] font-mono uppercase tracking-widest opacity-50 mb-2 block">
                  Technology Stack
                </label>
                <div className="flex gap-2">
                  {[
                    { id: 'node', label: 'Node (JS)', icon: Activity },
                    { id: 'rust', label: 'Rust (pdf-oxide)', icon: Database },
                    { id: 'python-rl', label: 'Python (ReportLab)', icon: Zap },
                    { id: 'python', label: 'Python (fpdf2)', icon: Zap },
                  ].map(engine => (
                    <button
                      key={engine.id}
                      onClick={() => setActiveEngine(engine.id as any)}
                      disabled={testMode}
                      className={`flex-1 py-3 px-2 text-xs font-bold rounded-xl border flex flex-col items-center gap-2 transition-all ${activeEngine === engine.id ? 'bg-[#141414] text-white border-[#141414] shadow-lg' : 'bg-transparent border-[#141414]/10 opacity-60 hover:opacity-100'}`}
                    >
                      <engine.icon className={`w-4 h-4 ${activeEngine === engine.id ? 'text-yellow-400' : ''}`} />
                      {engine.label}
                    </button>
                  ))}
                </div>
                {activeEngine === 'rust' && (
                  <p className="text-[9px] mt-2 opacity-50 italic">
                    Rust pdf-oxide — loads from MongoDB (MONGODB_URI). Requires pdf_oxide_gen binary and MongoDB.
                  </p>
                )}
                {activeEngine === 'python' && (
                  <p className="text-[9px] mt-2 opacity-50 italic">
                    Python fpdf2 — pure Python, LGPL. Uses bank-statements-500.txt.
                  </p>
                )}
                {activeEngine === 'python-rl' && (
                  <p className="text-[9px] mt-2 opacity-50 italic">
                    ReportLab C-extension — BSD open-source. ~26 PDF/sec with Form XObject chrome.
                  </p>
                )}
              </div>

              <div>
                <label className="text-[10px] font-mono uppercase tracking-widest opacity-50 mb-2 block">
                  Statement Count
                </label>
                <div className="flex gap-2">
                  {[10, 100, 250, 500].map(val => (
                    <button
                      key={val}
                      onClick={() => setTestCount(val)}
                      disabled={testMode}
                      className={`flex-1 py-2 text-xs font-bold rounded-lg border transition-all ${testCount === val ? 'bg-[#141414] text-white border-[#141414]' : 'bg-transparent border-[#141414]/20 hover:border-[#141414]'}`}
                    >
                      {val}
                    </button>
                  ))}
                </div>
              </div>

              {!testMode ? (
                <button
                  onClick={runStressTest}
                  className="w-full bg-[#141414] text-white py-4 rounded-xl font-bold flex items-center justify-center gap-3 hover:scale-[1.02] active:scale-[0.98] transition-all"
                >
                  <Play className="w-5 h-5 fill-white" /> Start Performance Test
                </button>
              ) : (
                <div className="space-y-4">
                  <div className="flex justify-between items-end mb-1">
                    <span className="text-xs font-bold uppercase tracking-widest animate-pulse">Running Engine…</span>
                    <span className="text-lg font-mono font-bold">
                      {Math.round((testProgress / testCount) * 100)}%
                    </span>
                  </div>
                  <div className="h-4 bg-[#F0F0F0] rounded-full overflow-hidden border border-[#141414]/5">
                    <motion.div
                      className="h-full bg-[#141414]"
                      initial={{ width: 0 }}
                      animate={{ width: `${(testProgress / testCount) * 100}%` }}
                    />
                  </div>
                  <p className="text-[10px] font-mono text-center opacity-40">PROCESSING BATCH OPERATIONS</p>
                </div>
              )}

              {results && (
                <motion.div
                  initial={{ opacity: 0, scale: 0.95 }}
                  animate={{ opacity: 1, scale: 1 }}
                  className="mt-6 p-6 bg-green-50 border-2 border-green-600 rounded-2xl relative overflow-hidden"
                >
                  <div className="absolute top-0 right-0 p-2 text-green-600/20">
                    <CheckCircle2 className="w-12 h-12" />
                  </div>
                  <div className="relative z-10">
                    <h4 className="text-green-900 font-bold text-xs uppercase tracking-widest mb-3">Test Results</h4>
                    <div className="grid grid-cols-2 gap-4">
                      <div>
                        <p className="text-[10px] text-green-700/60 uppercase font-mono">Engine Used</p>
                        <p className="text-sm font-bold text-green-950 truncate">{results.engine}</p>
                      </div>
                      <div className="text-right">
                        <p className="text-[10px] text-green-700/60 uppercase font-mono">Throughput</p>
                        <p className="text-2xl font-black text-green-950 font-mono tracking-tighter">
                          {results.tps} <span className="text-[10px]">s/sec</span>
                        </p>
                      </div>
                    </div>
                    {results.tps >= 30 ? (
                      <div className="mt-4 flex items-center gap-2 text-[10px] font-bold text-green-800 bg-green-200/50 p-2 rounded-lg">
                        <Zap className="w-3 h-3" /> TARGET 30S/SEC ACHIEVED
                      </div>
                    ) : (
                      <div className="mt-4 text-[9px] text-green-800/60 flex items-start gap-1">
                        <AlertCircle className="w-3 h-3 shrink-0" />
                        Performance varies by CPU. Native Node.js is typically 5-10x faster than browser.
                      </div>
                    )}
                  </div>
                </motion.div>
              )}
            </div>
          </div>

          <div className="bg-[#F8F8F7] border border-[#141414]/10 rounded-2xl p-6">
            <h4 className="font-bold text-xs uppercase tracking-widest mb-3 opacity-60">Engine Specifications</h4>
            <div className="space-y-3 text-xs leading-relaxed opacity-80">
              <p>• Architecture: Node.js Multi-threaded (Worker Threads)</p>
              <p>• Pool Manager: Piscina Worker Pool</p>
              <p>• Document Engine: PDFKit (Streaming)</p>
              <p>• Data Source: bank-statements-500.txt (JSON)</p>
              <p>• Python Capability: Ready via ReportLab (50+ s/sec)</p>
            </div>
          </div>
        </aside>
      </main>

      {error && (
        <div className="fixed bottom-6 right-6 bg-red-600 text-white p-4 rounded-xl shadow-2xl flex items-center gap-3 z-[100]">
          <AlertCircle className="w-5 h-5 text-white" />
          <span className="font-bold text-sm">{error}</span>
          <button
            onClick={() => setError(null)}
            className="ml-2 opacity-60 hover:opacity-100 uppercase text-[10px] font-bold tracking-widest"
          >
            Dismiss
          </button>
        </div>
      )}
    </div>
  );
}
