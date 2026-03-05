use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, Clear, ClearType},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use tokio::sync::mpsc;

use super::model::{BlockStatus, Dag, DagBlock};
use super::renderer::DagRenderer;

/// Events from WebSocket
pub enum DagEvent {
    BlockCreated(DagBlock),
    BlockAdded(String),      // hash
    BlockFinalized(String),  // hash
    Error(String),
}

/// The DAG TUI application
pub struct DagApp {
    pub dag: Dag,
    pub renderer: DagRenderer,
    pub scroll_offset: usize,
    pub selected_index: usize,
    pub show_details: bool,
    pub running: bool,
    pub event_receiver: Option<mpsc::Receiver<DagEvent>>,
    pub status_message: String,
    pub block_count: usize,
    pub follow_head: bool,  // If true, auto-scroll to show newest blocks at top
}

impl DagApp {
    pub fn new() -> Self {
        Self {
            dag: Dag::new(),
            renderer: DagRenderer::new(),
            scroll_offset: 0,
            selected_index: 0,
            show_details: false,
            running: true,
            event_receiver: None,
            status_message: "Connecting...".to_string(),
            block_count: 0,
            follow_head: true,  // Start following the head
        }
    }

    pub fn with_event_receiver(mut self, receiver: mpsc::Receiver<DagEvent>) -> Self {
        self.event_receiver = Some(receiver);
        self
    }

    /// Add initial blocks
    pub fn load_blocks(&mut self, blocks: Vec<DagBlock>) {
        for block in blocks {
            self.dag.add_block(block);
        }
        self.dag.compute_layout();
        self.block_count = self.dag.blocks.len();
        self.status_message = format!("Loaded {} blocks", self.block_count);
    }

    /// Run the TUI application
    pub async fn run(&mut self) -> io::Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, Clear(ClearType::All))?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        // Main loop
        let result = self.main_loop(&mut terminal).await;

        // Restore terminal
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        result
    }

    async fn main_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
        loop {
            // Check for WebSocket events (non-blocking)
            // Only process events when NOT in detail view to avoid screen updates while comparing hashes
            if !self.show_details {
                let events: Vec<DagEvent> = if let Some(ref mut receiver) = self.event_receiver {
                    let mut collected = Vec::new();
                    while let Ok(event) = receiver.try_recv() {
                        collected.push(event);
                    }
                    collected
                } else {
                    Vec::new()
                };
                for event in events {
                    self.handle_dag_event(event);
                }
            }

            // Draw
            terminal.draw(|frame| self.render(frame))?;

            // Handle input with timeout
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.handle_key(key.code);
                    }
                }
            }

            if !self.running {
                break;
            }
        }

        Ok(())
    }

    fn handle_dag_event(&mut self, event: DagEvent) {
        match event {
            DagEvent::BlockCreated(block) => {
                self.status_message = format!("New block: #{} {}", block.block_number, block.short_hash);
                self.dag.add_block(block);
                self.dag.compute_layout();
                self.block_count = self.dag.blocks.len();

                // If following head, keep selection at top
                if self.follow_head {
                    self.selected_index = 0;
                    self.scroll_offset = 0;
                }
            }
            DagEvent::BlockAdded(hash) => {
                self.dag.update_status(&hash, BlockStatus::Added);
            }
            DagEvent::BlockFinalized(hash) => {
                self.dag.update_status(&hash, BlockStatus::Finalized);
                self.status_message = format!("Finalized: {}...", &hash[..8.min(hash.len())]);
            }
            DagEvent::Error(msg) => {
                self.status_message = format!("Error: {}", msg);
            }
        }
    }

    fn handle_key(&mut self, code: KeyCode) {
        let num_rows = self.dag.graph_rows.len();

        match code {
            KeyCode::Char('q') | KeyCode::Esc => {
                if self.show_details {
                    self.show_details = false;
                } else {
                    self.running = false;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                    self.ensure_visible();
                    // Resume following if we're back at top
                    self.follow_head = self.scroll_offset == 0 && self.selected_index == 0;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected_index + 1 < num_rows {
                    self.selected_index += 1;
                    self.ensure_visible();
                    // Stop following when scrolling down
                    self.follow_head = false;
                }
            }
            KeyCode::PageUp => {
                self.selected_index = self.selected_index.saturating_sub(10);
                self.ensure_visible();
                self.follow_head = self.scroll_offset == 0 && self.selected_index == 0;
            }
            KeyCode::PageDown => {
                self.selected_index = (self.selected_index + 10).min(num_rows.saturating_sub(1));
                self.ensure_visible();
                self.follow_head = false;
            }
            KeyCode::Char('g') => {
                self.selected_index = 0;
                self.scroll_offset = 0;
                self.follow_head = true;  // Resume following at top
            }
            KeyCode::Char('G') => {
                self.selected_index = num_rows.saturating_sub(1);
                self.ensure_visible();
            }
            KeyCode::Enter => {
                self.show_details = !self.show_details;
            }
            _ => {}
        }
    }

    fn ensure_visible(&mut self) {
        // Assume viewport is about 20 lines (will be adjusted by actual render)
        let viewport_height = 20;
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + viewport_height {
            self.scroll_offset = self.selected_index - viewport_height + 1;
        }
    }

    fn render(&mut self, frame: &mut Frame) {
        let size = frame.area();

        if self.show_details {
            self.render_detail_view(frame, size);
        } else {
            self.render_main_view(frame, size);
        }
    }

    fn render_main_view(&mut self, frame: &mut Frame, area: Rect) {
        // Layout: main content + status bar
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(area);

        // Main DAG view
        let main_block = Block::default()
            .title(" F1R3FLY DAG Viewer ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner_area = main_block.inner(chunks[0]);
        frame.render_widget(main_block, chunks[0]);

        // Split inner area into header + content
        let content_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),  // Header rows (names + separator)
                Constraint::Min(0),     // Content
            ])
            .split(inner_area);

        // Render column header
        let content_width = content_chunks[1].width as usize;
        let header_line = self.renderer.render_header(content_width);
        let separator_line = Line::from(vec![
            Span::styled("─".repeat(content_chunks[0].width as usize), Style::default().fg(Color::DarkGray)),
        ]);
        let header = Paragraph::new(vec![header_line, separator_line]);
        frame.render_widget(header, content_chunks[0]);

        // Compute visible rows
        let viewport_height = content_chunks[1].height as usize;

        // Adjust scroll offset
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + viewport_height {
            self.scroll_offset = self.selected_index - viewport_height + 1;
        }

        // Render visible rows
        let mut items: Vec<ListItem> = Vec::new();

        let rows: Vec<_> = self.dag.graph_rows.iter().collect();
        for (i, row) in rows.iter().enumerate().skip(self.scroll_offset).take(viewport_height) {
            let is_selected = i == self.selected_index;
            let line = self.renderer.render_row(row, &self.dag, is_selected, content_width);

            let style = if is_selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            items.push(ListItem::new(line).style(style));
        }

        let list = List::new(items);
        frame.render_widget(list, content_chunks[1]);

        // Status bar
        let status_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let status_text = Line::from(vec![
            Span::styled(" [↑↓/jk] ", Style::default().fg(Color::Yellow)),
            Span::raw("Navigate  "),
            Span::styled("[Enter] ", Style::default().fg(Color::Yellow)),
            Span::raw("Details  "),
            Span::styled("[g/G] ", Style::default().fg(Color::Yellow)),
            Span::raw("Top/Bottom  "),
            Span::styled("[q] ", Style::default().fg(Color::Yellow)),
            Span::raw("Quit  "),
            Span::raw("  │  "),
            Span::styled(
                format!("Blocks: {}  ", self.block_count),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(
                &self.status_message,
                Style::default().fg(Color::Green),
            ),
        ]);

        let status = Paragraph::new(status_text).block(status_block);
        frame.render_widget(status, chunks[1]);
    }

    fn render_detail_view(&self, frame: &mut Frame, area: Rect) {
        let selected_hash = match self.dag.graph_rows.get(self.selected_index) {
            Some(row) => &row.block_hash,
            None => return,
        };

        let block = match self.dag.blocks.get(selected_hash) {
            Some(b) => b,
            None => return,
        };

        let detail_block = Block::default()
            .title(" Block Details ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = detail_block.inner(area);
        frame.render_widget(detail_block, area);

        // Build detail text
        let mut lines: Vec<Line> = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Hash:        ", Style::default().fg(Color::Yellow)),
                Span::raw(&block.hash),
            ]),
            Line::from(vec![
                Span::styled("  Block #:     ", Style::default().fg(Color::Yellow)),
                Span::raw(block.block_number.to_string()),
            ]),
            Line::from(vec![
                Span::styled("  Timestamp:   ", Style::default().fg(Color::Yellow)),
                Span::raw(block.timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string()),
            ]),
            Line::from(vec![
                Span::styled("  Creator:     ", Style::default().fg(Color::Yellow)),
                Span::raw(&block.creator),
            ]),
            Line::from(vec![
                Span::styled("  Seq Num:     ", Style::default().fg(Color::Yellow)),
                Span::raw(block.seq_num.to_string()),
            ]),
            Line::from(vec![
                Span::styled("  Shard:       ", Style::default().fg(Color::Yellow)),
                Span::raw(if block.shard_id.is_empty() { "root" } else { &block.shard_id }),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Parents:", Style::default().fg(Color::Yellow)),
            ]),
        ];

        if block.parents.is_empty() {
            lines.push(Line::from("    (genesis - no parents)"));
        } else {
            for parent in &block.parents {
                lines.push(Line::from(format!("    └─ {}...", &parent[..16.min(parent.len())])));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  State Transition:", Style::default().fg(Color::Yellow)),
        ]));
        lines.push(Line::from(format!(
            "    Pre:  {}...",
            if block.pre_state_hash.len() > 16 {
                &block.pre_state_hash[..16]
            } else if block.pre_state_hash.is_empty() {
                "(empty)"
            } else {
                &block.pre_state_hash
            }
        )));
        lines.push(Line::from(format!(
            "    Post: {}...",
            if block.post_state_hash.len() > 16 {
                &block.post_state_hash[..16]
            } else if block.post_state_hash.is_empty() {
                "(empty)"
            } else {
                &block.post_state_hash
            }
        )));

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                format!("  Deploys ({}):", block.deploy_count),
                Style::default().fg(Color::Yellow),
            ),
        ]));

        if block.deploys.is_empty() {
            lines.push(Line::from("    (no deploys)"));
        } else {
            for deploy in &block.deploys {
                let status_icon = if deploy.errored { "✗" } else { "✓" };
                let status_color = if deploy.errored { Color::Red } else { Color::Green };
                lines.push(Line::from(vec![
                    Span::raw("    └─ ["),
                    Span::styled(status_icon, Style::default().fg(status_color)),
                    Span::raw(format!(
                        "] {}  cost: {}  deployer: {}",
                        &deploy.id[..12.min(deploy.id.len())],
                        deploy.cost,
                        &deploy.deployer[..8.min(deploy.deployer.len())]
                    )),
                ]));
            }
        }

        lines.push(Line::from(""));
        let (status_str, status_color) = match block.status {
            BlockStatus::Finalized => ("FINALIZED", Color::Green),
            BlockStatus::Added => ("ADDED (pending finalization)", Color::Yellow),
            BlockStatus::Created => ("CREATED (pending validation)", Color::Cyan),
        };
        lines.push(Line::from(vec![
            Span::styled("  Status: ", Style::default().fg(Color::Yellow)),
            Span::styled(status_str, Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
        ]));

        lines.push(Line::from(""));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(" [Esc] ", Style::default().fg(Color::Yellow)),
            Span::raw("Back  "),
        ]));

        let detail_text = Paragraph::new(lines).wrap(Wrap { trim: false });
        frame.render_widget(detail_text, inner);
    }
}

impl Default for DagApp {
    fn default() -> Self {
        Self::new()
    }
}
