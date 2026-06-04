//! Integration tests for novel tools.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::json;

use novel::synopsis::SynopsisStore;
use novel::tools::write::NovelWriteTool;
use novel::tools::{ToolContext, ToolError, ToolSpec};

fn unique_workspace() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let base = std::env::temp_dir();
    let dir = base.join(format!(
        "novel-write-test-{}-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
        n
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[tokio::test]
async fn write_persists_chapter_file_and_synopsis() {
    let workspace = unique_workspace();
    let tool = NovelWriteTool;
    let context = ToolContext::for_workspace(&workspace);

    let content = "设定: 这个世界没有电。\n\n张三说道：“我们来比试一下。”\n李四问：“你要如何出手？”\n张三笑着回答。\n\n经过一番激战，张三最终获胜。\n";

    let input = json!({
        "chapter_number": 1,
        "title": "初入山谷",
        "content": content,
        "node_type": "Hook",
        "target_words": 3000,
    });

    let result = tool.execute(input, &context).await.expect("execute");
    assert!(result.success);

    // 章节文件应该存在
    let chapter_path = workspace.join("chapters").join("ch_001_初入山谷.md");
    assert!(chapter_path.exists(), "章节文件应存在");

    // 概要应已存储
    let store = SynopsisStore::load_all(&workspace).expect("load synopses");
    let syn = store.get(1).expect("get").expect("present");
    assert_eq!(syn.chapter_number, 1);
    assert_eq!(syn.chapter_title, "初入山谷");
    assert!(!syn.key_details.is_empty(), "key details 应被填充");

    // 事实应已落盘
    let facts_path = workspace.join(".novelwhale").join("facts.json");
    assert!(facts_path.exists(), "facts.json 应存在");
    let raw = std::fs::read_to_string(&facts_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let chars = v["characters"].as_array().unwrap();
    let names: Vec<&str> = chars
        .iter()
        .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
        .collect();
    assert!(names.contains(&"张三"));
    assert!(names.contains(&"李四"));

    let _ = std::fs::remove_dir_all(&workspace);
}

#[tokio::test]
async fn write_rejects_empty_chapter_number() {
    let workspace = unique_workspace();
    let tool = NovelWriteTool;
    let context = ToolContext::for_workspace(&workspace);

    let input = json!({
        "chapter_number": 0,
        "title": "no_number",
        "content": "abc",
    });
    let err = tool.execute(input, &context).await.unwrap_err();
    match err {
        ToolError::ExecutionFailed { .. } | ToolError::InvalidInput { .. } => {}
        other => panic!("unexpected error: {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&workspace);
}

#[tokio::test]
async fn write_rejects_empty_content() {
    let workspace = unique_workspace();
    let tool = NovelWriteTool;
    let context = ToolContext::for_workspace(&workspace);

    let input = json!({
        "chapter_number": 1,
        "title": "blank",
        "content": "  \n   \n",
    });
    let err = tool.execute(input, &context).await.unwrap_err();
    match err {
        ToolError::ExecutionFailed { .. } | ToolError::InvalidInput { .. } => {}
        other => panic!("unexpected error: {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&workspace);
}
