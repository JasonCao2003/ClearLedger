use super::Database;
use crate::domain::{ledger::*, AppError};
use rusqlite::{params, Connection, OptionalExtension};

const COLUMNS: &str =
    "id,business_date,nature,amount_minor,merchant,category,note,review_status,lifecycle,revision";
fn entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
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
fn replay(connection: &Connection, id: &str, payload: &str) -> Result<Option<Entry>, AppError> {
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

    pub fn list_entries(&self, query: EntryQuery) -> Result<EntryPage, AppError> {
        query.validate()?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::WorkerUnavailable)?;
        let mut stmt = connection.prepare(&format!("SELECT {COLUMNS} FROM transactions WHERE book_id='default' AND lifecycle='active' AND (?1='' OR substr(business_date,1,7)=?1) AND (?2='' OR nature=?2) AND (?3='' OR instr(lower(merchant || ' ' || category || ' ' || note),lower(?3))>0) AND (?4 IS NULL OR business_date<?4 OR (business_date=?4 AND id<?5)) ORDER BY business_date DESC,id DESC LIMIT 51"))?;
        let mut items = stmt
            .query_map(
                params![
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

    pub fn overview(&self, month: String) -> Result<Overview, AppError> {
        EntryQuery {
            month: month.clone(),
            ..Default::default()
        }
        .validate()?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::WorkerUnavailable)?;
        let mut stmt = connection.prepare("SELECT nature,amount_minor,review_status FROM transactions WHERE book_id='default' AND lifecycle='active' AND (?1='' OR substr(business_date,1,7)=?1)")?;
        let rows = stmt.query_map([month], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let (mut income, mut expense, mut refund) = (0i64, 0i64, 0i64);
        let (mut confirmed_count, mut pending_count) = (0, 0);
        for row in rows {
            let (nature, amount, status) = row?;
            if status == "pending" {
                pending_count += 1;
                continue;
            }
            confirmed_count += 1;
            let total = match nature.as_str() {
                "income" => &mut income,
                "expense" => &mut expense,
                "refund" => &mut refund,
                _ => continue,
            };
            *total = total
                .checked_add(amount)
                .ok_or(AppError::Validation("汇总金额超出范围，请缩小统计区间。"))?;
        }
        let net = expense
            .checked_sub(refund)
            .ok_or(AppError::Validation("汇总金额超出范围。"))?;
        let balance = income
            .checked_sub(net)
            .ok_or(AppError::Validation("汇总金额超出范围。"))?;
        Ok(Overview {
            income_minor: income.to_string(),
            expense_minor: expense.to_string(),
            refund_minor: refund.to_string(),
            net_expense_minor: net.to_string(),
            balance_minor: balance.to_string(),
            confirmed_count,
            pending_count,
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
}
