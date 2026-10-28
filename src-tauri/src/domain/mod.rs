use serde::Serialize;
pub mod ledger;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database schema is newer than this application")]
    SchemaTooNew,
    #[error("database worker is unavailable")]
    WorkerUnavailable,
    #[error("{0}")]
    Validation(&'static str),
    #[error("transaction was modified or voided")]
    RevisionConflict,
    #[error("request id reused with different input")]
    RequestConflict,
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorDto {
    code: &'static str,
    message: &'static str,
    trace_id: String,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum ApiResult<T> {
    Success { ok: bool, data: T },
    Failure { ok: bool, error: ErrorDto },
}

impl<T> From<Result<T, AppError>> for ApiResult<T> {
    fn from(result: Result<T, AppError>) -> Self {
        match result {
            Ok(data) => Self::Success { ok: true, data },
            Err(error) => {
                let trace_id = uuid::Uuid::new_v4().to_string();
                tracing::error!(%trace_id, %error, "Local service request failed");
                let (code, message) = match error {
                    AppError::Validation(message) => ("INVALID_INPUT", message),
                    AppError::RevisionConflict => {
                        ("REVISION_CONFLICT", "交易已被修改或作废，请刷新后重试。")
                    }
                    AppError::RequestConflict => {
                        ("REQUEST_CONFLICT", "请求标识已用于不同内容，请重新提交。")
                    }
                    AppError::SchemaTooNew => {
                        ("SCHEMA_TOO_NEW", "数据库版本高于当前应用，请升级应用。")
                    }
                    AppError::WorkerUnavailable => {
                        ("DB_UNAVAILABLE", "数据库服务暂时不可用，请重新启动应用。")
                    }
                    _ => ("DB_ERROR", "无法读取本地数据库，请检查磁盘和访问权限。"),
                };
                Self::Failure {
                    ok: false,
                    error: ErrorDto {
                        code,
                        message,
                        trace_id,
                    },
                }
            }
        }
    }
}
