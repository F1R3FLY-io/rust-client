# DAG Visualization Design Document

## Overview

This document describes the design for a real-time, scrollable DAG (Directed Acyclic Graph) visualization for the F1R3FLY rust-client. The visualization will display the blockchain's block structure in a git-log style graph format, allowing users to observe block creation, merging, and finalization in real-time.

## Goals

1. **Intuitive visualization** - Use familiar git-log style graph notation
2. **Real-time updates** - Stream new blocks via WebSocket as they're created
3. **Interactive navigation** - Scrollable, with block selection and details
4. **Terminal-native** - Works in any terminal without external dependencies
5. **Informative** - Show key block metadata at a glance

## User Experience

### Command Interface

```bash
# Start the DAG viewer
node-cli dag --host localhost --port 40403

# Options
node-cli dag --host <host> --port <port> [OPTIONS]
  --depth <N>        Initial blocks to load (default: 50)
  --no-live          Disable real-time updates
  --validators       Color-code by validator
  --show-deploys     Show deploy counts inline
```

### Visual Layout

```
┌─ F1R3FLY DAG Viewer ──────────────────────────────────────────────────────┐
│                                                                            │
│  * [a1b2c3d4] Block 142  validator1  2 deploys  12s ago            ✓ FINAL │
│  |\                                                                        │
│  | * [e5f6g7h8] Block 141  validator2  1 deploy  18s ago           ✓ FINAL │
│  * | [i9j0k1l2] Block 140  validator1  0 deploys  25s ago          ✓ FINAL │
│  |/                                                                        │
│  * [m3n4o5p6] Block 139  validator3  3 deploys  32s ago            ✓ FINAL │
│  |\                                                                        │
│  | * [q7r8s9t0] Block 138  validator2  1 deploy  45s ago           ✓ FINAL │
│  |/                                                                        │
│  * [u1v2w3x4] Block 137  validator1  0 deploys  58s ago            ✓ FINAL │
│  |                                                                         │
│  * [genesis] Block 0  system                                       ✓ FINAL │
│                                                                            │
├────────────────────────────────────────────────────────────────────────────┤
│ [↑↓] Navigate  [Enter] Details  [f] Filter  [q] Quit    Blocks: 142  │  │
└────────────────────────────────────────────────────────────────────────────┘
```

### Block Detail View (on Enter)

```
┌─ Block Details ───────────────────────────────────────────────────────────┐
│                                                                            │
│  Hash:        a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6q7r8s9t0u1v2w3x4y5z6       │
│  Block #:     142                                                          │
│  Timestamp:   2025-12-12 17:45:32 UTC                                      │
│  Creator:     validator1 (04a1b2c3...)                                     │
│  Seq Num:     47                                                           │
│  Shard:       root                                                         │
│                                                                            │
│  Parents:                                                                  │
│    └─ e5f6g7h8... (Block 141)                                              │
│    └─ i9j0k1l2... (Block 140)                                              │
│                                                                            │
│  State Transition:                                                         │
│    Pre:  abc123...                                                         │
│    Post: def456...                                                         │
│                                                                            │
│  Deploys (2):                                                              │
│    └─ [✓] deploy_abc123  cost: 1000  deployer: user1                       │
│    └─ [✗] deploy_def456  cost: 500   deployer: user2  (errored)            │
│                                                                            │
│  Status: FINALIZED                                                         │
│                                                                            │
├────────────────────────────────────────────────────────────────────────────┤
│ [Esc] Back  [p] Jump to Parent  [c] Copy Hash                              │
└────────────────────────────────────────────────────────────────────────────┘
```

### Color Scheme

| Element | Color | Meaning |
|---------|-------|---------|
| `✓ FINAL` | Green | Block is finalized |
| `● ADDED` | Yellow | Block added but not finalized |
| `○ NEW` | Cyan | Block just created |
| Graph lines | Dim white | DAG structure |
| Selected row | Inverse/highlight | Current selection |
| Validator colors | Rotating palette | Distinguish validators |

## Architecture

### Module Structure

```
src/
├── commands/
│   └── dag.rs              # Command entry point & TUI app
├── dag/
│   ├── mod.rs              # Module exports
│   ├── model.rs            # DAG data structures
│   ├── graph.rs            # Graph layout algorithm
│   ├── renderer.rs         # Git-style ASCII rendering
│   └── events.rs           # WebSocket event handling
```

### Core Data Structures

```rust
/// Represents a block in the DAG
#[derive(Clone, Debug)]
pub struct DagBlock {
    pub hash: String,
    pub short_hash: String,          // First 8 chars
    pub block_number: i64,
    pub timestamp: DateTime<Utc>,
    pub creator: String,
    pub creator_short: String,       // Truncated validator ID
    pub seq_num: i64,
    pub parents: Vec<String>,        // Parent block hashes
    pub deploy_count: u32,
    pub status: BlockStatus,
    pub shard_id: String,
    pub pre_state_hash: String,
    pub post_state_hash: String,
    pub deploys: Vec<DagDeploy>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BlockStatus {
    Created,    // Just proposed
    Added,      // Validated and added to DAG
    Finalized,  // Reached finality
}

#[derive(Clone, Debug)]
pub struct DagDeploy {
    pub id: String,
    pub cost: u64,
    pub deployer: String,
    pub errored: bool,
}

/// The DAG structure with layout information
pub struct Dag {
    blocks: HashMap<String, DagBlock>,
    children: HashMap<String, Vec<String>>,  // Reverse lookup
    roots: Vec<String>,                       // Blocks with no children (tips)
    layout: Vec<LayoutRow>,                   // Computed layout for rendering
}

/// A row in the rendered DAG
pub struct LayoutRow {
    pub block_hash: String,
    pub column: usize,              // Which "lane" this block is in
    pub connections: Vec<Connection>, // Lines to draw
}

pub struct Connection {
    pub from_col: usize,
    pub to_col: usize,
    pub style: ConnectionStyle,     // Straight, merge-left, merge-right, etc.
}
```

### Graph Layout Algorithm

The layout algorithm assigns blocks to columns (lanes) to create the git-style visualization:

```rust
impl Dag {
    /// Compute layout using topological sort + lane assignment
    pub fn compute_layout(&mut self) {
        // 1. Topological sort (newest blocks first)
        let sorted = self.topological_sort();

        // 2. Assign lanes using a greedy algorithm
        //    - Each active branch gets a lane
        //    - Merges reduce lane count
        //    - Forks increase lane count
        let mut active_lanes: Vec<Option<String>> = vec![];

        for block_hash in sorted {
            let block = &self.blocks[&block_hash];

            // Find or create lane for this block
            let lane = self.find_or_create_lane(&mut active_lanes, block);

            // Compute connections to parents
            let connections = self.compute_connections(block, lane, &active_lanes);

            self.layout.push(LayoutRow {
                block_hash: block_hash.clone(),
                column: lane,
                connections,
            });

            // Update active lanes based on parents
            self.update_lanes(&mut active_lanes, block, lane);
        }
    }
}
```

### ASCII Renderer

The renderer converts the layout into git-style ASCII:

```rust
pub struct DagRenderer {
    use_color: bool,
    show_deploys: bool,
    validator_colors: HashMap<String, Color>,
}

impl DagRenderer {
    /// Render a single row of the DAG
    pub fn render_row(&self, row: &LayoutRow, dag: &Dag) -> Vec<Span> {
        let block = &dag.blocks[&row.block_hash];
        let mut spans = vec![];

        // 1. Render graph portion (lanes and connections)
        spans.extend(self.render_graph_prefix(row));

        // 2. Render block info
        spans.push(self.render_block_info(block));

        spans
    }

    /// Render the graph prefix (*, |, \, /, etc.)
    fn render_graph_prefix(&self, row: &LayoutRow) -> Vec<Span> {
        // Build the prefix string like "* | " or "|\  " or "|/  "
        // Based on connections and current column
    }
}
```

Graph character set:
```
*   Block node
|   Vertical line (continuation)
\   Merge from right
/   Merge from left
─   Horizontal connection (rare in git-style)
```

### TUI Application

Using `ratatui` for the terminal UI:

```rust
pub struct DagApp {
    dag: Dag,
    scroll_offset: usize,
    selected_index: usize,
    viewport_height: usize,
    show_details: bool,
    ws_receiver: mpsc::Receiver<RChainEvent>,
    running: bool,
}

impl DagApp {
    pub async fn run(&mut self, terminal: &mut Terminal<impl Backend>) -> Result<()> {
        loop {
            // 1. Check for new WebSocket events (non-blocking)
            while let Ok(event) = self.ws_receiver.try_recv() {
                self.handle_block_event(event);
            }

            // 2. Render current state
            terminal.draw(|frame| self.render(frame))?;

            // 3. Handle input (with timeout for responsiveness)
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    self.handle_key(key)?;
                }
            }

            if !self.running {
                break;
            }
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.scroll_up(),
            KeyCode::Down | KeyCode::Char('j') => self.scroll_down(),
            KeyCode::PageUp => self.page_up(),
            KeyCode::PageDown => self.page_down(),
            KeyCode::Enter => self.toggle_details(),
            KeyCode::Char('q') | KeyCode::Esc => self.running = false,
            KeyCode::Char('g') => self.goto_top(),
            KeyCode::Char('G') => self.goto_bottom(),
            _ => {}
        }
        Ok(())
    }
}
```

### WebSocket Integration

Reuse existing WebSocket code from `commands/events.rs`:

```rust
pub async fn spawn_event_listener(
    ws_url: String,
    sender: mpsc::Sender<RChainEvent>,
) -> Result<JoinHandle<()>> {
    tokio::spawn(async move {
        loop {
            match connect_and_listen(&ws_url, &sender).await {
                Ok(_) => break,  // Clean shutdown
                Err(e) => {
                    // Log error, wait, reconnect
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
    })
}
```

## Dependencies

Add to `Cargo.toml`:

```toml
[dependencies]
# TUI
ratatui = "0.28"
crossterm = "0.28"

# Async channels for WebSocket -> TUI communication
tokio = { version = "1", features = ["full", "sync"] }
```

## Implementation Plan

### Phase 1: Core Data Model
- [ ] Create `dag/model.rs` with `DagBlock`, `Dag` structures
- [ ] Implement block insertion and parent tracking
- [ ] Add method to load initial blocks from API

### Phase 2: Graph Layout
- [ ] Implement topological sort
- [ ] Create lane assignment algorithm
- [ ] Handle merges and forks correctly

### Phase 3: ASCII Renderer
- [ ] Implement git-style graph prefix rendering
- [ ] Add block info formatting
- [ ] Add color support

### Phase 4: TUI Application
- [ ] Set up ratatui boilerplate
- [ ] Implement scrolling and selection
- [ ] Add block detail view
- [ ] Handle keyboard input

### Phase 5: Real-time Updates
- [ ] Integrate WebSocket event stream
- [ ] Update DAG model on new events
- [ ] Recompute layout incrementally
- [ ] Scroll to show new blocks (optional)

### Phase 6: Polish
- [ ] Add loading indicator
- [ ] Handle connection errors gracefully
- [ ] Add help overlay
- [ ] Performance optimization for large DAGs

## Edge Cases

1. **Genesis block** - Single block with no parents
2. **Linear chain** - No merges (single lane)
3. **Many validators** - Multiple parallel lanes
4. **Reconnection** - WebSocket disconnects and reconnects
5. **Large DAG** - Virtualized rendering for performance
6. **Missing parents** - Blocks received out of order

## Testing Strategy

1. **Unit tests** - Layout algorithm with known DAG structures
2. **Snapshot tests** - ASCII rendering output
3. **Integration tests** - Against running node
4. **Manual testing** - Visual verification

## Future Enhancements

- **Search** - Find block by hash or deploy ID
- **Filter** - Show only specific validator's blocks
- **Export** - Save DAG to DOT format for Graphviz
- **Time travel** - Jump to specific block height
- **Deploy tracking** - Highlight path of a specific deploy
- **Fork visualization** - Emphasize competing chains
- **Metrics overlay** - Show finalization rate, block time, etc.

---

*Design created: December 2025*
