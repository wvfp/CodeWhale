use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json;
use thiserror::Error;

use super::generator::ChapterSynopsis;

/// Error type for [`SynopsisStore`].
#[derive(Debug, Error)]
pub enum SynopsisError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("failed to serialize synopsis: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("invalid chapter number {0}: must be greater than zero")]
    InvalidChapterNumber(u32),
}

pub type Result<T> = std::result::Result<T, SynopsisError>;

/// Subdirectory of the project root where per-chapter synopses live.
pub const SYNOPSIS_DIR: &str = ".novelwhale/synopses";

/// Filename for a given chapter number, e.g. `ch_007.json`.
pub fn synopsis_filename(chapter_number: u32) -> String {
    format!("ch_{:03}.json", chapter_number)
}

/// File-based synopsis store: one JSON file per chapter under
/// `<project_root>/.novelwhale/synopses/ch_NNN.json`.
#[derive(Debug, Default, Clone)]
pub struct SynopsisStore {
    synopses: BTreeMap<u32, ChapterSynopsis>,
}

impl SynopsisStore {
    /// Create an empty in-memory store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace a synopsis for its chapter.
    pub fn add(&mut self, synopsis: ChapterSynopsis) -> Result<()> {
        if synopsis.chapter_number == 0 {
            return Err(SynopsisError::InvalidChapterNumber(synopsis.chapter_number));
        }
        self.synopses.insert(synopsis.chapter_number, synopsis);
        Ok(())
    }

    /// Look up a synopsis by chapter number.
    pub fn get(&self, chapter_number: u32) -> Result<Option<ChapterSynopsis>> {
        Ok(self.synopses.get(&chapter_number).cloned())
    }

    /// All chapter numbers that have a synopsis, in ascending order.
    pub fn list(&self) -> Vec<u32> {
        self.synopses.keys().copied().collect()
    }

    /// Iterate over `(chapter_number, &synopsis)` pairs in ascending chapter order.
    pub fn synopses(&self) -> impl Iterator<Item = (u32, &ChapterSynopsis)> {
        self.synopses.iter().map(|(n, s)| (*n, s))
    }

    /// The most recent `n` synopses, ordered by `generated_at` descending.
    pub fn recent(&self, n: usize) -> Vec<&ChapterSynopsis> {
        let mut values: Vec<&ChapterSynopsis> = self.synopses.values().collect();
        values.sort_by(|a, b| b.generated_at.cmp(&a.generated_at));
        values.into_iter().take(n).collect()
    }

    /// Number of synopses currently held in memory.
    pub fn len(&self) -> usize {
        self.synopses.len()
    }

    /// Whether the store has no synopses.
    pub fn is_empty(&self) -> bool {
        self.synopses.is_empty()
    }

    /// Resolve the synopses directory for a project root.
    pub fn dir_for(project_root: &Path) -> PathBuf {
        project_root.join(SYNOPSIS_DIR)
    }

    /// Persist every in-memory synopsis to `<project_root>/.novelwhale/synopses/`.
    pub fn save_all(&self, project_root: &Path) -> Result<()> {
        let dir = Self::dir_for(project_root);
        fs::create_dir_all(&dir)?;
        for (chapter_number, synopsis) in &self.synopses {
            let path = dir.join(synopsis_filename(*chapter_number));
            let json = serde_json::to_string_pretty(synopsis)?;
            fs::write(path, json)?;
        }
        Ok(())
    }

    /// Load every synopsis under `<project_root>/.novelwhale/synopses/`.
    pub fn load_all(project_root: &Path) -> Result<Self> {
        let dir = Self::dir_for(project_root);
        let mut synopses: BTreeMap<u32, ChapterSynopsis> = BTreeMap::new();

        if !dir.exists() {
            return Ok(Self { synopses });
        }

        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if !file_name.starts_with("ch_") {
                continue;
            }
            let bytes = fs::read(&path)?;
            let synopsis: ChapterSynopsis = serde_json::from_slice(&bytes)?;
            if synopsis.chapter_number == 0 {
                return Err(SynopsisError::InvalidChapterNumber(synopsis.chapter_number));
            }
            synopses.insert(synopsis.chapter_number, synopsis);
        }

        Ok(Self { synopses })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synopsis::generator::{ChapterSynopsis, EndingState};
    use chrono::Utc;

    fn sample(chapter_number: u32, title: &str) -> ChapterSynopsis {
        ChapterSynopsis {
            chapter_number,
            chapter_title: title.to_string(),
            event_chain: vec!["事件".to_string()],
            key_details: vec!["细节".to_string()],
            ending_state: EndingState {
                character_states: vec!["状态".to_string()],
                plot_point: "卡点".to_string(),
                location: "地点".to_string(),
                mood: "neutral".to_string(),
            },
            continuation_notes: vec!["续写".to_string()],
            pending_hooks: vec!["伏笔".to_string()],
            generated_at: Utc::now(),
        }
    }

    #[test]
    fn add_and_get_roundtrip() {
        let mut store = SynopsisStore::new();
        store.add(sample(1, "章一")).unwrap();
        let got = store.get(1).unwrap().expect("present");
        assert_eq!(got.chapter_title, "章一");
    }

    #[test]
    fn list_returns_sorted_chapter_numbers() {
        let mut store = SynopsisStore::new();
        store.add(sample(2, "b")).unwrap();
        store.add(sample(1, "a")).unwrap();
        store.add(sample(3, "c")).unwrap();
        assert_eq!(store.list(), vec![1, 2, 3]);
    }

    #[test]
    fn recent_returns_n_most_recent() {
        let mut store = SynopsisStore::new();
        for n in 1..=5 {
            store.add(sample(n, &format!("章{}", n))).unwrap();
        }
        let recent = store.recent(2);
        assert_eq!(recent.len(), 2);
    }

    #[test]
    fn rejects_zero_chapter_number() {
        let mut store = SynopsisStore::new();
        let err = store.add(sample(0, "空章")).unwrap_err();
        matches!(err, SynopsisError::InvalidChapterNumber(0));
    }

    #[test]
    fn save_and_load_roundtrip() {
        let tmp = tempdir();
        let mut store = SynopsisStore::new();
        store.add(sample(1, "章一")).unwrap();
        store.add(sample(2, "章二")).unwrap();
        store.save_all(&tmp).unwrap();

        let loaded = SynopsisStore::load_all(&tmp).unwrap();
        assert_eq!(loaded.list(), vec![1, 2]);
        let first = loaded.get(1).unwrap().expect("present");
        assert_eq!(first.chapter_title, "章一");
    }

    #[test]
    fn load_all_returns_empty_when_dir_missing() {
        let tmp = tempdir();
        let loaded = SynopsisStore::load_all(&tmp).unwrap();
        assert!(loaded.is_empty());
    }

    fn tempdir() -> PathBuf {
        let base = std::env::temp_dir();
        let unique = format!(
            "novel-synopsis-test-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        );
        let dir = base.join(unique);
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
