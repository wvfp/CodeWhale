use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum CreationStage {
    Concept,
    Outline,
    Detail,
    Draft,
    Polish,
}

#[derive(Error, Debug, PartialEq)]
pub enum StageTransitionError {
    #[error("cannot transition from {from:?} to {to:?}")]
    InvalidTransition { from: CreationStage, to: CreationStage },
}

impl Default for CreationStage {
    fn default() -> Self {
        Self::Concept
    }
}

impl CreationStage {
    pub fn can_transition_to(&self, next: &CreationStage) -> bool {
        match (self, next) {
            (CreationStage::Concept, CreationStage::Outline) => true,
            (CreationStage::Outline, CreationStage::Detail) => true,
            (CreationStage::Detail, CreationStage::Draft) => true,
            (CreationStage::Draft, CreationStage::Polish) => true,
            (s1, s2) if s1 == s2 => true,
            _ => false,
        }
    }

    pub fn try_transition(&self, next: CreationStage) -> Result<CreationStage, StageTransitionError> {
        if self.can_transition_to(&next) {
            Ok(next)
        } else {
            Err(StageTransitionError::InvalidTransition {
                from: self.clone(),
                to: next,
            })
        }
    }
}
