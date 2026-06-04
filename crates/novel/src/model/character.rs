use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Character {
    pub name: String,
    pub aliases: Vec<String>,
    pub age: Option<u32>,
    pub appearance: Option<String>,
    pub personality: Option<String>,
    pub background: Option<String>,
    pub goals: Vec<String>,
    pub relationships: Vec<CharacterRelationship>,
    pub status: CharacterStatus,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum CharacterStatus {
    Alive,
    Dead,
    Missing,
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CharacterRelationship {
    pub target_name: String,
    pub relation_type: String,
    pub description: Option<String>,
}
