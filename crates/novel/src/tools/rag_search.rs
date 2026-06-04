//! `novel_rag_search` tool — 在事实库 / 人物卡 / 章节摘要中检索相关设定。
//!
//! 模型在写章节前调用本工具，把与当前场景最相关的设定塞进上下文。

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::consistency::FactStore;
use crate::rag::{
    KeywordRetriever, RagHit, RagIndexer, RetrievalQuery, Retriever, RetrieverError,
    keyword_search,
};
use crate::synopsis::store::SynopsisStore;
use crate::tools::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    readonly_capabilities,
};

pub struct NovelRagSearchTool;

#[async_trait]
impl ToolSpec for NovelRagSearchTool {
    fn name(&self) -> &'static str {
        "novel_rag_search"
    }

    fn description(&self) -> &'static str {
        "在事实库 / 人物卡 / 章节摘要中检索相关设定，返回与查询最相关的 top-k 条。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "检索词（场景描述、章节细纲、问题等）。"
                },
                "top_k": {
                    "type": "integer",
                    "description": "返回的最大条数，默认 5。",
                    "minimum": 1,
                    "maximum": 20
                },
                "kinds": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "可选过滤：world_rule / character / synopsis / setting。"
                },
                "rebuild_index": {
                    "type": "boolean",
                    "description": "是否在检索前重建索引。默认 false。"
                }
            },
            "required": ["query"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        readonly_capabilities()
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(
        &self,
        input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let query_text = input
            .get("query")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::missing_field("query"))?;
        let top_k = input.get("top_k").and_then(Value::as_u64).unwrap_or(5) as usize;
        let rebuild = input
            .get("rebuild_index")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let kinds: Option<Vec<String>> = input.get("kinds").and_then(|v| v.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        });

        // 重建索引（如需）
        if rebuild {
            if let Err(e) = rebuild_index(&context.workspace) {
                return Err(ToolError::execution_failed(format!(
                    "重建 RAG 索引失败：{}",
                    e
                )));
            }
        }

        let query = RetrievalQuery {
            text: query_text.to_string(),
            top_k,
            min_score: 0.0,
            kinds,
        };
        let hits = match keyword_search(&context.workspace, &query) {
            Ok(h) => h,
            Err(RetrieverError::NoIndex) => {
                // 索引缺失：尝试自动建一次再查
                if rebuild_index(&context.workspace).is_err() {
                    return Ok(ToolResult::success(
                        serde_json::to_string_pretty(&json!({"hits": [], "note": "索引为空"}))
                            .unwrap(),
                    ));
                }
                KeywordRetriever::from_project(&context.workspace)
                    .and_then(|r| r.retrieve(&query))
                    .unwrap_or_default()
            }
            Err(e) => {
                return Err(ToolError::execution_failed(format!("检索失败：{}", e)));
            }
        };

        let json_hits: Vec<Value> = hits
            .iter()
            .map(|h: &RagHit| {
                json!({
                    "id": h.doc.id,
                    "kind": h.doc.kind,
                    "title": h.doc.title,
                    "score": h.score,
                    "highlights": h.highlights,
                    "content": h.doc.content,
                    "source_chapter": h.doc.source_chapter,
                })
            })
            .collect();
        let result = json!({
            "query": query_text,
            "top_k": top_k,
            "hits": json_hits,
        });
        let pretty = serde_json::to_string_pretty(&result)
            .map_err(|e| ToolError::execution_failed(e.to_string()))?;
        Ok(ToolResult::success(pretty))
    }
}

fn rebuild_index(project_root: &std::path::Path) -> Result<(), String> {
    let mut indexer = RagIndexer::load_existing(project_root)
        .map_err(|e| format!("加载索引失败：{}", e))?;
    if let Ok(store) = FactStore::load(project_root) {
        indexer.ingest_facts(&store);
    }
    if let Ok(summaries) = SynopsisStore::load_all(project_root) {
        indexer.ingest_summaries(&summaries);
    }
    indexer
        .finalize(project_root)
        .map_err(|e| format!("保存索引失败：{}", e))?;
    Ok(())
}

impl crate::tools::NovelTool for NovelRagSearchTool {}
