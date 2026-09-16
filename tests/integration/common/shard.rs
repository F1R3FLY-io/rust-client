//! Lifecycle of the shard the suite runs against.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use super::node::Node;

/// Compose project name, matching `topology/compose.yml`.
const PROJECT: &str = "rust-client-it";

/// Chain height every node must reach before the suite starts. Genesis plus a
/// few blocks, so the PoS contract is queryable and finalization has run.
const READY_HEIGHT: i64 = 3;

pub struct Shard {
    topology: PathBuf,
    keep: bool,
}

impl Shard {
    /// Pull the node image, start the shard, and wait for every node to
    /// finalize past [`READY_HEIGHT`].
    pub fn start() -> Result<Self, String> {
        let topology = topology_dir();
        let keep = std::env::var_os("FIREFLY_KEEP_SHARD").is_some();
        let shard = Shard { topology, keep };

        let image = image();
        run(Command::new("docker").args(["pull", &image]))?;
        println!("node image {image} at {}", shard.image_digest(&image));

        shard.compose(&["up", "-d"])?;
        shard.wait_until_ready(Node::all(), Duration::from_secs(180))?;
        Ok(shard)
    }

    /// Start the joiner validator, which the topology holds behind a profile.
    pub fn start_joiner(&self) -> Result<(), String> {
        self.compose(&["--profile", "joiner", "up", "-d"])?;
        self.wait_until_ready(&[Node::Validator4], Duration::from_secs(180))
    }

    /// Block until every node reports a finalized height of at least
    /// [`READY_HEIGHT`], failing with the logs of any container that exited.
    fn wait_until_ready(&self, nodes: &[Node], budget: Duration) -> Result<(), String> {
        let deadline = Instant::now() + budget;
        let mut waiting = nodes.to_vec();

        while !waiting.is_empty() {
            if let Some(exited) = self.exited_container()? {
                return Err(format!(
                    "{exited} exited during startup:\n{}",
                    self.logs(&exited)
                ));
            }
            if Instant::now() > deadline {
                return Err(format!(
                    "{waiting:?} did not reach block {READY_HEIGHT} within {budget:?}"
                ));
            }
            waiting.retain(|node| node.finalized_height().is_none_or(|h| h < READY_HEIGHT));
            if !waiting.is_empty() {
                std::thread::sleep(Duration::from_secs(2));
            }
        }
        Ok(())
    }

    /// The name of a shard container that is no longer running, if any. A node
    /// that refuses its configuration exits during boot, and one that dies
    /// mid-run fails every test that talks to it - both show up as something
    /// other than their own cause unless this is reported.
    pub fn exited_container(&self) -> Result<Option<String>, String> {
        let listing = self.compose_output(&["ps", "--all", "--format", "{{.Name}} {{.State}}"])?;
        Ok(listing
            .lines()
            .filter_map(|line| line.split_once(' '))
            .find(|(_, state)| *state != "running")
            .map(|(name, _)| name.to_string()))
    }

    pub fn logs(&self, container: &str) -> String {
        self.compose_output(&["logs", "--no-color", container])
            .unwrap_or_else(|e| format!("could not read logs: {e}"))
    }

    /// Write every container's logs into `dir`, for a failed run's artifacts.
    pub fn dump_logs(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        for name in self
            .compose_output(&["ps", "--all", "--format", "{{.Name}}"])?
            .lines()
        {
            std::fs::write(dir.join(format!("{name}.log")), self.logs(name))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn image_digest(&self, image: &str) -> String {
        Command::new("docker")
            .args(["image", "inspect", "--format", "{{index .RepoDigests 0}}", image])
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
            .unwrap_or_else(|| "unknown digest".to_string())
    }

    fn compose(&self, args: &[&str]) -> Result<(), String> {
        run(&mut self.compose_command(args))
    }

    fn compose_output(&self, args: &[&str]) -> Result<String, String> {
        let output = self
            .compose_command(args)
            .output()
            .map_err(|e| format!("docker compose: {e}"))?;
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn compose_command(&self, args: &[&str]) -> Command {
        let mut command = Command::new("docker");
        command
            .args(["compose", "-p", PROJECT, "-f"])
            .arg(self.topology.join("compose.yml"))
            .args(args);
        command
    }
}

impl Drop for Shard {
    fn drop(&mut self) {
        if self.keep {
            println!("FIREFLY_KEEP_SHARD set; leaving the shard up");
            return;
        }
        let _ = self.compose(&["--profile", "joiner", "down", "-v", "--remove-orphans"]);
    }
}

pub fn image() -> String {
    std::env::var("FIREFLY_NODE_IMAGE")
        .unwrap_or_else(|_| "f1r3flyindustries/f1r3fly-rust:dev".to_string())
}

fn topology_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/integration/topology")
}

fn run(command: &mut Command) -> Result<(), String> {
    let output = command
        .output()
        .map_err(|e| format!("{:?}: {e}", command.get_program()))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "{:?} failed ({}): {}",
        command.get_program(),
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}
