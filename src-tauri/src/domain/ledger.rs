use super::AppError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub business_date: String,
    pub nature: String,
    pub amount_minor: String,
    pub merchant: String,
    pub category: String,
    pub note: String,
    pub review_status: String,
    pub lifecycle: String,
    pub revision: i64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveEntry {
    pub id: Option<String>,
    pub expected_revision: Option<i64>,
    pub request_id: String,
    pub business_date: String,
    pub nature: String,
    pub amount_minor: String,
    pub merchant: String,
    pub category: String,
    pub note: String,
    pub review_status: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VoidEntry {
    pub id: String,
    pub expected_revision: i64,
    pub request_id: String,
    pub reason: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfirmEntries {
    pub request_id: String,
    pub entries: Vec<ConfirmItem>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfirmItem {
    pub id: String,
    pub expected_revision: i64,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EntryQuery {
    pub year: String,
    pub month: String,
    pub nature: String,
    pub keyword: String,
    pub cursor_date: Option<String>,
    pub cursor_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryPage {
    pub items: Vec<Entry>,
    pub next_cursor_date: Option<String>,
    pub next_cursor_id: Option<String>,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub income_minor: String,
    pub expense_minor: String,
    pub refund_minor: String,
    pub net_expense_minor: String,
    pub balance_minor: String,
    pub confirmed_count: u64,
    pub pending_count: u64,
    pub monthly: Vec<MonthlyOverview>,
    pub category_spending: Vec<CategorySpending>,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MonthlyOverview {
    pub month: String,
    pub income_minor: String,
    pub expense_minor: String,
    pub refund_minor: String,
    pub net_expense_minor: String,
    pub balance_minor: String,
    pub confirmed_count: u64,
    pub pending_count: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorySpending {
    pub category: String,
    pub amount_minor: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthDetail {
    pub month: String,
    pub days: Vec<DailySpending>,
    pub totals: MonthDetailTotals,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailySpending {
    pub date: String,
    pub income_minor: String,
    pub expense_minor: String,
    pub refund_minor: String,
    pub net_expense_minor: String,
    pub pending_count: u64,
    pub categories: std::collections::BTreeMap<String, String>,
    pub memo: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthDetailTotals {
    pub income_minor: String,
    pub expense_minor: String,
    pub refund_minor: String,
    pub net_expense_minor: String,
    pub pending_count: u64,
    pub categories: std::collections::BTreeMap<String, String>,
}

pub fn validate_date(value: &str) -> Result<(), AppError> {
    let invalid = || AppError::Validation("请输入有效日期（YYYY-MM-DD）。");
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return Err(invalid());
    }
    let year: u32 = value[..4].parse().map_err(|_| invalid())?;
    let month: usize = value[5..7].parse().map_err(|_| invalid())?;
    let day: u32 = value[8..].parse().map_err(|_| invalid())?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if year == 0 || !(1..=12).contains(&month) || day == 0 || day > days[month - 1] {
        return Err(invalid());
    }
    Ok(())
}

pub fn validate_nature(value: &str) -> Result<(), AppError> {
    if ![
        "expense",
        "income",
        "refund",
        "transfer",
        "repayment",
        "loan",
        "unknown",
    ]
    .contains(&value)
    {
        return Err(AppError::Validation("交易性质无效。"));
    }
    Ok(())
}

impl SaveEntry {
    pub fn validate(&self) -> Result<i64, AppError> {
        validate_date(&self.business_date)?;
        validate_nature(&self.nature)?;
        if !["pending", "confirmed"].contains(&self.review_status.as_str())
            || (self.nature == "unknown" && self.review_status == "confirmed")
        {
            return Err(AppError::Validation("未知性质不能确认记账。"));
        }
        if self.amount_minor.is_empty()
            || self.amount_minor.len() > 19
            || !self.amount_minor.bytes().all(|c| c.is_ascii_digit())
        {
            return Err(AppError::Validation("金额必须为正整数分。"));
        }
        let amount: i64 = self
            .amount_minor
            .parse()
            .map_err(|_| AppError::Validation("金额超出范围。"))?;
        if amount <= 0 {
            return Err(AppError::Validation("金额必须大于零。"));
        }
        if self.id.is_some() != self.expected_revision.is_some()
            || self.expected_revision.is_some_and(|r| r < 1)
            || self.merchant.chars().count() > 200
            || self.category.chars().count() > 80
            || self.note.chars().count() > 2000
        {
            return Err(AppError::Validation("版本或文本长度无效。"));
        }
        validate_request(&self.request_id)?;
        Ok(amount)
    }
}

pub fn validate_request(id: &str) -> Result<(), AppError> {
    uuid::Uuid::parse_str(id).map_err(|_| AppError::Validation("请求标识无效。"))?;
    Ok(())
}

impl ConfirmEntries {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_request(&self.request_id)?;
        if self.entries.is_empty() || self.entries.len() > 200 {
            return Err(AppError::Validation("请选择 1 至 200 笔交易。"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for item in &self.entries {
            if item.id.trim().is_empty()
                || item.expected_revision < 1
                || !seen.insert(item.id.as_str())
            {
                return Err(AppError::Validation("待核实交易清单无效。"));
            }
        }
        Ok(())
    }
}

impl EntryQuery {
    pub fn validate(&self) -> Result<(), AppError> {
        if !self.year.is_empty() {
            if self.year.len() != 4 {
                return Err(AppError::Validation("年份格式无效。"));
            }
            validate_date(&format!("{}-01-01", self.year))?;
        }
        if !self.month.is_empty() {
            if self.month.len() != 7 {
                return Err(AppError::Validation("月份格式无效。"));
            }
            validate_date(&format!("{}-01", self.month))?;
        }
        if !self.nature.is_empty() {
            validate_nature(&self.nature)?;
        }
        if self.keyword.chars().count() > 200
            || self.cursor_date.is_some() != self.cursor_id.is_some()
        {
            return Err(AppError::Validation("查询条件无效。"));
        }
        if let Some(date) = &self.cursor_date {
            validate_date(date)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_real_calendar_dates() {
        assert!(validate_date("2024-02-29").is_ok());
        for date in [
            "2025-02-29",
            "0000-01-01",
            "2026-13-01",
            "2026-04-31",
            "２０２６-01-01",
        ] {
            assert!(validate_date(date).is_err(), "{date}");
        }
    }
}
