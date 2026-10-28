//! Refund matching never uses amount/time/merchant similarity as proof of identity.
use super::{ledger::Entry, AppError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const RULE_VERSION: &str = "refund-v1";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SourceRecord {
    pub id: String,
    pub source: String,
    pub time: String,
    pub amount: i64,
    pub direction: String,
    #[serde(default)]
    pub merchant: String,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub nature: String,
    #[serde(default)]
    pub raw: BTreeMap<String, Value>,
}

#[derive(Clone)]
pub struct EvidenceEntry {
    pub transaction: Entry,
    pub sources: Vec<SourceRecord>,
    pub disputed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RefundLink {
    pub refund_id: String,
    pub expense_id: Option<String>,
    pub state: String,
    pub refund_revision: i64,
    pub expense_revision: Option<i64>,
    pub amount_minor: String,
    pub business_date: String,
    pub merchant: String,
    pub expense_date: Option<String>,
    pub expense_merchant: Option<String>,
    pub sources: Vec<String>,
    pub candidate_ids: Vec<String>,
    pub evidence: Vec<String>,
    pub rule_version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AmountCheck {
    pub expense_id: String,
    pub expense_revision: i64,
    pub merchant: String,
    pub business_date: String,
    pub exported_minor: String,
    pub mode: String,
    pub gross_minor: Option<String>,
    pub refund_minor: String,
    pub evidence: Vec<String>,
    pub rule_version: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefundReport {
    pub links: Vec<RefundLink>,
    pub checks: Vec<AmountCheck>,
    pub evidence_available: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RejectRefund {
    pub refund_id: String,
    pub expected_refund_revision: i64,
    pub expected_expense_revision: i64,
    #[serde(default)]
    pub reset: bool,
}

fn text(record: &SourceRecord, key: &str) -> String {
    record
        .raw
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}
fn identifier(value: &str) -> Option<String> {
    let value = value.trim().trim_start_matches(['\'', '`']).trim();
    if value.is_empty() || ["/", "-", "0", "null", "None"].contains(&value) {
        None
    } else {
        Some(value.to_string())
    }
}
fn account_compatible(a: &SourceRecord, b: &SourceRecord) -> bool {
    let account = |record: &SourceRecord| {
        record
            .account_id
            .as_deref()
            .and_then(identifier)
            .or_else(|| {
                ["本方账号", "账单账号", "账户ID"]
                    .iter()
                    .find_map(|key| identifier(&text(record, key)))
            })
    };
    match (account(a), account(b)) {
        (Some(a), Some(b)) => a == b,
        _ => true,
    }
}
fn same_merchant(a: &SourceRecord, b: &SourceRecord) -> bool {
    let left = identifier(&text(a, "对方账号"));
    let right = identifier(&text(b, "对方账号"));
    if let (Some(left), Some(right)) = (left, right) {
        return left == right;
    }
    let normalize = |r: &SourceRecord| {
        let name = if r.merchant.is_empty() {
            let value = text(r, "交易对方");
            if value.is_empty() {
                text(r, "商户名称")
            } else {
                value
            }
        } else {
            r.merchant.clone()
        };
        name.chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .trim_end_matches("-退款")
            .trim_end_matches("退款")
            .to_lowercase()
    };
    let (a, b) = (normalize(a), normalize(b));
    !a.is_empty()
        && !["/", "支付宝", "财付通", "银联", "美团", "京东商城"].contains(&a.as_str())
        && a == b
}
fn ids(record: &SourceRecord, references: bool) -> BTreeSet<(String, String)> {
    let mut result = BTreeSet::new();
    let fields = if references {
        vec![
            ("原交易单号", "payment"),
            ("原交易订单号", "payment"),
            ("原支付单号", "payment"),
            ("原商家订单号", "merchant"),
            ("原商户单号", "merchant"),
            ("交易订单号", "payment"),
            ("交易单号", "payment"),
            ("商家订单号", "merchant"),
            ("商户单号", "merchant"),
            ("商家单号", "merchant"),
        ]
    } else {
        vec![
            ("交易订单号", "payment"),
            ("交易单号", "payment"),
            ("商家订单号", "merchant"),
            ("商户单号", "merchant"),
            ("商家单号", "merchant"),
        ]
    };
    let explicit_original = references
        && fields.iter().any(|(field, _)| {
            field.starts_with('原') && identifier(&text(record, field)).is_some()
        });
    for (field, namespace) in fields {
        if explicit_original && !field.starts_with('原') {
            continue;
        }
        if let Some(value) = identifier(&text(record, field)) {
            result.insert((namespace.to_string(), value));
        }
    }
    result
}
fn explicit_refund(record: &SourceRecord) -> bool {
    record.kind.contains("退款")
        && !record.kind.contains("申请")
        && (record.direction == "收入" || record.direction == "不计收支")
}
fn successful(record: &SourceRecord) -> bool {
    if ["失败", "处理中", "申请中", "审核", "关闭"]
        .iter()
        .any(|s| record.status.contains(s))
    {
        return false;
    }
    record.status.contains("退款成功")
        || record.status.contains("已全额退款")
        || record.status.contains("已退款")
        || (record.source == "美团"
            && record.kind == "退款"
            && !text(record, "交易成功时间").is_empty())
}
fn currency_ok(record: &SourceRecord) -> bool {
    ["币种", "货币种类"].iter().all(|key| {
        let currency = text(record, key);
        currency.is_empty() || ["CNY", "人民币", "RMB"].contains(&currency.as_str())
    })
}
fn payment(record: &SourceRecord) -> bool {
    record.direction == "支出"
        && !["退款", "转账", "还款", "红包", "充值"]
            .iter()
            .any(|word| record.kind.contains(word))
}
pub fn cents(value: &str) -> Option<i64> {
    let cleaned = value
        .trim()
        .trim_start_matches(['¥', '￥'])
        .replace(',', "");
    let parts: Vec<_> = cleaned.split('.').collect();
    if parts.len() > 2 || parts[0].is_empty() || !parts[0].bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let whole = parts[0].parse::<i64>().ok()?.checked_mul(100)?;
    let fraction = if parts.len() == 2 {
        if parts[1].is_empty()
            || parts[1].len() > 2
            || !parts[1].bytes().all(|c| c.is_ascii_digit())
        {
            return None;
        }
        parts[1]
            .parse::<i64>()
            .ok()?
            .checked_mul(if parts[1].len() == 1 { 10 } else { 1 })?
    } else {
        0
    };
    whole.checked_add(fraction)
}
fn status_refund(record: &SourceRecord) -> Option<i64> {
    if record.source != "微信" || !record.status.starts_with("已退款") {
        return None;
    }
    let value = record
        .status
        .trim_start_matches("已退款")
        .trim_matches(['(', ')', '（', '）']);
    cents(value)
}
fn amount(entry: &Entry) -> Result<i64, AppError> {
    entry
        .amount_minor
        .parse()
        .map_err(|_| AppError::Validation("交易金额无效。"))
}

pub fn reconcile(
    entries: &[EvidenceEntry],
    previous: &[RefundLink],
) -> Result<RefundReport, AppError> {
    let mut index: BTreeMap<(String, String, String), BTreeSet<usize>> = BTreeMap::new();
    for (i, entry) in entries.iter().enumerate() {
        if entry.transaction.lifecycle != "active"
            || !["expense", "unknown"].contains(&entry.transaction.nature.as_str())
        {
            continue;
        }
        for record in entry.sources.iter().filter(|r| payment(r)) {
            for (namespace, value) in ids(record, false) {
                index
                    .entry((record.source.clone(), namespace, value))
                    .or_default()
                    .insert(i);
            }
        }
    }
    let mut links = Vec::new();
    let mut duplicates: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for entry in entries
        .iter()
        .filter(|e| e.transaction.lifecycle == "active")
    {
        let refunds: Vec<_> = entry
            .sources
            .iter()
            .filter(|r| explicit_refund(r))
            .collect();
        if refunds.is_empty()
            && !(entry.transaction.nature == "refund" && !entry.sources.is_empty())
        {
            continue;
        }
        let mut candidates = BTreeSet::new();
        let mut evidence = Vec::new();
        for record in &refunds {
            for (namespace, value) in ids(record, true) {
                if let Some(matches) =
                    index.get(&(record.source.clone(), namespace.clone(), value.clone()))
                {
                    let mut matched = false;
                    for &i in matches {
                        let scoped = entries[i].sources.iter().any(|p| {
                            p.source == record.source
                                && ids(p, false).contains(&(namespace.clone(), value.clone()))
                                && account_compatible(record, p)
                                && (namespace != "merchant" || same_merchant(record, p))
                        });
                        if entries[i].transaction.id != entry.transaction.id && scoped {
                            candidates.insert(i);
                            matched = true;
                        }
                    }
                    if matched {
                        evidence.push(format!(
                            "{}：同命名空间{}标识精确匹配（原始凭证 {}）",
                            record.source, namespace, record.id
                        ));
                    }
                }
            }
            let event_ids = ids(record, false);
            if !event_ids.is_empty() {
                let fingerprint = serde_json::to_string(&(
                    record.source.clone(),
                    event_ids,
                    record.time.clone(),
                    record.amount,
                ))?;
                duplicates
                    .entry(fingerprint)
                    .or_default()
                    .insert(entry.transaction.id.clone());
            }
        }
        let original = if candidates.len() == 1 {
            candidates.first().map(|&i| &entries[i])
        } else {
            None
        };
        let mut state = if original.is_some() {
            "linked"
        } else if candidates.is_empty() {
            "unmatched"
        } else {
            "review"
        };
        if candidates.is_empty() {
            evidence.push(
                "未找到同渠道、同标识命名空间的原支付；不会按同金额或商户相似自动关联。".into(),
            );
        }
        if candidates.len() > 1 {
            evidence.push("标识对应多个原支付，可能是拆单或重复凭证，需人工核实。".into());
        }
        if refunds.is_empty()
            || !refunds.iter().all(|r| {
                successful(r)
                    && currency_ok(r)
                    && r.amount == amount(&entry.transaction).unwrap_or(-1)
            })
            || entry.transaction.nature != "refund"
            || entry.disputed
        {
            state = "conflict";
            evidence.push("退款成功状态、币种、当前性质/金额或原分组存在冲突。".into());
        }
        if let Some(original) = original {
            let chronology_ok = refunds.iter().all(|refund| {
                original
                    .sources
                    .iter()
                    .filter(|p| {
                        p.source == refund.source
                            && payment(p)
                            && !ids(p, false).is_disjoint(&ids(refund, true))
                    })
                    .all(|p| p.time <= refund.time)
            });
            if !chronology_ok
                || original.sources.iter().any(|record| !currency_ok(record))
                || original.transaction.business_date > entry.transaction.business_date
                || original.disputed
            {
                state = "conflict";
                evidence.push("退款早于原支付、币种不兼容，或原支付存在未解决分组争议。".into());
            }
        }
        let mut link = RefundLink {
            refund_id: entry.transaction.id.clone(),
            expense_id: original.map(|o| o.transaction.id.clone()),
            state: state.into(),
            refund_revision: entry.transaction.revision,
            expense_revision: original.map(|o| o.transaction.revision),
            amount_minor: entry.transaction.amount_minor.clone(),
            business_date: entry.transaction.business_date.clone(),
            merchant: entry.transaction.merchant.clone(),
            expense_date: original.map(|o| o.transaction.business_date.clone()),
            expense_merchant: original.map(|o| o.transaction.merchant.clone()),
            sources: refunds
                .iter()
                .map(|r| r.source.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            candidate_ids: candidates
                .iter()
                .map(|&i| entries[i].transaction.id.clone())
                .collect(),
            evidence,
            rule_version: RULE_VERSION.into(),
        };
        if previous.iter().any(|p| {
            p.state == "rejected"
                && p.refund_id == link.refund_id
                && p.expense_id == link.expense_id
                && p.refund_revision == link.refund_revision
                && p.expense_revision == link.expense_revision
        }) {
            link.state = "rejected".into();
            link.evidence
                .push("人工已驳回该关联，重新扫描不会恢复。".into());
        }
        links.push(link);
    }
    let duplicated: BTreeSet<_> = duplicates
        .values()
        .filter(|ids| ids.len() > 1)
        .flatten()
        .cloned()
        .collect();
    for link in &mut links {
        if duplicated.contains(&link.refund_id) && link.state != "rejected" {
            link.state = "conflict".into();
            link.evidence
                .push("不同交易包含相同渠道、标识、时间和金额的退款事件，需先核对重复。".into());
        }
    }
    let mut checks = Vec::new();
    for entry in entries
        .iter()
        .filter(|e| e.transaction.lifecycle == "active")
    {
        let associated: Vec<_> = links
            .iter()
            .filter(|l| {
                l.expense_id.as_deref() == Some(&entry.transaction.id) && l.state != "rejected"
            })
            .collect();
        let flagged = entry.sources.iter().any(|r| {
            payment(r)
                && (r.status.contains("退款")
                    || r.nature == "退款状态待核实"
                    || ["退款后金额", "退款后实付金额", "交易净额", "净支出"]
                        .iter()
                        .any(|key| !text(r, key).is_empty())
                    || ["原支付金额", "原实付金额", "原交易实付金额"]
                        .iter()
                        .any(|key| {
                            cents(&text(r, key))
                                .is_some_and(|v| v.to_string() != entry.transaction.amount_minor)
                        }))
        });
        if associated.is_empty() && !flagged {
            continue;
        }
        let total = associated.iter().try_fold(0i64, |sum, link| {
            sum.checked_add(link.amount_minor.parse::<i64>().ok()?)
        });
        let total = total.ok_or(AppError::Validation("累计退款金额超出范围。"))?;
        let check = validate_amount(entry, total, &associated)?;
        if check.mode == "conflict" {
            for link in &mut links {
                if link.expense_id.as_deref() == Some(&entry.transaction.id)
                    && link.state == "linked"
                {
                    link.state = "conflict".into();
                    link.evidence
                        .push("原支付金额口径校验冲突，暂停计入。".into());
                }
            }
        }
        checks.push(check);
    }
    links.sort_by(|a, b| {
        b.business_date
            .cmp(&a.business_date)
            .then(a.refund_id.cmp(&b.refund_id))
    });
    checks.sort_by(|a, b| {
        b.business_date
            .cmp(&a.business_date)
            .then(a.expense_id.cmp(&b.expense_id))
    });
    Ok(RefundReport {
        evidence_available: entries.iter().any(|e| !e.sources.is_empty()),
        links,
        checks,
    })
}

fn validate_amount(
    entry: &EvidenceEntry,
    total: i64,
    links: &[&RefundLink],
) -> Result<AmountCheck, AppError> {
    let exported = amount(&entry.transaction)?;
    let mut check = AmountCheck {
        expense_id: entry.transaction.id.clone(),
        expense_revision: entry.transaction.revision,
        merchant: entry.transaction.merchant.clone(),
        business_date: entry.transaction.business_date.clone(),
        exported_minor: exported.to_string(),
        mode: "unknown".into(),
        gross_minor: None,
        refund_minor: total.to_string(),
        evidence: Vec::new(),
        rule_version: RULE_VERSION.into(),
    };
    if links.is_empty() {
        check
            .evidence
            .push("只有原消费退款状态，未找到独立退款事件；不凭状态生成退款。".into());
        return Ok(check);
    }
    if links.iter().any(|l| l.state != "linked") || entry.disputed {
        check
            .evidence
            .push("关联或来源分组尚有争议，金额口径保持待核实。".into());
        return Ok(check);
    }
    let mut paid = BTreeSet::new();
    let mut net = BTreeSet::new();
    let refund_channels: BTreeSet<_> = links
        .iter()
        .flat_map(|l| l.sources.iter().cloned())
        .collect();
    for record in &entry.sources {
        if !payment(record) || !currency_ok(record) {
            continue;
        }
        // Face-value order totals and coupon discounts are deliberately excluded.
        for key in ["原支付金额", "原实付金额", "原交易实付金额"] {
            let value = text(record, key);
            if !value.is_empty() {
                if let Some(value) = cents(&value) {
                    paid.insert(value);
                    check
                        .evidence
                        .push(format!("{} 凭证含明确的{key}字段。", record.source));
                } else {
                    check.mode = "conflict".into();
                    check.evidence.push(format!("{key}不是合法金额。"));
                    return Ok(check);
                }
            }
        }
        for key in ["退款后金额", "退款后实付金额", "交易净额", "净支出"] {
            let value = text(record, key);
            if !value.is_empty() {
                if let Some(value) = cents(&value) {
                    net.insert(value);
                } else {
                    check.mode = "conflict".into();
                    check.evidence.push(format!("{key}不是合法金额。"));
                    return Ok(check);
                }
            }
        }
        if record.source == "工商银行"
            && !refund_channels.contains(&record.source)
            && record.amount > 0
        {
            paid.insert(record.amount);
            check
                .evidence
                .push("已归组的独立资金渠道扣款提供原支付金额证据。".into());
        }
        if record.source == "微信" && record.amount == exported {
            if record.status == "已全额退款" && total != exported {
                check.mode = "conflict".into();
                check
                    .evidence
                    .push("微信全额退款状态与独立退款合计不一致，可能缺少或重复退款流水。".into());
                return Ok(check);
            }
            if record.status == "已全额退款" && total == exported {
                paid.insert(exported);
                check
                    .evidence
                    .push("微信原支出金额与独立退款事件、状态累计退款金额一致。".into());
            } else if record.status.starts_with("已退款")
                && status_refund(record).is_some_and(|v| v != total)
            {
                check.mode = "conflict".into();
                check
                    .evidence
                    .push("微信状态累计退款与已关联退款事件合计不一致。".into());
                return Ok(check);
            } else if status_refund(record) == Some(total) {
                check.evidence.push(
                    "微信状态累计退款与独立事件一致，但部分退款状态本身不能证明原支付毛额。".into(),
                );
            }
        }
    }
    if paid.len() > 1 || net.len() > 1 {
        check.mode = "conflict".into();
        check.evidence.push("原支付或净额证据之间不一致。".into());
        return Ok(check);
    }
    if let Some(&gross) = paid.first() {
        if gross <= 0 || total > gross {
            check.mode = "conflict".into();
            check
                .evidence
                .push("累计退款超过已验证的用户原支付金额。".into());
            return Ok(check);
        }
        if gross == exported && net.first().is_none_or(|&value| value == gross - total) {
            check.mode = "gross".into();
            check.gross_minor = Some(gross.to_string());
            check
                .evidence
                .push("导出金额为原支出，退款须作为独立事件冲减。".into());
        } else if exported.checked_add(total) == Some(gross)
            && net.first().is_none_or(|&value| value == exported)
        {
            check.mode = "net".into();
            check.gross_minor = Some(gross.to_string());
            check.evidence.push("导出金额已扣退款：导出净额 + 独立退款合计 = 原实付；统计时还原原支出再记退款，防止二次扣减。".into());
        } else {
            check.mode = "conflict".into();
            check
                .evidence
                .push("原实付、导出金额及退款合计无法满足毛额/净额等式。".into());
        }
    } else {
        check.evidence.push(
            "缺少原实付或独立扣款证据，不能仅凭退款状态或订单标价判定净额；优惠差额不等于退款。"
                .into(),
        );
    }
    Ok(check)
}
