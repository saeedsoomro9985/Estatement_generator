export interface Transaction {
  id: string;
  date: string;
  description: string;
  amount: number;
  type: 'DEBIT' | 'CREDIT';
  balance: number;
}

export interface Account {
  id: string;
  accountNumber: string;
  accountType: 'SAVINGS' | 'CURRENT' | 'SALARY' | 'BUSINESS';
  balance: number;
  currency: string;
  transactions: Transaction[];
}

export interface TDR {
  id: string;
  tdrNumber: string;
  principalAmount: number;
  interestRate: number;
  maturityDate: string;
  status: 'ACTIVE' | 'MATURED';
  interestPaid: number;
  transactions: Transaction[];
}

export interface CustomerData {
  id: string;
  name: string;
  email: string;
  phone: string;
  address: string;
  cif?: string;
  period?: { from: string; to: string };
  accounts: Account[];
  tdr: TDR[];
}

// ── Raw JSON types ───────────────────────────────────────────────────────────

interface RawTransaction {
  transactionDate: string;
  transactionDetails: string;
  docNo: string;
  debitAmountLc: string;
  creditAmountLc: string;
  balance: string;
}

interface RawAccount {
  title: string;
  accountType: string;
  currency: string;
  accountNo: string;
  iban: string;
  transactions: RawTransaction[];
}

interface RawTdrTransaction {
  startDate: string;
  maturity: string;
  tenure: string;
  rupeesAmount: string;
  certificateType: string;
  profitOption: string;
}

interface RawTermDeposit {
  certNo: string;
  title: string;
  currency: string;
  accountNo: string;
  tdrTransactions: RawTdrTransaction[];
}

interface RawSummaryAccount {
  accountNo: string;
  closingBalance: string;
  openingBalance: string;
  accountStatus: string;
  accountType: string;
  currency: string;
}

interface RawSummaryTD {
  certNo: string;
  openingBalance: string;
  closingBalance: string;
  accountStatus: string;
}

export interface RawStatement {
  _id: string;
  statementId: string;
  customer: {
    customerId: string;
    cif: string;
    name: string;
    email: string;
    address: string;
  };
  meta: {
    fromDate: string;
    toDate: string;
    currency: string;
  };
  summary: {
    accounts: RawSummaryAccount[];
    termDeposits: RawSummaryTD[];
  };
  accounts: RawAccount[];
  termDeposits: RawTermDeposit[];
}

// ── Mapper ───────────────────────────────────────────────────────────────────

const accountTypeMap: Record<string, Account['accountType']> = {
  savings: 'SAVINGS',
  current: 'CURRENT',
  salary: 'SALARY',
  business: 'BUSINESS',
};

export function mapStatementToCustomerData(record: RawStatement): CustomerData {
  const accounts: Account[] = record.accounts.map((acc, idx) => {
    const summaryAcc = record.summary.accounts.find(s => s.accountNo === acc.accountNo);
    return {
      id: `A${idx + 1}`,
      accountNumber: acc.accountNo,
      accountType: accountTypeMap[acc.accountType.toLowerCase()] ?? 'CURRENT',
      balance: parseFloat(summaryAcc?.closingBalance ?? '0'),
      currency: acc.currency,
      transactions: acc.transactions.map((tx, i) => {
        const debit = parseFloat(tx.debitAmountLc);
        const credit = parseFloat(tx.creditAmountLc);
        const isDebit = debit > 0;
        return {
          id: tx.docNo || `T${i}`,
          date: tx.transactionDate,
          description: tx.transactionDetails,
          amount: isDebit ? debit : credit,
          type: isDebit ? 'DEBIT' : 'CREDIT',
          balance: parseFloat(tx.balance),
        };
      }),
    };
  });

  const tdr: TDR[] = record.termDeposits.map((td, idx) => {
    const summaryTd = record.summary.termDeposits.find(s => s.certNo === td.certNo);
    const firstTx = td.tdrTransactions[0];
    return {
      id: `TD${idx + 1}`,
      tdrNumber: td.certNo,
      principalAmount: parseFloat(summaryTd?.openingBalance ?? '0'),
      interestRate: 0,
      maturityDate: firstTx?.maturity ?? record.meta.toDate,
      status: summaryTd?.accountStatus?.toLowerCase() === 'active' ? 'ACTIVE' : 'MATURED',
      interestPaid: 0,
      transactions: td.tdrTransactions.map((tx, i) => ({
        id: `TDR${idx}-TX${i}`,
        date: tx.startDate,
        description: `${tx.certificateType} - ${tx.tenure} - ${tx.profitOption}`.substring(0, 60),
        amount: parseFloat(tx.rupeesAmount),
        type: 'CREDIT' as const,
        balance: parseFloat(tx.rupeesAmount),
      })),
    };
  });

  return {
    id: record.customer.customerId,
    name: record.customer.name,
    email: record.customer.email,
    phone: '',
    address: record.customer.address,
    cif: record.customer.cif,
    period: { from: record.meta.fromDate, to: record.meta.toDate },
    accounts,
    tdr,
  };
}

// ── In-memory generated data (kept for fallback) ─────────────────────────────

export const generateCustomerData = (): CustomerData => {
  const customerId = `CID-${Math.random().toString(36).substring(2, 6)}`;
  const dayMs = 86400000;
  const now = Date.now();

  const generateTransactions = (count: number, bal: number): Transaction[] => {
    const txs: Transaction[] = new Array(count);
    let currentBalance = bal;
    for (let i = 0; i < count; i++) {
      const isDebit = Math.random() > 0.5;
      const amt = Math.floor(Math.random() * 100) + 1;
      currentBalance += isDebit ? -amt : amt;
      txs[i] = {
        id: 'T' + i,
        date: new Date(now - i * dayMs).toISOString().substring(0, 10),
        description: isDebit ? 'Debit Transaction' : 'Credit Deposit',
        amount: amt,
        type: isDebit ? 'DEBIT' : 'CREDIT',
        balance: currentBalance,
      };
    }
    return txs;
  };

  const accounts: Account[] = [
    { id: 'A1', accountNumber: '400123456', accountType: 'SAVINGS', balance: 5000, currency: 'USD', transactions: generateTransactions(250, 5000) },
    { id: 'A2', accountNumber: '400987654', accountType: 'CURRENT', balance: 12000, currency: 'USD', transactions: generateTransactions(250, 12000) },
    { id: 'A3', accountNumber: '400555666', accountType: 'SALARY', balance: 8000, currency: 'USD', transactions: generateTransactions(250, 8000) },
    { id: 'A4', accountNumber: '400111222', accountType: 'BUSINESS', balance: 45000, currency: 'USD', transactions: generateTransactions(250, 45000) },
  ];

  const tdr: TDR[] = [
    { id: 'T1', tdrNumber: 'TDR-888001', principalAmount: 50000, interestRate: 7.5, maturityDate: '2027-12-01', status: 'ACTIVE', interestPaid: 1250, transactions: generateTransactions(250, 50000) },
    { id: 'T2', tdrNumber: 'TDR-888002', principalAmount: 75000, interestRate: 8.2, maturityDate: '2028-06-15', status: 'MATURED', interestPaid: 3400, transactions: generateTransactions(250, 75000) },
  ];

  return { id: customerId, name: 'Customer ' + customerId, email: 'c@m.com', phone: '555', address: 'Enterprise City, Suite 100', accounts, tdr };
};
