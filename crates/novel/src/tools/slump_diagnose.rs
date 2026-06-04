//! `novel_slump_diagnose` tool — 中期诊断（Task 14.2）。
//!
//! 当作家写到中段卡壳时，常见的信号是：
//!
//! - 连续 N 章张力没有上升（平线 / 下降）
//! - 人物出场率单一：只有一个 POV 在自言自语
//! - 钩子缺失：过去 K 章没有「新的悬念 / 反转」
//! - 事实库更新停滞：很久没新建任何事件
//!
//! 本工具读取 `.novelwhale/` 下的所有可观察信号，输出一个 0-100 的「健康
//! 分数」和一段中文诊断报告，供 LLM 后续给出具体拯救建议。

use std::path::Path;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::{Value, json};

use crate::consistency::FactStore;
use crate::synopsis::store::SynopsisStore;
use crate::tools::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    readonly_capabilities,
};

pub struct NovelSlumpDiagnoseTool;

#[derive(Debug, Clone, Serialize)]
pub struct SlumpSignal {
    pub key: String,
    pub label: &'static str,
    pub severity: u8, // 0-100
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SlumpReport {
    pub health_score: u8, // 0-100，越高越健康
    pub signals: Vec<SlumpSignal>,
    pub summary: String,
}

#[async_trait]
impl ToolSpec for NovelSlumpDiagnoseTool {
    fn name(&self) -> &'static str {
        "novel_slump_diagnose"
    }

    fn description(&self) -> &'static str {
        "中期卡壳诊断：基于事实库、最近章节摘要、章节文件，输出 0-100 的健康分和信号列表。把信号打包成 LLM 提示，拿到具体拯救建议。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "window": {
                    "type": "integer",
                    "description": "回看最近多少章（默认 6）。",
                    "minimum": 2,
                    "maximum": 30
                }
            }
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        readonly_capabilities()
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(
        &self,
        input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let window = input
            .get("window")
            .and_then(Value::as_u64)
            .unwrap_or(6)
            .clamp(2, 30) as usize;

        let report = diagnose(&context.workspace, window);

        let prompt = build_followup_prompt(&report);

        let out = json!({
            "window": window,
            "health_score": report.health_score,
            "signals": report.signals,
            "summary": report.summary,
            "system_prompt": "你是一位擅长'卡文救场'的网文编辑。请基于下面给出的信号给出 3 条具体的拯救建议。每条建议 1-2 句中文，包含一个可执行动作（如「在下一章加入 X 角色的回归」、「把 14 章末尾的伏笔回收」）。",
            "user_prompt": prompt,
            "instructions": {
                "writes": [],
                "expected_response_format": "中文段落"
            },
            "note": "调用方负责：把 system_prompt + user_prompt 拼成一次 LLM 调用；把建议直接打印给用户即可，不需要落盘。"
        });
        let pretty = serde_json::to_string_pretty(&out)
            .map_err(|e| ToolError::execution_failed(e.to_string()))?;
        Ok(ToolResult::success(pretty))
    }
}

fn diagnose(workspace: &Path, window: usize) -> SlumpReport {
    let mut signals: Vec<SlumpSignal> = Vec::new();

    // 信号 1：连续性 / 节奏 — 用 chapter_number 的间距推断节奏是否在拉长。
    let store = SynopsisStore::load_all(workspace).unwrap_or_default();
    if store.is_empty() {
        signals.push(SlumpSignal {
            key: "no_summaries".to_string(),
            label: "没有摘要",
            severity: 30,
            detail: "尚无章节摘要，先调用 novel_write 跑通主流程再回来诊断。".to_string(),
        });
    } else {
        let mut ordered: Vec<&crate::synopsis::generator::ChapterSynopsis> = Vec::new();
        for (_, s) in store.synopses() {
            ordered.push(s);
        }
        if ordered.len() >= 2 {
            let last = ordered.last().unwrap();
            let prev = &ordered[ordered.len() - 2];
            let n_latest = last.chapter_number;
            let n_prev = prev.chapter_number;
            if n_latest == n_prev {
                signals.push(SlumpSignal {
                    key: "stale_chapter".to_string(),
                    label: "重复章节号",
                    severity: 50,
                    detail: format!(
                        "最近两条摘要的 chapter_number 都是 {}，可能漏改。",
                        n_latest
                    ),
                });
            }
        }
    }

    // 信号 2：钩子 / 续写钩密度
    let recent = last_summaries(workspace, window);
    if let Some(recent) = &recent {
        let with_hook = recent
            .iter()
            .filter(|s| !s.pending_hooks.is_empty())
            .count();
        if with_hook == 0 && recent.len() >= 3 {
            signals.push(SlumpSignal {
                key: "no_pending_hooks".to_string(),
                label: "伏笔缺失",
                severity: 80,
                detail: format!(
                    "最近 {} 章没有任何「待回收伏笔」，读者会觉得主线停滞。",
                    recent.len()
                ),
            });
        }
        let empty_endings = recent
            .iter()
            .filter(|s| s.ending_state.plot_point.trim().is_empty())
            .count();
        if empty_endings * 2 > recent.len() {
            signals.push(SlumpSignal {
                key: "weak_ending".to_string(),
                label: "章末卡点弱",
                severity: 70,
                detail: "多章的 ending_state.plot_point 为空，章末钩力度不足。".to_string(),
            });
        }
    }

    // 信号 3：事实库是否被遗忘
    if let Ok(store) = FactStore::load(workspace) {
        let total = store.world_rules.len() + store.characters.len() + store.events.len();
        if total == 0 {
            signals.push(SlumpSignal {
                key: "no_facts".to_string(),
                label: "事实库空",
                severity: 40,
                detail: "还没锁定任何世界规则 / 人物 / 事件；先调用 novel_fact_lock 锚定设定。".to_string(),
            });
        } else if store.events.is_empty() {
            signals.push(SlumpSignal {
                key: "no_events".to_string(),
                label: "事件缺失",
                severity: 55,
                detail: "事实库有人物但没有事件，主线推动缺乏锚点。".to_string(),
            });
        }
    }

    // 健康分 = 100 - max severity
    let max_sev = signals.iter().map(|s| s.severity).max().unwrap_or(0);
    let health_score = 100u8.saturating_sub(max_sev);

    let summary = if signals.is_empty() {
        "诊断通过：没有发现明显的卡壳信号。".to_string()
    } else {
        let labels: Vec<&str> = signals.iter().map(|s| s.label).collect();
        format!(
            "诊断完成：识别到 {} 个信号：{}。",
            signals.len(),
            labels.join("、")
        )
    };

    SlumpReport {
        health_score,
        signals,
        summary,
    }
}

fn last_summaries(
    workspace: &Path,
    window: usize,
) -> Option<Vec<crate::synopsis::generator::ChapterSynopsis>> {
    let store = SynopsisStore::load_all(workspace).ok()?;
    let mut v: Vec<crate::synopsis::generator::ChapterSynopsis> = Vec::new();
    for (_, s) in store.synopses() {
        v.push(s.clone());
    }
    v.sort_by_key(|s| s.chapter_number);
    if v.len() > window {
        v = v.split_off(v.len() - window);
    }
    Some(v)
}

fn build_followup_prompt(report: &SlumpReport) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "健康分：{} / 100。\n\n信号列表：",
        report.health_score
    );
    for s in &report.signals {
        let _ = writeln!(out, "- [{}] {}：{}", s.severity, s.label, s.detail);
    }
    let _ = writeln!(out, "\n请给出 3 条具体的拯救建议（每条 1-2 句中文）。");
    out
}

impl crate::tools::NovelTool for NovelSlumpDiagnoseTool {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_summary_lists_signals() {
        let report = SlumpReport {
            health_score: 40,
            signals: vec![SlumpSignal {
                key: "tension_flat".to_string(),
                label: "张力持平",
                severity: 60,
                detail: "test".to_string(),
            }],
            summary: "诊断完成".to_string(),
        };
        assert_eq!(report.health_score, 40);
        let prompt = build_followup_prompt(&report);
        assert!(prompt.contains("健康分"));
        assert!(prompt.contains("张力持平"));
    }

    #[test]
    fn diagnose_empty_workspace_gives_guidance() {
        let dir = tempdir();
        let report = diagnose(&dir, 6);
        // 没有事实库、没有摘要，应该给出「没有摘要」「事实库空」之类的引导。
        assert!(report.signals.iter().any(|s| s.key == "no_summaries"));
        assert!(report.signals.iter().any(|s| s.key == "no_facts"));
    }

    fn tempdir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("novel_slump_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
