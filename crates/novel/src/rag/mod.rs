//! 设定 / 知识的检索增强生成（RAG）模块。
//!
//! 中文网络小说的「设定」分散在事实库、世界规则、人物卡等地方。当模型在写
//! 新章节时，需要把「跟当前场景最相关」的一小撮设定塞进上下文。这里就是
//! 这一小撮设定的挑选器。
//!
//! MVP 提供一个不依赖任何外部 embedding 服务的「关键词回退」实现（始终可用），
//! 同时通过 [`Retriever`] trait 暴露接口，后续可以替换成向量召回实现（例如
//! 接入一个本地模型或在线 embedding 服务）。

pub mod indexer;
pub mod retriever;
pub mod store;
pub mod types;

pub use indexer::RagIndexer;
pub use retriever::{
    KeywordRetriever, RagHit, RetrievalBackend, RetrievalQuery, Retriever, RetrieverError,
    keyword_search,
};
pub use store::{RagDocument, RagIndex, RagMeta, RAG_INDEX_PATH};
pub use types::{RagError, RagResult};
