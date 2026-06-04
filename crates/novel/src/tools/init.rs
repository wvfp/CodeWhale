//! `novel_init` tool — initialize a novel project.

use std::path::PathBuf;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::model::project::{Genre, NovelProject, TargetPlatform};
use crate::tools::{
    ApprovalRequirement, NovelToolError, ToolCapability, ToolContext, ToolError, ToolResult,
    ToolSpec, optional_str, optional_u64, required_str, write_capabilities,
};

use super::super::model::stage::CreationStage;

pub struct NovelInitTool;

#[async_trait]
impl ToolSpec for NovelInitTool {
    fn name(&self) -> &'static str {
        "novel_init"
    }

    fn description(&self) -> &'static str {
        "初始化一个新的小说项目。创建标准化的目录结构（world/、characters/、outline/、chapters/、.novelwhale/）并写入项目配置。每本新小说开写前调用一次。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "小说标题（必填）"
                },
                "genre": {
                    "type": "string",
                    "enum": ["xianxia", "urban", "scifi", "fantasy", "historical", "romance", "suspense", "other"],
                    "description": "题材分类（玄幻 / 都市 / 科幻 / 奇幻 / 历史 / 言情 / 悬疑 / 其他）"
                },
                "target_platform": {
                    "type": "string",
                    "enum": ["qidian", "zongheng", "fanqie", "jinjiang", "web", "other"],
                    "description": "目标发布平台（起点 / 纵横 / 番茄 / 晋江 / 通用网页 / 其他）"
                },
                "author": {
                    "type": "string",
                    "description": "作者笔名（可选）"
                },
                "target_word_count": {
                    "type": "integer",
                    "description": "目标总字数（可选，默认 1000000）"
                }
            },
            "required": ["title", "genre"]
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
        let title = required_str(&input, "title")?;
        let genre_str = required_str(&input, "genre")?;
        let platform_str = optional_str(&input, "target_platform").unwrap_or("web");
        let author = optional_str(&input, "author").unwrap_or("").to_string();
        let target_words = optional_u64(&input, "target_word_count", 1_000_000) as u32;

        let genre = parse_genre(genre_str)
            .ok_or_else(|| NovelToolError::InvalidInput(format!("未知的题材：{genre_str}")))?;
        let platform = parse_platform(platform_str).ok_or_else(|| {
            NovelToolError::InvalidInput(format!("未知的发布平台：{platform_str}"))
        })?;

        let project = NovelProject {
            title: title.to_string(),
            genre,
            target_platform: platform,
            style_notes: Some(String::new()),
            target_word_count: target_words as u64,
            author,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let project_root = resolve_project_root(context);
        create_project_structure(&project_root, &project).map_err(ToolError::from)?;

        let config_path = project_root.join(".novelwhale").join("novel.toml");
        let config_toml = build_config_toml(&project);
        std::fs::write(&config_path, config_toml).map_err(NovelToolError::from)?;

        let state = json!({
            "project_root": project_root.display().to_string(),
            "config_path": config_path.display().to_string(),
            "title": project.title,
            "genre": project.genre,
            "target_platform": project.target_platform,
            "stage": CreationStage::Concept,
        });

        let state_json = serde_json::to_string(&state)
            .map_err(|e| ToolError::execution_failed(e.to_string()))?;
        Ok(ToolResult::success(state_json))
    }
}

fn parse_genre(s: &str) -> Option<Genre> {
    match s.to_ascii_lowercase().as_str() {
        "xianxia" => Some(Genre::Xianxia),
        "urban" => Some(Genre::Urban),
        "scifi" => Some(Genre::SciFi),
        "fantasy" => Some(Genre::Fantasy),
        "historical" => Some(Genre::Historical),
        "romance" => Some(Genre::Romance),
        "suspense" => Some(Genre::Suspense),
        _ => Some(Genre::Other),
    }
}

fn parse_platform(s: &str) -> Option<TargetPlatform> {
    match s.to_ascii_lowercase().as_str() {
        "qidian" => Some(TargetPlatform::Qidian),
        "zongheng" => Some(TargetPlatform::Zongheng),
        "fanqie" => Some(TargetPlatform::Fanqie),
        "jinjiang" => Some(TargetPlatform::Jinjiang),
        "web" => Some(TargetPlatform::Web),
        _ => Some(TargetPlatform::Other),
    }
}

fn resolve_project_root(context: &ToolContext) -> PathBuf {
    context.workspace.clone()
}

fn create_project_structure(
    root: &std::path::Path,
    project: &NovelProject,
) -> Result<(), NovelToolError> {
    let dirs = [
        ".novelwhale",
        ".novelwhale/synopses",
        "world",
        "characters",
        "outline",
        "outline/arcs",
        "outline/chapters",
        "chapters",
        "drafts",
    ];
    for d in dirs {
        std::fs::create_dir_all(root.join(d))?;
    }

    // constitution.md — top-level, captures immutable rules
    let constitution = "# 创作宪章 (Constitution)\n\n\
        # This file documents world rules and character constraints that MUST NOT be contradicted by any chapter.\n\
        # Use `novel_fact_lock` to mark rules as immutable.\n\n\
        ## Project\n\n\
        - Title: "
        .to_string()
        + &project.title
        + "\n\
          - Genre: "
        + genre_label(&project.genre)
        + "\n\
          - Target platform: "
        + platform_label(&project.target_platform)
        + "\n\n\
          ## World Rules (auto-managed)\n\n\
          _No immutable rules yet. Use `novel_fact_lock` to add._\n";

    std::fs::write(root.join("constitution.md"), constitution)?;

    // style.md — style notes (empty by default)
    std::fs::write(
        root.join("style.md"),
        "# 文风规范 (Style)\n\n\
        Describe the desired prose style here. Examples:\n\n\
        - 第一人称 / 第三人称\n\
        - 简洁 / 细腻 / 热血\n\
        - 网络文学爽文节奏 / 文学性强\n\
        - 对话占比 30%\n",
    )?;

    // world/intro.md — empty world doc
    std::fs::write(
        root.join("world").join("intro.md"),
        "# 世界观 (World)\n\nDescribe the world here.\n",
    )?;

    // outline/README.md
    std::fs::write(
        root.join("outline").join("README.md"),
        "# 大纲 (Outline)\n\nUse `novel_outline_build` to build arc outlines here.\n",
    )?;

    // characters/README.md
    std::fs::write(
        root.join("characters").join("README.md"),
        "# 人物 (Characters)\n\nStore character sheets as separate `.md` files here.\n",
    )?;

    // chapters/README.md
    std::fs::write(
        root.join("chapters").join("README.md"),
        "# 正文 (Manuscript)\n\nChapters saved by `novel_write` will appear here as `ch_NNN_*.md`.\n",
    )?;

    // .novelwhale/project-state.json
    let state = json!({
        "title": project.title,
        "genre": project.genre,
        "target_platform": project.target_platform,
        "target_word_count": project.target_word_count,
        "current_stage": CreationStage::Concept,
        "created_at": project.created_at,
        "updated_at": project.updated_at,
    });
    std::fs::write(
        root.join(".novelwhale").join("project-state.json"),
        serde_json::to_string_pretty(&state)?,
    )?;

    // .novelwhale/facts.json (empty store)
    let facts: Value = json!({
        "characters": [],
        "events": [],
        "world_rules": [],
        "last_modified": project.created_at,
    });
    std::fs::write(
        root.join(".novelwhale").join("facts.json"),
        serde_json::to_string_pretty(&facts)?,
    )?;

    Ok(())
}

fn build_config_toml(project: &NovelProject) -> String {
    format!(
        "# NovelWhale project configuration\n\
         title = \"{}\"\n\
         genre = \"{}\"\n\
         target_platform = \"{}\"\n\
         target_word_count = {}\n\
         author = \"{}\"\n\
         created_at = \"{}\"\n",
        project.title,
        genre_label(&project.genre),
        platform_label(&project.target_platform),
        project.target_word_count,
        project.author,
        project.created_at.to_rfc3339(),
    )
}

fn genre_label(g: &Genre) -> &'static str {
    match g {
        Genre::Xianxia => "xianxia",
        Genre::Urban => "urban",
        Genre::SciFi => "scifi",
        Genre::Fantasy => "fantasy",
        Genre::Historical => "historical",
        Genre::Romance => "romance",
        Genre::Suspense => "suspense",
        Genre::Other => "other",
    }
}

fn platform_label(p: &TargetPlatform) -> &'static str {
    match p {
        TargetPlatform::Qidian => "qidian",
        TargetPlatform::Zongheng => "zongheng",
        TargetPlatform::Fanqie => "fanqie",
        TargetPlatform::Jinjiang => "jinjiang",
        TargetPlatform::Web => "web",
        TargetPlatform::Other => "other",
    }
}

impl crate::tools::NovelTool for NovelInitTool {}
