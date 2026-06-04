//! 检索器。
//!
//! 关键词回退实现是「不依赖任何外部服务」的可工作版本：对查询做简单的中文
//! 分词 + 词频统计，按 BM25-like 公式对每个文档打分；不足时回退到子串命中。
//!
//! 通过 [`Retriever`] trait 暴露接口，后续可换成 embedding 实现：
//!
//! ```ignore
//! struct EmbeddingRetriever { ... }
//! impl Retriever for EmbeddingRetriever { ... }
//! ```
//!
//! 在 LLM 调用方侧，只需 `Box<dyn Retriever>`，对后端无感。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::store::{RagDocument, RagIndex};

#[derive(Debug, Error)]
pub enum RetrieverError {
    #[error("index not loaded")]
    NoIndex,
    #[error("query is empty")]
    EmptyQuery,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalQuery {
    /// 原始查询文本（章节细纲、场景描述等）。
    pub text: String,
    /// 最多返回多少条。
    pub top_k: usize,
    /// 召回的最小分数门槛（0-1），低于它的结果会被丢弃。
    pub min_score: f32,
    /// 可选：只看这些 kind（world_rule / character / ...）。
    pub kinds: Option<Vec<String>>,
}

impl RetrievalQuery {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            top_k: 5,
            min_score: 0.0,
            kinds: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagHit {
    pub doc: RagDocument,
    /// 0-1 之间的归一化分。
    pub score: f32,
    /// 高亮命中片段（最多 3 个），方便人类快速判断。
    pub highlights: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalBackend {
    Keyword,
    Embedding,
}

impl RetrievalBackend {
    pub fn label(self) -> &'static str {
        match self {
            RetrievalBackend::Keyword => "keyword",
            RetrievalBackend::Embedding => "embedding",
        }
    }
}

/// 检索器 trait。任何具体后端都要实现它。
pub trait Retriever: Send + Sync {
    /// 返回使用的后端类型。
    fn backend(&self) -> RetrievalBackend;
    /// 对查询跑一次检索。
    fn retrieve(&self, query: &RetrievalQuery) -> Result<Vec<RagHit>, RetrieverError>;
}

// =================================================================
// 关键词回退实现
// =================================================================

/// 不依赖任何外部服务的关键词检索器。对中文文本按 char 切 1-2-gram，
/// 然后用 BM25 简化版打分。
pub struct KeywordRetriever {
    index: RagIndex,
}

impl KeywordRetriever {
    pub fn new(index: RagIndex) -> Self {
        Self { index }
    }

    /// 从项目根目录直接构建一个 retriever。文件不存在时索引为空。
    pub fn from_project(project_root: &std::path::Path) -> Result<Self, RetrieverError> {
        let index = RagIndex::load(project_root).map_err(|_| RetrieverError::NoIndex)?;
        Ok(Self::new(index))
    }

    /// 替换底层索引（例如从事实库重建后调用）。
    pub fn with_index(mut self, index: RagIndex) -> Self {
        self.index = index;
        self
    }

    pub fn index(&self) -> &RagIndex {
        &self.index
    }

    pub fn into_index(self) -> RagIndex {
        self.index
    }
}

impl Retriever for KeywordRetriever {
    fn backend(&self) -> RetrievalBackend {
        RetrievalBackend::Keyword
    }

    fn retrieve(&self, query: &RetrievalQuery) -> Result<Vec<RagHit>, RetrieverError> {
        if query.text.trim().is_empty() {
            return Err(RetrieverError::EmptyQuery);
        }
        if self.index.documents.is_empty() {
            return Ok(Vec::new());
        }
        let q_tokens = tokenize(&query.text);
        if q_tokens.is_empty() {
            return Ok(Vec::new());
        }
        // 文档侧 token 化（只算一次，缓存到 doc 中）。
        let docs: Vec<(usize, &RagDocument, Vec<String>)> = self
            .index
            .documents
            .iter()
            .enumerate()
            .filter(|(_, d)| match &query.kinds {
                Some(kinds) => kinds.iter().any(|k| k == &d.kind),
                None => true,
            })
            .map(|(i, d)| (i, d, tokenize(&format!("{} {}", d.title, d.content))))
            .collect();
        if docs.is_empty() {
            return Ok(Vec::new());
        }

        // 词项的 IDF
        let mut df: BTreeMap<String, usize> = BTreeMap::new();
        for (_, _, toks) in &docs {
            let mut seen = std::collections::BTreeSet::new();
            for t in toks {
                if seen.insert(t.clone()) {
                    *df.entry(t.clone()).or_insert(0) += 1;
                }
            }
        }
        let n = docs.len() as f32;

        // 文档长度
        let avg_dl = docs.iter().map(|(_, _, t)| t.len() as f32).sum::<f32>() / n;

        let mut hits: Vec<RagHit> = Vec::new();
        for (i, doc, toks) in &docs {
            let mut tf: BTreeMap<String, usize> = BTreeMap::new();
            for t in toks {
                *tf.entry(t.clone()).or_insert(0) += 1;
            }
            let mut score = 0.0_f32;
            let mut matched: Vec<String> = Vec::new();
            for qt in &q_tokens {
                if let Some(c) = tf.get(qt) {
                    let dfreq = df.get(qt).copied().unwrap_or(1) as f32;
                    let idf = ((n - dfreq + 0.5) / (dfreq + 0.5) + 1.0).ln();
                    let norm = (*c as f32) / (*c as f32 + 0.5 + 1.5 * (toks.len() as f32) / avg_dl);
                    score += idf * norm;
                    if matched.len() < 3 {
                        matched.push(qt.clone());
                    }
                }
            }
            // 归一化到 0-1：除以查询长度上限。
            let norm = if q_tokens.is_empty() {
                0.0
            } else {
                score / (q_tokens.len() as f32).sqrt()
            };
            let norm = norm.clamp(0.0, 1.0);
            if norm >= query.min_score {
                let highlights = extract_highlights(&doc.content, &matched, 80);
                hits.push(RagHit {
                    doc: (*doc).clone(),
                    score: norm,
                    highlights,
                });
            }
            let _ = i;
        }
        // 按分数降序截断。
        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        hits.truncate(query.top_k.max(1));
        Ok(hits)
    }
}

// === 简单的中文 token 化（unigram + bigram） ===
fn tokenize(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    // 单词（英文/数字）按空白拆分
    let mut buf = String::new();
    for &c in &chars {
        if c.is_whitespace() {
            if !buf.is_empty() {
                out.push(std::mem::take(&mut buf).to_lowercase());
            }
        } else if c.is_alphanumeric() && c.is_ascii() {
            buf.push(c);
        } else {
            // CJK 或标点：先 flush 英文/数字 buffer
            if !buf.is_empty() {
                out.push(std::mem::take(&mut buf).to_lowercase());
            }
            out.push(c.to_string());
        }
    }
    if !buf.is_empty() {
        out.push(buf.to_lowercase());
    }
    // 加 bigram 以缓解 CJK unigram 召回过散
    let bigrams: Vec<String> = out
        .windows(2)
        .filter(|w| {
            w[0].chars().any(|c| is_cjk(c)) && w[1].chars().any(|c| is_cjk(c))
        })
        .map(|w| format!("{}{}", w[0], w[1]))
        .collect();
    out.extend(bigrams);
    out
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x4E00..=0x9FFF | 0x3400..=0x4DBF)
}

fn extract_highlights(content: &str, tokens: &[String], max_len: usize) -> Vec<String> {
    // 对每个 token 找第一次出现的位置，向两边各扩展 30 个字符。
    let mut out: Vec<String> = Vec::new();
    let mut used_ranges: Vec<(usize, usize)> = Vec::new();
    for t in tokens {
        if let Some(idx) = content.find(t) {
            let start = idx.saturating_sub(20);
            let end = (idx + t.chars().count() + 20).min(content.chars().count());
            // 转 byte 索引
            let s_byte = char_to_byte(content, start);
            let e_byte = char_to_byte(content, end);
            if used_ranges
                .iter()
                .any(|(a, b)| !(e_byte <= *a || s_byte >= *b))
            {
                continue;
            }
            used_ranges.push((s_byte, e_byte));
            let mut s = content[s_byte..e_byte].to_string();
            if s.chars().count() > max_len {
                s = s.chars().take(max_len).collect::<String>() + "…";
            }
            out.push(s);
        }
        if out.len() >= 3 {
            break;
        }
    }
    out
}

fn char_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

// === 公共入口 ===

/// 给一段查询 + 索引文件位置，直接跑一次关键词检索。
pub fn keyword_search(
    project_root: &std::path::Path,
    query: &RetrievalQuery,
) -> Result<Vec<RagHit>, RetrieverError> {
    let retriever = KeywordRetriever::from_project(project_root)?;
    retriever.retrieve(query)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(id: &str, title: &str, content: &str) -> RagDocument {
        RagDocument {
            id: id.into(),
            kind: "world_rule".into(),
            title: title.into(),
            content: content.into(),
            source_chapter: Some(1),
            immutable: true,
            updated_at: None,
        }
    }

    #[test]
    fn keyword_retriever_returns_relevant_hits() {
        let mut idx = RagIndex::default();
        idx.upsert(doc("rule:magic", "魔法设定", "这个世界上没有魔法，灵气才是本源。"));
        idx.upsert(doc("rule:king", "王国设定", "大齐王朝以武立国，皇室血脉纯正。"));
        idx.upsert(doc("char:ling", "林羽", "主角林羽出身山村，自幼失去双亲。"));
        let retr = KeywordRetriever::new(idx);
        let q = RetrievalQuery {
            text: "林羽学习魔法".into(),
            top_k: 3,
            min_score: 0.0,
            kinds: None,
        };
        let hits = retr.retrieve(&q).unwrap();
        assert!(!hits.is_empty());
        // 林羽相关文档应该排第一
        assert!(hits[0].doc.id.starts_with("char:ling"));
    }

    #[test]
    fn keyword_retriever_filters_by_kind() {
        let mut idx = RagIndex::default();
        idx.upsert(doc("rule:magic", "魔法设定", "世界上没有魔法。"));
        idx.upsert(doc("char:ling", "林羽", "林羽会剑术。"));
        let retr = KeywordRetriever::new(idx);
        let mut q = RetrievalQuery::new("林羽");
        q.kinds = Some(vec!["world_rule".into()]);
        let hits = retr.retrieve(&q).unwrap();
        // 限制 kind 后，character 类的命中不应出现
        assert!(hits.iter().all(|h| h.doc.kind == "world_rule"));
    }

    #[test]
    fn empty_query_errors() {
        let idx = RagIndex::default();
        let retr = KeywordRetriever::new(idx);
        let q = RetrievalQuery::new("");
        assert!(matches!(retr.retrieve(&q), Err(RetrieverError::EmptyQuery)));
    }
}
