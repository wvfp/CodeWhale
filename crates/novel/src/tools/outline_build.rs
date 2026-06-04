//! `novel_outline_build` tool — build a narrative outline.

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::model::graph::NarrativeNode;
use crate::model::stage::CreationStage;
use crate::tools::{
    ApprovalRequirement, NovelToolError, ToolCapability, ToolContext, ToolError, ToolResult,
    ToolSpec, optional_u64, required_str, write_capabilities,
};

pub struct NovelOutlineBuildTool;

#[async_trait]
impl ToolSpec for NovelOutlineBuildTool {
    fn name(&self) -> &'static str {
        "novel_outline_build"
    }

    fn description(&self) -> &'static str {
        "为一个故事弧线搭建叙事大纲。生成一个节点树（钩子 / 铺垫 / 冲突 / 高潮 / 转折 / 兑现 / 交代 / 收束），并把大纲写入 outline/arcs/<arc_name>.md。返回生成树的 JSON 结构。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "arc_name": {
                    "type": "string",
                    "description": "故事弧线的名称（如 arc_01_开篇）"
                },
                "target_chapter_count": {
                    "type": "integer",
                    "description": "本弧线的章节数（默认 10）"
                },
                "summary": {
                    "type": "string",
                    "description": "弧线剧情的简要描述（用于在 LLM 驱动的大纲生成中作为种子）"
                }
            },
            "required": ["arc_name"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        write_capabilities()
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Suggest
    }

    async fn execute(
        &self,
        input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let arc_name = required_str(&input, "arc_name")?;
        let chapter_count = optional_u64(&input, "target_chapter_count", 10) as u32;
        let summary = input
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if arc_name.is_empty() {
            return Err(NovelToolError::InvalidInput("arc_name 为空".into()).into());
        }
        if chapter_count == 0 || chapter_count > 1000 {
            return Err(NovelToolError::InvalidInput(format!(
                "target_chapter_count 越界：{chapter_count}"
            ))
            .into());
        }

        // Build a balanced narrative skeleton: Hook -> Setup -> Conflict -> Climax -> Resolution.
        // This is a deterministic template; in production this would invoke the LLM
        // with the arc `summary` to produce tailored node content.
        let nodes = build_skeleton(chapter_count, summary);
        let arc_path = context
            .workspace
            .join("outline")
            .join("arcs")
            .join(format!("{arc_name}.md"));
        if let Some(parent) = arc_path.parent() {
            std::fs::create_dir_all(parent).map_err(NovelToolError::from)?;
        }
        let rendered = render_outline_markdown(arc_name, summary, &nodes);
        std::fs::write(&arc_path, rendered).map_err(NovelToolError::from)?;

        let tree = nodes
            .iter()
            .map(|n| {
                json!({
                    "id": n.id,
                    "title": n.title,
                    "node_type": n.node_type,
                    "emotional_valence": n.emotional_valence,
                    "tension": n.tension,
                })
            })
            .collect::<Vec<_>>();

        let payload = json!({
            "arc_name": arc_name,
            "arc_path": arc_path.display().to_string(),
            "chapter_count": chapter_count,
            "nodes": tree,
            "current_stage": CreationStage::Outline,
        });

        let result_json = serde_json::to_string(&payload)
            .map_err(|e| ToolError::execution_failed(e.to_string()))?;
        Ok(ToolResult::success(result_json))
    }
}

fn build_skeleton(chapter_count: u32, _summary: &str) -> Vec<NarrativeNode> {
    // The narrative skeleton alternates setup, conflict, and payoff with a
    // hook on chapter 1 and a climax near the end. This is intentionally
    // rule-based for the MVP — the LLM-driven generator is a later phase.
    let mut nodes = Vec::with_capacity(chapter_count as usize);
    for i in 0..chapter_count {
        let (node_type, title, valence, tension) = classify_chapter(i, chapter_count);
        nodes.push(NarrativeNode::new(
            uuid::Uuid::new_v4(),
            title.to_string(),
            node_type,
            None,
            valence,
            tension,
        ));
    }
    nodes
}

fn classify_chapter(
    i: u32,
    total: u32,
) -> (crate::model::chapter::NarrativeNodeType, &'static str, f32, f32) {
    use crate::model::chapter::NarrativeNodeType::*;
    if i == 0 {
        return (Hook, "钩子 Hook", 0.6, 0.7);
    }
    if i + 1 == total {
        return (Climax, "高潮 Climax", 0.9, 1.0);
    }
    if i + 1 == total - 1 {
        return (Twist, "转折 Twist", 0.3, 0.9);
    }
    let phase = i % 5;
    match phase {
        0 => (Setup, "铺垫 Setup", 0.2, 0.4),
        1 => (Setup, "铺垫 Setup", 0.3, 0.5),
        2 => (Conflict, "冲突 Conflict", -0.2, 0.7),
        3 => (Conflict, "冲突 Conflict", 0.1, 0.8),
        4 => (Payoff, "爽点 Payoff", 0.8, 0.5),
        _ => (Setup, "铺垫 Setup", 0.2, 0.4),
    }
}

fn render_outline_markdown(
    arc_name: &str,
    summary: &str,
    nodes: &[NarrativeNode],
) -> String {
    let mut out = String::new();
    out.push_str(&format!("# 大纲: {arc_name}\n\n"));
    if !summary.is_empty() {
        out.push_str(&format!("> 剧情概要：{summary}\n\n"));
    }
    out.push_str("## 章节节点\n\n");
    for (i, n) in nodes.iter().enumerate() {
        out.push_str(&format!(
            "{}. **[{:?}]** {} (情绪: {:.1}, 张力: {:.1})\n",
            i + 1,
            n.node_type,
            n.title,
            n.emotional_valence,
            n.tension,
        ));
    }
    out
}

impl crate::tools::NovelTool for NovelOutlineBuildTool {}
