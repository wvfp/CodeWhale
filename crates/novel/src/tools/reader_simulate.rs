//! `novel_reader_simulate` tool — 读者视角模拟（Task 14.3）。
//!
//! 把自己当成「目标读者」，对最近一章 / 整本书做三件事：
//!
//! 1. 留下一个「情绪日记」：阅读时哪一段最紧张 / 最闷 / 最有共鸣？
//! 2. 预测读者最想问的 3 个问题（用来给下一章制造钩子）。
//! 3. 给出「如果我读到这会弃书吗？」的判断。
//!
//! 与 14.1 / 14.2 一样：工具只负责「把上下文 + system prompt 打包」，LLM 负
//! 责真正模拟。本工具是只读 + 不落盘（结果直接给用户看）。

use std::path::Path;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::consistency::FactStore;
use crate::model::project::Genre;
use crate::synopsis::store::SynopsisStore;
use crate::tools::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    readonly_capabilities,
};

pub struct NovelReaderSimulateTool;

#[async_trait]
impl ToolSpec for NovelReaderSimulateTool {
    fn name(&self) -> &'static str {
        "novel_reader_simulate"
    }

    fn description(&self) -> &'static str {
        "读者视角模拟：把 LLM 当成目标读者，针对指定章节（或最近一章）输出情绪日记 + 3 个潜在疑问 + 弃书概率。题材决定读者画像（仙侠读者 / 都市读者 / 推理读者）。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "chapter": {
                    "type": "integer",
                    "description": "要评估哪一章；留空则评估最近一章。"
                },
                "genre": {
                    "type": "string",
                    "description": "读者画像所依据的题材（xianxia/urban/scifi/fantasy/historical/romance/suspense/other）。",
                    "enum": ["xianxia", "urban", "scifi", "fantasy", "historical", "romance", "suspense", "other"]
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
        let genre = parse_genre(input.get("genre").and_then(Value::as_str).unwrap_or(""));
        let chapter = input.get("chapter").and_then(Value::as_u64).map(|c| c as u32);

        let chapter_text = read_chapter(&context.workspace, chapter);
        let chapter_meta = load_summary(&context.workspace, chapter);

        let user_prompt = build_user_prompt(
            &genre,
            chapter,
            chapter_text.as_deref(),
            chapter_meta.as_ref(),
            &context.workspace,
        );

        let out = json!({
            "genre": format!("{:?}", genre).to_lowercase(),
            "chapter": chapter,
            "system_prompt": SYSTEM_PROMPT,
            "user_prompt": user_prompt,
            "instructions": {
                "writes": [],
                "expected_response_format": "JSON {mood_journal: [...], burning_questions: [...], drop_risk: 0-100, next_chapter_promise: 1-2句}"
            },
            "note": "调用方负责：把 system_prompt + user_prompt 拼成一次 LLM 调用；按 schema 解析结果，直接展示给用户。"
        });
        let pretty = serde_json::to_string_pretty(&out)
            .map_err(|e| ToolError::execution_failed(e.to_string()))?;
        Ok(ToolResult::success(pretty))
    }
}

const SYSTEM_PROMPT: &str = r#"你是一位"目标读者"。你不会被作者的意图说服 — 你只会用「我作为读者」的标准评判最近一章。

请输出严格的 JSON：
{
  "mood_journal": [
    "第一段：刚打开书，我的感受…（1 句）",
    "中段：到第 N 段时…（1 句）",
    "结尾：合上这一章时…（1 句）"
  ],
  "burning_questions": [
    "我作为读者最想问的 3 个问题（短句）"
  ],
  "drop_risk": 0-100,
  "drop_risk_reason": "1 句中文解释",
  "next_chapter_promise": "作者在下一章必须兑现的 1-2 件事（短句）"
}

规则：
- 评分必须基于你看到的章节内容；如果没有正文，明确说"正文缺失，无法判断"。
- 不要鼓励作者 — 你只说真话。
- 题材决定读者画像：xianxia 关心境界 / 资源 / 师门；urban 关心金钱 / 情感 / 职场；scifi 关心设定自洽；romance 关心情感递进；suspense 关心线索 / 公平竞争。
"#;

fn parse_genre(s: &str) -> Genre {
    match s.to_lowercase().as_str() {
        "xianxia" | "玄幻" | "仙侠" => Genre::Xianxia,
        "urban" | "都市" => Genre::Urban,
        "scifi" | "sci-fi" | "科幻" => Genre::SciFi,
        "fantasy" | "奇幻" | "西幻" => Genre::Fantasy,
        "historical" | "history" | "历史" | "穿越" => Genre::Historical,
        "romance" | "言情" => Genre::Romance,
        "suspense" | "悬疑" | "推理" => Genre::Suspense,
        _ => Genre::Other,
    }
}

fn read_chapter(workspace: &Path, chapter: Option<u32>) -> Option<String> {
    let dir = workspace.join("chapters");
    let pattern = chapter
        .map(|n| format!("ch_{n:03}_"))
        .unwrap_or_default();
    let entries = std::fs::read_dir(&dir).ok()?;
    let mut candidates: Vec<_> = entries
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                == Some("md")
        })
        .filter(|e| {
            if pattern.is_empty() {
                true
            } else {
                e.file_name().to_string_lossy().starts_with(&pattern)
            }
        })
        .collect();
    candidates.sort_by_key(|e| e.file_name());
    let last = candidates.last()?;
    let path = last.path();
    let body = std::fs::read_to_string(path).ok()?;
    // 限制长度，避免 prompt 爆掉
    let max = 4000;
    if body.chars().count() > max {
        let mut truncated = body.chars().take(max).collect::<String>();
        truncated.push_str("\n\n…(已截断)");
        Some(truncated)
    } else {
        Some(body)
    }
}

fn load_summary(
    workspace: &Path,
    chapter: Option<u32>,
) -> Option<crate::synopsis::generator::ChapterSynopsis> {
    let store = SynopsisStore::load_all(workspace).ok()?;
    match chapter {
        Some(n) => store.get(n).ok().flatten(),
        None => store.recent(1).into_iter().next().cloned(),
    }
}

fn build_user_prompt(
    genre: &Genre,
    chapter: Option<u32>,
    body: Option<&str>,
    summary: Option<&crate::synopsis::generator::ChapterSynopsis>,
    workspace: &Path,
) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let label = match genre {
        Genre::Xianxia => "仙侠读者（在意境界、资源、师门）",
        Genre::Urban => "都市读者（在意金钱、情感、职场）",
        Genre::SciFi => "科幻读者（在意设定自洽）",
        Genre::Fantasy => "奇幻读者",
        Genre::Historical => "穿越 / 历史读者",
        Genre::Romance => "言情读者（在意情感递进）",
        Genre::Suspense => "悬疑读者（在意线索与公平竞争）",
        Genre::Other => "一般读者",
    };
    let _ = writeln!(out, "你是一位「{label}」。");
    if let Some(n) = chapter {
        let _ = writeln!(out, "请评估第 {n} 章。");
    } else {
        let _ = writeln!(out, "请评估最近一章。");
    }
    if let Some(s) = summary {
        let _ = writeln!(
            out,
            "\n【摘要】{}\n事件链：{}\n章末卡点：{}\n待回收伏笔：{}",
            s.chapter_title,
            s.event_chain.join(" → "),
            s.ending_state.plot_point,
            s.pending_hooks.join("、")
        );
    }
    match body {
        Some(text) if !text.trim().is_empty() => {
            let _ = writeln!(out, "\n【正文】\n{text}");
        }
        _ => {
            let _ = writeln!(out, "\n（正文缺失，无法判断）");
        }
    }
    if let Ok(store) = FactStore::load(workspace)
        && !store.world_rules.is_empty()
    {
        let _ = writeln!(out, "\n【世界规则速览】");
        for r in store.world_rules.iter().take(4) {
            let _ = writeln!(out, "- [{}] {}", r.category, r.rule);
        }
    }
    out
}

impl crate::tools::NovelTool for NovelReaderSimulateTool {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::project::Genre;

    #[test]
    fn parse_genre_zh_works() {
        assert!(matches!(parse_genre("仙侠"), Genre::Xianxia));
        assert!(matches!(parse_genre("都市"), Genre::Urban));
        assert!(matches!(parse_genre(""), Genre::Other));
    }

    #[test]
    fn build_user_prompt_includes_genre_label() {
        let dir = tempdir();
        let prompt = build_user_prompt(&Genre::Xianxia, Some(3), Some("正文"), None, &dir);
        assert!(prompt.contains("仙侠"));
        assert!(prompt.contains("正文"));
    }

    fn tempdir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("novel_reader_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
