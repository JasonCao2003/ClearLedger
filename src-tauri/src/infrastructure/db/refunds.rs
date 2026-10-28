use super::{
    ledger::{entry, COLUMNS},
    Database,
};
use crate::domain::{ledger::Entry, refunds::*, AppError};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeMap;

pub(super) fn transactions(connection: &Connection) -> Result<BTreeMap<String, Entry>, AppError> {
    let mut stmt = connection.prepare(&format!(
        "SELECT {COLUMNS} FROM transactions WHERE book_id='default'"
    ))?;
    let values = stmt.query_map([], entry)?.collect::<Result<Vec<_>, _>>()?;
    Ok(values.into_iter().map(|e| (e.id.clone(), e)).collect())
}
fn load_evidence(connection: &Connection) -> Result<Vec<EvidenceEntry>, AppError> {
    let entries = transactions(connection)?;
    let mut evidence: BTreeMap<String, Vec<SourceRecord>> = BTreeMap::new();
    let mut disputed = BTreeMap::new();
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='legacy_records')",
        [],
        |r| r.get(0),
    )?;
    if exists {
        let mut stmt=connection.prepare("SELECT m.transaction_id,r.raw_json,g.pending_reason FROM legacy_transaction_map m JOIN legacy_groups g ON g.id=m.group_id JOIN legacy_group_members gm ON gm.group_id=g.id JOIN legacy_records r ON r.legacy_id=gm.legacy_record_id")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (id, raw, reason) = row?;
            evidence
                .entry(id.clone())
                .or_default()
                .push(serde_json::from_str(&raw)?);
            if reason.contains("跨组重复") {
                disputed.insert(id, true);
            }
        }
    }
    Ok(entries
        .into_values()
        .map(|transaction| EvidenceEntry {
            sources: evidence.remove(&transaction.id).unwrap_or_default(),
            disputed: disputed.contains_key(&transaction.id),
            transaction,
        })
        .collect())
}
fn read_report(connection: &Connection) -> Result<RefundReport, AppError> {
    let mut stmt = connection.prepare("SELECT result_json FROM refund_links ORDER BY refund_id")?;
    let links = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .map(|r| Ok(serde_json::from_str(&r?)?))
        .collect::<Result<Vec<RefundLink>, AppError>>()?;
    let mut stmt =
        connection.prepare("SELECT result_json FROM refund_semantics ORDER BY expense_id")?;
    let checks = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .map(|r| Ok(serde_json::from_str(&r?)?))
        .collect::<Result<Vec<AmountCheck>, AppError>>()?;
    let evidence_available = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='legacy_records')",
        [],
        |r| r.get(0),
    )?;
    Ok(RefundReport {
        links,
        checks,
        evidence_available,
    })
}
fn audit(
    connection: &Connection,
    action: &str,
    id: &str,
    before: Option<&str>,
    after: &str,
) -> Result<(), AppError> {
    if before != Some(after) {
        connection.execute("INSERT INTO refund_audit_events(action,entity_id,before_json,after_json) VALUES(?1,?2,?3,?4)",params![action,id,before,after])?;
    }
    Ok(())
}
fn refresh(connection: &Connection) -> Result<RefundReport, AppError> {
    let previous = read_report(connection)?;
    let report = reconcile(&load_evidence(connection)?, &previous.links)?;
    // Persist even an empty result so retired/voided links cannot affect reporting.
    connection.execute("DELETE FROM refund_links", [])?;
    connection.execute("DELETE FROM refund_semantics", [])?;
    for link in &report.links {
        let json = serde_json::to_string(link)?;
        let before = previous
            .links
            .iter()
            .find(|old| old.refund_id == link.refund_id)
            .map(serde_json::to_string)
            .transpose()?;
        audit(
            connection,
            "scan_link",
            &link.refund_id,
            before.as_deref(),
            &json,
        )?;
        connection.execute(
            "INSERT INTO refund_links VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                link.refund_id,
                link.expense_id,
                link.state,
                link.refund_revision,
                link.expense_revision,
                json
            ],
        )?;
    }
    for check in &report.checks {
        let json = serde_json::to_string(check)?;
        let before = previous
            .checks
            .iter()
            .find(|old| old.expense_id == check.expense_id)
            .map(serde_json::to_string)
            .transpose()?;
        audit(
            connection,
            "scan_semantics",
            &check.expense_id,
            before.as_deref(),
            &json,
        )?;
        let gross = check
            .gross_minor
            .as_deref()
            .map(str::parse::<i64>)
            .transpose()
            .map_err(|_| AppError::Validation("原支付金额无效。"))?;
        let refunded = check
            .refund_minor
            .parse::<i64>()
            .map_err(|_| AppError::Validation("退款金额无效。"))?;
        connection.execute(
            "INSERT INTO refund_semantics VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                check.expense_id,
                check.expense_revision,
                check.mode,
                gross,
                refunded,
                json
            ],
        )?;
    }
    for old in &previous.links {
        if !report
            .links
            .iter()
            .any(|new| new.refund_id == old.refund_id)
        {
            audit(
                connection,
                "retire_link",
                &old.refund_id,
                Some(&serde_json::to_string(old)?),
                "{\"state\":\"inactive\"}",
            )?;
        }
    }
    for old in &previous.checks {
        if !report
            .checks
            .iter()
            .any(|new| new.expense_id == old.expense_id)
        {
            audit(
                connection,
                "retire_semantics",
                &old.expense_id,
                Some(&serde_json::to_string(old)?),
                "{\"mode\":\"inactive\"}",
            )?;
        }
    }
    Ok(report)
}

impl Database {
    pub(super) fn ensure_refund_scan(&self) -> Result<(), AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::WorkerUnavailable)?;
        let current = fingerprint(&connection)?;
        let cached = *self
            .refund_fingerprint
            .lock()
            .map_err(|_| AppError::WorkerUnavailable)?;
        drop(connection);
        if cached != Some(current) {
            self.scan_refunds()?;
        }
        Ok(())
    }
    pub fn scan_refunds(&self) -> Result<RefundReport, AppError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| AppError::WorkerUnavailable)?;
        let tx = connection.transaction()?;
        let report = refresh(&tx)?;
        let current = fingerprint(&tx)?;
        tx.commit()?;
        *self
            .refund_fingerprint
            .lock()
            .map_err(|_| AppError::WorkerUnavailable)? = Some(current);
        Ok(report)
    }
    pub fn refund_report(&self) -> Result<RefundReport, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::WorkerUnavailable)?;
        read_report(&connection)
    }
    pub fn reject_refund(&self, input: RejectRefund) -> Result<RefundReport, AppError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| AppError::WorkerUnavailable)?;
        let tx = connection.transaction()?;
        let old: Option<String> = tx
            .query_row(
                "SELECT result_json FROM refund_links WHERE refund_id=?1",
                [&input.refund_id],
                |r| r.get(0),
            )
            .optional()?;
        let old = old.ok_or(AppError::RevisionConflict)?;
        let mut link: RefundLink = serde_json::from_str(&old)?;
        let current = transactions(&tx)?;
        let original = link
            .expense_id
            .as_ref()
            .and_then(|id| current.get(id))
            .ok_or(AppError::RevisionConflict)?;
        let refund = current
            .get(&link.refund_id)
            .ok_or(AppError::RevisionConflict)?;
        if refund.revision != input.expected_refund_revision
            || original.revision != input.expected_expense_revision
            || link.refund_revision != refund.revision
            || link.expense_revision != Some(original.revision)
            || !["linked", "rejected"].contains(&link.state.as_str())
        {
            return Err(AppError::RevisionConflict);
        }
        link.state = if input.reset { "linked" } else { "rejected" }.into();
        let json = serde_json::to_string(&link)?;
        tx.execute(
            "UPDATE refund_links SET state=?1,result_json=?2 WHERE refund_id=?3",
            params![link.state, json, link.refund_id],
        )?;
        audit(
            &tx,
            if input.reset {
                "reset_rejection"
            } else {
                "reject_link"
            },
            &link.refund_id,
            Some(&old),
            &json,
        )?;
        let report = refresh(&tx)?;
        tx.commit()?;
        Ok(report)
    }
}

fn fingerprint(connection: &Connection) -> Result<(i64, i64, i64), AppError> {
    let version = connection.pragma_query_value(None, "data_version", |r| r.get(0))?;
    let (count, revisions) = connection.query_row(
        "SELECT COUNT(*),COALESCE(SUM(revision),0) FROM transactions",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok((version, count, revisions))
}

pub(super) fn after_edit(
    connection: &Connection,
    before: Option<&Entry>,
    after: &Entry,
) -> Result<(), AppError> {
    let Some(before) = before else { return Ok(()) };
    let financial = before.amount_minor != after.amount_minor
        || before.nature != after.nature
        || before.business_date != after.business_date
        || before.lifecycle != after.lifecycle;
    let report = read_report(connection)?;
    for mut link in report.links {
        if link.refund_id != after.id && link.expense_id.as_deref() != Some(&after.id) {
            continue;
        }
        let old = serde_json::to_string(&link)?;
        if link.refund_id == after.id {
            link.refund_revision = after.revision;
            link.merchant = after.merchant.clone();
            link.business_date = after.business_date.clone();
            link.amount_minor = after.amount_minor.clone();
        }
        if link.expense_id.as_deref() == Some(&after.id) {
            link.expense_revision = Some(after.revision);
            link.expense_merchant = Some(after.merchant.clone());
            link.expense_date = Some(after.business_date.clone());
        }
        if financial {
            link.state = "stale".into();
        }
        let json = serde_json::to_string(&link)?;
        connection.execute("UPDATE refund_links SET state=?1,refund_revision=?2,expense_revision=?3,result_json=?4 WHERE refund_id=?5",params![link.state,link.refund_revision,link.expense_revision,json,link.refund_id])?;
        audit(
            connection,
            "edit_invalidates_or_refreshes_link",
            &link.refund_id,
            Some(&old),
            &json,
        )?;
    }
    for mut check in report.checks {
        let affected=check.expense_id==after.id || (financial && connection.query_row("SELECT EXISTS(SELECT 1 FROM refund_links WHERE expense_id=?1 AND refund_id=?2)",params![check.expense_id,after.id],|r|r.get::<_,bool>(0))?);
        if !affected {
            continue;
        }
        let old = serde_json::to_string(&check)?;
        if check.expense_id == after.id {
            check.expense_revision = after.revision;
            check.merchant = after.merchant.clone();
            check.business_date = after.business_date.clone();
        }
        if financial {
            check.mode = "stale".into();
        }
        let json = serde_json::to_string(&check)?;
        connection.execute("UPDATE refund_semantics SET expense_revision=?1,mode=?2,result_json=?3 WHERE expense_id=?4",params![check.expense_revision,check.mode,json,check.expense_id])?;
        audit(
            connection,
            "edit_invalidates_or_refreshes_semantics",
            &check.expense_id,
            Some(&old),
            &json,
        )?;
    }
    Ok(())
}

pub(super) fn posting_amounts(
    connection: &Connection,
) -> Result<BTreeMap<String, Option<i64>>, AppError> {
    qualified_postings(&transactions(connection)?, &read_report(connection)?)
}

pub fn qualified_postings(
    entries: &BTreeMap<String, Entry>,
    report: &RefundReport,
) -> Result<BTreeMap<String, Option<i64>>, AppError> {
    let mut result = BTreeMap::new();
    for check in &report.checks {
        let Some(original) = entries.get(&check.expense_id) else {
            continue;
        };
        let associated: Vec<_> = report
            .links
            .iter()
            .filter(|l| l.expense_id.as_deref() == Some(&check.expense_id) && l.state != "rejected")
            .collect();
        let mut valid = ["gross", "net"].contains(&check.mode.as_str())
            && original.revision == check.expense_revision
            && original.nature == "expense"
            && original.review_status == "confirmed"
            && original.lifecycle == "active"
            && !associated.is_empty();
        let mut total = 0i64;
        for link in &associated {
            valid &= link.state == "linked" && link.expense_revision == Some(original.revision);
            if let Some(refund) = entries.get(&link.refund_id) {
                valid &= refund.revision == link.refund_revision
                    && refund.review_status == "confirmed"
                    && refund.lifecycle == "active"
                    && refund.nature == "refund"
                    && refund.amount_minor == link.amount_minor;
                total = total
                    .checked_add(
                        refund
                            .amount_minor
                            .parse()
                            .map_err(|_| AppError::Validation("退款金额无效。"))?,
                    )
                    .ok_or(AppError::Validation("退款汇总溢出。"))?;
            } else {
                valid = false;
            }
        }
        let gross = check
            .gross_minor
            .as_deref()
            .and_then(|v| v.parse::<i64>().ok());
        let exported = original
            .amount_minor
            .parse::<i64>()
            .map_err(|_| AppError::Validation("原支付金额无效。"))?;
        valid &= check.refund_minor == total.to_string()
            && gross.is_some_and(|g| g > 0 && total <= g)
            && if check.mode == "net" {
                exported.checked_add(total) == gross
            } else {
                Some(exported) == gross
            };
        result.insert(original.id.clone(), if valid { gross } else { None });
        for link in associated {
            result.insert(
                link.refund_id.clone(),
                if valid {
                    link.amount_minor.parse::<i64>().ok()
                } else {
                    None
                },
            );
        }
    }
    for link in &report.links {
        if link.state != "linked" || !result.contains_key(&link.refund_id) {
            result.insert(link.refund_id.clone(), None);
        }
        if ["review", "conflict", "stale"].contains(&link.state.as_str()) {
            for id in &link.candidate_ids {
                result.insert(id.clone(), None);
            }
        }
    }
    // A separate ambiguous candidate may block an otherwise valid original.
    // Propagate that exclusion to its validated refunds as well.
    for link in report.links.iter().filter(|l| l.state == "linked") {
        if let Some(original) = &link.expense_id {
            if result.get(original).is_some_and(Option::is_none)
                || result.get(&link.refund_id).is_some_and(Option::is_none)
            {
                result.insert(original.clone(), None);
                result.insert(link.refund_id.clone(), None);
            }
        }
    }
    Ok(result)
}
