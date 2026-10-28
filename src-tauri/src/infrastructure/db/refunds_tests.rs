use super::Database;
use crate::domain::{ledger::SaveEntry, refunds::RejectRefund, AppError};
use rusqlite::params;
use serde_json::json;

fn input(nature: &str, amount: &str, date: &str) -> SaveEntry {
    SaveEntry {
        id: None,
        expected_revision: None,
        request_id: uuid::Uuid::new_v4().to_string(),
        business_date: date.into(),
        nature: nature.into(),
        amount_minor: amount.into(),
        merchant: "合成商户".into(),
        category: "餐饮".into(),
        note: "".into(),
        review_status: "confirmed".into(),
    }
}
fn attach(db: &Database, transaction_id: &str, is_refund: bool) {
    let c = db.connection.lock().unwrap();
    c.execute_batch("CREATE TABLE IF NOT EXISTS legacy_records(legacy_id TEXT PRIMARY KEY,raw_json TEXT NOT NULL); CREATE TABLE IF NOT EXISTS legacy_groups(id TEXT PRIMARY KEY,pending_reason TEXT NOT NULL); CREATE TABLE IF NOT EXISTS legacy_group_members(group_id TEXT,legacy_record_id TEXT); CREATE TABLE IF NOT EXISTS legacy_transaction_map(group_id TEXT,transaction_id TEXT);").unwrap();
    let raw = if is_refund {
        json!({"id":"source-refund","source":"支付宝","time":"2026-02-10 12:00:00","amount":3000,"direction":"收入","kind":"退款","status":"退款成功","nature":"退款","raw":{"交易单号":"REF-1","原交易单号":"PAY-1"}})
    } else {
        json!({"id":"source-payment","source":"支付宝","time":"2026-01-10 12:00:00","amount":7000,"direction":"支出","kind":"消费","status":"交易成功","nature":"消费候选","raw":{"交易单号":"PAY-1","原支付金额":"100.00","退款后金额":"70.00"}})
    };
    c.execute(
        "INSERT INTO legacy_records VALUES(?1,?2)",
        params![transaction_id, raw.to_string()],
    )
    .unwrap();
    c.execute("INSERT INTO legacy_groups VALUES(?1,'')", [transaction_id])
        .unwrap();
    c.execute(
        "INSERT INTO legacy_group_members VALUES(?1,?1)",
        [transaction_id],
    )
    .unwrap();
    c.execute(
        "INSERT INTO legacy_transaction_map VALUES(?1,?1)",
        [transaction_id],
    )
    .unwrap();
}

#[test]
fn scans_are_idempotent_and_cross_month_net_is_reconstructed() {
    let directory =
        std::env::temp_dir().join(format!("clearledger-refunds-{}", uuid::Uuid::new_v4()));
    {
        let db = Database::open(&directory).unwrap();
        let p = db
            .save_entry(input("expense", "7000", "2026-01-10"))
            .unwrap();
        let r = db
            .save_entry(input("refund", "3000", "2026-02-10"))
            .unwrap();
        attach(&db, &p.id, false);
        attach(&db, &r.id, true);
        let report = db.scan_refunds().unwrap();
        assert_eq!(report.links[0].state, "linked");
        assert_eq!(report.checks[0].mode, "net");
        let audits = || {
            db.connection
                .lock()
                .unwrap()
                .query_row("SELECT count(*) FROM refund_audit_events", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap()
        };
        let count = audits();
        db.scan_refunds().unwrap();
        assert_eq!(audits(), count);
        let jan = db.overview("2026-01".into()).unwrap();
        let feb = db.overview("2026-02".into()).unwrap();
        assert_eq!(jan.expense_minor, "10000");
        assert_eq!(feb.refund_minor, "3000");
        assert_eq!(db.overview("".into()).unwrap().net_expense_minor, "7000");
        let reject = || RejectRefund {
            refund_id: r.id.clone(),
            expected_refund_revision: 1,
            expected_expense_revision: 1,
            reset: false,
        };
        assert_eq!(
            db.reject_refund(reject()).unwrap().links[0].state,
            "rejected"
        );
        let count = audits();
        db.reject_refund(reject()).unwrap();
        assert_eq!(audits(), count);
        assert_eq!(db.overview("".into()).unwrap().net_expense_minor, "0");
        assert_eq!(db.scan_refunds().unwrap().links[0].state, "rejected");
        db.reject_refund(RejectRefund {
            reset: true,
            ..reject()
        })
        .unwrap();
        assert_eq!(db.overview("".into()).unwrap().net_expense_minor, "7000");
        let mut update = input("refund", "3000", "2026-02-10");
        update.id = Some(r.id.clone());
        update.expected_revision = Some(1);
        update.note = "仅改备注".into();
        db.save_entry(update).unwrap();
        assert_eq!(db.overview("".into()).unwrap().net_expense_minor, "7000");
        assert!(matches!(
            db.reject_refund(reject()),
            Err(AppError::RevisionConflict)
        ));
        let mut update = input("refund", "4000", "2026-02-10");
        update.id = Some(r.id.clone());
        update.expected_revision = Some(2);
        db.save_entry(update).unwrap();
        assert_eq!(db.refund_report().unwrap().links[0].state, "stale");
        let report = db.scan_refunds().unwrap();
        assert_eq!(report.links[0].state, "conflict");
        assert_eq!(db.overview("".into()).unwrap().net_expense_minor, "0");
        // Source evidence and original amount are immutable throughout the scan.
        let c = db.connection.lock().unwrap();
        let amount: i64 = c
            .query_row(
                "SELECT amount_minor FROM transactions WHERE id=?1",
                [&p.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(amount, 7000);
        assert_eq!(
            c.query_row("SELECT count(*) FROM legacy_records", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
#[ignore = "requires an explicitly configured local database COPY"]
fn inspect_local_database_copy() {
    let directory = std::path::PathBuf::from(
        std::env::var("CLEARLEDGER_REFUND_TEST_DIR")
            .expect("configure a copy directory, never the live app data directory"),
    );
    let db = Database::open(&directory).unwrap();
    let count = || {
        db.connection
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM transactions", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
    };
    let before = count();
    let snapshot = super::refunds::transactions(&db.connection.lock().unwrap()).unwrap();
    let started = std::time::Instant::now();
    let report = db.scan_refunds().unwrap();
    let elapsed = started.elapsed();
    db.scan_refunds().unwrap();
    assert_eq!(count(), before);
    assert_eq!(
        super::refunds::transactions(&db.connection.lock().unwrap()).unwrap(),
        snapshot
    );
    let json = serde_json::to_string_pretty(&report).unwrap();
    std::fs::write(directory.join("refund-report.json"), json).unwrap();
    let mut states = std::collections::BTreeMap::<String, usize>::new();
    let mut modes = std::collections::BTreeMap::<String, usize>::new();
    for link in &report.links {
        *states.entry(link.state.clone()).or_default() += 1;
    }
    for check in &report.checks {
        *modes.entry(check.mode.clone()).or_default() += 1;
    }
    println!(
        "rows={before}, refund_states={states:?}, amount_modes={modes:?}, scan_elapsed={elapsed:?}"
    );
}

#[test]
fn an_external_import_invalidates_a_cached_overview() {
    let directory =
        std::env::temp_dir().join(format!("clearledger-refunds-{}", uuid::Uuid::new_v4()));
    {
        let db = Database::open(&directory).unwrap();
        let p = db
            .save_entry(input("expense", "7000", "2026-01-10"))
            .unwrap();
        let r = db
            .save_entry(input("refund", "3000", "2026-02-10"))
            .unwrap();
        assert_eq!(db.overview("".into()).unwrap().net_expense_minor, "4000");
        {
            let external = Database::open(&directory).unwrap();
            attach(&external, &p.id, false);
            attach(&external, &r.id, true);
        }
        assert_eq!(db.overview("".into()).unwrap().net_expense_minor, "7000");
    }
    std::fs::remove_dir_all(directory).unwrap();
}
