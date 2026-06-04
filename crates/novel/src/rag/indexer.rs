//! 从事实库 / 人物卡 / 章节摘要构建 RAG 索引。

use std::path::Path;

use crate::consistency::FactStore;
use crate::rag::store::{RagDocument, RagIndex, RagMeta};
use crate::rag::types::RagResult;
use crate::synopsis::store::SynopsisStore;

/// 从事实库、人物卡、章节摘要一次性重建 RAG 索引。
///
/// 这是一个「尽力而为」的索引器：事实库里的世界规则、人物、章节摘要都会被
/// 切成一个或多个文档塞进索引；旧条目按 id 覆盖。
pub struct RagIndexer {
    pub index: RagIndex,
}

impl RagIndexer {
    pub fn new() -> Self {
        Self {
            index: RagIndex::default(),
        }
    }

    /// 从项目根目录读入已存在的索引（如果存在）。
    pub fn load_existing(project_root: &Path) -> RagResult<Self> {
        let index = RagIndex::load(project_root)?;
        Ok(Self { index })
    }

    /// 灌入事实库：每条世界规则 / 人物 / 事件变成一条文档。
    pub fn ingest_facts(&mut self, store: &FactStore) {
        for r in &store.world_rules {
            self.index.upsert(RagDocument {
                id: format!("rule:{}", r.id),
                kind: "world_rule".into(),
                title: format!("[规则]{}", r.category),
                content: r.rule.clone(),
                source_chapter: r.source_chapter,
                immutable: r.immutable,
                updated_at: None,
            });
        }
        for ch in &store.characters {
            self.index.upsert(RagDocument {
                id: format!("character:{}", ch.name),
                kind: "character".into(),
                title: ch.name.clone(),
                content: format!(
                    "{}（{}）",
                    ch.name,
                    if ch.status.is_empty() {
                        "状态未知".to_string()
                    } else {
                        ch.status.clone()
                    }
                ),
                source_chapter: ch.chapter_introduced,
                immutable: !ch.alive,
                updated_at: None,
            });
        }
    }

    /// 灌入章节摘要：每章一个文档。
    pub fn ingest_summaries(&mut self, summaries: &SynopsisStore) {
        for (_n, s) in summaries.synopses() {
            let content = format!(
                "{}\n事件链：{}\n关键细节：{}\n续写要点：{}\n待回收伏笔：{}",
                s.chapter_title,
                s.event_chain.join(" → "),
                s.key_details.join("；"),
                s.continuation_notes.join("；"),
                s.pending_hooks.join("；"),
            );
            self.index.upsert(RagDocument {
                id: format!("synopsis:{}", s.chapter_number),
                kind: "synopsis".into(),
                title: format!("第 {} 章 {}", s.chapter_number, s.chapter_title),
                content,
                source_chapter: Some(s.chapter_number),
                immutable: false,
                updated_at: Some(s.generated_at.to_rfc3339()),
            });
        }
    }

    /// 完成灌入后写回磁盘。
    pub fn finalize(mut self, project_root: &Path) -> RagResult<RagIndex> {
        self.index.meta = RagMeta {
            backend: "keyword".into(),
            last_rebuilt_at: Some(now_iso8601()),
            doc_count: self.index.documents.len(),
        };
        self.index.save(project_root)?;
        Ok(self.index)
    }
}

impl Default for RagIndexer {
    fn default() -> Self {
        Self::new()
    }
}

fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 简化：只用秒级时间戳。LLM 看到也无所谓。
    format!("epoch:{}", secs)
}
