//! `novel_write` tool — draft a chapter.

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::facts::FactExtractor;
use crate::model::chapter::{Chapter, ChapterStatus, NarrativeNodeType};
use crate::model::stage::CreationStage;
use crate::synopsis::{HeuristicSynopsisGenerator, SynopsisGenerator, SynopsisStore};
use crate::tools::{
    ApprovalRequirement, NovelToolError, ToolCapability, ToolContext, ToolError, ToolResult,
    ToolSpec, optional_str, optional_u64, required_str, write_capabilities,
};

pub struct NovelWriteTool;

#[async_trait]
impl ToolSpec for NovelWriteTool {
    fn name(&self) -> &'static str {
        "novel_write"
    }

    fn description(&self) -> &'static str {
        "写一章正文。模型按系统提示 + 注入的大纲/概要来写；本工具把章节内容落盘到 chapters/ch_NNN_<title>.md，并把章节登记到项目状态。正文生成发生在 LLM 这一轮，工具只接收最终内容。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "chapter_number": {
                    "type": "integer",
                    "description": "章节号（1 起，必填）"
                },
                "title": {
                    "type": "string",
                    "description": "章节标题"
                },
                "content": {
                    "type": "string",
                    "description": "完整章节正文（LLM 在本轮生成的）"
                },
                "node_type": {
                    "type": "string",
                    "enum": ["Hook", "Setup", "Conflict", "Climax", "Twist", "Payoff", "Exposition", "Resolution"],
                    "description": "本章在大纲中的叙事角色"
                },
                "target_words": {
                    "type": "integer",
                    "description": "本章目标字数（默认 3000）"
                }
            },
            "required": ["chapter_number", "content"]
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
        let chapter_number = optional_u64(&input, "chapter_number", 0) as u32;
        if chapter_number == 0 {
            return Err(NovelToolError::InvalidInput("缺少 chapter_number 字段".into()).into());
        }
        let content = required_str(&input, "content")?;
        let title = optional_str(&input, "title").unwrap_or("").to_string();
        let target_words = optional_u64(&input, "target_words", 3000) as u32;
        let node_type = parse_node_type(optional_str(&input, "node_type"));
        let node_type_for_json = node_type.clone();

        if content.trim().is_empty() {
            return Err(NovelToolError::InvalidInput("正文为空".into()).into());
        }

        let chapter = Chapter {
            id: uuid::Uuid::new_v4(),
            title: title.clone(),
            number: chapter_number,
            outline: None,
            content: Some(content.to_string()),
            status: ChapterStatus::Completed,
            node_type,
            emotional_valence: 0.0,
            tension: 0.5,
        };

        let file_name = if title.is_empty() {
            format!("ch_{:03}.md", chapter_number)
        } else {
            // sanitize title for filesystem
            let safe: String = title
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '_' || c == '-' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            format!("ch_{:03}_{}.md", chapter_number, safe)
        };

        let path = context.workspace.join("chapters").join(&file_name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(NovelToolError::from)?;
        }

        let body = format!(
            "# 第{}章 {}\n\n{}\n",
            chapter_number, title, content
        );
        std::fs::write(&path, body).map_err(NovelToolError::from)?;

        let actual_words = content.chars().filter(|c| !c.is_whitespace()).count() as u32;

        // Auto-extract structured facts from the chapter content.
        let extracted = FactExtractor::extract(content);
        let mut extracted_chars = 0usize;
        let mut extracted_rules = 0usize;
        if !extracted.is_empty() {
            if let Err(e) = FactExtractor::persist_to(&context.workspace, chapter_number, &extracted) {
                // Persistence is best-effort: log into the result payload but do
                // not fail the tool call.
                eprintln!("[novel_write] fact persistence failed: {e}");
            }
            extracted_chars = extracted.characters.len();
            extracted_rules = extracted.world_rules.len();
        }

        // Auto-generate a synopsis for this chapter. The MVP uses a
        // heuristic generator; an LLM-backed generator can be plugged in
        // later by injecting it into the tool context.
        let generator = HeuristicSynopsisGenerator;
        let synopsis = generator
            .generate(content, chapter_number, &title)
            .await
            .map_err(|e| ToolError::execution_failed(format!("synopsis generation: {e}")))?;
        let mut store = SynopsisStore::load_all(&context.workspace).unwrap_or_default();
        if let Err(e) = store.add(synopsis.clone()) {
            eprintln!("[novel_write] synopsis add failed: {e}");
        } else if let Err(e) = store.save_all(&context.workspace) {
            eprintln!("[novel_write] synopsis save failed: {e}");
        }

        let payload = json!({
            "chapter_id": chapter.id,
            "chapter_number": chapter_number,
            "title": title,
            "path": path.display().to_string(),
            "target_words": target_words,
            "actual_words": actual_words,
            "node_type": node_type_for_json,
            "current_stage": CreationStage::Draft,
            "facts_extracted": {
                "characters": extracted_chars,
                "world_rules": extracted_rules,
            },
            "synopsis_path": SynopsisStore::dir_for(&context.workspace)
                .join(crate::synopsis::store::synopsis_filename(chapter_number))
                .display()
                .to_string(),
            "synopsis": {
                "event_chain": synopsis.event_chain,
                "key_details": synopsis.key_details,
                "ending_state": synopsis.ending_state,
                "continuation_notes": synopsis.continuation_notes,
                "pending_hooks": synopsis.pending_hooks,
            },
        });

        let result_json = serde_json::to_string(&payload)
            .map_err(|e| ToolError::execution_failed(e.to_string()))?;
        Ok(ToolResult::success(result_json))
    }
}

fn parse_node_type(s: Option<&str>) -> NarrativeNodeType {
    use NarrativeNodeType::*;
    match s.unwrap_or("").to_ascii_lowercase().as_str() {
        "hook" => Hook,
        "setup" => Setup,
        "conflict" => Conflict,
        "climax" => Climax,
        "twist" => Twist,
        "payoff" => Payoff,
        "exposition" => Exposition,
        "resolution" => Resolution,
        _ => Setup,
    }
}

impl crate::tools::NovelTool for NovelWriteTool {}
