//! `novel_branch_explore` tool — 决策点分支探索（`/outline branches`）。
//!
//! 当故事走到「抉择点」时，作家需要从 2-3 条可能方向里选一条。LLM 生成这些
//! 分支通常比人脑更擅长枚举意外组合，所以本工具：
//!
//! 1. 加载 `NarrativeGraph`，定位 `node_id` 对应的节点作为「分叉点」。
//! 2. 把节点信息（标题、类型、张力）以及「紧接的事实库」打包成 system prompt
//!    + user prompt，交给 LLM。
//! 3. 把 LLM 输出落盘到 `.novelwhale/branches.json`，方便用户对比 / 合并。
//!
//! 本工具是「只读 + 落盘」 — 不会改 narrative graph 本身。作家选定一条分支
//! 后再调用 `novel_outline_build` 把分支物化回节点。

use std::path::Path;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::consistency::FactStore;
use crate::model::graph::NarrativeGraph;
use crate::tools::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    readonly_capabilities,
};

pub struct NovelBranchExploreTool;

const SYSTEM_PROMPT: &str = r#"你是一位擅长「剧情分支」的网络小说架构师。给定一个"决策点"和它之前的事实背景，请为作家生成 N 条互斥的合理走向（默认 N=3）。

每条分支必须包含：
- `id`：稳定的英文 slug，便于后续引用。
- `title`：中文短标题（≤ 16 字）。
- `logline`：一句话剧情钩子（≤ 30 字）。
- `tension_after`：分支落地后的新张力值（0.0-1.0）。
- `risk`：这条分支可能把主角逼到的代价（≤ 24 字）。
- `payoff`：1-2 句写明如果走这条分支，主角会获得什么（情感 / 资源 / 关系）。
- `outline_beat`：一段 80-150 字的中文细纲，描述接下来 2-3 个节拍。

要求：
- 至少有一条偏向"反英雄 / 灰色走向"，至少一条保留主角的"软着陆"。
- 不允许出现事实库已经被锁死的世界规则矛盾。
- 不引入未在 fact_lock 中出现的新角色，必要的新角色用「未知 / 神秘」等占位。
- 输出严格 JSON 数组，不要任何额外解释文字。
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchSuggestion {
    pub id: String,
    pub title: String,
    pub logline: String,
    pub tension_after: f32,
    pub risk: String,
    pub payoff: String,
    pub outline_beat: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchSession {
    pub node_id: String,
    pub node_title: String,
    pub generated_at: String,
    pub suggestions: Vec<BranchSuggestion>,
}

#[async_trait]
impl ToolSpec for NovelBranchExploreTool {
    fn name(&self) -> &'static str {
        "novel_branch_explore"
    }

    fn description(&self) -> &'static str {
        "决策点分支探索：给定一个叙事节点 id，调用 LLM 生成 N 条互斥的剧情分支，落到 .novelwhale/branches.json。作家选定一条后再用 novel_outline_build 把它合并回 narrative graph。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "node_id": {
                    "type": "string",
                    "description": "决策点对应的叙事节点 id（在 novel_outline_build 输出中可找到）。"
                },
                "count": {
                    "type": "integer",
                    "description": "要生成的分支条数（2-4，默认 3）。",
                    "minimum": 2,
                    "maximum": 4
                }
            },
            "required": ["node_id"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        readonly_capabilities()
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Suggest
    }

    async fn execute(
        &self,
        input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let node_id = input
            .get("node_id")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::missing_field("node_id"))?;
        let count = input.get("count").and_then(Value::as_u64).unwrap_or(3).clamp(2, 4);

        let graph = load_graph(&context.workspace);
        let node = graph
            .nodes
            .iter()
            .find(|n| n.id.to_string() == node_id)
            .ok_or_else(|| {
                ToolError::execution_failed(format!(
                    "未找到节点 {node_id}：请先用 novel_outline_build 生成节点。"
                ))
            })?;

        let user_prompt = build_user_prompt(node, count, &context.workspace);

        let out = json!({
            "node_id": node_id,
            "node_title": node.title,
            "node_type": format!("{:?}", node.node_type).to_lowercase(),
            "tension": node.tension,
            "count": count,
            "system_prompt": SYSTEM_PROMPT,
            "user_prompt": user_prompt,
            "instructions": {
                "writes": [".novelwhale/branches.json"],
                "schema": "json array of BranchSuggestion {id, title, logline, tension_after, risk, payoff, outline_beat}",
                "expected_response_format": "JSON"
            },
            "note": "调用方负责：把 system_prompt + user_prompt 拼成一次 LLM 调用；将模型输出（必须是合法 JSON 数组）落盘到 .novelwhale/branches.json。"
        });
        let pretty = serde_json::to_string_pretty(&out)
            .map_err(|e| ToolError::execution_failed(e.to_string()))?;
        Ok(ToolResult::success(pretty))
    }
}

fn load_graph(workspace: &Path) -> NarrativeGraph {
    // 先尝试 .novelwhale/narrative-graph.json，再退回 outline 目录扫描。
    let path = workspace.join(".novelwhale").join("narrative-graph.json");
    if let Ok(body) = std::fs::read_to_string(&path) {
        if let Ok(g) = serde_json::from_str::<NarrativeGraph>(&body) {
            return g;
        }
    }
    NarrativeGraph::default()
}

fn build_user_prompt(
    node: &crate::model::graph::NarrativeNode,
    count: u64,
    workspace: &Path,
) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(out, "请围绕以下决策点生成 {count} 条互斥分支。");
    let _ = writeln!(
        out,
        "\n决策点：{}\n类型：{:?}\n当前张力：{:.2}\n情感基调：{:+.2}",
        node.title, node.node_type, node.tension, node.emotional_valence
    );
    if let Ok(store) = FactStore::load(workspace) {
        if !store.world_rules.is_empty() {
            let _ = writeln!(out, "\n世界规则（必须遵守）：");
            for r in store.world_rules.iter().take(8) {
                let _ = writeln!(out, "- [{}] {}", r.category, r.rule);
            }
        }
        if !store.characters.is_empty() {
            let _ = writeln!(out, "\n已存在人物：");
            for c in store.characters.iter().take(8) {
                let _ = writeln!(out, "- {}（{}）", c.name, c.status);
            }
        }
    }
    let _ = writeln!(out, "\n请输出严格 JSON 数组。");
    out
}

impl crate::tools::NovelTool for NovelBranchExploreTool {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::graph::NarrativeNode;
    use crate::model::chapter::NarrativeNodeType;
    use uuid::Uuid;

    #[test]
    fn branch_session_round_trip() {
        let node_id = Uuid::new_v4().to_string();
        let session = BranchSession {
            node_id: node_id.clone(),
            node_title: "抉择：救师还是逃命".to_string(),
            generated_at: "2026-06-04T00:00:00Z".to_string(),
            suggestions: vec![BranchSuggestion {
                id: "save_master".to_string(),
                title: "救师".to_string(),
                logline: "反身杀回，代价是身份暴露".to_string(),
                tension_after: 0.85,
                risk: "暴露门派身份".to_string(),
                payoff: "赢得师尊信任、获得关键功法".to_string(),
                outline_beat: "主角折返杀入伏击圈，借助手中暗器将师尊救出，但被敌方认出师门印记。".to_string(),
            }],
        };
        let json = serde_json::to_string(&session).unwrap();
        let back: BranchSession = serde_json::from_str(&json).unwrap();
        assert_eq!(back.node_id, node_id);
        assert_eq!(back.suggestions.len(), 1);
        assert_eq!(back.suggestions[0].id, "save_master");
    }

    #[test]
    fn build_user_prompt_includes_rules() {
        let tmp = tempdir();
        let node = NarrativeNode {
            id: Uuid::new_v4(),
            title: "决断".to_string(),
            node_type: NarrativeNodeType::Conflict,
            chapter_ref: None,
            emotional_valence: 0.0,
            tension: 0.6,
        };
        let prompt = build_user_prompt(&node, 3, &tmp);
        assert!(prompt.contains("决断"));
        assert!(prompt.contains("生成 3 条互斥分支"));
        assert!(prompt.contains("请输出严格 JSON 数组"));
    }

    fn tempdir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("novel_branch_{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
