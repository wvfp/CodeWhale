//! Regex/heuristic fact extractor for chapter text.
//!
//! The MVP implementation is intentionally lightweight: it scans chapter
//! content for common Chinese web-novel patterns and pushes candidate facts
//! into a [`FactStore`]. It is designed to be replaced or complemented by an
//! LLM-backed extractor later — the trait is the stable interface.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use crate::model::character::Character;
use crate::model::world::{WorldCategory, WorldSetting};

/// Errors returned by the fact extractor.
#[derive(Debug, Error)]
pub enum FactError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, FactError>;

/// Subdirectory of the project root where the fact store lives.
pub const FACTS_PATH: &str = ".novelwhale/facts.json";

/// What the extractor found in the chapter.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ExtractedFacts {
    /// Newly introduced characters (heuristic, may overlap with existing).
    pub characters: Vec<Character>,
    /// New world rules / settings (heuristic).
    pub world_rules: Vec<WorldSetting>,
}

impl ExtractedFacts {
    pub fn is_empty(&self) -> bool {
        self.characters.is_empty() && self.world_rules.is_empty()
    }
}

/// Regex/heuristic fact extractor.
pub struct FactExtractor;

impl FactExtractor {
    /// Extract candidate characters from the chapter text.
    ///
    /// The MVP detector is intentionally simple: it finds quoted Chinese
    /// names of the form `“张三说道”` / `「张三道」` and the canonical
    /// `张三` / `张君山` / `欧阳锋` style 2-3 character names mentioned with
    /// the verbs `说`, `道`, `笑`, `怒`, `喝`, `问`, `答`.
    pub fn extract_characters(content: &str) -> Vec<Character> {
        use std::collections::BTreeMap;
        let mut hits: BTreeMap<String, u32> = BTreeMap::new();

        // Split the text by Chinese/ASCII punctuation and quotes so we can
        // inspect each clause independently.
        let clauses: Vec<&str> = content
            .split(|c: char| {
                matches!(c,
                    '。' | '，' | '、' | '；' | '：' | '！' | '？'
                    | '"' | '「' | '」'
                    | '“' | '”' | '‘' | '’'
                    | '(' | ')' | '（' | '）' | '\n' | '\r'
                )
            })
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();

        for clause in clauses {
            if let Some(name) = take_name_before_verb(clause) {
                *hits.entry(name.to_string()).or_insert(0) += 1;
            }
        }

        hits.into_iter()
            .filter(|(_, count)| *count >= 1)
            .map(|(name, _)| Character {
                aliases: vec![],
                age: None,
                appearance: None,
                personality: None,
                background: None,
                goals: vec![],
                relationships: vec![],
                status: crate::model::character::CharacterStatus::Alive,
                name,
            })
            .collect()
    }

    /// Extract candidate world rules. Looks for `设定:` / `规则:` style
    /// explicit declarations, plus the "这个世界……" pattern.
    pub fn extract_world_rules(content: &str) -> Vec<WorldSetting> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for raw in content.lines() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = strip_prefix(line, "设定:") {
                push_rule(&mut out, &mut seen, rest, WorldCategory::Other, None);
            } else if let Some(rest) = strip_prefix(line, "设定：") {
                push_rule(&mut out, &mut seen, rest, WorldCategory::Other, None);
            } else if let Some(rest) = strip_prefix(line, "规则:") {
                push_rule(&mut out, &mut seen, rest, WorldCategory::Other, None);
            } else if let Some(rest) = strip_prefix(line, "规则：") {
                push_rule(&mut out, &mut seen, rest, WorldCategory::Other, None);
            } else if line.contains("这个世界") && line.contains(['。', '.']) {
                if let Some(rule) = line.split(['。', '.']).next() {
                    push_rule(&mut out, &mut seen, rule, WorldCategory::Other, None);
                }
            }
        }

        out
    }

    /// Convenience: extract both characters and world rules at once.
    pub fn extract(content: &str) -> ExtractedFacts {
        ExtractedFacts {
            characters: Self::extract_characters(content),
            world_rules: Self::extract_world_rules(content),
        }
    }

    /// Merge extracted facts into the project's `facts.json`.
    ///
    /// Existing facts are preserved; only new names/rules are appended. New
    /// world rules are non-immutable by default; the user can lock them later
    /// via the `novel_fact_lock` tool.
    pub fn persist_to(
        project_root: &Path,
        chapter_number: u32,
        facts: &ExtractedFacts,
    ) -> Result<()> {
        let path = project_root.join(FACTS_PATH);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut store: Value = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            serde_json::from_str(&raw).unwrap_or_else(|_| empty_store())
        } else {
            empty_store()
        };

        // Append characters
        if let Some(chars) = store.get_mut("characters").and_then(|v| v.as_array_mut()) {
            for ch in &facts.characters {
                if !chars.iter().any(|c| c.get("name").and_then(|n| n.as_str()) == Some(ch.name.as_str())) {
                    chars.push(json!({
                        "name": ch.name,
                        "status": "alive",
                        "chapter_introduced": chapter_number,
                    }));
                }
            }
        }

        // Append world rules
        if let Some(rules) = store.get_mut("world_rules").and_then(|v| v.as_array_mut()) {
            for r in &facts.world_rules {
                if !rules.iter().any(|existing| existing.get("rule").and_then(|v| v.as_str()) == Some(r.value.as_str())) {
                    rules.push(json!({
                        "id": slugify(&r.value),
                        "rule": r.value,
                        "category": category_label(&r.category),
                        "source_chapter": chapter_number,
                        "immutable": r.immutable,
                    }));
                }
            }
        }

        if let Some(obj) = store.as_object_mut() {
            obj.insert("last_modified".to_string(), json!(chrono::Utc::now()));
            let entry = obj
                .entry("extraction_chapters".to_string())
                .or_insert_with(|| json!(Vec::<u32>::new()));
            if let Some(arr) = entry.as_array_mut() {
                if !arr.iter().any(|v| v.as_u64() == Some(chapter_number as u64)) {
                    arr.push(json!(chapter_number));
                }
            }
        }

        std::fs::write(&path, serde_json::to_string_pretty(&store)?)?;
        Ok(())
    }
}

fn empty_store() -> Value {
    json!({
        "characters": [],
        "events": [],
        "world_rules": [],
        "last_modified": chrono::Utc::now(),
        "extraction_chapters": [],
    })
}

fn strip_prefix<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.starts_with(prefix) {
        Some(s[prefix.len()..].trim())
    } else {
        None
    }
}

fn push_rule(
    out: &mut Vec<WorldSetting>,
    seen: &mut std::collections::HashSet<String>,
    value: &str,
    category: WorldCategory,
    source_chapter: Option<u32>,
) {
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    let key = slugify(value);
    if !seen.insert(key.clone()) {
        return;
    }
    out.push(WorldSetting {
        key,
        value: value.to_string(),
        category,
        source_chapter,
        immutable: false,
    });
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

/// Look at the tail of `prefix` for a Chinese name (2 or 3 characters) sitting
/// immediately before a speaking verb we care about.
fn take_name_before_verb(text: &str) -> Option<&str> {
    let verbs = ['说', '道', '笑', '怒', '喝', '问', '答', '叫', '喊', '叹'];
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for (i, (_, c)) in chars.iter().enumerate() {
        if verbs.contains(c) {
            let prefix_end = chars.get(i).map(|(idx, _)| *idx).unwrap_or(text.len());
            let prefix = &text[..prefix_end];
            return last_chinese_name(prefix);
        }
    }
    None
}

/// Walk back from the end of `text` and return the longest run of Chinese
/// ideographs (2-3 chars) found, if any.
fn last_chinese_name(text: &str) -> Option<&str> {
    let trimmed = text.trim_end();
    if trimmed.is_empty() {
        return None;
    }
    let chars: Vec<(usize, char)> = trimmed.char_indices().collect();
    let mut start_byte = trimmed.len();
    let mut count = 0;
    for (idx, c) in chars.iter().rev() {
        if is_cjk(*c) {
            start_byte = *idx;
            count += 1;
            if count >= 3 {
                break;
            }
        } else {
            break;
        }
    }
    if (2..=3).contains(&count) {
        Some(&trimmed[start_byte..])
    } else {
        None
    }
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF
        | 0x3400..=0x4DBF
        | 0x20000..=0x2A6DF
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_chinese_names_before_speech_verbs() {
        let text = "张三说道：“此事不妥”。李四问：“何以见得？”";
        let chars = FactExtractor::extract_characters(text);
        let names: Vec<&str> = chars.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"张三"));
        assert!(names.contains(&"李四"));
    }

    #[test]
    fn extracts_world_rules_from_setting_marker() {
        let text = "设定: 这个世界没有电。\n设定: 修士可以御剑飞行。";
        let rules = FactExtractor::extract_world_rules(text);
        assert!(rules.iter().any(|r| r.value.contains("没有电")));
        assert!(rules.iter().any(|r| r.value.contains("御剑飞行")));
        assert!(rules.iter().all(|r| !r.immutable));
    }

    #[test]
    fn extract_handles_chinese_period() {
        let text = "这个世界没有魔法。";
        let rules = FactExtractor::extract_world_rules(text);
        assert_eq!(rules.len(), 1);
        assert!(rules[0].value.contains("没有魔法"));
    }

    #[test]
    fn persist_appends_new_facts_and_dedupes() {
        let tmp = std::env::temp_dir().join(format!(
            "novel-fact-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::fs::create_dir_all(&tmp).unwrap();

        let facts = ExtractedFacts {
            characters: vec![Character {
                name: "赵五".to_string(),
                aliases: vec![],
                age: None,
                appearance: None,
                personality: None,
                background: None,
                goals: vec![],
                relationships: vec![],
                status: crate::model::character::CharacterStatus::Alive,
            }],
            world_rules: vec![WorldSetting {
                key: "no_magic".to_string(),
                value: "这个世界没有魔法".to_string(),
                category: WorldCategory::Other,
                source_chapter: Some(1),
                immutable: false,
            }],
        };

        FactExtractor::persist_to(&tmp, 1, &facts).unwrap();
        FactExtractor::persist_to(&tmp, 2, &facts).unwrap(); // second call should be a no-op for dups

        let raw = std::fs::read_to_string(tmp.join(FACTS_PATH)).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["characters"].as_array().unwrap().len(), 1);
        assert_eq!(v["world_rules"].as_array().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
