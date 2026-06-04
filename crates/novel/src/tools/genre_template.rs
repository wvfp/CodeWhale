//! `novel_genre_template` tool — 查询 / 应用题材模板。
//!
//! 返回指定题材的节奏配方、推荐字数、推荐故事弧线、关键要诀、常见爽点。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::genre::{GenreRule, GenreTemplate, RuleSeverity, rules_for};
use crate::model::project::Genre;
use crate::tools::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    readonly_capabilities,
};

pub struct NovelGenreTemplateTool;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GenreTemplateJson {
    genre: String,
    display_name: String,
    recommended_words: u32,
    min_words: u32,
    max_words: u32,
    scene_mix: Value,
    arc: Value,
    key_principles: Vec<String>,
    dopamine_moments: Vec<String>,
    rules: Vec<RuleJson>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuleJson {
    id: String,
    description: String,
    severity: String,
    check_hint: String,
}

#[async_trait]
impl ToolSpec for NovelGenreTemplateTool {
    fn name(&self) -> &'static str {
        "novel_genre_template"
    }

    fn description(&self) -> &'static str {
        "查询 / 应用题材模板：返回指定题材的节奏配方、推荐字数、故事弧线、关键要诀、常见爽点，以及题材专有的一致性规则。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "genre": {
                    "type": "string",
                    "description": "题材：xianxia / urban / scifi / fantasy / historical / romance / suspense / other",
                    "enum": [
                        "xianxia", "urban", "scifi", "fantasy", "historical", "romance", "suspense", "other"
                    ]
                }
            },
            "required": ["genre"]
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
        let _ = context;
        let genre_str = input
            .get("genre")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::missing_field("genre"))?;
        let genre = parse_genre(genre_str)
            .ok_or_else(|| ToolError::execution_failed(format!("未知题材：{}", genre_str)))?;
        let tpl = GenreTemplate::for_genre(genre.clone());
        let rules: Vec<GenreRule> = rules_for(genre.clone());
        let out = to_json(&tpl, &rules);
        let pretty = serde_json::to_string_pretty(&out)
            .map_err(|e| ToolError::execution_failed(e.to_string()))?;
        Ok(ToolResult::success(pretty))
    }
}

fn to_json(tpl: &GenreTemplate, rules: &[GenreRule]) -> GenreTemplateJson {
    let arc_json: Vec<Value> = tpl
        .arc
        .phases
        .iter()
        .map(|p| {
            json!({
                "name": p.name,
                "goal": p.goal,
                "weight": p.weight,
                "requires_hook": p.requires_hook,
            })
        })
        .collect();
    GenreTemplateJson {
        genre: format!("{:?}", tpl.genre).to_lowercase(),
        display_name: tpl.display_name.clone(),
        recommended_words: tpl.pace.recommended_words,
        min_words: tpl.pace.min_words,
        max_words: tpl.pace.max_words,
        scene_mix: json!({
            "dialogue": tpl.scene_mix.dialogue,
            "action": tpl.scene_mix.action,
            "description": tpl.scene_mix.description,
            "introspection": tpl.scene_mix.introspection,
        }),
        arc: json!({
            "total_chapters": tpl.arc.total_chapters,
            "phases": arc_json,
        }),
        key_principles: tpl.key_principles.clone(),
        dopamine_moments: tpl.dopamine_moments.clone(),
        rules: rules
            .iter()
            .map(|r| RuleJson {
                id: r.id.clone(),
                description: r.description.clone(),
                severity: severity_str(r.severity).to_string(),
                check_hint: r.check_hint.clone(),
            })
            .collect(),
    }
}

fn severity_str(s: RuleSeverity) -> &'static str {
    match s {
        RuleSeverity::Info => "info",
        RuleSeverity::Warning => "warning",
        RuleSeverity::Hard => "hard",
    }
}

fn parse_genre(s: &str) -> Option<Genre> {
    match s.to_lowercase().as_str() {
        "xianxia" | "玄幻" | "仙侠" => Some(Genre::Xianxia),
        "urban" | "都市" | "都市日常" => Some(Genre::Urban),
        "scifi" | "sci-fi" | "科幻" => Some(Genre::SciFi),
        "fantasy" | "奇幻" | "西幻" => Some(Genre::Fantasy),
        "historical" | "history" | "历史" | "穿越" => Some(Genre::Historical),
        "romance" | "言情" => Some(Genre::Romance),
        "suspense" | "悬疑" | "推理" => Some(Genre::Suspense),
        "other" | "其他" => Some(Genre::Other),
        _ => None,
    }
}

impl crate::tools::NovelTool for NovelGenreTemplateTool {}
