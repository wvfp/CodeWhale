//! RAG 子模块共用的错误 / 结果类型。

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RagError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("retriever error: {0}")]
    Retriever(String),
}

pub type RagResult<T> = std::result::Result<T, RagError>;
