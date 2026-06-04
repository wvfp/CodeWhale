//! 子 Agent 角色定义。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    /// 主编：总控，决定阶段流程。
    Director,
    /// 架构师：负责核心冲突 / 世界观 / 人物。
    Architect,
    /// 大纲师：负责卷章两级大纲。
    Outliner,
    /// 写手：负责章节正文。
    Writer,
    /// 编辑：负责审校 / 去 AI / 一致性。
    Editor,
}

impl AgentRole {
    pub fn label(self) -> &'static str {
        match self {
            AgentRole::Director => "主编",
            AgentRole::Architect => "架构师",
            AgentRole::Outliner => "大纲师",
            AgentRole::Writer => "写手",
            AgentRole::Editor => "编辑",
        }
    }

    pub fn system_prompt(self) -> &'static str {
        match self {
            AgentRole::Director => include_str!("prompts/director.md"),
            AgentRole::Architect => include_str!("prompts/architect.md"),
            AgentRole::Outliner => include_str!("prompts/outliner.md"),
            AgentRole::Writer => include_str!("prompts/writer.md"),
            AgentRole::Editor => include_str!("prompts/editor.md"),
        }
    }
}

/// 内部 id，跟 `AgentRole` 区分（一个角色可以有多个实例）。
pub type AgentId = String;

/// 一次 Agent 输出。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentTurn {
    pub agent: AgentRole,
    pub phase: String,
    /// 这次 turn 写下的内容（人话解释、清单、JSON 等）。
    pub content: String,
    /// 引用到的工具调用 / 文件路径（用于审计）。
    pub artifacts: Vec<String>,
}
