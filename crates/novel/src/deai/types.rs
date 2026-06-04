//! 错误 / 结果类型。

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DeAiError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

pub type DeAiResult<T> = std::result::Result<T, DeAiError>;
