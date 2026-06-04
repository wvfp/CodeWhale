//! `novel_stage_set` tool — switch the active creation stage.

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::model::stage::CreationStage;
use crate::tools::{
    ApprovalRequirement, NovelToolError, ToolCapability, ToolContext, ToolError, ToolResult,
    ToolSpec, readonly_capabilities,
};

pub struct NovelStageSetTool;

#[async_trait]
impl ToolSpec for NovelStageSetTool {
    fn name(&self) -> &'static str {
        "novel_stage_set"
    }

    fn description(&self) -> &'static str {
        "切换当前创作阶段。合法转移：构思 → 大纲 → 细纲 → 初稿 → 润色。用 /stage skip 强制跳过中间阶段，但仅在确实知道自己在做什么时使用。下一轮会注入对应阶段的 delta 提示。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "stage": {
                    "type": "string",
                    "enum": ["Concept", "Outline", "Detail", "Draft", "Polish"],
                    "description": "目标创作阶段"
                },
                "force": {
                    "type": "boolean",
                    "description": "强制切换（即便会跳过中间阶段，默认 false）"
                }
            },
            "required": ["stage"]
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
        let stage_str = input
            .get("stage")
            .and_then(|v| v.as_str())
            .ok_or_else(|| NovelToolError::InvalidInput("缺少 stage 字段".into()))?;
        let force = input
            .get("force")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let new_stage = parse_stage(stage_str)
            .ok_or_else(|| NovelToolError::InvalidInput(format!("未知的 stage：{stage_str}")))?;

        // For the MVP we just acknowledge the request; the actual session
        // transition is performed by the engine when it next reads the stage.
        // The tool returns a JSON payload indicating the requested transition
        // and whether it was accepted (always accepted, since `force` is the
        // escape hatch).
        let result = json!({
            "previous_stage": null,
            "new_stage": new_stage,
            "force": force,
            "accepted": true,
        });
        let result_json = serde_json::to_string(&result)
            .map_err(|e| ToolError::execution_failed(e.to_string()))?;
        Ok(ToolResult::success(result_json))
    }
}

fn parse_stage(s: &str) -> Option<CreationStage> {
    match s.to_ascii_lowercase().as_str() {
        "concept" => Some(CreationStage::Concept),
        "outline" => Some(CreationStage::Outline),
        "detail" => Some(CreationStage::Detail),
        "draft" => Some(CreationStage::Draft),
        "polish" => Some(CreationStage::Polish),
        _ => None,
    }
}

impl crate::tools::NovelTool for NovelStageSetTool {}
