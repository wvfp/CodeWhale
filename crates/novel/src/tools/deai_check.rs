//! `novel_deai_check` tool — 对单章正文做去 AI 化审计。
//!
//! 输出：
//! - 0-100 的「人类度」分数
//! - 套话命中清单
//! - 给 LLM 看的修改建议

use std::path::PathBuf;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::deai::{
    ClicheDictionary, build_suggestions, score_chapter,
};
use crate::tools::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    readonly_capabilities,
};

pub struct NovelDeaiCheckTool;

#[async_trait]
impl ToolSpec for NovelDeaiCheckTool {
    fn name(&self) -> &'static str {
        "novel_deai_check"
    }

    fn description(&self) -> &'static str {
        "对单章正文做去 AI 化审计：扫描套话（抽象描写、转折、公式化对话、副词滥用等），输出人类度分数和给 LLM 看的修改建议。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "章节正文。"
                },
                "chapter_path": {
                    "type": "string",
                    "description": "可选：相对工作区的章节文件路径。"
                },
                "custom_dictionary": {
                    "type": "array",
                    "description": "可选：额外的套话条目 [{phrase, category, severity, reason, suggestion}]。",
                    "items": {
                        "type": "object"
                    }
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
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let content: String = if let Some(c) = input.get("content").and_then(Value::as_str) {
            c.to_string()
        } else if let Some(rel) = input.get("chapter_path").and_then(Value::as_str) {
            let path = resolve_chapter_path(&context.workspace, rel);
            std::fs::read_to_string(&path).map_err(|e| {
                ToolError::execution_failed(format!("无法读取章节文件 {}：{}", path.display(), e))
            })?
        } else {
            return Err(ToolError::missing_field("content"));
        };

        let mut dict = ClicheDictionary::builtin();
        if let Some(extra) = input.get("custom_dictionary").and_then(Value::as_array) {
            for item in extra {
                if let Ok(entry) = serde_json::from_value::<crate::deai::ClicheEntry>(item.clone()) {
                    dict.push(entry);
                }
            }
        }

        let score = score_chapter(&content, &dict);
        let suggestions = build_suggestions(&score.hits);
        let result = json!({
            "humanity": score.humanity,
            "distinct_hits": score.distinct_hits,
            "total_occurrences": score.total_occurrences,
            "by_category": score
                .by_category
                .iter()
                .map(|(c, n)| json!({"category": c.label(), "count": n}))
                .collect::<Vec<_>>(),
            "hits": score
                .hits
                .iter()
                .map(|h| json!({
                    "phrase": h.entry.phrase,
                    "category": h.entry.category.label(),
                    "severity": h.entry.severity,
                    "count": h.count,
                    "first_line": h.first_line,
                    "reason": h.entry.reason,
                }))
                .collect::<Vec<_>>(),
            "suggestions": suggestions
                .iter()
                .map(|s| json!({
                    "phrase": s.phrase,
                    "count": s.count,
                    "first_line": s.first_line,
                    "severity": s.severity,
                    "instruction": s.instruction,
                    "reason": s.reason,
                }))
                .collect::<Vec<_>>(),
        });
        let pretty = serde_json::to_string_pretty(&result)
            .map_err(|e| ToolError::execution_failed(e.to_string()))?;
        Ok(ToolResult::success(pretty))
    }
}

fn resolve_chapter_path(workspace: &std::path::Path, rel: &str) -> PathBuf {
    let p = workspace.join(rel);
    if p.exists() {
        return p;
    }
    let num = rel.trim_start_matches("chapters/").trim_end_matches(".md");
    let candidate = workspace
        .join("chapters")
        .join(format!("{:03}.md", num.parse::<u32>().unwrap_or(0)));
    if candidate.exists() {
        return candidate;
    }
    p
}

impl crate::tools::NovelTool for NovelDeaiCheckTool {}
