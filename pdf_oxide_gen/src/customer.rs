use std::collections::HashMap;

use crate::statement::StatementDocument;

#[derive(Debug, Clone)]
pub struct Customer {
    pub id: String,
    pub name: String,
    pub address: String,
    pub period_from: String,
    pub period_to: String,
    pub accounts: Vec<Account>,
    pub tdr: Vec<Tdr>,
}

#[derive(Debug, Clone)]
pub struct Account {
    pub account_number: String,
    pub account_type: String,
    pub balance: f64,
    pub transactions: Vec<Transaction>,
}

#[derive(Debug, Clone)]
pub struct Tdr {
    pub tdr_number: String,
    pub principal_amount: f64,
    pub interest_rate: f64,
    pub maturity_date: String,
    pub transactions: Vec<Transaction>,
}

#[derive(Debug, Clone)]
pub struct Transaction {
    pub date: String,
    pub description: String,
    pub amount: f64,
    pub balance: f64,
}

pub fn fmt_money(v: f64) -> String {
    let neg = v < 0.0;
    let s = format!("{:.2}", v.abs());
    let mut parts = s.split('.');
    let int_part = parts.next().unwrap_or("0");
    let frac = parts.next().unwrap_or("00");
    let mut grouped_rev = String::new();
    for (i, ch) in int_part.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            grouped_rev.push(',');
        }
        grouped_rev.push(ch);
    }
    let grouped: String = grouped_rev.chars().rev().collect();
    if neg {
        format!("-{grouped}.{frac}")
    } else {
        format!("{grouped}.{frac}")
    }
}

/// Map a MongoDB statement document into the internal PDF model (same logic as before).
pub fn map_statement(rec: &StatementDocument) -> Customer {
    let summary = rec.summary.as_ref();
    let sum_accs: HashMap<&str, f64> = summary
        .map(|s| {
            s.accounts
                .iter()
                .map(|a| (a.account_no.as_str(), a.closing_balance.as_f64()))
                .collect()
        })
        .unwrap_or_default();

    let sum_tds: HashMap<&str, f64> = summary
        .map(|s| {
            s.term_deposits
                .iter()
                .map(|t| (t.cert_no.as_str(), t.opening_balance.as_f64()))
                .collect()
        })
        .unwrap_or_default();

    let accounts = rec
        .accounts
        .iter()
        .map(|acc| {
            let balance = *sum_accs.get(acc.account_no.as_str()).unwrap_or(&0.0);
            let transactions = acc
                .transactions
                .iter()
                .map(|tx| {
                    let debit = tx.debit_amount_lc.as_f64();
                    let credit = tx.credit_amount_lc.as_f64();
                    Transaction {
                        date: tx.transaction_date.clone(),
                        description: tx.transaction_details.clone(),
                        amount: if debit > 0.0 { debit } else { credit },
                        balance: tx.balance.as_f64(),
                    }
                })
                .collect();
            Account {
                account_number: acc.account_no.clone(),
                account_type: acc.account_type.clone(),
                balance,
                transactions,
            }
        })
        .collect();

    let tdr = rec
        .term_deposits
        .iter()
        .map(|td| {
            let principal = *sum_tds.get(td.cert_no.as_str()).unwrap_or(&0.0);
            let first = td.tdr_transactions.first();
            let maturity_date = first.map(|t| t.maturity.clone()).unwrap_or_default();
            let transactions = td
                .tdr_transactions
                .iter()
                .map(|tx| {
                    let amt = tx.rupees_amount.as_f64();
                    Transaction {
                        date: tx.start_date.clone(),
                        description: format!(
                            "{} - {} - {}",
                            tx.certificate_type, tx.tenure, tx.profit_option
                        ),
                        amount: amt,
                        balance: amt,
                    }
                })
                .collect();
            Tdr {
                tdr_number: td.cert_no.clone(),
                principal_amount: principal,
                interest_rate: 0.0,
                maturity_date,
                transactions,
            }
        })
        .collect();

    let id = if !rec.customer.customer_id.is_empty() {
        rec.customer.customer_id.clone()
    } else {
        rec.statement_id.clone().unwrap_or_else(|| "unknown".to_string())
    };

    Customer {
        id,
        name: rec.customer.name.clone().unwrap_or_default(),
        address: rec.customer.address.clone().unwrap_or_default(),
        period_from: rec.meta.from_date.clone(),
        period_to: rec.meta.to_date.clone(),
        accounts,
        tdr,
    }
}
