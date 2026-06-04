use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::chapter::NarrativeNodeType;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct NarrativeGraph {
    pub nodes: Vec<NarrativeNode>,
    pub arcs: Vec<StoryArc>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NarrativeNode {
    pub id: Uuid,
    pub title: String,
    pub node_type: NarrativeNodeType,
    pub chapter_ref: Option<Uuid>,
    pub emotional_valence: f32,
    pub tension: f32,
}

impl NarrativeNode {
    /// Build a new node. Convenience constructor for skeleton generators.
    pub fn new(
        id: Uuid,
        title: String,
        node_type: NarrativeNodeType,
        chapter_ref: Option<Uuid>,
        emotional_valence: f32,
        tension: f32,
    ) -> Self {
        Self {
            id,
            title,
            node_type,
            chapter_ref,
            emotional_valence,
            tension,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StoryArc {
    pub id: Uuid,
    pub name: String,
    pub arc_type: ArcType,
    pub nodes: Vec<Uuid>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum ArcType {
    MainLine,
    SideQuest,
    CharacterArc,
    Filler,
}
