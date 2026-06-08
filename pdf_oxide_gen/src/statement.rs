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

    #[serde(default)]
    pub title: Option<String>,

    #[serde(rename = "accountType", default)]
    pub account_type: Option<String>,

    #[serde(default)]
    pub currency: Option<String>,

    #[serde(rename = "accountNo", default)]
    pub account_no: Option<String>,

    #[serde(default)]
    pub iban: Option<String>,

    #[serde(rename = "fromDate", default)]
    pub from_date: Option<String>,

    #[serde(rename = "toDate", default)]
    pub to_date: Option<String>,

    #[serde(default)]
    pub cif: Option<String>,

    #[serde(rename = "tdrTransactions", default)]
    pub tdr_transactions: Vec<StatementTdrTransaction>,
}

#[derive(Debug, Deserialize)]
pub struct StatementTdrTransaction {
    #[serde(default)]
    pub idpk: Option<i64>,

    #[serde(rename = "accountNo", default)]
    pub account_no: Option<String>,

    #[serde(rename = "certificateNo", default)]
    pub certificate_no: Option<String>,

    #[serde(rename = "profitOption", default)]
    pub profit_option: String,

    #[serde(rename = "startDate", default)]
    pub start_date: String,

    #[serde(default)]
    pub maturity: String,

    #[serde(default)]
    pub tenure: String,

    #[serde(rename = "fcyAmount", default)]
    pub fcy_amount: FlexibleAmount,

    #[serde(rename = "rupeesAmount", default)]
    pub rupees_amount: FlexibleAmount,

    #[serde(rename = "totalCertificates", default)]
    pub total_certificates: Option<String>,

    #[serde(rename = "certificateType", default)]
    pub certificate_type: String,

    #[serde(default)]
    pub currency: Option<String>,

    #[serde(rename = "requestId", default)]
    pub request_id: Option<String>,

    #[serde(rename = "requestDetailId", default)]
    pub request_detail_id: Option<String>,

    #[serde(rename = "productNumber", default)]
    pub product_number: Option<String>,

    #[serde(default)]
    pub iban: Option<String>,
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
    #[serde(default)]
    pub cif: Option<String>,

    #[serde(default)]
    pub product: Option<String>,

    #[serde(rename = "accountNo")]
    pub account_no: String,

    #[serde(default)]
    pub iban: Option<String>,

    #[serde(default)]
    pub currency: Option<String>,

    #[serde(default)]
    pub title: Option<String>,

    #[serde(rename = "subProductCode", default)]
    pub sub_product_code: Option<String>,

    #[serde(rename = "segmentCode", default)]
    pub segment_code: Option<String>,

    #[serde(rename = "openingBalance", default)]
    pub opening_balance: FlexibleAmount,

    #[serde(rename = "closingBalance", default)]
    pub closing_balance: FlexibleAmount,

    #[serde(rename = "currentBalance", default)]
    pub current_balance: FlexibleAmount,

    #[serde(rename = "openingBalanceFcy", default)]
    pub opening_balance_fcy: FlexibleAmount,

    #[serde(rename = "closingBalanceFcy", default)]
    pub closing_balance_fcy: FlexibleAmount,

    #[serde(rename = "balanceDate", default)]
    pub balance_date: Option<String>,

    #[serde(rename = "averageBalance", default)]
    pub average_balance: FlexibleAmount,

    #[serde(rename = "accountStatus", default)]
    pub account_status: Option<String>,

    #[serde(rename = "accountType", default)]
    pub account_type: Option<String>,

    #[serde(rename = "productDescription", default)]
    pub product_description: Option<String>,

    // #[serde(default)]
    // pub branch: Option<StatementBranch>,

    // #[serde(rename = "customerBranch", default)]
    // pub customer_branch: Option<StatementBranch>,

    #[serde(rename = "requestDetailId", default)]
    pub request_detail_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct StatementSummaryTermDeposit {
    #[serde(default)]
    pub cif: Option<String>,

    #[serde(rename = "certType", default)]
    pub cert_type: Option<String>,

    #[serde(rename = "certNo")]
    pub cert_no: String,

    #[serde(default)]
    pub iban: Option<String>,

    #[serde(default)]
    pub currency: Option<String>,

    #[serde(default)]
    pub product: Option<String>,

    #[serde(default)]
    pub title: Option<String>,

    #[serde(rename = "subProductCode", default)]
    pub sub_product_code: Option<String>,

    #[serde(rename = "segmentCode", default)]
    pub segment_code: Option<String>,

    #[serde(rename = "openingBalance", default)]
    pub opening_balance: FlexibleAmount,

    #[serde(rename = "closingBalance", default)]
    pub closing_balance: FlexibleAmount,

    #[serde(rename = "currentBalance", default)]
    pub current_balance: FlexibleAmount,

    #[serde(rename = "openingBalanceFcy", default)]
    pub opening_balance_fcy: FlexibleAmount,

    #[serde(rename = "closingBalanceFcy", default)]
    pub closing_balance_fcy: FlexibleAmount,

    #[serde(rename = "balanceDate", default)]
    pub balance_date: Option<String>,

    #[serde(rename = "averageBalance", default)]
    pub average_balance: FlexibleAmount,

    #[serde(rename = "accountStatus", default)]
    pub account_status: Option<String>,

    #[serde(rename = "accountType", default)]
    pub account_type: Option<String>,

    #[serde(rename = "productDescription", default)]
    pub product_description: Option<String>,

    // #[serde(default)]
    // pub branch: Option<StatementBranch>,

    // #[serde(rename = "customerBranch", default)]
    // pub customer_branch: Option<StatementBranch>,
}


