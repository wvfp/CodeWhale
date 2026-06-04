//! `novel_consistency_check` tool — run a consistency audit on a chapter.
//!
//! The tool loads `.novelwhale/facts.json` (the structured fact store) and the
//! chapter body (either from the workspace `chapters/` directory or a direct
//! content payload) and returns a [`ConsistencyReport`] serialized to JSON.

use std::path::PathBuf;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::consistency::{ConsistencyChecker, FactStore, report_to_json};
use crate::tools::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    readonly_capabilities,
};

pub struct NovelConsistencyCheckTool;

#[async_trait]
impl ToolSpec for NovelConsistencyCheckTool {
    fn name(&self) -> &'static str {
        "novel_consistency_check"
    }

    fn description(&self) -> &'static str {
        "对单章正文做一致性审计：检查世界规则、已退场角色、量词重复、角色名歧义等。输入 chapter_number + content（或 chapter_path），输出 0-100 分 + 问题清单。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "chapter_number": {
                    "type": "integer",
                    "description": "章节号（必填）。",
                    "minimum": 1
                },
                "content": {
                    "type": "string",
                    "description": "章节正文。如果同时提供了 chapter_path，会覆盖文件内容。"
                },
                "chapter_path": {
                    "type": "string",
                    "description": "相对工作区的章节文件路径（如 'chapters/003.md'）。"
                }
            },
            "required": ["chapter_number"]
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
        let chapter_number = input
            .get("chapter_number")
            .and_then(Value::as_u64)
            .ok_or_else(|| ToolError::missing_field("chapter_number"))?
            as u32;

        // 解析章节正文：优先 content，其次 chapter_path。
        let content: String = if let Some(c) = input.get("content").and_then(Value::as_str) {
            c.to_string()
        } else if let Some(rel) = input.get("chapter_path").and_then(Value::as_str) {
            let path = resolve_chapter_path(&context.workspace, rel);
            std::fs::read_to_string(&path).map_err(|e| {
                ToolError::execution_failed(format!(
                    "无法读取章节文件 {}：{}",
                    path.display(),
                    e
                ))
            })?
        } else {
            return Err(ToolError::missing_field("content"));
        };

        let store = FactStore::load(&context.workspace).map_err(|e| {
            ToolError::execution_failed(format!("加载事实库失败：{}", e))
        })?;

        let checker = ConsistencyChecker::new();
        let report = checker.check(chapter_number, &content, &store);
        let json = report_to_json(&report);
        let pretty = serde_json::to_string_pretty(&json)
            .map_err(|e| ToolError::execution_failed(e.to_string()))?;
        Ok(ToolResult::success(pretty))
    }
}

fn resolve_chapter_path(workspace: &std::path::Path, rel: &str) -> PathBuf {
    let p = workspace.join(rel);
    if p.exists() {
        return p;
    }
    // 兼容「仅给章节号」的情况
    let num = rel.trim_start_matches("chapters/").trim_end_matches(".md");
    let candidate = workspace.join("chapters").join(format!("{:03}.md", num.parse::<u32>().unwrap_or(0)));
    if candidate.exists() {
        return candidate;
    }
    p
}

impl crate::tools::NovelTool for NovelConsistencyCheckTool {}
