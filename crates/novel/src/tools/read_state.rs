//! `novel_read_state` tool — read the current novel session state.

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::model::stage::CreationStage;
use crate::tools::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    readonly_capabilities,
};

pub struct NovelReadStateTool;

#[async_trait]
impl ToolSpec for NovelReadStateTool {
    fn name(&self) -> &'static str {
        "novel_read_state"
    }

    fn description(&self) -> &'static str {
        "读取当前小说项目的状态：标题、题材、当前阶段、事实库摘要、已写章节数。用于在每轮开始时把模型校准回项目上下文。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {}
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
        _input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let root = &context.workspace;
        let config_path = root.join(".novelwhale").join("novel.toml");
        let state_path = root.join(".novelwhale").join("project-state.json");
        let facts_path = root.join(".novelwhale").join("facts.json");

        let title = if config_path.exists() {
            std::fs::read_to_string(&config_path)
                .ok()
                .and_then(|s| {
                    s.lines()
                        .find(|l| l.starts_with("title"))
                        .and_then(|l| l.split('=').nth(1))
                        .map(|v| v.trim().trim_matches('"').to_string())
                })
                .unwrap_or_default()
        } else {
            String::new()
        };

        let state_json: Value = if state_path.exists() {
            std::fs::read_to_string(&state_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_else(|| json!({}))
        } else {
            json!({})
        };

        let facts: Value = if facts_path.exists() {
            std::fs::read_to_string(&facts_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_else(|| json!({}))
        } else {
            json!({})
        };

        let rule_count = facts
            .get("world_rules")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let char_count = facts
            .get("characters")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let event_count = facts
            .get("events")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);

        let chapters_dir = root.join("chapters");
        let chapter_files = if chapters_dir.exists() {
            std::fs::read_dir(&chapters_dir)
                .map(|rd| {
                    rd.filter_map(|e| e.ok())
                        .filter(|e| {
                            e.path()
                                .extension()
                                .and_then(|x| x.to_str())
                                .map(|x| x == "md")
                                .unwrap_or(false)
                        })
                        .count()
                })
                .unwrap_or(0)
        } else {
            0
        };

        let result = json!({
            "title": title,
            "current_stage": CreationStage::Concept,
            "project_state": state_json,
            "facts_summary": {
                "world_rules": rule_count,
                "characters": char_count,
                "events": event_count,
            },
            "chapters_written": chapter_files,
        });
        let result_json = serde_json::to_string(&result)
            .map_err(|e| ToolError::execution_failed(e.to_string()))?;
        Ok(ToolResult::success(result_json))
    }
}

impl crate::tools::NovelTool for NovelReadStateTool {}
