use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Chapter {
    pub id: Uuid,
    pub title: String,
    pub number: u32,
    pub outline: Option<String>,
    pub content: Option<String>,
    pub status: ChapterStatus,
    pub node_type: NarrativeNodeType,
    pub emotional_valence: f32,
    pub tension: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum ChapterStatus {
    Planned,
    Outlined,
    Drafting,
    Completed,
    Published,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum NarrativeNodeType {
    Hook,
    Setup,
    Conflict,
    Climax,
    Twist,
    Payoff,
    Exposition,
    Resolution,
}
