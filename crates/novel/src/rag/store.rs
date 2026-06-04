//! RAG 索引的持久化层。
//!
//! 索引把若干段文本（每段对应一条世界设定 / 人物卡 / 事件摘要 / 章节细纲）
//! 持久化到 `.novelwhale/rag-index.json`。当 embedding 不可用时，关键词回退
//! 实现就直接消费这里面的 token 频率。

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::types::RagResult;

/// 默认的索引文件位置。
pub const RAG_INDEX_PATH: &str = ".novelwhale/rag-index.json";

/// 一条可被检索的文档 / 段落。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RagDocument {
    /// 文档唯一 id（通常是 `<source>:<key>`）。
    pub id: String,
    /// 文档分类：world_rule / character / event / synopsis / setting。
    pub kind: String,
    /// 短标题，便于人读。
    pub title: String,
    /// 主体内容。检索时既匹配这里的词频，也作为召回后的原样返回。
    pub content: String,
    /// 来源章节号（如果是从章节中提取的）。
    #[serde(default)]
    pub source_chapter: Option<u32>,
    /// 是否不可变（写章节时这些信息不可被覆盖）。
    #[serde(default)]
    pub immutable: bool,
    /// 写入 / 更新时间（RFC3339）。
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// 索引元数据。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RagMeta {
    /// 索引后端：keyword / embedding。
    #[serde(default = "default_backend")]
    pub backend: String,
    /// 上次重建时间。
    #[serde(default)]
    pub last_rebuilt_at: Option<String>,
    /// 文档总数。
    #[serde(default)]
    pub doc_count: usize,
}

fn default_backend() -> String {
    "keyword".to_string()
}

/// RAG 索引文件 = 元数据 + 文档列表。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RagIndex {
    #[serde(default)]
    pub meta: RagMeta,
    #[serde(default)]
    pub documents: Vec<RagDocument>,
}

impl RagIndex {
    /// 从项目根目录加载索引。文件不存在时返回空索引。
    pub fn load(project_root: &Path) -> RagResult<Self> {
        let path = project_root.join(RAG_INDEX_PATH);
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(&path)?;
        let idx: Self = serde_json::from_str(&raw).unwrap_or_default();
        Ok(idx)
    }

    /// 把索引写回磁盘。
    pub fn save(&self, project_root: &Path) -> RagResult<()> {
        let path = project_root.join(RAG_INDEX_PATH);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let pretty = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, pretty)?;
        Ok(())
    }

    /// 添加 / 覆盖一个文档（按 id 去重）。
    pub fn upsert(&mut self, doc: RagDocument) {
        if let Some(existing) = self.documents.iter_mut().find(|d| d.id == doc.id) {
            *existing = doc;
        } else {
            self.documents.push(doc);
        }
        self.meta.doc_count = self.documents.len();
    }

    /// 按 id 删除文档。
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.documents.len();
        self.documents.retain(|d| d.id != id);
        let removed = self.documents.len() != before;
        if removed {
            self.meta.doc_count = self.documents.len();
        }
        removed
    }

    /// 清空索引。
    pub fn clear(&mut self) {
        self.documents.clear();
        self.meta.doc_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_doc(id: &str) -> RagDocument {
        RagDocument {
            id: id.to_string(),
            kind: "world_rule".into(),
            title: "样例".into(),
            content: "世界上没有魔法".into(),
            source_chapter: Some(1),
            immutable: true,
            updated_at: None,
        }
    }

    #[test]
    fn upsert_replaces_existing() {
        let mut idx = RagIndex::default();
        idx.upsert(sample_doc("rule:no_magic"));
        idx.upsert(sample_doc("rule:no_magic"));
        assert_eq!(idx.documents.len(), 1);
    }

    #[test]
    fn remove_returns_true_when_present() {
        let mut idx = RagIndex::default();
        idx.upsert(sample_doc("a"));
        idx.upsert(sample_doc("b"));
        assert!(idx.remove("a"));
        assert_eq!(idx.documents.len(), 1);
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = std::env::temp_dir().join("novelwhale_rag_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut idx = RagIndex::default();
        idx.upsert(sample_doc("x"));
        idx.save(&dir).unwrap();
        let loaded = RagIndex::load(&dir).unwrap();
        assert_eq!(loaded.documents.len(), 1);
        assert_eq!(loaded.documents[0].id, "x");
    }
}
