//! `/novel` 命令族 — TUI 弹窗视图的入口。
//!
//! 一共三个子命令：
//!
//! - `tree`       — 打开叙事树视图（`/novel tree`）
//! - `characters` — 打开人物列表视图（`/novel characters`）
//! - `progress`   — 打开章节进度视图（`/novel progress`）
//!
//! 这三个子命令都通过返回 [`crate::commands::CommandResult`] 中的
//! [`crate::tui::app::AppAction::OpenNovelTreeView`] / `OpenNovelCharactersView` /
//! `OpenNovelProgressView`，由 [`crate::tui::ui`] 在收到 Action 后实际把视图
//! 压入 [`crate::tui::app::App::view_stack`]。
//!
//! 这层包装使得命令注册（`COMMANDS` 数组）和视图实现完全解耦：将来如果
//! 接入 CLI-only 模式，只要在 `commands::execute` 分支里走另一条路即可。

use super::CommandResult;
use crate::tui::app::{App, AppAction};

/// `/novel` 命令族的统一入口。
///
/// 解析子命令并产出对应的 `AppAction`；子命令不匹配时返回错误信息。
pub fn novel(_app: &mut App, arg: Option<&str>) -> CommandResult {
    let Some(arg) = arg else {
        return CommandResult::error(
            "缺少子命令。试试 `/novel tree` / `/novel characters` / `/novel progress`。",
        );
    };
    let sub = arg.trim().split_whitespace().next().unwrap_or("");
    match sub {
        "tree" | "shu" | "树" => CommandResult::action(AppAction::OpenNovelTreeView),
        "characters" | "c" | "renwu" | "人物" => {
            CommandResult::action(AppAction::OpenNovelCharactersView)
        }
        "progress" | "p" | "jindu" | "进度" => {
            CommandResult::action(AppAction::OpenNovelProgressView)
        }
        _ => CommandResult::error(format!(
            "未知的 /novel 子命令 `{sub}`。可用：tree / characters / progress。"
        )),
    }
}
