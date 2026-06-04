//! 子 Agent 的系统提示词集中放在 `prompts/` 子目录下。
//!
//! 用 `AgentRole::system_prompt()` 在运行时通过 `include_str!` 加载。

// 这个文件本身只是为了让 `pub mod prompts;` 在 Rust 2018 之下不会报「找不到
// `mod prompts`」。具体的 .md 文件由子目录 `prompts/*.md` 提供。
