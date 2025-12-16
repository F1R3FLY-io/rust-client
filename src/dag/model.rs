use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Status of a block in the DAG
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockStatus {
    Created,   // Just proposed
    Added,     // Validated and added to DAG
    Finalized, // Reached finality
}

/// A deploy within a block
#[derive(Clone, Debug)]
pub struct DagDeploy {
    pub id: String,
    pub cost: u64,
    pub deployer: String,
    pub errored: bool,
}

/// A block in the DAG
#[derive(Clone, Debug)]
pub struct DagBlock {
    pub hash: String,
    pub short_hash: String,
    pub block_number: i64,
    pub timestamp: DateTime<Utc>,
    pub creator: String,
    pub creator_short: String,
    pub seq_num: i64,
    pub parents: Vec<String>,
    pub deploy_count: u32,
    pub status: BlockStatus,
    pub shard_id: String,
    pub pre_state_hash: String,
    pub post_state_hash: String,
    pub deploys: Vec<DagDeploy>,
}

impl DagBlock {
    pub fn new(
        hash: String,
        block_number: i64,
        timestamp: DateTime<Utc>,
        creator: String,
        seq_num: i64,
        parents: Vec<String>,
        deploy_count: u32,
        status: BlockStatus,
    ) -> Self {
        let short_hash = if hash.len() >= 8 {
            hash[..8].to_string()
        } else {
            hash.clone()
        };
        let creator_short = if creator.len() >= 8 {
            creator[..8].to_string()
        } else {
            creator.clone()
        };

        Self {
            hash,
            short_hash,
            block_number,
            timestamp,
            creator,
            creator_short,
            seq_num,
            parents,
            deploy_count,
            status,
            shard_id: String::new(),
            pre_state_hash: String::new(),
            post_state_hash: String::new(),
            deploys: Vec::new(),
        }
    }

    /// Time since block was created
    pub fn age(&self) -> chrono::Duration {
        Utc::now() - self.timestamp
    }

    /// Human-readable age string
    pub fn age_string(&self) -> String {
        let age = self.age();
        if age.num_seconds() < 60 {
            format!("{}s ago", age.num_seconds())
        } else if age.num_minutes() < 60 {
            format!("{}m ago", age.num_minutes())
        } else if age.num_hours() < 24 {
            format!("{}h ago", age.num_hours())
        } else {
            format!("{}d ago", age.num_days())
        }
    }
}

/// A row in the git-style graph output
#[derive(Clone, Debug)]
pub struct GraphRow {
    pub block_hash: String,
    pub node_column: usize,           // Which column has the node (●)
    pub columns: Vec<GraphColumn>,    // State of each column
    pub edges: Vec<GraphEdge>,        // Edges to draw on this row
}

/// What's in a graph column
#[derive(Clone, Debug, PartialEq)]
pub enum GraphColumn {
    Empty,
    Line(String),      // Continuing line tracking a block hash
    Node,              // The node for this row's block
}

/// An edge to draw (connects node to a parent)
#[derive(Clone, Debug)]
pub struct GraphEdge {
    pub from_col: usize,
    pub to_col: usize,
    pub parent_hash: String,
}

/// The DAG structure
pub struct Dag {
    pub blocks: HashMap<String, DagBlock>,
    pub children: HashMap<String, Vec<String>>, // parent -> children
    pub tips: Vec<String>,                      // Blocks with no children
    pub graph_rows: Vec<GraphRow>,
    pub sorted_hashes: Vec<String>,             // Sorted by block number descending
    pub max_columns: usize,
}

impl Dag {
    pub fn new() -> Self {
        Self {
            blocks: HashMap::new(),
            children: HashMap::new(),
            tips: Vec::new(),
            graph_rows: Vec::new(),
            sorted_hashes: Vec::new(),
            max_columns: 0,
        }
    }

    /// Add or update a block in the DAG
    pub fn add_block(&mut self, block: DagBlock) {
        let hash = block.hash.clone();
        let parents = block.parents.clone();
        let is_update = self.blocks.contains_key(&hash);

        // Only update parent-child relationships for new blocks
        if !is_update {
            // Update children map
            for parent in &parents {
                self.children
                    .entry(parent.clone())
                    .or_insert_with(Vec::new)
                    .push(hash.clone());

                // Parent is no longer a tip
                if let Some(pos) = self.tips.iter().position(|h| h == parent) {
                    self.tips.remove(pos);
                }
            }

            // This block is a tip (for now)
            self.tips.push(hash.clone());
        }

        // Insert or update the block
        self.blocks.insert(hash, block);
    }

    /// Update block status
    pub fn update_status(&mut self, hash: &str, status: BlockStatus) {
        if let Some(block) = self.blocks.get_mut(hash) {
            block.status = status;
        }
    }

    /// Get blocks sorted by block number (descending)
    pub fn blocks_by_number(&self) -> Vec<&DagBlock> {
        let mut blocks: Vec<_> = self.blocks.values().collect();
        blocks.sort_by(|a, b| b.block_number.cmp(&a.block_number));
        blocks
    }

    /// Sort blocks by block number (newest/highest first)
    fn sort_blocks(&mut self) {
        let mut block_list: Vec<_> = self.blocks.iter()
            .map(|(hash, block)| (hash.clone(), block.block_number, block.timestamp))
            .collect();

        // Sort by block number descending, then by timestamp descending for same block number
        block_list.sort_by(|a, b| {
            match b.1.cmp(&a.1) {
                std::cmp::Ordering::Equal => b.2.cmp(&a.2),
                other => other,
            }
        });

        self.sorted_hashes = block_list.into_iter().map(|(hash, _, _)| hash).collect();
    }

    /// Compute the git-style graph layout
    /// This implements the same algorithm as `git log --graph`
    pub fn compute_layout(&mut self) {
        self.sort_blocks();
        self.graph_rows.clear();

        if self.sorted_hashes.is_empty() {
            return;
        }

        // Active columns: each column tracks which block hash it's "following"
        // When we see a block, we "continue" from its parent in that column
        let mut columns: Vec<Option<String>> = Vec::new();

        // Map from block hash to which column it occupies when we reach it
        let mut hash_to_column: HashMap<String, usize> = HashMap::new();

        // Process blocks from newest to oldest (top to bottom in display)
        for hash in &self.sorted_hashes.clone() {
            let block = match self.blocks.get(hash) {
                Some(b) => b,
                None => continue,
            };

            // Find which column this block should be in
            let node_col = if let Some(&col) = hash_to_column.get(hash) {
                // A child already reserved a column for us
                col
            } else {
                // No child reserved us a spot - find first empty column or add new one
                let col = columns.iter().position(|c| c.is_none())
                    .unwrap_or_else(|| {
                        columns.push(None);
                        columns.len() - 1
                    });
                col
            };

            // Ensure columns vector is big enough
            while columns.len() <= node_col {
                columns.push(None);
            }

            // Build the row's column state before drawing edges
            let mut row_columns: Vec<GraphColumn> = columns.iter().map(|c| {
                match c {
                    Some(h) => GraphColumn::Line(h.clone()),
                    None => GraphColumn::Empty,
                }
            }).collect();

            // Mark this column as having the node
            row_columns[node_col] = GraphColumn::Node;

            // Collect edges to parents
            let mut edges: Vec<GraphEdge> = Vec::new();
            let parents = &block.parents;

            // Process parents and assign them columns
            for (i, parent_hash) in parents.iter().enumerate() {
                if !self.blocks.contains_key(parent_hash) {
                    // Parent not in our view, skip
                    continue;
                }

                let parent_col = if i == 0 {
                    // First parent continues in our column
                    hash_to_column.insert(parent_hash.clone(), node_col);
                    node_col
                } else {
                    // Additional parents need their own column
                    if let Some(&existing_col) = hash_to_column.get(parent_hash) {
                        // Parent already has a column assigned
                        existing_col
                    } else {
                        // Find an empty column for this parent
                        let col = columns.iter().enumerate()
                            .position(|(idx, c)| c.is_none() && idx != node_col)
                            .unwrap_or_else(|| {
                                columns.push(None);
                                columns.len() - 1
                            });
                        hash_to_column.insert(parent_hash.clone(), col);
                        col
                    }
                };

                edges.push(GraphEdge {
                    from_col: node_col,
                    to_col: parent_col,
                    parent_hash: parent_hash.clone(),
                });
            }

            // Update active columns for next iteration
            // This block is now "consumed" - its column tracks its first parent
            if let Some(first_parent) = parents.first() {
                if self.blocks.contains_key(first_parent) {
                    columns[node_col] = Some(first_parent.clone());
                } else {
                    columns[node_col] = None;
                }
            } else {
                // No parents (genesis) - column becomes empty
                columns[node_col] = None;
            }

            // Additional parents get their columns set
            for parent_hash in parents.iter().skip(1) {
                if self.blocks.contains_key(parent_hash) {
                    if let Some(&parent_col) = hash_to_column.get(parent_hash) {
                        while columns.len() <= parent_col {
                            columns.push(None);
                        }
                        columns[parent_col] = Some(parent_hash.clone());
                    }
                }
            }

            // Clean up trailing empty columns
            while columns.last() == Some(&None) && columns.len() > 1 {
                columns.pop();
            }

            self.graph_rows.push(GraphRow {
                block_hash: hash.clone(),
                node_column: node_col,
                columns: row_columns,
                edges,
            });
        }

        // Calculate max columns used
        self.max_columns = self.graph_rows.iter()
            .map(|r| r.columns.len())
            .max()
            .unwrap_or(1);
    }

    /// Get the maximum column used in the layout (for backwards compat)
    pub fn max_column(&self) -> usize {
        self.max_columns
    }

    /// For backwards compatibility with old layout field access
    pub fn layout_len(&self) -> usize {
        self.graph_rows.len()
    }

    /// Get a graph row by index
    pub fn get_row(&self, index: usize) -> Option<&GraphRow> {
        self.graph_rows.get(index)
    }
}

impl Default for Dag {
    fn default() -> Self {
        Self::new()
    }
}
