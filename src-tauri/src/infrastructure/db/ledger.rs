use super::Database;
use crate::domain::{ledger::*, AppError};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeMap;

#[derive(Default)]
struct SummaryAccumulator {
    income: i64,
    expense: i64,
    refund: i64,
    confirmed_count: u64,
    pending_count: u64,
    categories: BTreeMap<String, i64>,
}

impl SummaryAccumulator {
    fn add_confirmed(&mut self, nature: &str, amount: i64, category: &str) -> Result<(), AppError> {
        match nature {
            "income" => self.income = checked_add(self.income, amount)?,
            "expense" => {
                self.expense = checked_add(self.expense, amount)?;
                let category = if category.trim().is_empty() {
                    "未分类"
                } else {
                    category.trim()
                };
                let current = self.categories.get(category).copied().unwrap_or(0);
                self.categories
                    .insert(category.to_string(), checked_add(current, amount)?);
            }
            "refund" => self.refund = checked_add(self.refund, amount)?,
            _ => {}
        }
        Ok(())
    }
}

#[derive(Default)]
struct DailyAccumulator {
    summary: SummaryAccumulator,
    memo: Vec<String>,
}

fn add_memo(day: &mut DailyAccumulator, merchant: &str, note: &str) {
    let mut value = merchant.trim().to_string();
    let note = note.trim();
    if !note.is_empty() {
        if !value.is_empty() {
            value.push_str(" · ");
        }
        value.extend(note.chars().take(60));
        if note.chars().count() > 60 {
            value.push('…');
        }
    }
    if !value.is_empty() && !day.memo.contains(&value) {
        day.memo.push(value);
    }
}

fn checked_add(left: i64, right: i64) -> Result<i64, AppError> {
    left.checked_add(right)
        .ok_or(AppError::Validation("汇总金额超出范围，请缩小统计区间。"))
}

fn category_strings(categories: BTreeMap<String, i64>) -> BTreeMap<String, String> {
    categories
        .into_iter()
        .map(|(category, amount)| (category, amount.to_string()))
        .collect()
}

pub(super) const COLUMNS: &str =
    "id,business_date,nature,amount_minor,merchant,category,note,review_status,lifecycle,revision";
pub(super) fn entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: row.get(0)?,
        business_date: row.get(1)?,
        nature: row.get(2)?,
        amount_minor: row.get::<_, i64>(3)?.to_string(),
        merchant: row.get(4)?,
        category: row.get(5)?,
        note: row.get(6)?,
        review_status: row.get(7)?,
        lifecycle: row.get(8)?,
        revision: row.get(9)?,
    })
}
fn get(connection: &Connection, id: &str) -> Result<Entry, AppError> {
    connection
        .query_row(
            &format!("SELECT {COLUMNS} FROM transactions WHERE id=?1 AND book_id='default'"),
            [id],
            entry,
        )
        .optional()?
        .ok_or(AppError::RevisionConflict)
}
fn replay<T: serde::de::DeserializeOwned>(
    connection: &Connection,
    id: &str,
    payload: &str,
) -> Result<Option<T>, AppError> {
    let saved: Option<(String, String)> = connection
        .query_row(
            "SELECT payload_json,response_json FROM command_requests WHERE request_id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    match saved {
        Some((old, response)) if old == payload => Ok(Some(serde_json::from_str(&response)?)),
        Some(_) => Err(AppError::RequestConflict),
        None => Ok(None),
    }
}
fn record(
    connection: &Connection,
    request_id: &str,
    payload: &str,
    before: Option<&Entry>,
    after: &Entry,
    action: &str,
    reason: &str,
) -> Result<(), AppError> {
    let json = serde_json::to_string(after)?;
    connection.execute("INSERT INTO audit_events(id,operation_id,entity_id,action,before_json,after_json,reason) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![uuid::Uuid::new_v4().to_string(),request_id,after.id,action,before.map(serde_json::to_string).transpose()?,json,reason])?;
    connection.execute(
        "INSERT INTO command_requests VALUES(?1,?2,?3)",
        params![request_id, payload, json],
    )?;
    Ok(())
}

impl Database {
    pub fn save_entry(&self, input: SaveEntry) -> Result<Entry, AppError> {
        let amount = input.validate()?;
        let payload = format!("save:{}", serde_json::to_string(&input)?);
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| AppError::WorkerUnavailable)?;
        let tx = connection.transaction()?;
        if let Some(saved) = replay(&tx, &input.request_id, &payload)? {
            return Ok(saved);
        }
        let id = input
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let before = if input.id.is_some() {
            Some(get(&tx, &id)?)
        } else {
            None
        };
        if let Some(previous) = &before {
            if Some(previous.revision) != input.expected_revision || previous.lifecycle != "active"
            {
                return Err(AppError::RevisionConflict);
            }
            tx.execute("UPDATE transactions SET business_date=?1,nature=?2,amount_minor=?3,merchant=?4,category=?5,note=?6,review_status=?7,revision=revision+1,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?8",
                params![input.business_date,input.nature,amount,input.merchant.trim(),input.category.trim(),input.note,input.review_status,id])?;
        } else {
            tx.execute("INSERT INTO transactions(id,book_id,business_date,nature,amount_minor,merchant,category,note,review_status) VALUES(?1,'default',?2,?3,?4,?5,?6,?7,?8)",
                params![id,input.business_date,input.nature,amount,input.merchant.trim(),input.category.trim(),input.note,input.review_status])?;
        }
        let saved = get(&tx, &id)?;
        super::refunds::after_edit(&tx, before.as_ref(), &saved)?;
        record(
            &tx,
            &input.request_id,
            &payload,
            before.as_ref(),
            &saved,
            if before.is_some() { "update" } else { "create" },
            "",
        )?;
        tx.commit()?;
        Ok(saved)
    }

    pub fn void_entry(&self, input: VoidEntry) -> Result<Entry, AppError> {
        validate_request(&input.request_id)?;
        if input.reason.trim().is_empty() || input.reason.chars().count() > 500 {
            return Err(AppError::Validation("请填写作废原因（最多500字）。"));
        }
        let payload = format!("void:{}", serde_json::to_string(&input)?);
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| AppError::WorkerUnavailable)?;
        let tx = connection.transaction()?;
        if let Some(saved) = replay(&tx, &input.request_id, &payload)? {
            return Ok(saved);
        }
        let before = get(&tx, &input.id)?;
        if before.revision != input.expected_revision || before.lifecycle != "active" {
            return Err(AppError::RevisionConflict);
        }
        tx.execute("UPDATE transactions SET lifecycle='voided',revision=revision+1,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1", [&input.id])?;
        let saved = get(&tx, &input.id)?;
        super::refunds::after_edit(&tx, Some(&before), &saved)?;
        record(
            &tx,
            &input.request_id,
            &payload,
            Some(&before),
            &saved,
            "void",
            &input.reason,
        )?;
        tx.commit()?;
        Ok(saved)
    }

    pub fn confirm_entries(&self, input: ConfirmEntries) -> Result<Vec<Entry>, AppError> {
        input.validate()?;
        let payload = format!("confirm:{}", serde_json::to_string(&input)?);
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| AppError::WorkerUnavailable)?;
        let tx = connection.transaction()?;
        if let Some(saved) = replay::<Vec<Entry>>(&tx, &input.request_id, &payload)? {
            return Ok(saved);
        }
        let mut confirmed = Vec::with_capacity(input.entries.len());
        for item in &input.entries {
            let before = get(&tx, &item.id)?;
            if before.revision != item.expected_revision
                || before.lifecycle != "active"
                || before.review_status == "confirmed"
            {
                return Err(AppError::RevisionConflict);
            }
            if before.nature == "unknown" {
                return Err(AppError::Validation("请先填写 unknown 交易的性质再核实。"));
            }
            // Confirming does not change money, so the refund link state survives;
            // only its revision pointer has to follow the new revision.
            tx.execute("UPDATE transactions SET review_status='confirmed',revision=revision+1,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1", [&item.id])?;
            let saved = get(&tx, &item.id)?;
            super::refunds::after_edit(&tx, Some(&before), &saved)?;
            let after = serde_json::to_string(&saved)?;
            tx.execute("INSERT INTO audit_events(id,operation_id,entity_id,action,before_json,after_json,reason) VALUES(?1,?2,?3,'confirm',?4,?5,'')",
                params![uuid::Uuid::new_v4().to_string(),input.request_id,before.id,serde_json::to_string(&before)?,after])?;
            confirmed.push(saved);
        }
        tx.execute(
            "INSERT INTO command_requests VALUES(?1,?2,?3)",
            params![
                input.request_id,
                payload,
                serde_json::to_string(&confirmed)?
            ],
        )?;
        tx.commit()?;
        Ok(confirmed)
    }

    pub fn list_entries(&self, query: EntryQuery) -> Result<EntryPage, AppError> {
        query.validate()?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::WorkerUnavailable)?;
        let mut stmt = connection.prepare(&format!("SELECT {COLUMNS} FROM transactions WHERE book_id='default' AND lifecycle='active' AND (?1='' OR substr(business_date,1,4)=?1) AND (?2='' OR substr(business_date,1,7)=?2) AND (?3='' OR nature=?3) AND (?4='' OR instr(lower(id || ' ' || merchant || ' ' || category || ' ' || note),lower(?4))>0) AND (?5 IS NULL OR business_date<?5 OR (business_date=?5 AND id<?6)) ORDER BY business_date DESC,id DESC LIMIT 51"))?;
        let mut items = stmt
            .query_map(
                params![
                    query.year,
                    query.month,
                    query.nature,
                    query.keyword,
                    query.cursor_date,
                    query.cursor_id
                ],
                entry,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let more = items.len() > 50;
        items.truncate(50);
        let last = if more { items.last() } else { None };
        Ok(EntryPage {
            next_cursor_date: last.map(|e| e.business_date.clone()),
            next_cursor_id: last.map(|e| e.id.clone()),
            items,
        })
    }

    pub fn entry_years(&self) -> Result<Vec<String>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::WorkerUnavailable)?;
        let mut stmt = connection.prepare(
            "SELECT DISTINCT substr(business_date,1,4) AS year FROM transactions \
             WHERE book_id='default' AND lifecycle='active' ORDER BY year DESC",
        )?;
        let years = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(years)
    }

    pub fn overview(&self, month: String) -> Result<Overview, AppError> {
        EntryQuery {
            month: month.clone(),
            ..Default::default()
        }
        .validate()?;
        // Re-evaluate evidence before reporting, including externally imported rows.
        self.ensure_refund_scan()?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::WorkerUnavailable)?;
        let posting = super::refunds::posting_amounts(&connection)?;
        let year = month.get(..4).unwrap_or("");
        let all_dates = year.is_empty();
        let mut months: BTreeMap<String, SummaryAccumulator> = if year.is_empty() {
            BTreeMap::new()
        } else {
            (1..=12)
                .map(|month_number| {
                    (
                        format!("{year}-{month_number:02}"),
                        SummaryAccumulator::default(),
                    )
                })
                .collect()
        };
        let mut selected = SummaryAccumulator::default();
        let mut stmt = connection.prepare("SELECT business_date,nature,amount_minor,review_status,id,category FROM transactions WHERE book_id='default' AND lifecycle='active' AND (?1='' OR substr(business_date,1,4)=?1)")?;
        let rows = stmt.query_map([year], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;
        for row in rows {
            let (business_date, nature, mut amount, status, id, category) = row?;
            let month_key = business_date.get(..7).unwrap_or("");
            let mut month_summary = if year.is_empty() {
                None
            } else {
                Some(
                    months
                        .get_mut(month_key)
                        .ok_or(AppError::Validation("交易日期不在统计年度内。"))?,
                )
            };
            if status == "pending" || posting.get(&id).is_some_and(Option::is_none) {
                if all_dates || month_key == month {
                    selected.pending_count += 1;
                }
                if let Some(summary) = month_summary.as_deref_mut() {
                    summary.pending_count += 1;
                }
                continue;
            }
            if all_dates || month_key == month {
                selected.confirmed_count += 1;
            }
            if let Some(summary) = month_summary.as_deref_mut() {
                summary.confirmed_count += 1;
            }
            if let Some(Some(effective)) = posting.get(&id) {
                amount = *effective;
            }
            if all_dates || month_key == month {
                selected.add_confirmed(&nature, amount, &category)?;
            }
            if let Some(summary) = month_summary {
                summary.add_confirmed(&nature, amount, &category)?;
            }
        }
        let net = selected
            .expense
            .checked_sub(selected.refund)
            .ok_or(AppError::Validation("汇总金额超出范围。"))?;
        let balance = selected
            .income
            .checked_sub(net)
            .ok_or(AppError::Validation("汇总金额超出范围。"))?;
        let monthly = months
            .into_iter()
            .map(|(month, summary)| {
                let net = summary
                    .expense
                    .checked_sub(summary.refund)
                    .ok_or(AppError::Validation("汇总金额超出范围。"))?;
                let balance = summary
                    .income
                    .checked_sub(net)
                    .ok_or(AppError::Validation("汇总金额超出范围。"))?;
                Ok(MonthlyOverview {
                    month,
                    income_minor: summary.income.to_string(),
                    expense_minor: summary.expense.to_string(),
                    refund_minor: summary.refund.to_string(),
                    net_expense_minor: net.to_string(),
                    balance_minor: balance.to_string(),
                    confirmed_count: summary.confirmed_count,
                    pending_count: summary.pending_count,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        let mut category_spending = selected
            .categories
            .into_iter()
            .map(|(category, amount_minor)| CategorySpending {
                category,
                amount_minor: amount_minor.to_string(),
            })
            .collect::<Vec<_>>();
        category_spending.sort_by(|a, b| {
            b.amount_minor
                .len()
                .cmp(&a.amount_minor.len())
                .then_with(|| b.amount_minor.cmp(&a.amount_minor))
                .then_with(|| a.category.cmp(&b.category))
        });
        Ok(Overview {
            income_minor: selected.income.to_string(),
            expense_minor: selected.expense.to_string(),
            refund_minor: selected.refund.to_string(),
            net_expense_minor: net.to_string(),
            balance_minor: balance.to_string(),
            confirmed_count: selected.confirmed_count,
            pending_count: selected.pending_count,
            monthly,
            category_spending,
        })
    }

    pub fn month_detail(&self, month: String) -> Result<MonthDetail, AppError> {
        if month.is_empty() {
            return Err(AppError::Validation("请选择要查看的月份。"));
        }
        EntryQuery {
            month: month.clone(),
            ..Default::default()
        }
        .validate()?;
        self.ensure_refund_scan()?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::WorkerUnavailable)?;
        let posting = super::refunds::posting_amounts(&connection)?;
        let mut days = (1..=31)
            .filter_map(|day| {
                let date = format!("{month}-{day:02}");
                validate_date(&date).ok().map(|_| {
                    (
                        date,
                        DailyAccumulator {
                            summary: SummaryAccumulator::default(),
                            memo: Vec::new(),
                        },
                    )
                })
            })
            .collect::<BTreeMap<_, _>>();
        let mut totals = SummaryAccumulator::default();
        let mut stmt = connection.prepare("SELECT business_date,nature,amount_minor,review_status,id,category,merchant,note FROM transactions WHERE book_id='default' AND lifecycle='active' AND substr(business_date,1,7)=?1 ORDER BY business_date,id")?;
        let rows = stmt.query_map([&month], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
            ))
        })?;
        for row in rows {
            let (date, nature, mut amount, status, id, category, merchant, note) = row?;
            let day = days
                .get_mut(&date)
                .ok_or(AppError::Validation("交易日期不在所选月份内。"))?;
            add_memo(day, &merchant, &note);
            if status == "pending" || posting.get(&id).is_some_and(Option::is_none) {
                day.summary.pending_count += 1;
                totals.pending_count += 1;
                continue;
            }
            day.summary.confirmed_count += 1;
            totals.confirmed_count += 1;
            if let Some(Some(effective)) = posting.get(&id) {
                amount = *effective;
            }
            day.summary.add_confirmed(&nature, amount, &category)?;
            totals.add_confirmed(&nature, amount, &category)?;
        }
        let net_expense = totals
            .expense
            .checked_sub(totals.refund)
            .ok_or(AppError::Validation("汇总金额超出范围。"))?;
        let days = days
            .into_iter()
            .map(|(date, day)| {
                let net_expense = day
                    .summary
                    .expense
                    .checked_sub(day.summary.refund)
                    .ok_or(AppError::Validation("汇总金额超出范围。"))?;
                Ok(DailySpending {
                    date,
                    income_minor: day.summary.income.to_string(),
                    expense_minor: day.summary.expense.to_string(),
                    refund_minor: day.summary.refund.to_string(),
                    net_expense_minor: net_expense.to_string(),
                    pending_count: day.summary.pending_count,
                    categories: category_strings(day.summary.categories),
                    memo: day.memo.join("；"),
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        Ok(MonthDetail {
            month,
            days,
            totals: MonthDetailTotals {
                income_minor: totals.income.to_string(),
                expense_minor: totals.expense.to_string(),
                refund_minor: totals.refund.to_string(),
                net_expense_minor: net_expense.to_string(),
                pending_count: totals.pending_count,
                categories: category_strings(totals.categories),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(nature: &str, amount: &str) -> SaveEntry {
        SaveEntry {
            id: None,
            expected_revision: None,
            request_id: uuid::Uuid::new_v4().to_string(),
            business_date: "2026-10-28".into(),
            nature: nature.into(),
            amount_minor: amount.into(),
            merchant: "测试商户".into(),
            category: "日常".into(),
            note: String::new(),
            review_status: "confirmed".into(),
        }
    }
    #[test]
    fn protects_retries_revisions_and_statistics() {
        let directory = std::env::temp_dir().join(format!("ledger-test-{}", uuid::Uuid::new_v4()));
        {
            let db = Database::open(&directory).unwrap();
            let request = input("expense", "10000");
            let saved = db.save_entry(request.clone()).unwrap();
            assert_eq!(db.save_entry(request.clone()).unwrap(), saved);
            let mut changed = request.clone();
            changed.amount_minor = "999".into();
            assert!(matches!(
                db.save_entry(changed),
                Err(AppError::RequestConflict)
            ));
            db.save_entry(input("refund", "3000")).unwrap();
            db.save_entry(input("transfer", "80000")).unwrap();
            let mut pending = input("income", "9000");
            pending.review_status = "pending".into();
            db.save_entry(pending).unwrap();
            let report = db.overview("2026-10".into()).unwrap();
            assert_eq!(report.net_expense_minor, "7000");
            assert_eq!(report.balance_minor, "-7000");
            assert_eq!(report.pending_count, 1);
            assert_eq!(report.monthly.len(), 12);
            assert_eq!(report.monthly[9].month, "2026-10");
            assert_eq!(report.monthly[9].net_expense_minor, "7000");
            assert_eq!(report.category_spending.len(), 1);
            assert_eq!(report.category_spending[0].category, "日常");
            assert_eq!(report.category_spending[0].amount_minor, "10000");
            let detail = db.month_detail("2026-10".into()).unwrap();
            assert_eq!(detail.days.len(), 31);
            let day = detail
                .days
                .iter()
                .find(|day| day.date == "2026-10-28")
                .unwrap();
            assert_eq!(
                day.categories.get("日常").map(String::as_str),
                Some("10000")
            );
            assert_eq!(day.refund_minor, "3000");
            assert_eq!(day.net_expense_minor, "7000");
            assert_eq!(detail.totals.net_expense_minor, "7000");
            assert_eq!(
                detail.totals.categories.get("日常").map(String::as_str),
                Some("10000")
            );
            let mut update = request;
            update.id = Some(saved.id.clone());
            update.expected_revision = Some(1);
            update.request_id = uuid::Uuid::new_v4().to_string();
            assert_eq!(db.save_entry(update.clone()).unwrap().revision, 2);
            update.request_id = uuid::Uuid::new_v4().to_string();
            assert!(matches!(
                db.save_entry(update),
                Err(AppError::RevisionConflict)
            ));
            let void = VoidEntry {
                id: saved.id,
                expected_revision: 2,
                request_id: uuid::Uuid::new_v4().to_string(),
                reason: "录入错误".into(),
            };
            db.void_entry(void.clone()).unwrap();
            db.void_entry(void).unwrap();
            assert_eq!(
                db.overview("2026-10".into()).unwrap().net_expense_minor,
                "-3000"
            );
            assert_eq!(
                db.list_entries(EntryQuery::default()).unwrap().items.len(),
                3
            );
            let c = db.connection.lock().unwrap();
            let audits: i64 = c
                .query_row("SELECT count(*) FROM audit_events", [], |r| r.get(0))
                .unwrap();
            assert_eq!(audits, 6);
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn filters_entries_by_year_with_and_without_month() {
        let directory = std::env::temp_dir().join(format!("ledger-test-{}", uuid::Uuid::new_v4()));
        {
            let db = Database::open(&directory).unwrap();
            let mut entry = input("expense", "1000");
            entry.business_date = "2025-03-05".into();
            db.save_entry(entry).unwrap();
            entry = input("expense", "2000");
            entry.business_date = "2026-02-11".into();
            db.save_entry(entry).unwrap();
            entry = input("expense", "3000");
            entry.business_date = "2026-09-30".into();
            db.save_entry(entry).unwrap();

            let by_year = |year: &str, month: &str| {
                db.list_entries(EntryQuery {
                    year: year.into(),
                    month: month.into(),
                    ..Default::default()
                })
                .unwrap()
                .items
                .len()
            };
            assert_eq!(by_year("2025", ""), 1);
            assert_eq!(by_year("2026", ""), 2);
            assert_eq!(by_year("2026", "2026-09"), 1);
            assert_eq!(by_year("2024", ""), 0);
            assert_eq!(
                db.entry_years().unwrap(),
                vec!["2026".to_string(), "2025".to_string()]
            );
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn confirming_entries_is_atomic_idempotent_and_audited() {
        let directory = std::env::temp_dir().join(format!("ledger-test-{}", uuid::Uuid::new_v4()));
        {
            let db = Database::open(&directory).unwrap();
            let mut first = input("expense", "1000");
            first.review_status = "pending".into();
            let first = db.save_entry(first).unwrap();
            let mut second = input("income", "2000");
            second.review_status = "pending".into();
            let second = db.save_entry(second).unwrap();
            let request = ConfirmEntries {
                request_id: uuid::Uuid::new_v4().to_string(),
                entries: vec![
                    ConfirmItem {
                        id: first.id.clone(),
                        expected_revision: first.revision,
                    },
                    ConfirmItem {
                        id: second.id.clone(),
                        expected_revision: second.revision,
                    },
                ],
            };

            // A stale revision anywhere rejects the whole batch.
            let mut stale = request.clone();
            stale.request_id = uuid::Uuid::new_v4().to_string();
            stale.entries[1].expected_revision = second.revision + 5;
            assert!(matches!(
                db.confirm_entries(stale),
                Err(AppError::RevisionConflict)
            ));
            let status = |id: &str| {
                db.list_entries(EntryQuery::default())
                    .unwrap()
                    .items
                    .into_iter()
                    .find(|e| e.id == id)
                    .unwrap()
                    .review_status
            };
            assert_eq!(status(&first.id), "pending");
            assert_eq!(status(&second.id), "pending");

            let confirmed = db.confirm_entries(request.clone()).unwrap();
            assert_eq!(confirmed.len(), 2);
            assert!(confirmed.iter().all(|e| e.review_status == "confirmed"));
            assert_eq!(db.confirm_entries(request).unwrap(), confirmed);
            assert_eq!(db.overview("2026-10".into()).unwrap().confirmed_count, 2);

            // Re-confirming an already confirmed row is a conflict, not a silent no-op.
            let mut unknown = input("unknown", "500");
            unknown.review_status = "pending".into();
            let unknown = db.save_entry(unknown).unwrap();
            assert!(matches!(
                db.confirm_entries(ConfirmEntries {
                    request_id: uuid::Uuid::new_v4().to_string(),
                    entries: vec![ConfirmItem {
                        id: unknown.id.clone(),
                        expected_revision: unknown.revision,
                    }],
                }),
                Err(AppError::Validation(_))
            ));

            let c = db.connection.lock().unwrap();
            let confirm_audits: i64 = c
                .query_row(
                    "SELECT count(*) FROM audit_events WHERE action='confirm'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(confirm_audits, 2);
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
}
