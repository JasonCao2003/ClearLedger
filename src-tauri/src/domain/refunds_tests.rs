use super::ledger::Entry;
use super::refunds::*;
use serde_json::json;
use std::collections::BTreeMap;

fn transaction(id: &str, nature: &str, amount: &str, date: &str) -> Entry {
    Entry {
        id: id.into(),
        nature: nature.into(),
        amount_minor: amount.into(),
        business_date: date.into(),
        merchant: "合成商户".into(),
        category: "餐饮".into(),
        note: "".into(),
        review_status: "confirmed".into(),
        lifecycle: "active".into(),
        revision: 1,
    }
}
fn original(id: &str, amount: &str) -> EvidenceEntry {
    EvidenceEntry {transaction:transaction(id,"expense",amount,"2026-01-10"),disputed:false,
        sources:vec![serde_json::from_value(json!({"id":format!("raw-{id}"),"source":"支付宝","time":"2026-01-10 12:00:00","amount":amount.parse::<i64>().unwrap(),"direction":"支出","kind":"消费","status":"交易成功","nature":"消费候选","raw":{"交易单号":"PAY-001","原支付金额":"100.00"}})).unwrap()]}
}
fn refund(id: &str, amount: &str) -> EvidenceEntry {
    EvidenceEntry {transaction:transaction(id,"refund",amount,"2026-02-10"),disputed:false,
        sources:vec![serde_json::from_value(json!({"id":format!("raw-{id}"),"source":"支付宝","time":"2026-02-10 12:00:00","amount":amount.parse::<i64>().unwrap(),"direction":"收入","kind":"退款","status":"退款成功","nature":"退款","raw":{"原交易单号":"PAY-001","交易单号":format!("REF-{id}")}})).unwrap()]}
}

#[test]
fn exact_references_link_and_net_amount_is_not_refunded_twice() {
    let entries = vec![original("payment", "7000"), refund("refund", "3000")];
    let report = reconcile(&entries, &[]).unwrap();
    assert_eq!(report.links[0].state, "linked");
    assert_eq!(report.checks[0].mode, "net");
    assert_eq!(report.checks[0].gross_minor.as_deref(), Some("10000"));
    let map = entries
        .iter()
        .map(|e| (e.transaction.id.clone(), e.transaction.clone()))
        .collect();
    let postings = crate::infrastructure::db::refunds::qualified_postings(&map, &report).unwrap();
    // January is reconstructed to 100; February returns 30; total remains 70.
    assert_eq!(postings["payment"], Some(10000));
    assert_eq!(postings["refund"], Some(3000));
    assert_eq!(
        postings["payment"].unwrap() - postings["refund"].unwrap(),
        7000
    );
}

#[test]
fn partial_refunds_accumulate_and_do_not_merge_events() {
    let report = reconcile(
        &[
            original("payment", "10000"),
            refund("r1", "3000"),
            refund("r2", "2000"),
        ],
        &[],
    )
    .unwrap();
    assert_eq!(report.links.len(), 2);
    assert!(report.links.iter().all(|l| l.state == "linked"));
    assert_eq!(report.checks[0].mode, "gross");
    assert_eq!(report.checks[0].refund_minor, "5000");
}

#[test]
fn amount_and_merchant_similarity_never_establish_identity() {
    let p = original("p", "10000");
    let mut r = refund("r", "10000");
    r.sources[0].raw.clear();
    assert_eq!(
        reconcile(&[p.clone(), r.clone()], &[]).unwrap().links[0].state,
        "unmatched"
    );
    r = refund("r", "10000");
    r.sources[0].source = "微信".into();
    assert_eq!(reconcile(&[p, r], &[]).unwrap().links[0].state, "unmatched");
}

#[test]
fn id_namespaces_and_explicit_original_references_cannot_be_overridden() {
    let mut p = original("p", "10000");
    let mut r = refund("r", "1000");
    r.sources[0].raw.clear();
    r.sources[0]
        .raw
        .insert("商家订单号".into(), json!("PAY-001"));
    assert_eq!(
        reconcile(&[p.clone(), r.clone()], &[]).unwrap().links[0].state,
        "unmatched"
    );
    p.sources[0]
        .raw
        .insert("商家订单号".into(), json!("MERCHANT-1"));
    r.sources[0]
        .raw
        .insert("商家订单号".into(), json!("MERCHANT-1"));
    r.sources[0]
        .raw
        .insert("原交易单号".into(), json!("MISSING-ORIGINAL"));
    assert_eq!(reconcile(&[p, r], &[]).unwrap().links[0].state, "unmatched");
}

#[test]
fn merchant_order_ids_are_scoped_to_merchant_and_known_account() {
    let mut p = original("p", "10000");
    p.sources[0].merchant = "合成商户甲".into();
    p.sources[0]
        .raw
        .insert("商家订单号".into(), json!("ORDER-1"));
    let mut r = refund("r", "1000");
    r.sources[0].merchant = "合成商户乙".into();
    r.sources[0].raw.clear();
    r.sources[0]
        .raw
        .insert("商家订单号".into(), json!("ORDER-1"));
    assert_eq!(
        reconcile(&[p.clone(), r.clone()], &[]).unwrap().links[0].state,
        "unmatched"
    );
    r.sources[0].merchant = "合成商户甲-退款".into();
    assert_eq!(
        reconcile(&[p.clone(), r.clone()], &[]).unwrap().links[0].state,
        "linked"
    );
    p.sources[0].account_id = Some("ACCOUNT-A".into());
    r.sources[0].account_id = Some("ACCOUNT-B".into());
    assert_eq!(reconcile(&[p, r], &[]).unwrap().links[0].state, "unmatched");
}

#[test]
fn ambiguous_orders_remain_review_and_duplicate_events_conflict() {
    let report = reconcile(
        &[
            original("p1", "10000"),
            original("p2", "10000"),
            refund("r", "1000"),
        ],
        &[],
    )
    .unwrap();
    assert_eq!(report.links[0].state, "review");
    assert_eq!(report.links[0].candidate_ids.len(), 2);
    let r1 = refund("r1", "1000");
    let mut r2 = refund("r2", "1000");
    r2.sources[0].raw = r1.sources[0].raw.clone();
    let report = reconcile(&[original("p", "10000"), r1, r2], &[]).unwrap();
    assert!(report.links.iter().all(|l| l.state == "conflict"));
}

#[test]
fn over_refunds_failed_states_and_early_refunds_are_blocked() {
    let report = reconcile(&[original("p", "10000"), refund("r", "10001")], &[]).unwrap();
    assert_eq!(report.checks[0].mode, "conflict");
    assert_eq!(report.links[0].state, "conflict");
    let mut r = refund("r", "1000");
    r.sources[0].status = "退款处理中".into();
    assert_eq!(
        reconcile(&[original("p", "10000"), r], &[]).unwrap().links[0].state,
        "conflict"
    );
    let mut r = refund("r", "1000");
    r.transaction.business_date = "2025-01-01".into();
    assert_eq!(
        reconcile(&[original("p", "10000"), r], &[]).unwrap().links[0].state,
        "conflict"
    );
}

#[test]
fn coupons_and_order_face_value_are_not_refund_amounts() {
    let mut p = original("p", "7000");
    p.sources[0].raw.remove("原支付金额");
    p.sources[0].raw.insert("订单金额".into(), json!("100.00"));
    p.sources[0].raw.insert("实付金额".into(), json!("70.00"));
    assert_eq!(
        reconcile(&[p, refund("r", "3000")], &[]).unwrap().checks[0].mode,
        "unknown"
    );
}

#[test]
fn another_payment_platform_is_not_independent_cash_gross_proof() {
    let mut p = original("p", "7000");
    p.sources[0].raw.remove("原支付金额");
    let mut other = p.sources[0].clone();
    other.source = "微信".into();
    other.raw.clear();
    p.sources.push(other);
    assert_eq!(
        reconcile(&[p.clone(), refund("r", "3000")], &[])
            .unwrap()
            .checks[0]
            .mode,
        "unknown"
    );
    p.sources[1].source = "工商银行".into();
    p.sources[1].amount = 10000;
    assert_eq!(
        reconcile(&[p, refund("r", "3000")], &[]).unwrap().checks[0].mode,
        "net"
    );
}

#[test]
fn refund_status_alone_never_creates_an_event() {
    let mut p = original("p", "10000");
    p.sources[0].status = "已全额退款".into();
    p.sources[0].source = "微信".into();
    let report = reconcile(&[p], &[]).unwrap();
    assert!(report.links.is_empty());
    assert_eq!(report.checks[0].mode, "unknown");
}

#[test]
fn imported_refund_without_a_distinct_event_marker_is_quarantined() {
    let mut r = refund("r", "1000");
    r.sources[0].kind = "交通出行".into();
    let report = reconcile(&[original("p", "10000"), r], &[]).unwrap();
    assert_eq!(report.links.len(), 1);
    assert_eq!(report.links[0].state, "conflict");
}

#[test]
fn wechat_status_total_must_match_independent_events() {
    let mut p = original("p", "10000");
    p.sources[0].raw.remove("原支付金额");
    p.sources[0].source = "微信".into();
    p.sources[0].status = "已退款(¥30.00)".into();
    let mut r = refund("r", "3000");
    r.sources[0].source = "微信".into();
    assert_eq!(
        reconcile(&[p.clone(), r.clone()], &[]).unwrap().checks[0].mode,
        "unknown"
    );
    p.sources[0]
        .raw
        .insert("原支付金额".into(), json!("100.00"));
    assert_eq!(
        reconcile(&[p.clone(), r.clone()], &[]).unwrap().checks[0].mode,
        "gross"
    );
    p.sources[0].status = "已退款¥20.00".into();
    assert_eq!(reconcile(&[p, r], &[]).unwrap().checks[0].mode, "conflict");
}

#[test]
fn unresolved_or_unconfirmed_pairs_do_not_leak_into_statistics() {
    let entries = vec![original("p", "7000"), refund("r", "3000")];
    let report = reconcile(&entries, &[]).unwrap();
    let mut map: BTreeMap<_, _> = entries
        .iter()
        .map(|e| (e.transaction.id.clone(), e.transaction.clone()))
        .collect();
    map.get_mut("r").unwrap().review_status = "pending".into();
    let postings = crate::infrastructure::db::refunds::qualified_postings(&map, &report).unwrap();
    assert_eq!(postings["p"], None);
    assert_eq!(postings["r"], None);
    map.get_mut("r").unwrap().review_status = "confirmed".into();
    map.get_mut("p").unwrap().revision = 2;
    let postings = crate::infrastructure::db::refunds::qualified_postings(&map, &report).unwrap();
    assert_eq!(postings["p"], None);
    assert_eq!(postings["r"], None);
}

#[test]
fn an_ambiguous_related_refund_blocks_the_whole_group() {
    let mut p1 = original("p1", "10000");
    let mut p2 = original("p2", "10000");
    for p in [&mut p1, &mut p2] {
        p.sources[0].merchant = "同一合成商户".into();
        p.sources[0]
            .raw
            .insert("商家订单号".into(), json!("SHARED-ORDER"));
    }
    p2.sources[0]
        .raw
        .insert("交易单号".into(), json!("PAY-002"));
    let r1 = refund("r1", "1000");
    let mut r2 = refund("r2", "1000");
    r2.sources[0].raw.clear();
    r2.sources[0].merchant = "同一合成商户".into();
    r2.sources[0]
        .raw
        .insert("商家订单号".into(), json!("SHARED-ORDER"));
    let entries = vec![p1, p2, r1, r2];
    let report = reconcile(&entries, &[]).unwrap();
    assert!(report.links.iter().any(|l| l.state == "linked"));
    assert!(report.links.iter().any(|l| l.state == "review"));
    let map = entries
        .into_iter()
        .map(|e| (e.transaction.id.clone(), e.transaction))
        .collect();
    let postings = crate::infrastructure::db::refunds::qualified_postings(&map, &report).unwrap();
    for id in ["p1", "p2", "r1", "r2"] {
        assert_eq!(postings[id], None);
    }
}

#[test]
fn rejecting_a_link_survives_a_rescan() {
    let entries = vec![original("p", "10000"), refund("r", "3000")];
    let report = reconcile(&entries, &[]).unwrap();
    let mut link = report.links[0].clone();
    link.state = "rejected".into();
    assert_eq!(
        reconcile(&entries, &[link]).unwrap().links[0].state,
        "rejected"
    );
}

#[test]
fn declared_money_uses_checked_decimal_conversion() {
    assert_eq!(cents("¥1,234.56"), Some(123456));
    assert_eq!(cents("0.29"), Some(29));
    for value in ["1.001", "NaN", "-1", "92233720368547758.08"] {
        assert_eq!(cents(value), None);
    }
}
