use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Chinese-language prompt template for LLM-based synopsis generation.
pub const SYNOPSIS_PROMPT_TEMPLATE: &str = r#"请为以下网络小说章节生成结构化概要：

章节：第{chapter_num}章 {chapter_title}
内容：
{chapter_content}

请按以下JSON结构输出：
{{
  "event_chain": ["事件1", "事件2", ...],
  "key_details": ["细节1", ...],
  "ending_state": {{
    "character_states": ["人物1的状态", ...],
    "plot_point": "本章结尾的剧情卡点",
    "location": "结尾地点",
    "mood": "整体情绪基调"
  }},
  "continuation_notes": ["续写注意事项1", ...],
  "pending_hooks": ["待回收伏笔1", ...]
}}"#;

/// Error type for synopsis generation.
#[derive(Debug, Error)]
pub enum GeneratorError {
    #[error("LLM call failed: {0}")]
    Llm(String),
    #[error("LLM returned an invalid response: {0}")]
    InvalidResponse(String),
}

/// Result alias used by synopsis generators.
pub type Result<T> = std::result::Result<T, GeneratorError>;

/// Structured ending state of a chapter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct EndingState {
    pub character_states: Vec<String>,
    pub plot_point: String,
    pub location: String,
    pub mood: String,
}

/// A structured synopsis of a single chapter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChapterSynopsis {
    pub chapter_number: u32,
    pub chapter_title: String,
    pub event_chain: Vec<String>,
    pub key_details: Vec<String>,
    pub ending_state: EndingState,
    pub continuation_notes: Vec<String>,
    pub pending_hooks: Vec<String>,
    pub generated_at: chrono::DateTime<Utc>,
}

impl ChapterSynopsis {
    /// Build a `ChapterSynopsis` with the current UTC timestamp.
    pub fn new(
        chapter_number: u32,
        chapter_title: impl Into<String>,
        event_chain: Vec<String>,
        key_details: Vec<String>,
        ending_state: EndingState,
        continuation_notes: Vec<String>,
        pending_hooks: Vec<String>,
    ) -> Self {
        Self {
            chapter_number,
            chapter_title: chapter_title.into(),
            event_chain,
            key_details,
            ending_state,
            continuation_notes,
            pending_hooks,
            generated_at: Utc::now(),
        }
    }
}

/// Abstract LLM client used by [`LlmSynopsisGenerator`].
///
/// Implementations only need to be able to send a single user prompt and
/// return the raw completion text (which is expected to be valid JSON).
#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, prompt: &str) -> Result<String>;
}

/// Trait for generating a [`ChapterSynopsis`] from raw chapter text.
#[async_trait]
pub trait SynopsisGenerator: Send + Sync {
    async fn generate(
        &self,
        chapter_content: &str,
        chapter_num: u32,
        chapter_title: &str,
    ) -> Result<ChapterSynopsis>;
}

/// LLM-backed synopsis generator.
pub struct LlmSynopsisGenerator {
    client: Arc<dyn LlmClient>,
}

impl LlmSynopsisGenerator {
    pub fn new(client: Arc<dyn LlmClient>) -> Self {
        Self { client }
    }

    /// Render the prompt template with the given chapter data.
    pub fn render_prompt(chapter_num: u32, chapter_title: &str, chapter_content: &str) -> String {
        SYNOPSIS_PROMPT_TEMPLATE
            .replace("{chapter_num}", &chapter_num.to_string())
            .replace("{chapter_title}", chapter_title)
            .replace("{chapter_content}", chapter_content)
    }
}

#[derive(Debug, Deserialize)]
struct LlmSynopsisPayload {
    #[serde(default)]
    event_chain: Vec<String>,
    #[serde(default)]
    key_details: Vec<String>,
    ending_state: Option<EndingState>,
    #[serde(default)]
    continuation_notes: Vec<String>,
    #[serde(default)]
    pending_hooks: Vec<String>,
}

#[async_trait]
impl SynopsisGenerator for LlmSynopsisGenerator {
    async fn generate(
        &self,
        chapter_content: &str,
        chapter_num: u32,
        chapter_title: &str,
    ) -> Result<ChapterSynopsis> {
        let prompt = Self::render_prompt(chapter_num, chapter_title, chapter_content);
        let raw = self.client.complete(&prompt).await?;
        let trimmed = raw.trim();
        let payload: LlmSynopsisPayload = serde_json::from_str(trimmed)
            .map_err(|err| GeneratorError::InvalidResponse(err.to_string()))?;
        let ending_state = payload.ending_state.unwrap_or_default();
        Ok(ChapterSynopsis::new(
            chapter_num,
            chapter_title,
            payload.event_chain,
            payload.key_details,
            ending_state,
            payload.continuation_notes,
            payload.pending_hooks,
        ))
    }
}

/// Heuristic synopsis generator that does not require an LLM.
///
/// Splits the chapter into paragraphs, uses the first and last paragraph as
/// `key_details`, leaves `event_chain` and `pending_hooks` empty, and sets the
/// mood to "neutral".
pub struct HeuristicSynopsisGenerator;

impl Default for HeuristicSynopsisGenerator {
    fn default() -> Self {
        Self
    }
}

impl HeuristicSynopsisGenerator {
    /// Split content into non-empty paragraphs.
    pub fn split_paragraphs(content: &str) -> Vec<&str> {
        content
            .split('\n')
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect()
    }
}

#[async_trait]
impl SynopsisGenerator for HeuristicSynopsisGenerator {
    async fn generate(
        &self,
        chapter_content: &str,
        chapter_num: u32,
        chapter_title: &str,
    ) -> Result<ChapterSynopsis> {
        let paragraphs = Self::split_paragraphs(chapter_content);
        let key_details: Vec<String> = match paragraphs.as_slice() {
            [] => Vec::new(),
            [only] => vec![(*only).to_string()],
            [first, .., last] => {
                let mut details = Vec::with_capacity(2);
                details.push((*first).to_string());
                if first != last {
                    details.push((*last).to_string());
                }
                details
            }
        };

        let ending_state = EndingState {
            mood: "neutral".to_string(),
            ..EndingState::default()
        };

        Ok(ChapterSynopsis::new(
            chapter_num,
            chapter_title,
            Vec::new(),
            key_details,
            ending_state,
            Vec::new(),
            Vec::new(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct MockClient(&'static str);

    #[async_trait]
    impl LlmClient for MockClient {
        async fn complete(&self, _prompt: &str) -> Result<String> {
            Ok(self.0.to_string())
        }
    }

    #[tokio::test]
    async fn llm_generator_parses_payload() {
        let payload = r#"{
            "event_chain": ["主角进入山谷", "发现古剑"],
            "key_details": ["山谷中雾气弥漫"],
            "ending_state": {
                "character_states": ["主角持剑在手"],
                "plot_point": "古剑认主",
                "location": "无名山谷",
                "mood": "紧张"
            },
            "continuation_notes": ["古剑来历待揭示"],
            "pending_hooks": ["古剑上的神秘符文"]
        }"#;
        let g = LlmSynopsisGenerator::new(Arc::new(MockClient(payload)));
        let synopsis = g
            .generate("原文内容...", 1, "初入山谷")
            .await
            .expect("generate");
        assert_eq!(synopsis.chapter_number, 1);
        assert_eq!(synopsis.chapter_title, "初入山谷");
        assert_eq!(synopsis.event_chain.len(), 2);
        assert_eq!(synopsis.ending_state.mood, "紧张");
        assert_eq!(synopsis.pending_hooks, vec!["古剑上的神秘符文"]);
    }

    #[tokio::test]
    async fn heuristic_generator_uses_paragraphs() {
        let g = HeuristicSynopsisGenerator;
        let content = "第一段描述。\n第二段过渡。\n第三段收尾。";
        let synopsis = g.generate(content, 2, "测试章").await.expect("generate");
        assert_eq!(synopsis.chapter_number, 2);
        assert_eq!(synopsis.ending_state.mood, "neutral");
        assert!(synopsis.event_chain.is_empty());
        assert_eq!(synopsis.key_details.len(), 2);
        assert_eq!(synopsis.key_details[0], "第一段描述。");
        assert_eq!(synopsis.key_details[1], "第三段收尾。");
    }

    #[tokio::test]
    async fn heuristic_generator_handles_empty_content() {
        let g = HeuristicSynopsisGenerator;
        let synopsis = g.generate("", 3, "空白章").await.expect("generate");
        assert!(synopsis.key_details.is_empty());
        assert_eq!(synopsis.ending_state.mood, "neutral");
    }

    #[test]
    fn render_prompt_substitutes_fields() {
        let prompt = LlmSynopsisGenerator::render_prompt(5, "测试章", "内容");
        assert!(prompt.contains("第5章"));
        assert!(prompt.contains("测试章"));
        assert!(prompt.contains("内容"));
    }
}
