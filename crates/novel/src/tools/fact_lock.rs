//! `novel_fact_lock` tool — lock an immutable fact.

use std::path::Path;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::model::world::{WorldCategory, WorldSetting};
use crate::tools::{
    ApprovalRequirement, NovelToolError, ToolCapability, ToolContext, ToolError, ToolResult,
    ToolSpec, optional_str, required_str, write_capabilities,
};

pub struct NovelFactLockTool;

#[async_trait]
impl ToolSpec for NovelFactLockTool {
    fn name(&self) -> &'static str {
        "novel_fact_lock"
    }

    fn description(&self) -> &'static str {
        "把一条世界规则锁为不可变。一旦锁定，任何后续章节生成都不得违反这条事实。用于核心世界规则（例如「这个世界没有电」「主角拥有火系异能」）。该条事实会写入 .novelwhale/facts.json，并同步追加到 constitution.md 以便出现在模型提示中。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "rule": {
                    "type": "string",
                    "description": "要锁定的规则文本"
                },
                "category": {
                    "type": "string",
                    "enum": ["geography", "history", "culture", "magic", "technology", "politics", "economy", "other"],
                    "description": "世界分类（默认 other）"
                },
                "source_chapter": {
                    "type": "integer",
                    "description": "该规则首次出现的章节（可选）"
                }
            },
            "required": ["rule"]
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
        let rule = required_str(&input, "rule")?;
        let category = parse_category(optional_str(&input, "category"));
        let source_chapter = input
            .get("source_chapter")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);

        let fact = WorldSetting {
            key: slugify(rule),
            value: rule.to_string(),
            category: category.clone(),
            source_chapter,
            immutable: true,
        };

        append_fact(context.workspace.as_path(), &fact).map_err(ToolError::from)?;
        append_to_constitution(context.workspace.as_path(), &fact).map_err(ToolError::from)?;

        let payload = json!({
            "id": fact.key,
            "rule": rule,
            "category": category,
            "source_chapter": source_chapter,
            "immutable": true,
            "status": "locked",
        });

        let result_json = serde_json::to_string(&payload)
            .map_err(|e| ToolError::execution_failed(e.to_string()))?;
        Ok(ToolResult::success(result_json))
    }
}

fn parse_category(s: Option<&str>) -> WorldCategory {
    match s.unwrap_or("other").to_ascii_lowercase().as_str() {
        "geography" => WorldCategory::Geography,
        "history" => WorldCategory::History,
        "culture" => WorldCategory::Culture,
        "magic" => WorldCategory::MagicSystem,
        "technology" => WorldCategory::Technology,
        "politics" => WorldCategory::Politics,
        "economy" => WorldCategory::Economy,
        _ => WorldCategory::Other,
    }
}

fn slugify(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
        if out.len() >= 48 {
            break;
        }
    }
    out.trim_matches('_').to_string()
}

fn append_fact(root: &Path, fact: &WorldSetting) -> Result<(), NovelToolError> {
    let path = root.join(".novelwhale").join("facts.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut store: Value = if path.exists() {
        let raw = std::fs::read_to_string(&path)?;
        serde_json::from_str(&raw).unwrap_or_else(|_| empty_facts())
    } else {
        empty_facts()
    };

    if let Some(rules) = store.get_mut("world_rules").and_then(|v| v.as_array_mut()) {
        rules.push(json!({
            "id": fact.key,
            "rule": fact.value,
            "category": fact.category,
            "source_chapter": fact.source_chapter,
            "immutable": fact.immutable,
        }));
    }
    if let Some(obj) = store.as_object_mut() {
        obj.insert("last_modified".to_string(), json!(chrono::Utc::now()));
    }
    std::fs::write(&path, serde_json::to_string_pretty(&store)?)?;
    Ok(())
}

fn empty_facts() -> Value {
    json!({
        "characters": [],
        "events": [],
        "world_rules": [],
        "last_modified": chrono::Utc::now(),
    })
}

fn append_to_constitution(root: &Path, fact: &WorldSetting) -> Result<(), NovelToolError> {
    let path = root.join("constitution.md");
    let mut content = if path.exists() {
        std::fs::read_to_string(&path)?
    } else {
        String::from("# 创作宪章 (Constitution)\n\n## World Rules (auto-managed)\n")
    };
    let marker = format!("\n- [{}] {}", category_label(&fact.category), fact.value);
    if !content.contains(&fact.value) {
        content.push_str(&marker);
        std::fs::write(&path, content)?;
    }
    Ok(())
}

fn category_label(c: &WorldCategory) -> &'static str {
    match c {
        WorldCategory::Geography => "geography",
        WorldCategory::History => "history",
        WorldCategory::Culture => "culture",
        WorldCategory::MagicSystem => "magic",
        WorldCategory::Technology => "technology",
        WorldCategory::Politics => "politics",
        WorldCategory::Economy => "economy",
        WorldCategory::Other => "other",
    }
}

impl crate::tools::NovelTool for NovelFactLockTool {}
