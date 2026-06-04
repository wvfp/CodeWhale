use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NovelProject {
    pub title: String,
    pub genre: Genre,
    pub target_platform: TargetPlatform,
    pub style_notes: Option<String>,
    /// Total target word count for the novel (used for pace planning).
    pub target_word_count: u64,
    /// Author pen name (optional).
    pub author: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Genre {
    Xianxia,
    Urban,
    SciFi,
    Fantasy,
    Historical,
    Romance,
    Suspense,
    Other,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum TargetPlatform {
    Qidian,
    Zongheng,
    Fanqie,
    Jinjiang,
    Web,
    Other,
}
