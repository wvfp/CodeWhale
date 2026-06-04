use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorldSetting {
    pub key: String,
    pub value: String,
    pub category: WorldCategory,
    pub source_chapter: Option<u32>,
    pub immutable: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum WorldCategory {
    Geography,
    History,
    Culture,
    MagicSystem,
    Technology,
    Politics,
    Economy,
    Other,
}
