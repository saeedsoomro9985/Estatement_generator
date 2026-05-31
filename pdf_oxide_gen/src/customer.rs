use std::collections::HashMap;

use crate::statement::StatementDocument;

/// Statement models aligned with production JSON (meta, customer, summary, accounts, termDeposits).
#[derive(Debug, Clone)]
pub struct Statement {
    pub customer_name: String,
    pub customer_id: String,
    pub cif: String,
    pub address: String,
    pub from_date: String,
    pub to_date: String,
    pub account_summary: Vec<AccountSummaryRow>,
    pub tdr_summary: Vec<TdrSummaryRow>,
    pub accounts: Vec<AccountDetail>,
    pub term_deposits: Vec<TermDepositDetail>,
}

#[derive(Debug, Clone)]
pub struct AccountSummaryRow {
    pub product: String,
    pub account_number: String,
    pub iban: String,
    pub currency: String,
    pub fcy_balance: String,
    pub balance: String,
}

#[derive(Debug, Clone)]
pub struct TdrSummaryRow {
    pub certificate_type: String,
    pub number_of_certificates: String,
    pub iban: String,
    pub currency: String,
    pub fcy_balance: String,
    pub balance: String,
}

#[derive(Debug, Clone)]
pub struct AccountDetail {
    pub title: String,
    pub account_type: String,
    pub account_number: String,
    pub iban: String,
    pub currency: String,
    pub from_date: String,
    pub to_date: String,
    pub branch: String,
    pub opening_balance: String,
    pub closing_balance: String,
    pub transactions: Vec<AccountTransactionRow>,
}

#[derive(Debug, Clone)]
pub struct AccountTransactionRow {
    pub date: String,
    pub value_date: String,
    pub doc_no: String,
    pub particular: String,
    pub debit: String,
    pub credit: String,
    pub balance: String,
}

#[derive(Debug, Clone)]
pub struct TermDepositDetail {
    pub title: String,
    pub cert_no: String,
    pub account_type: String,
    pub as_of_date: String,
    pub certificates: Vec<TdrCertificateRow>,
}

#[derive(Debug, Clone)]
pub struct TdrCertificateRow {
    pub certificate_no: String,
    pub profit_option: String,
    pub start_date: String,
    pub maturity_date: String,
    pub tenure: String,
    pub currency: String,
    pub fcy_balance: String,
    pub amount: String,
    pub cert_type_label: String,
}

pub fn fmt_money(v: f64) -> String {
    format!("{:.2}", v)
}


/// Map a MongoDB statement document into the internal PDF model (same logic as before).
pub fn map_statement(rec: &StatementDocument) -> Statement {
    let summary = rec.summary.as_ref();

    let account_summary = summary
        .map(|s| {
            s.accounts
                .iter()
                .map(|a| AccountSummaryRow {
                    product: String::new(),
                    account_number: a.account_no.clone(),
                    iban: String::new(),
                    currency: rec.meta.currency.clone().unwrap_or_default(),
                    fcy_balance: String::new(),
                    balance: fmt_money(a.closing_balance.as_f64()),
                })
                .collect()
        })
        .unwrap_or_default();

    let tdr_summary = summary
        .map(|s| {
            s.term_deposits
                .iter()
                .map(|t| TdrSummaryRow {
                    certificate_type: String::new(),
                    number_of_certificates: "1".to_string(),
                    iban: String::new(),
                    currency: rec.meta.currency.clone().unwrap_or_default(),
                    fcy_balance: String::new(),
                    balance: fmt_money(t.opening_balance.as_f64()),
                })
                .collect()
        })
        .unwrap_or_default();

    let accounts = rec
        .accounts
        .iter()
        .map(|acc| {
            let closing_balance = summary
                .and_then(|s| {
                    s.accounts
                        .iter()
                        .find(|x| x.account_no == acc.account_no)
                })
                .map(|x| fmt_money(x.closing_balance.as_f64()))
                .unwrap_or_default();

            let transactions = acc
                .transactions
                .iter()
                .map(|tx| AccountTransactionRow {
                    date: tx.transaction_date.clone(),
                    value_date: tx.transaction_date.clone(),
                    doc_no: String::new(),
                    particular: tx.transaction_details.clone(),
                    debit: fmt_money(tx.debit_amount_lc.as_f64()),
                    credit: fmt_money(tx.credit_amount_lc.as_f64()),
                    balance: fmt_money(tx.balance.as_f64()),
                })
                .collect();

            AccountDetail {
                title: acc.title.clone().unwrap_or_default(),
                account_type: acc.account_type.clone(),
                account_number: acc.account_no.clone(),
                iban: String::new(),
                currency: acc.currency.clone().unwrap_or_default(),
                from_date: rec.meta.from_date.clone(),
                to_date: rec.meta.to_date.clone(),
                branch: String::new(),
                opening_balance: String::new(),
                closing_balance,
                transactions,
            }
        })
        .collect();

    let term_deposits = rec
        .term_deposits
        .iter()
        .map(|td| {
            let certificates = td
                .tdr_transactions
                .iter()
                .map(|tx| TdrCertificateRow {
                    certificate_no: td.cert_no.clone(),
                    profit_option: tx.profit_option.clone(),
                    start_date: tx.start_date.clone(),
                    maturity_date: tx.maturity.clone(),
                    tenure: tx.tenure.clone(),
                    currency: rec.meta.currency.clone().unwrap_or_default(),
                    fcy_balance: String::new(),
                    amount: fmt_money(tx.rupees_amount.as_f64()),
                    cert_type_label: tx.certificate_type.clone(),
                })
                .collect();

            TermDepositDetail {
                title: String::new(),
                cert_no: td.cert_no.clone(),
                account_type: String::new(),
                as_of_date: rec.meta.to_date.clone(),
                certificates,
            }
        })
        .collect();

    Statement {
        customer_name: rec.customer.name.clone().unwrap_or_default(),
        customer_id: rec.customer.customer_id.clone(),
        cif: rec.customer.cif.clone().unwrap_or_default(),
        address: rec.customer.address.clone().unwrap_or_default(),
        from_date: rec.meta.from_date.clone(),
        to_date: rec.meta.to_date.clone(),
        account_summary,
        tdr_summary,
        accounts,
        term_deposits,
    }
}
