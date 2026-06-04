//! `novel_pipeline_run` tool — 启动 / 推进子 Agent 流水线。
//!
//! 实际的 LLM 调用由上层负责；本工具负责「把当前步骤需要的 system prompt
//! + 上下文打包成模型所需的输入」并把进度写回 `.novelwhale/pipeline.json`。

use std::path::Path;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::agents::{
    Pipeline, PipelinePhase, PipelineRecord, PipelineStatus, PipelineStep,
};
use crate::consistency::FactStore;
use crate::model::project::Genre;
use crate::synopsis::store::SynopsisStore;
use crate::tools::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    readonly_capabilities,
};

pub struct NovelPipelineRunTool;

#[async_trait]
impl ToolSpec for NovelPipelineRunTool {
    fn name(&self) -> &'static str {
        "novel_pipeline_run"
    }

    fn description(&self) -> &'static str {
        "启动 / 推进子 Agent 流水线：返回当前步骤、该步骤对应的 Agent、它的系统提示、要写到哪里、要读什么上下文。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "idea": {
                    "type": "string",
                    "description": "用户的原始灵感（概念阶段必填）。"
                },
                "genre": {
                    "type": "string",
                    "description": "题材（首次启动时必填）。",
                    "enum": ["xianxia", "urban", "scifi", "fantasy", "historical", "romance", "suspense", "other"]
                },
                "step": {
                    "type": "string",
                    "description": "指定要跑的步骤。留空则从当前进度继续。",
                    "enum": ["architect_concept", "outliner_arc", "outliner_chapter", "writer_draft", "editor_polish"]
                },
                "chapter": {
                    "type": "integer",
                    "description": "要写 / 校哪一章（draft / polish 阶段必填）。"
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
        // 加载 / 初始化 record
        let mut record = PipelineRecord::load(&context.workspace).unwrap_or_else(|_| {
            let genre = parse_genre(input.get("genre").and_then(Value::as_str).unwrap_or("other"))
                .unwrap_or(Genre::Other);
            PipelineRecord::new(genre)
        });

        // 更新 record
        if let Some(g) = input.get("genre").and_then(Value::as_str) {
            if let Some(parsed) = parse_genre(g) {
                record.genre = parsed;
            }
        }
        if let Some(step) = input.get("step").and_then(Value::as_str) {
            if let Some(s) = parse_step(step) {
                record.step = s;
                record.current_agent = s.agent();
                record.phase = s.phase();
            }
        }
        if let Some(ch) = input.get("chapter").and_then(Value::as_u64) {
            record.chapter = Some(ch as u32);
        }
        record.status = PipelineStatus::Running;
        let _ = record.save(&context.workspace);

        // 准备 step 要的输入
        let step = record.step;
        let agent = step.agent();
        let system_prompt = agent.system_prompt();
        let user_prompt = build_user_prompt(
            &record,
            step,
            input.get("idea").and_then(Value::as_str).unwrap_or(""),
            &context.workspace,
        );

        // 把所有要读的文件 / 要写的文件也列出来
        let reads = collect_reads(&record, &context.workspace);
        let writes = vec![format!(".novelwhale/pipeline.json")];

        let out = json!({
            "step": format!("{:?}", step),
            "step_label": step.label(),
            "agent": format!("{:?}", agent).to_lowercase(),
            "agent_label": agent.label(),
            "system_prompt": system_prompt,
            "user_prompt": user_prompt,
            "reads": reads,
            "writes": writes,
            "phase": format!("{:?}", record.phase).to_lowercase(),
            "phase_label": record.phase.label(),
            "next_step": step.next().map(|s| format!("{:?}", s)),
            "note": "调用方负责：把 system_prompt + user_prompt 拼成一次 LLM 调用；将模型输出按 schema 写回 writes；再调用 novel_pipeline_advance 进入下一步。"
        });
        let pretty = serde_json::to_string_pretty(&out)
            .map_err(|e| ToolError::execution_failed(e.to_string()))?;
        Ok(ToolResult::success(pretty))
    }
}

fn parse_genre(s: &str) -> Option<Genre> {
    match s.to_lowercase().as_str() {
        "xianxia" | "玄幻" | "仙侠" => Some(Genre::Xianxia),
        "urban" | "都市" => Some(Genre::Urban),
        "scifi" | "sci-fi" | "科幻" => Some(Genre::SciFi),
        "fantasy" | "奇幻" | "西幻" => Some(Genre::Fantasy),
        "historical" | "history" | "历史" | "穿越" => Some(Genre::Historical),
        "romance" | "言情" => Some(Genre::Romance),
        "suspense" | "悬疑" | "推理" => Some(Genre::Suspense),
        "other" | "其他" => Some(Genre::Other),
        _ => None,
    }
}

fn parse_step(s: &str) -> Option<PipelineStep> {
    match s.to_lowercase().as_str() {
        "architect_concept" => Some(PipelineStep::ArchitectConcept),
        "outliner_arc" => Some(PipelineStep::OutlinerArc),
        "outliner_chapter" => Some(PipelineStep::OutlinerChapter),
        "writer_draft" => Some(PipelineStep::WriterDraft),
        "editor_polish" => Some(PipelineStep::EditorPolish),
        _ => None,
    }
}

fn build_user_prompt(
    record: &PipelineRecord,
    step: PipelineStep,
    idea: &str,
    workspace: &Path,
) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(out, "题材：{:?}", record.genre);
    if let Some(ch) = record.chapter {
        let _ = writeln!(out, "目标章节：第 {} 章", ch);
    }
    if !idea.is_empty() {
        let _ = writeln!(out, "\n用户灵感：\n{}", idea);
    }
    // 把已经成型的设定 / 摘要 / 大纲塞进去
    if let Ok(store) = FactStore::load(workspace) {
        if !store.world_rules.is_empty() {
            let _ = writeln!(out, "\n已锁定的世界规则：");
            for r in &store.world_rules {
                let _ = writeln!(out, "- [{}] {}", r.category, r.rule);
            }
        }
        if !store.characters.is_empty() {
            let _ = writeln!(out, "\n已存在的人物：");
            for c in &store.characters {
                let _ = writeln!(out, "- {}：{}", c.name, c.status);
            }
        }
    }
    if let Ok(summaries) = SynopsisStore::load_all(workspace) {
        let recent: Vec<&_> = summaries.recent(3);
        if !recent.is_empty() {
            let _ = writeln!(out, "\n最近章节摘要：");
            for s in recent {
                let _ = writeln!(out, "- 第 {} 章 {}：{}", s.chapter_number, s.chapter_title, s.event_chain.join(" → "));
            }
        }
    }
    let _ = writeln!(out, "\n当前步骤：{:?}", step);
    out
}

fn collect_reads(record: &PipelineRecord, workspace: &Path) -> Vec<String> {
    let mut out = vec![".novelwhale/pipeline.json".to_string()];
    match record.step {
        PipelineStep::ArchitectConcept => {
            out.push(".novelwhale/concept.md".to_string());
        }
        PipelineStep::OutlinerArc | PipelineStep::OutlinerChapter => {
            out.push(".novelwhale/outline.md".to_string());
            out.push(".novelwhale/facts.json".to_string());
        }
        PipelineStep::WriterDraft => {
            if let Some(ch) = record.chapter {
                out.push(format!("chapters/{:03}.md", ch));
            }
            out.push(".novelwhale/facts.json".to_string());
            out.push(".novelwhale/outline.md".to_string());
            // 不会读但推荐读
            out.push(".novelwhale/rag-index.json".to_string());
        }
        PipelineStep::EditorPolish => {
            if let Some(ch) = record.chapter {
                out.push(format!("chapters/{:03}.md", ch));
            }
            out.push(".novelwhale/facts.json".to_string());
        }
    }
    // 把所有 reads 都加上工作区前缀便于 LLM 看到
    for r in &mut out {
        if !r.starts_with('/') {
            *r = workspace.join(&r).display().to_string();
        }
    }
    out
}

impl crate::tools::NovelTool for NovelPipelineRunTool {}

// === 第二把工具：标记当前步骤完成 + 进入下一步 ===

pub struct NovelPipelineAdvanceTool;

#[async_trait]
impl ToolSpec for NovelPipelineAdvanceTool {
    fn name(&self) -> &'static str {
        "novel_pipeline_advance"
    }

    fn description(&self) -> &'static str {
        "把当前步骤标记为完成，把下一步写入 pipeline.json。模型把工件写完后调用本工具。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "artifact_summary": {
                    "type": "string",
                    "description": "这次步骤的产物简短人话描述。"
                },
                "status": {
                    "type": "string",
                    "description": "done / failed / needs_human_input",
                    "enum": ["done", "failed", "needs_human_input"]
                }
            },
            "required": ["status"]
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
        let mut record = PipelineRecord::load(&context.workspace).map_err(|e| {
            ToolError::execution_failed(format!("找不到 pipeline 状态：{}", e))
        })?;
        let status = match input.get("status").and_then(Value::as_str).unwrap_or("done") {
            "done" => PipelineStatus::Done,
            "failed" => PipelineStatus::Failed,
            _ => PipelineStatus::NeedsHumanInput,
        };
        record.status = status;
        if let Some(s) = input.get("artifact_summary").and_then(Value::as_str) {
            record.last_artifact_summary = s.to_string();
        }
        if status == PipelineStatus::Done {
            record.completed_steps.push(record.step);
            if let Some(next) = Pipeline::next_step(record.step) {
                record.step = next;
                record.current_agent = next.agent();
                record.phase = next.phase();
                record.status = PipelineStatus::Running;
            }
        }
        record.updated_at = crate::agents::state::now_iso8601();
        record
            .save(&context.workspace)
            .map_err(|e| ToolError::execution_failed(format!("保存 pipeline 状态失败：{}", e)))?;
        let pretty = serde_json::to_string_pretty(&record)
            .map_err(|e| ToolError::execution_failed(e.to_string()))?;
        Ok(ToolResult::success(pretty))
    }
}

impl crate::tools::NovelTool for NovelPipelineAdvanceTool {}

#[allow(dead_code)]
fn _pipeline_phase_label() -> &'static str {
    PipelinePhase::Concept.label()
}
