//! `novel_synopsis_read` tool — read prior chapter synopses for context compression.

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::synopsis::SynopsisStore;
use crate::tools::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_u64, readonly_capabilities,
};

pub struct NovelSynopsisReadTool;

#[async_trait]
impl ToolSpec for NovelSynopsisReadTool {
    fn name(&self) -> &'static str {
        "novel_synopsis_read"
    }

    fn description(&self) -> &'static str {
        "读取之前章节的概要，用于上下文压缩。长项目里，构造下一章的提示时，应优先使用早期章节的概要而不是全文。最近的 `recent_chapters` 章可以按需回退到全文。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "up_to_chapter": {
                    "type": "integer",
                    "description": "返回章节号严格小于此值的概要（默认 0，即返回全部）"
                },
                "limit": {
                    "type": "integer",
                    "description": "返回概要的最大数量（默认 20，按最近排序）"
                },
                "include_pending_hooks": {
                    "type": "boolean",
                    "description": "是否包含 pending_hooks 字段（默认 true）"
                }
            }
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
        _context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let up_to = optional_u64(&input, "up_to_chapter", 0) as u32;
        let limit = optional_u64(&input, "limit", 20) as usize;
        let include_pending_hooks = input
            .get("include_pending_hooks")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // We intentionally read from the workspace-relative path even though
        // the context is not strictly required for read-only operations.
        let store = SynopsisStore::load_all(&_context.workspace).map_err(|e| {
            ToolError::execution_failed(format!("load synopses: {e}"))
        })?;

        let mut entries: Vec<Value> = Vec::new();
        for (chapter_number, synopsis) in store.synopses() {
            if up_to > 0 && chapter_number >= up_to {
                continue;
            }
            let mut entry = json!({
                "chapter_number": synopsis.chapter_number,
                "chapter_title": synopsis.chapter_title,
                "event_chain": synopsis.event_chain,
                "key_details": synopsis.key_details,
                "ending_state": synopsis.ending_state,
                "continuation_notes": synopsis.continuation_notes,
                "generated_at": synopsis.generated_at,
            });
            if include_pending_hooks {
                entry["pending_hooks"] = json!(synopsis.pending_hooks);
            }
            entries.push(entry);
        }

        // Sort by chapter number ascending then truncate to the most recent `limit`.
        entries.sort_by(|a, b| {
            a.get("chapter_number")
                .and_then(|v| v.as_u64())
                .cmp(&b.get("chapter_number").and_then(|v| v.as_u64()))
        });
        if entries.len() > limit {
            let drop = entries.len() - limit;
            entries.drain(0..drop);
        }

        let payload = json!({
            "count": entries.len(),
            "synopses": entries,
        });

        let result_json = serde_json::to_string(&payload)
            .map_err(|e| ToolError::execution_failed(e.to_string()))?;
        Ok(ToolResult::success(result_json))
    }
}

impl crate::tools::NovelTool for NovelSynopsisReadTool {}
