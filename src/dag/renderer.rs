use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use super::model::{BlockStatus, Dag, GraphRow};

/// Color palette for validators
const VALIDATOR_COLORS: [Color; 8] = [
    Color::Cyan,
    Color::Magenta,
    Color::Yellow,
    Color::Blue,
    Color::Green,
    Color::Red,
    Color::LightCyan,
    Color::LightMagenta,
];

// Fixed column widths
const CREATOR_WIDTH: usize = 10;
const BLOCK_WIDTH: usize = 7;
const HASH_WIDTH: usize = 10;
const DEPLOYS_WIDTH: usize = 10;
const STATUS_WIDTH: usize = 8;
const AGE_WIDTH: usize = 8;
const SPACING: usize = 2; // Space between column groups

/// Renderer for the DAG visualization
pub struct DagRenderer {
    pub use_color: bool,
    pub show_deploys: bool,
}

impl DagRenderer {
    pub fn new() -> Self {
        Self {
            use_color: true,
            show_deploys: true,
        }
    }

    /// Get color for a validator based on column index
    fn validator_color(&self, col: usize) -> Color {
        VALIDATOR_COLORS[col % VALIDATOR_COLORS.len()]
    }

    /// Calculate fixed width (everything except PARENTS column)
    fn fixed_width(&self) -> usize {
        CREATOR_WIDTH + BLOCK_WIDTH + HASH_WIDTH + SPACING + SPACING + DEPLOYS_WIDTH + STATUS_WIDTH + AGE_WIDTH
    }

    /// Render a single row of the DAG
    pub fn render_row(&self, row: &GraphRow, dag: &Dag, selected: bool, total_width: usize) -> Line<'static> {
        let block = match dag.blocks.get(&row.block_hash) {
            Some(b) => b,
            None => return Line::from(""),
        };

        let mut spans: Vec<Span> = Vec::new();

        // === LEFT SIDE (left-aligned): CREATOR, BLOCK, HASH ===

        // Creator
        let creator_color = self.validator_color(row.node_column);
        spans.push(Span::styled(
            format!("{:<width$}", &block.creator_short, width = CREATOR_WIDTH),
            Style::default().fg(creator_color),
        ));

        // Block number
        let block_num_str = if block.block_number < 0 {
            "???".to_string()
        } else {
            format!("{}", block.block_number)
        };
        spans.push(Span::styled(
            format!("#{:<width$}", block_num_str, width = BLOCK_WIDTH - 1),
            Style::default().fg(Color::Gray),
        ));

        // Hash
        let hash_style = if selected {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(Span::styled(
            format!("{:<width$}", &block.short_hash, width = HASH_WIDTH),
            hash_style,
        ));

        spans.push(Span::raw("  "));

        // === CENTER (centered): PARENTS - uses all available space ===
        let parents_width = total_width.saturating_sub(self.fixed_width());
        let parents_str = if block.parents.is_empty() {
            "(genesis)".to_string()
        } else {
            // Try enriched format first: hash[creator:#blocknum] | hash[creator:#blocknum]
            let enriched: Vec<String> = block.parents
                .iter()
                .map(|p| {
                    let short_hash = if p.len() >= 8 { &p[..8] } else { p.as_str() };
                    if let Some(parent_block) = dag.blocks.get(p) {
                        format!("{}[{}:#{}]", short_hash, &parent_block.creator_short, parent_block.block_number)
                    } else {
                        short_hash.to_string()
                    }
                })
                .collect();
            let enriched_str = enriched.join("  |  ");

            // Fall back to simple format if enriched doesn't fit
            if enriched_str.len() <= parents_width {
                enriched_str
            } else {
                // Simple format: just hashes
                block.parents
                    .iter()
                    .map(|p| {
                        if p.len() >= 8 {
                            p[..8].to_string()
                        } else {
                            p.clone()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("  ")
            }
        };

        // Center the parents string within the available width
        let parents_display = if parents_str.len() >= parents_width {
            // Truncate if too long
            parents_str.chars().take(parents_width).collect::<String>()
        } else {
            // Center it
            let padding = parents_width - parents_str.len();
            let left_pad = padding / 2;
            let right_pad = padding - left_pad;
            format!("{}{}{}", " ".repeat(left_pad), parents_str, " ".repeat(right_pad))
        };

        spans.push(Span::styled(
            parents_display,
            Style::default().fg(Color::DarkGray),
        ));

        spans.push(Span::raw("  "));

        // === RIGHT SIDE (right-aligned): DEPLOYS, STATUS, AGE ===

        // Deploy count (right-aligned)
        let deploy_style = if block.deploy_count > 0 {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let deploy_str = format!("{} dep", block.deploy_count);
        spans.push(Span::styled(
            format!("{:>width$}", deploy_str, width = DEPLOYS_WIDTH),
            deploy_style,
        ));

        // Status (right-aligned)
        let (status_str, status_color) = match block.status {
            BlockStatus::Finalized => ("FINAL", Color::Green),
            BlockStatus::Added => ("ADDED", Color::Yellow),
            BlockStatus::Created => ("NEW", Color::Cyan),
        };
        spans.push(Span::styled(
            format!("{:>width$}", status_str, width = STATUS_WIDTH),
            Style::default().fg(status_color),
        ));

        // Age (right-aligned)
        spans.push(Span::styled(
            format!("{:>width$}", block.age_string(), width = AGE_WIDTH),
            Style::default().fg(Color::DarkGray),
        ));

        Line::from(spans)
    }

    /// Render the column header
    pub fn render_header(&self, total_width: usize) -> Line<'static> {
        let mut spans = Vec::new();

        // Left side (left-aligned)
        spans.push(Span::styled(
            format!("{:<width$}", "CREATOR", width = CREATOR_WIDTH),
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!("{:<width$}", "BLOCK", width = BLOCK_WIDTH),
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!("{:<width$}", "HASH", width = HASH_WIDTH),
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
        ));

        spans.push(Span::raw("  "));

        // Center (centered)
        let parents_width = total_width.saturating_sub(self.fixed_width());
        let header_text = "PARENTS";
        let parents_header = if header_text.len() >= parents_width {
            header_text.to_string()
        } else {
            let padding = parents_width - header_text.len();
            let left_pad = padding / 2;
            let right_pad = padding - left_pad;
            format!("{}{}{}", " ".repeat(left_pad), header_text, " ".repeat(right_pad))
        };
        spans.push(Span::styled(
            parents_header,
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
        ));

        spans.push(Span::raw("  "));

        // Right side (right-aligned)
        spans.push(Span::styled(
            format!("{:>width$}", "DEPLOYS", width = DEPLOYS_WIDTH),
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!("{:>width$}", "STATUS", width = STATUS_WIDTH),
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!("{:>width$}", "AGE", width = AGE_WIDTH),
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
        ));

        Line::from(spans)
    }
}

impl Default for DagRenderer {
    fn default() -> Self {
        Self::new()
    }
}
