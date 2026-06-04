pub mod agents;
pub mod consistency;
pub mod deai;
pub mod facts;
pub mod genre;
pub mod model;
pub mod rag;
pub mod synopsis;
pub mod tools;

pub use agents::{
    AgentId, AgentRole, AgentTurn, Pipeline, PipelineInput, PipelinePhase, PipelineResult,
    PipelineStep, PIPELINE_PATH,
};
pub use consistency::{
    ConsistencyChecker, ConsistencyError, ConsistencyIssue, ConsistencyReport, FactCharacter,
    FactStore, FactWorldRule, IssueCategory, IssueStats, Result as ConsistencyResult,
    report_to_json,
};
pub use deai::{
    ClicheCategory, ClicheDictionary, ClicheEntry, ClicheHit, ChapterAiScore, DeAiError,
    DeAiResult, Suggestion, build_suggestions, score_chapter,
};
pub use facts::{ExtractedFacts, FactError, FactExtractor, FactResult, FACTS_PATH};
pub use genre::{
    ArcPhase, ArcTemplate, ClichéBlacklist, ClichéMatch, GenreRule, GenreTemplate, PaceTarget,
    RuleSeverity, SceneMix,
};
pub use model::*;
pub use rag::{
    KeywordRetriever, RagDocument, RagError, RagHit, RagIndex, RagIndexer, RagMeta, RagResult,
    RetrievalBackend, RetrievalQuery, Retriever, RetrieverError, RAG_INDEX_PATH, keyword_search,
};
pub use synopsis::*;
pub use tools::*;
