//! Structured fact database.
//!
//! The MVP stores facts in `.novelwhale/facts.json` and uses a regex/heuristic
//! extractor to bootstrap entries from chapter text. Immutable rules are
//! locked via the `novel_fact_lock` tool.

pub mod extractor;

pub use extractor::{ExtractedFacts, FactError, FactExtractor, Result as FactResult, FACTS_PATH};
