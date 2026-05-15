use serde::Deserialize;

/// Amount fields in MongoDB may be stored as strings or numbers.
#[derive(Debug, Default, Deserialize, Clone)]
#[serde(untagged)]
pub enum FlexibleAmount {
    #[default]
    Null,
    F64(f64),
    I64(i64),
    U64(u64),
    String(String),
}

impl FlexibleAmount {
    pub fn as_f64(&self) -> f64 {
        match self {
            Self::F64(v) => *v,
            Self::I64(v) => *v as f64,
            Self::U64(v) => *v as f64,
            Self::String(s) => s.parse().unwrap_or(0.0),
            Self::Null => 0.0,
        }
    }
}

/// Statement document as stored in MongoDB (`EStatements.Statements`).
#[derive(Debug, Deserialize)]
pub struct StatementDocument {
    #[serde(rename = "statementId", default)]
    pub statement_id: Option<String>,
    pub customer: StatementCustomer,
    pub meta: StatementMeta,
    #[serde(default)]
    pub accounts: Vec<StatementAccount>,
    #[serde(rename = "termDeposits", default)]
    pub term_deposits: Vec<StatementTermDeposit>,
    #[serde(default)]
    pub summary: Option<StatementSummary>,
}

#[derive(Debug, Deserialize)]
pub struct StatementCustomer {
    #[serde(rename = "customerId", default)]
    pub customer_id: String,
    #[serde(default)]
    pub cif: Option<String>,
    pub name: Option<String>,
    pub email: Option<String>,
    pub address: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct StatementMeta {
    #[serde(rename = "fromDate", default)]
    pub from_date: String,
    #[serde(rename = "toDate", default)]
    pub to_date: String,
    #[serde(rename = "templateId", default)]
    pub template_id: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub currency: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct StatementAccount {
    #[serde(rename = "accountNo")]
    pub account_no: String,
    #[serde(rename = "accountType", default)]
    pub account_type: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub transactions: Vec<StatementTransaction>,
}

#[derive(Debug, Deserialize)]
pub struct StatementTransaction {
    #[serde(rename = "transactionDate", default)]
    pub transaction_date: String,
    #[serde(rename = "transactionDetails", default)]
    pub transaction_details: String,
    #[serde(rename = "debitAmountLc", default)]
    pub debit_amount_lc: FlexibleAmount,
    #[serde(rename = "creditAmountLc", default)]
    pub credit_amount_lc: FlexibleAmount,
    #[serde(default)]
    pub balance: FlexibleAmount,
}

#[derive(Debug, Deserialize)]
pub struct StatementTermDeposit {
    #[serde(rename = "certNo")]
    pub cert_no: String,
    #[serde(rename = "tdrTransactions", default)]
    pub tdr_transactions: Vec<StatementTdrTransaction>,
}

#[derive(Debug, Deserialize)]
pub struct StatementTdrTransaction {
    #[serde(rename = "startDate", default)]
    pub start_date: String,
    #[serde(rename = "certificateType", default)]
    pub certificate_type: String,
    #[serde(rename = "tenure", default)]
    pub tenure: String,
    #[serde(rename = "profitOption", default)]
    pub profit_option: String,
    #[serde(rename = "rupeesAmount", default)]
    pub rupees_amount: FlexibleAmount,
    #[serde(default)]
    pub maturity: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct StatementSummary {
    #[serde(default)]
    pub accounts: Vec<StatementSummaryAccount>,
    #[serde(rename = "termDeposits", default)]
    pub term_deposits: Vec<StatementSummaryTermDeposit>,
}

#[derive(Debug, Deserialize)]
pub struct StatementSummaryAccount {
    #[serde(rename = "accountNo")]
    pub account_no: String,
    #[serde(rename = "closingBalance", default)]
    pub closing_balance: FlexibleAmount,
}

#[derive(Debug, Deserialize)]
pub struct StatementSummaryTermDeposit {
    #[serde(rename = "certNo")]
    pub cert_no: String,
    #[serde(rename = "openingBalance", default)]
    pub opening_balance: FlexibleAmount,
}
