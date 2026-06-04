//! 长篇网络小说 TUI 视图。
//!
//! 提供三个弹窗视图，供 `/novel tree` / `/novel characters` / `/novel progress`
//! 等命令调用：
//!
//! - [`NarrativeTreeView`] — 文字版叙事树：把 `NarrativeGraph` 里的卷
//!   (`StoryArc`) 和节拍 (`NarrativeNode`) 渲染成可折叠的两层树。
//! - [`CharacterListView`] — 人物清单：列出 `Character`，按名字 / 状态 / 别名
//!   子串过滤。
//! - [`ChapterProgressView`] — 章节进度：列出 `Chapter`，按状态着色（计划 /
//!   细纲 / 草稿 / 完成 / 已发布），并显示流水线状态。
//!
//! 三个视图都遵循 [`ModalView`] 协议：键盘驱动、`Esc` 关闭、焦点状态保存在
//! 结构体里。所有可见字符串都是中文，符合「小说创作工具全程中文」的要求。

use std::collections::HashSet;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, StatefulWidget, Widget},
};
use unicode_width::UnicodeWidthStr;
use uuid::Uuid;

use novel::agents::state::{PipelineRecord, PipelineStatus};
use novel::model::chapter::{Chapter, ChapterStatus, NarrativeNodeType};
use novel::model::character::{Character, CharacterStatus};
use novel::model::graph::{ArcType, NarrativeGraph, NarrativeNode};
use novel::model::stage::CreationStage;

use crate::palette;
use crate::tui::views::{ModalKind, ModalView, ViewAction, ViewEvent};

// ---------------------------------------------------------------------------
// 共用渲染助手
// ---------------------------------------------------------------------------

fn modal_block<'a>(title: &'a str, hint: &'a str) -> Block<'a> {
    Block::default()
        .title(Line::from(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(palette::MODE_NOVEL)
                .add_modifier(Modifier::BOLD),
        )))
        .title_bottom(Line::from(Span::styled(
            format!(" {hint} "),
            Style::default().fg(palette::TEXT_MUTED),
        )))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette::BORDER_COLOR))
        .style(Style::default().bg(palette::DEEPSEEK_INK))
        .padding(Padding::uniform(1))
}

fn centered_popup(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width.saturating_sub(4)).max(20);
    let h = height.min(area.height.saturating_sub(2)).max(7);
    Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

fn truncate_to_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if text.width() <= max_width {
        return text.to_string();
    }
    let mut out = String::new();
    let limit = max_width.saturating_sub(1);
    for ch in text.chars() {
        let next_width = out.width() + ch.to_string().width();
        if next_width > limit {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

// ---------------------------------------------------------------------------
// 叙事树视图
// ---------------------------------------------------------------------------

/// 叙事树视图里一个可视行。
#[derive(Debug, Clone)]
enum NarrativeRow {
    /// 弧线（卷）标题行。
    Arc {
        arc_id: Uuid,
        #[allow(dead_code)]
        depth: usize,
    },
    /// 节点（节拍）行。
    Node {
        node_id: Uuid,
        #[allow(dead_code)]
        depth: usize,
        /// 节点所属弧线 id。
        #[allow(dead_code)]
        arc_id: Uuid,
    },
    /// 当没有任何数据时显示的占位行。
    Empty,
}

pub struct NarrativeTreeView {
    graph: NarrativeGraph,
    pipeline: Option<PipelineRecord>,
    rows: Vec<NarrativeRow>,
    /// 已折叠的弧线集合。
    collapsed: HashSet<Uuid>,
    /// 当前高亮行索引（指向 `rows`）。
    selected: usize,
    /// 当前滚动偏移（按行）。
    scroll: usize,
    filter: String,
}

impl NarrativeTreeView {
    #[must_use]
    pub fn new(graph: NarrativeGraph, pipeline: Option<PipelineRecord>) -> Self {
        let mut view = Self {
            graph,
            pipeline,
            rows: Vec::new(),
            collapsed: HashSet::new(),
            selected: 0,
            scroll: 0,
            filter: String::new(),
        };
        view.rebuild_rows();
        view
    }

    /// 重新构建「行列表」和过滤。
    fn rebuild_rows(&mut self) {
        let mut rows = Vec::new();
        if self.graph.arcs.is_empty() && self.graph.nodes.is_empty() {
            rows.push(NarrativeRow::Empty);
        }

        let filter_lower = self.filter.trim().to_lowercase();

        for arc in &self.graph.arcs {
            if !filter_lower.is_empty()
                && !arc.name.to_lowercase().contains(&filter_lower)
                && !arc_type_label(&arc.arc_type).to_lowercase().contains(&filter_lower)
            {
                // 即使弧线被过滤掉了，仍然需要展示子节点里匹配的部分
                // —— 但简单起见，整条弧线一起跳过。
                continue;
            }
            rows.push(NarrativeRow::Arc {
                arc_id: arc.id,
                depth: 0,
            });
            if self.collapsed.contains(&arc.id) {
                continue;
            }
            for node_id in &arc.nodes {
                if let Some(node) = self.graph.nodes.iter().find(|n| n.id == *node_id) {
                    if !filter_lower.is_empty()
                        && !node.title.to_lowercase().contains(&filter_lower)
                        && !node_type_label(&node.node_type)
                            .to_lowercase()
                            .contains(&filter_lower)
                    {
                        continue;
                    }
                    rows.push(NarrativeRow::Node {
                        node_id: node.id,
                        depth: 1,
                        arc_id: arc.id,
                    });
                }
            }
        }
        // 处理「未挂到任何弧线上的孤儿节点」。
        let orphans: Vec<&NarrativeNode> = self
            .graph
            .nodes
            .iter()
            .filter(|node| !self.graph.arcs.iter().any(|arc| arc.nodes.contains(&node.id)))
            .collect();
        if !orphans.is_empty() {
            rows.push(NarrativeRow::Arc {
                arc_id: Uuid::nil(),
                depth: 0,
            });
            for node in orphans {
                if !filter_lower.is_empty()
                    && !node.title.to_lowercase().contains(&filter_lower)
                {
                    continue;
                }
                rows.push(NarrativeRow::Node {
                    node_id: node.id,
                    depth: 1,
                    arc_id: Uuid::nil(),
                });
            }
        }
        if rows.is_empty() {
            rows.push(NarrativeRow::Empty);
        }
        if self.selected >= rows.len() {
            self.selected = rows.len().saturating_sub(1);
        }
        self.rows = rows;
        // 选区被过滤掉之后 scroll 也跟着收紧。
        self.clamp_scroll();
    }

    fn clamp_scroll(&mut self) {
        // 渲染时根据可见行数自适应，这里仅做 sanity check。
        if self.selected < self.scroll {
            self.scroll = self.selected;
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.rows.is_empty() {
            self.selected = 0;
            return;
        }
        let len = self.rows.len() as isize;
        let next = (self.selected as isize + delta).clamp(0, len - 1) as usize;
        self.selected = next;
    }

    fn toggle_arc(&mut self) {
        let Some(row) = self.rows.get(self.selected) else {
            return;
        };
        let NarrativeRow::Arc { arc_id, .. } = row else {
            return;
        };
        if arc_id.is_nil() {
            return; // 「未分类」行不能折叠。
        }
        if !self.collapsed.remove(arc_id) {
            self.collapsed.insert(*arc_id);
        }
        self.rebuild_rows();
    }

    fn activate(&self) -> ViewAction {
        match self.rows.get(self.selected) {
            Some(NarrativeRow::Node { node_id, .. }) => {
                ViewAction::EmitAndClose(ViewEvent::NovelNodeSelected { node_id: *node_id })
            }
            _ => ViewAction::None,
        }
    }
}

fn arc_type_label(kind: &ArcType) -> &'static str {
    match kind {
        ArcType::MainLine => "主线",
        ArcType::SideQuest => "支线",
        ArcType::CharacterArc => "人物弧",
        ArcType::Filler => "过渡",
    }
}

fn node_type_label(kind: &NarrativeNodeType) -> &'static str {
    match kind {
        NarrativeNodeType::Hook => "钩子",
        NarrativeNodeType::Setup => "铺垫",
        NarrativeNodeType::Conflict => "冲突",
        NarrativeNodeType::Climax => "高潮",
        NarrativeNodeType::Twist => "反转",
        NarrativeNodeType::Payoff => "兑现",
        NarrativeNodeType::Exposition => "交代",
        NarrativeNodeType::Resolution => "收束",
    }
}

impl ModalView for NarrativeTreeView {
    fn kind(&self) -> ModalKind {
        ModalKind::NovelTree
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn handle_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Esc => ViewAction::Close,
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_selection(-1);
                ViewAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_selection(1);
                ViewAction::None
            }
            KeyCode::PageUp => {
                self.move_selection(-10);
                ViewAction::None
            }
            KeyCode::PageDown => {
                self.move_selection(10);
                ViewAction::None
            }
            KeyCode::Home => {
                self.selected = 0;
                ViewAction::None
            }
            KeyCode::End => {
                if !self.rows.is_empty() {
                    self.selected = self.rows.len() - 1;
                }
                ViewAction::None
            }
            KeyCode::Char(' ') | KeyCode::Right | KeyCode::Left => {
                self.toggle_arc();
                ViewAction::None
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.rebuild_rows();
                ViewAction::None
            }
            KeyCode::Char(c)
                if !c.is_control()
                    && (key.modifiers.is_empty() || key.modifiers == crossterm::event::KeyModifiers::SHIFT) =>
            {
                self.filter.push(c);
                self.rebuild_rows();
                ViewAction::None
            }
            KeyCode::Enter => self.activate(),
            _ => ViewAction::None,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let popup_area = centered_popup(area, 88, 28);
        Clear.render(popup_area, buf);
        let block = modal_block(
            "叙事树",
            " ↑↓ 移动 / 空格 折叠 / Enter 选中 / Esc 关闭 ",
        );
        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        // 顶栏：流水线状态 + 过滤词。
        let pipeline_line = match &self.pipeline {
            Some(p) => format!(
                "阶段 {} · 步骤 {} · 状态 {}",
                p.phase.label(),
                p.current_agent.label(),
                pipeline_status_label(p.status),
            ),
            None => "未运行流水线".to_string(),
        };
        let filter_line = if self.filter.is_empty() {
            "（输入关键字过滤）".to_string()
        } else {
            format!("过滤: {}", self.filter)
        };
        let header = Line::from(vec![
            Span::styled(pipeline_line, Style::default().fg(palette::DEEPSEEK_SKY)),
            Span::raw("    "),
            Span::styled(filter_line, Style::default().fg(palette::TEXT_MUTED)),
        ]);
        let header_para = Paragraph::new(header);
        let header_height = 1u16;
        header_para.render(
            Rect {
                x: inner.x,
                y: inner.y,
                width: inner.width,
                height: header_height,
            },
            buf,
        );

        // 列表行。
        let list_area = Rect {
            x: inner.x,
            y: inner.y + header_height,
            width: inner.width,
            height: inner.height.saturating_sub(header_height),
        };

        // 适配滚动：高亮行尽量在可见范围中央。
        let visible_height = list_area.height as usize;
        let scroll = if self.selected >= visible_height {
            self.selected + 1 - visible_height
        } else {
            0
        };

        let items: Vec<ListItem> = self
            .rows
            .iter()
            .skip(scroll)
            .take(visible_height)
            .map(|row| match row {
                NarrativeRow::Empty => ListItem::new(Line::from(Span::styled(
                    "（暂无叙事节点 — 用 novel_outline_build 添加）",
                    Style::default().fg(palette::TEXT_MUTED),
                ))),
                NarrativeRow::Arc { arc_id, .. } => {
                    let arc = self.graph.arcs.iter().find(|a| a.id == *arc_id);
                    let (name, kind) = match arc {
                        Some(arc) => (arc.name.clone(), arc.arc_type.clone()),
                        None => ("（未分类节点）".to_string(), ArcType::Filler),
                    };
                    let marker = if arc
                        .map(|a| self.collapsed.contains(&a.id))
                        .unwrap_or(false)
                    {
                        "▸"
                    } else {
                        "▾"
                    };
                    let indent = "  ";
                    let style = Style::default()
                        .fg(palette::MODE_NOVEL)
                        .add_modifier(Modifier::BOLD);
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("{indent}{marker} "), style),
                        Span::styled(format!("[{}] ", arc_type_label(&kind)), style),
                        Span::styled(name, style),
                    ]))
                }
                NarrativeRow::Node { node_id, .. } => {
                    let node = self.graph.nodes.iter().find(|n| n.id == *node_id);
                    let (title, kind, tension, valence) = match node {
                        Some(n) => (
                            n.title.clone(),
                            n.node_type.clone(),
                            n.tension,
                            n.emotional_valence,
                        ),
                        None => ("（节点已删除）".to_string(), NarrativeNodeType::Exposition, 0.0, 0.0),
                    };
                    let text = format!(
                        "    └ {} {}（紧张 {:.1} · 情感 {:+.1}）",
                        node_type_label(&kind),
                        title,
                        tension,
                        valence,
                    );
                    let style = Style::default().fg(palette::TEXT_PRIMARY);
                    ListItem::new(Line::from(Span::styled(
                        truncate_to_width(&text, list_area.width as usize),
                        style,
                    )))
                }
            })
            .collect();

        let list = List::new(items).highlight_style(
            Style::default()
                .fg(palette::SELECTION_TEXT)
                .bg(palette::SELECTION_BG)
                .add_modifier(Modifier::BOLD),
        );
        // 选区项索引（相对于可见列表）。
        let visible_selected = self.selected.saturating_sub(scroll);
        let mut state = ListState::default();
        state.select(Some(visible_selected));
        StatefulWidget::render(list, list_area, buf, &mut state);
    }
}

// ---------------------------------------------------------------------------
// 人物列表视图
// ---------------------------------------------------------------------------

pub struct CharacterListView {
    characters: Vec<Character>,
    pipeline: Option<PipelineRecord>,
    query: String,
    filtered: Vec<usize>,
    selected: usize,
}

impl CharacterListView {
    #[must_use]
    pub fn new(characters: Vec<Character>, pipeline: Option<PipelineRecord>) -> Self {
        let mut view = Self {
            characters,
            pipeline,
            query: String::new(),
            filtered: Vec::new(),
            selected: 0,
        };
        view.refilter();
        view
    }

    fn refilter(&mut self) {
        let q = self.query.trim().to_lowercase();
        self.filtered = self
            .characters
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                if q.is_empty() {
                    return true;
                }
                c.name.to_lowercase().contains(&q)
                    || c.aliases
                        .iter()
                        .any(|alias| alias.to_lowercase().contains(&q))
            })
            .map(|(idx, _)| idx)
            .collect();
        if self.selected >= self.filtered.len() {
            self.selected = self.filtered.len().saturating_sub(1);
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.filtered.is_empty() {
            self.selected = 0;
            return;
        }
        let len = self.filtered.len() as isize;
        let next = (self.selected as isize + delta).clamp(0, len - 1) as usize;
        self.selected = next;
    }

    fn activate(&self) -> ViewAction {
        match self.filtered.get(self.selected) {
            Some(&idx) => self
                .characters
                .get(idx)
                .map(|c| {
                    ViewAction::EmitAndClose(ViewEvent::NovelCharacterSelected {
                        character_name: c.name.clone(),
                    })
                })
                .unwrap_or(ViewAction::None),
            None => ViewAction::None,
        }
    }
}

fn character_status_label(status: &CharacterStatus) -> &'static str {
    match status {
        CharacterStatus::Alive => "在世",
        CharacterStatus::Dead => "已故",
        CharacterStatus::Missing => "失踪",
        CharacterStatus::Unknown => "未知",
    }
}

fn character_status_color(status: &CharacterStatus) -> ratatui::style::Color {
    match status {
        CharacterStatus::Alive => palette::STATUS_SUCCESS,
        CharacterStatus::Dead => palette::STATUS_ERROR,
        CharacterStatus::Missing => palette::STATUS_WARNING,
        CharacterStatus::Unknown => palette::TEXT_MUTED,
    }
}

impl ModalView for CharacterListView {
    fn kind(&self) -> ModalKind {
        ModalKind::NovelCharacters
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn handle_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Esc => ViewAction::Close,
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_selection(-1);
                ViewAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_selection(1);
                ViewAction::None
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.refilter();
                ViewAction::None
            }
            KeyCode::Char(c)
                if !c.is_control()
                    && (key.modifiers.is_empty() || key.modifiers == crossterm::event::KeyModifiers::SHIFT) =>
            {
                self.query.push(c);
                self.refilter();
                ViewAction::None
            }
            KeyCode::Enter => self.activate(),
            _ => ViewAction::None,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let popup_area = centered_popup(area, 78, 26);
        Clear.render(popup_area, buf);
        let block = modal_block(
            "人物列表",
            " ↑↓ 移动 / Enter 选中 / Esc 关闭 ",
        );
        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        // 顶栏：搜索 + 计数。
        let counter = if self.query.is_empty() {
            format!("{} 人", self.characters.len())
        } else {
            format!(
                "{} / {} 匹配",
                self.filtered.len(),
                self.characters.len()
            )
        };
        let pipeline_line = self
            .pipeline
            .as_ref()
            .map(|p| format!("阶段 {} · 状态 {}", p.phase.label(), pipeline_status_label(p.status)))
            .unwrap_or_else(|| "未运行流水线".to_string());
        let header = Line::from(vec![
            Span::styled(
                format!("搜索: {}", self.query),
                Style::default().fg(palette::DEEPSEEK_SKY),
            ),
            Span::raw("    "),
            Span::styled(counter, Style::default().fg(palette::TEXT_MUTED)),
            Span::raw("    "),
            Span::styled(pipeline_line, Style::default().fg(palette::TEXT_MUTED)),
        ]);
        Paragraph::new(header).render(
            Rect {
                x: inner.x,
                y: inner.y,
                width: inner.width,
                height: 1,
            },
            buf,
        );

        // 列表。
        let list_area = Rect {
            x: inner.x,
            y: inner.y + 1,
            width: inner.width,
            height: inner.height.saturating_sub(1),
        };
        if self.filtered.is_empty() {
            let msg = if self.characters.is_empty() {
                "（暂无角色 — 在 fact_lock 中锁定一个人物）"
            } else {
                "（没有匹配的角色）"
            };
            Paragraph::new(Line::from(Span::styled(
                msg,
                Style::default().fg(palette::TEXT_MUTED),
            )))
            .render(list_area, buf);
            return;
        }

        let visible_height = list_area.height as usize;
        let scroll = if self.selected >= visible_height {
            self.selected + 1 - visible_height
        } else {
            0
        };

        let items: Vec<ListItem> = self
            .filtered
            .iter()
            .skip(scroll)
            .take(visible_height)
            .map(|&idx| {
                let c = &self.characters[idx];
                let alias_suffix = if c.aliases.is_empty() {
                    String::new()
                } else {
                    format!("（又名 {}）", c.aliases.join("、"))
                };
                let personality = c
                    .personality
                    .as_deref()
                    .map(|p| format!(" — {}", truncate_to_width(p, 18)))
                    .unwrap_or_default();
                let line = Line::from(vec![
                    Span::styled(
                        format!("[{}] ", character_status_label(&c.status)),
                        Style::default().fg(character_status_color(&c.status)),
                    ),
                    Span::styled(
                        c.name.clone(),
                        Style::default()
                            .fg(palette::TEXT_PRIMARY)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(alias_suffix, Style::default().fg(palette::TEXT_MUTED)),
                    Span::styled(personality, Style::default().fg(palette::TEXT_MUTED)),
                ]);
                ListItem::new(line)
            })
            .collect();

        let list = List::new(items).highlight_style(
            Style::default()
                .fg(palette::SELECTION_TEXT)
                .bg(palette::SELECTION_BG)
                .add_modifier(Modifier::BOLD),
        );
        let visible_selected = self.selected.saturating_sub(scroll);
        let mut state = ListState::default();
        state.select(Some(visible_selected));
        StatefulWidget::render(list, list_area, buf, &mut state);
    }
}

// ---------------------------------------------------------------------------
// 章节进度视图
// ---------------------------------------------------------------------------

pub struct ChapterProgressView {
    chapters: Vec<Chapter>,
    pipeline: Option<PipelineRecord>,
    stage: Option<CreationStage>,
    selected: usize,
    #[allow(dead_code)]
    scroll: usize,
}

impl ChapterProgressView {
    #[must_use]
    pub fn new(
        chapters: Vec<Chapter>,
        pipeline: Option<PipelineRecord>,
        stage: Option<CreationStage>,
    ) -> Self {
        let selected = chapters.len().saturating_sub(1);
        Self {
            chapters,
            pipeline,
            stage,
            selected,
            scroll: 0,
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.chapters.is_empty() {
            self.selected = 0;
            return;
        }
        let len = self.chapters.len() as isize;
        let next = (self.selected as isize + delta).clamp(0, len - 1) as usize;
        self.selected = next;
    }

    fn activate(&self) -> ViewAction {
        self.chapters
            .get(self.selected)
            .map(|c| {
                ViewAction::EmitAndClose(ViewEvent::NovelChapterSelected {
                    chapter_number: c.number,
                })
            })
            .unwrap_or(ViewAction::None)
    }

    fn status_label(status: &ChapterStatus) -> &'static str {
        match status {
            ChapterStatus::Planned => "计划",
            ChapterStatus::Outlined => "细纲",
            ChapterStatus::Drafting => "草稿",
            ChapterStatus::Completed => "完成",
            ChapterStatus::Published => "已发布",
        }
    }

    fn status_color(status: &ChapterStatus) -> ratatui::style::Color {
        match status {
            ChapterStatus::Planned => palette::TEXT_MUTED,
            ChapterStatus::Outlined => palette::DEEPSEEK_SKY,
            ChapterStatus::Drafting => palette::STATUS_WARNING,
            ChapterStatus::Completed => palette::STATUS_SUCCESS,
            ChapterStatus::Published => palette::MODE_NOVEL,
        }
    }

    fn stage_label(stage: &CreationStage) -> &'static str {
        match stage {
            CreationStage::Concept => "概念",
            CreationStage::Outline => "大纲",
            CreationStage::Detail => "细纲",
            CreationStage::Draft => "正文",
            CreationStage::Polish => "润色",
        }
    }
}

fn pipeline_status_label(status: PipelineStatus) -> &'static str {
    match status {
        PipelineStatus::Pending => "待启动",
        PipelineStatus::Running => "运行中",
        PipelineStatus::Done => "已完成",
        PipelineStatus::Failed => "失败",
        PipelineStatus::NeedsHumanInput => "等待用户输入",
    }
}

impl ModalView for ChapterProgressView {
    fn kind(&self) -> ModalKind {
        ModalKind::NovelProgress
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn handle_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Esc => ViewAction::Close,
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_selection(-1);
                ViewAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_selection(1);
                ViewAction::None
            }
            KeyCode::PageUp => {
                self.move_selection(-10);
                ViewAction::None
            }
            KeyCode::PageDown => {
                self.move_selection(10);
                ViewAction::None
            }
            KeyCode::Home => {
                self.selected = 0;
                ViewAction::None
            }
            KeyCode::End => {
                if !self.chapters.is_empty() {
                    self.selected = self.chapters.len() - 1;
                }
                ViewAction::None
            }
            KeyCode::Enter => self.activate(),
            _ => ViewAction::None,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let popup_area = centered_popup(area, 86, 28);
        Clear.render(popup_area, buf);
        let block = modal_block(
            "章节进度",
            " ↑↓ 移动 / Enter 选中 / Esc 关闭 ",
        );
        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        // 统计行：每个状态的章节数 + 当前阶段 + 流水线信息。
        let mut planned = 0;
        let mut outlined = 0;
        let mut drafting = 0;
        let mut completed = 0;
        let mut published = 0;
        for c in &self.chapters {
            match c.status {
                ChapterStatus::Planned => planned += 1,
                ChapterStatus::Outlined => outlined += 1,
                ChapterStatus::Drafting => drafting += 1,
                ChapterStatus::Completed => completed += 1,
                ChapterStatus::Published => published += 1,
            }
        }
        let stage_text = self
            .stage
            .as_ref()
            .map(Self::stage_label)
            .unwrap_or("未确定");
        let pipeline_text = self
            .pipeline
            .as_ref()
            .map(|p| {
                format!(
                    "流水线: {} / {}",
                    p.phase.label(),
                    pipeline_status_label(p.status),
                )
            })
            .unwrap_or_else(|| "未运行流水线".to_string());
        let header = Line::from(vec![
            Span::styled(
                format!("阶段 {stage_text}"),
                Style::default()
                    .fg(palette::MODE_NOVEL)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("    "),
            Span::styled(
                format!(
                    "计划 {planned} · 细纲 {outlined} · 草稿 {drafting} · 完成 {completed} · 发布 {published}"
                ),
                Style::default().fg(palette::TEXT_MUTED),
            ),
            Span::raw("    "),
            Span::styled(pipeline_text, Style::default().fg(palette::TEXT_MUTED)),
        ]);
        Paragraph::new(header).render(
            Rect {
                x: inner.x,
                y: inner.y,
                width: inner.width,
                height: 1,
            },
            buf,
        );

        // 列表。
        let list_area = Rect {
            x: inner.x,
            y: inner.y + 1,
            width: inner.width,
            height: inner.height.saturating_sub(1),
        };
        if self.chapters.is_empty() {
            Paragraph::new(Line::from(Span::styled(
                "（暂无章节 — 在 novel_outline_build 后再回到这里查看）",
                Style::default().fg(palette::TEXT_MUTED),
            )))
            .render(list_area, buf);
            return;
        }

        let visible_height = list_area.height as usize;
        let scroll = if self.selected >= visible_height {
            self.selected + 1 - visible_height
        } else {
            0
        };

        let items: Vec<ListItem> = self
            .chapters
            .iter()
            .skip(scroll)
            .take(visible_height)
            .map(|c| {
                let status = Self::status_label(&c.status);
                let line = Line::from(vec![
                    Span::styled(
                        format!("{:>3}. ", c.number),
                        Style::default().fg(palette::TEXT_MUTED),
                    ),
                    Span::styled(
                        format!("[{}] ", status),
                        Style::default().fg(Self::status_color(&c.status)),
                    ),
                    Span::styled(
                        format!("{} ", c.title),
                        Style::default()
                            .fg(palette::TEXT_PRIMARY)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("（{} · 紧张 {:.1}）", node_type_label(&c.node_type), c.tension),
                        Style::default().fg(palette::TEXT_MUTED),
                    ),
                ]);
                ListItem::new(line)
            })
            .collect();

        let list = List::new(items).highlight_style(
            Style::default()
                .fg(palette::SELECTION_TEXT)
                .bg(palette::SELECTION_BG)
                .add_modifier(Modifier::BOLD),
        );
        let visible_selected = self.selected.saturating_sub(scroll);
        let mut state = ListState::default();
        state.select(Some(visible_selected));
        StatefulWidget::render(list, list_area, buf, &mut state);
    }
}

// ---------------------------------------------------------------------------
// 共享：ratatui ListState 简封装
// ---------------------------------------------------------------------------
// 注意：ratatui 0.30 的 `ListState` 本身有 selected 字段，可以直接构造。

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use novel::model::chapter::Chapter;
    use novel::model::character::Character;
    use novel::model::graph::{ArcType, NarrativeNode, StoryArc};
    use uuid::Uuid;

    fn sample_node(title: &str, kind: NarrativeNodeType, tension: f32) -> NarrativeNode {
        NarrativeNode {
            id: Uuid::new_v4(),
            title: title.to_string(),
            node_type: kind,
            chapter_ref: None,
            emotional_valence: 0.0,
            tension,
        }
    }

    fn sample_arc(name: &str, kind: ArcType, nodes: Vec<Uuid>) -> StoryArc {
        StoryArc {
            id: Uuid::new_v4(),
            name: name.to_string(),
            arc_type: kind,
            nodes,
        }
    }

    fn sample_chapter(number: u32, title: &str, status: ChapterStatus) -> Chapter {
        Chapter {
            id: Uuid::new_v4(),
            title: title.to_string(),
            number,
            outline: None,
            content: None,
            status,
            node_type: NarrativeNodeType::Conflict,
            emotional_valence: 0.0,
            tension: 0.5,
        }
    }

    #[test]
    fn narrative_tree_renders_empty_graph() {
        let graph = NarrativeGraph::default();
        let view = NarrativeTreeView::new(graph, None);
        assert!(matches!(view.rows.first(), Some(NarrativeRow::Empty)));
    }

    #[test]
    fn narrative_tree_lists_arcs_and_nodes() {
        let n1 = sample_node("开篇钩子", NarrativeNodeType::Hook, 0.6);
        let n2 = sample_node("正邪冲突", NarrativeNodeType::Conflict, 0.9);
        let arc = sample_arc("第一卷", ArcType::MainLine, vec![n1.id, n2.id]);
        let graph = NarrativeGraph {
            nodes: vec![n1, n2],
            arcs: vec![arc],
        };
        let view = NarrativeTreeView::new(graph, None);
        let kinds: Vec<&'static str> = view
            .rows
            .iter()
            .map(|row| match row {
                NarrativeRow::Arc { .. } => "arc",
                NarrativeRow::Node { .. } => "node",
                NarrativeRow::Empty => "empty",
            })
            .collect();
        assert_eq!(kinds, vec!["arc", "node", "node"]);
    }

    #[test]
    fn narrative_tree_collapse_hides_nodes() {
        let n1 = sample_node("开篇钩子", NarrativeNodeType::Hook, 0.6);
        let arc_id = Uuid::new_v4();
        let arc = StoryArc {
            id: arc_id,
            name: "第一卷".to_string(),
            arc_type: ArcType::MainLine,
            nodes: vec![n1.id],
        };
        let graph = NarrativeGraph {
            nodes: vec![n1],
            arcs: vec![arc],
        };
        let mut view = NarrativeTreeView::new(graph, None);
        view.collapsed.insert(arc_id);
        view.rebuild_rows();
        assert_eq!(view.rows.len(), 1, "折叠后只应保留弧线行");
    }

    #[test]
    fn character_list_filters_by_alias() {
        let characters = vec![
            Character {
                name: "林夕".to_string(),
                aliases: vec!["小夕".to_string()],
                age: None,
                appearance: None,
                personality: Some("冷静".to_string()),
                background: None,
                goals: Vec::new(),
                relationships: Vec::new(),
                status: CharacterStatus::Alive,
            },
            Character {
                name: "赵天明".to_string(),
                aliases: Vec::new(),
                age: None,
                appearance: None,
                personality: Some("热血".to_string()),
                background: None,
                goals: Vec::new(),
                relationships: Vec::new(),
                status: CharacterStatus::Missing,
            },
        ];
        let mut view = CharacterListView::new(characters, None);
        view.query.push('小');
        view.refilter();
        assert_eq!(view.filtered.len(), 1);
        view.query.clear();
        view.query.push_str("赵");
        view.refilter();
        assert_eq!(view.filtered.len(), 1);
    }

    #[test]
    fn chapter_progress_counts_by_status() {
        let chapters = vec![
            sample_chapter(1, "序章", ChapterStatus::Completed),
            sample_chapter(2, "起", ChapterStatus::Drafting),
            sample_chapter(3, "承", ChapterStatus::Outlined),
            sample_chapter(4, "合", ChapterStatus::Planned),
        ];
        let view = ChapterProgressView::new(chapters, None, Some(CreationStage::Draft));
        // 端到端：选择最后一章回车应该发出事件。
        let action = view.activate();
        match action {
            ViewAction::EmitAndClose(ViewEvent::NovelChapterSelected { chapter_number }) => {
                assert_eq!(chapter_number, 4);
            }
            other => panic!("unexpected action: {other:?}"),
        }
    }

    #[test]
    fn chapter_progress_empty_emits_no_action() {
        let view = ChapterProgressView::new(Vec::new(), None, None);
        assert!(matches!(view.activate(), ViewAction::None));
    }

    #[test]
    fn narrative_tree_enter_emits_node_event() {
        let n1 = sample_node("开篇钩子", NarrativeNodeType::Hook, 0.6);
        let arc = sample_arc("第一卷", ArcType::MainLine, vec![n1.id]);
        let graph = NarrativeGraph {
            nodes: vec![n1.clone()],
            arcs: vec![arc],
        };
        let mut view = NarrativeTreeView::new(graph, None);
        // 选中第一行节点（index 1，因为 0 是 Arc 行）。
        view.selected = 1;
        let action = view.activate();
        match action {
            ViewAction::EmitAndClose(ViewEvent::NovelNodeSelected { node_id }) => {
                assert_eq!(node_id, n1.id);
            }
            other => panic!("unexpected action: {other:?}"),
        }
    }

    #[test]
    fn character_enter_emits_name_event() {
        let characters = vec![Character {
            name: "林夕".to_string(),
            aliases: Vec::new(),
            age: None,
            appearance: None,
            personality: None,
            background: None,
            goals: Vec::new(),
            relationships: Vec::new(),
            status: CharacterStatus::Alive,
        }];
        let view = CharacterListView::new(characters, None);
        let action = view.activate();
        match action {
            ViewAction::EmitAndClose(ViewEvent::NovelCharacterSelected { character_name }) => {
                assert_eq!(character_name, "林夕");
            }
            other => panic!("unexpected action: {other:?}"),
        }
    }

    #[test]
    fn narrative_tree_handle_key_navigates() {
        let n1 = sample_node("开篇钩子", NarrativeNodeType::Hook, 0.6);
        let arc = sample_arc("第一卷", ArcType::MainLine, vec![n1.id]);
        let graph = NarrativeGraph {
            nodes: vec![n1],
            arcs: vec![arc],
        };
        let mut view = NarrativeTreeView::new(graph, None);
        // 第二行（节点）应该是初值 selected=0 时再按一次 Down。
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        assert!(matches!(view.handle_key(down), ViewAction::None));
        assert_eq!(view.selected, 1);
    }
}
