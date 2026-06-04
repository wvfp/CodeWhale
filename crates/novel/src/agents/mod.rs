//! 子 Agent 协作流水线。
//!
//! 把「写一本网络小说」拆成 5 个角色协作：
//!
//! - **Director（主编）**：接收用户意图，决定走哪几个阶段。
//! - **Architect（架构师）**：把模糊想法整理成「核心冲突 / 世界观 / 主角人设」清单。
//! - **Outliner（大纲师）**：把架构师输出展开成「卷 → 章」两层大纲。
//! - **Writer（写手）**：把章节大纲写成正文章节。
//! - **Editor（编辑）**：审校、去 AI 化、一致性检查，给出修改指令。
//!
//! 流水线由 [`pipeline::Pipeline`] 驱动，运行时把每个阶段的状态写到
//! `.novelwhale/pipeline.json`，方便回看和断点续跑。

pub mod agents;
pub mod pipeline;
pub mod prompts;
pub mod state;

pub use agents::{AgentId, AgentRole, AgentTurn};
pub use pipeline::{Pipeline, PipelineInput, PipelineResult, PipelineStep};
pub use state::{PipelinePhase, PipelineRecord, PipelineStatus, PIPELINE_PATH};
