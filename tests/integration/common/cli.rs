//! Running the client binary the way a user would.

use std::process::Command;

use super::node::Node;

pub struct Output {
    pub status: std::process::ExitStatus,
    /// stdout and stderr merged: the client writes results to one and tracing
    /// to the other, and assertions care about what the user sees.
    pub text: String,
}

impl Output {
    pub fn success(&self) -> bool {
        self.status.success()
    }

    pub fn contains(&self, needle: &str) -> bool {
        self.text.contains(needle)
    }

    /// Fail with the whole output, which is the only way to tell why a CLI
    /// assertion missed.
    pub fn expect_success(self, what: &str) -> Self {
        assert!(
            self.success(),
            "{what} exited {}:\n{}",
            self.status,
            self.text
        );
        self
    }

    pub fn expect_failure(self, what: &str) -> Self {
        assert!(
            !self.success(),
            "{what} unexpectedly succeeded:\n{}",
            self.text
        );
        self
    }

    /// The value the client printed after `label`, from its `Label: value`
    /// output lines.
    pub fn value_after(&self, label: &str) -> &str {
        self.text
            .lines()
            .find_map(|line| line.split_once(label)?.1.split(',').next())
            .map(str::trim)
            .unwrap_or_else(|| panic!("no {label:?} in output:\n{}", self.text))
    }

    pub fn expect_contains(self, needle: &str) -> Self {
        assert!(
            self.contains(needle),
            "output does not contain {needle:?}:\n{}",
            self.text
        );
        self
    }
}

/// A `node_cli` invocation against the shard.
pub struct Cli {
    args: Vec<String>,
}

impl Cli {
    pub fn new(command: &str) -> Self {
        Cli {
            args: vec![command.to_string()],
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I: IntoIterator<Item = S>, S: Into<String>>(mut self, args: I) -> Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// For commands taking both ports: `-p` is gRPC, `--http-port` is HTTP.
    pub fn at(self, node: Node) -> Self {
        self.grpc_at(node)
            .args(["--http-port".to_string(), node.http_port().to_string()])
    }

    /// For commands whose only `-p` is the gRPC port.
    pub fn grpc_at(self, node: Node) -> Self {
        self.args([
            "-H".to_string(),
            "localhost".to_string(),
            "-p".to_string(),
            node.grpc_port().to_string(),
        ])
    }

    /// For the query commands, whose only `-p` is the HTTP port.
    pub fn http_at(self, node: Node) -> Self {
        self.args([
            "-H".to_string(),
            "localhost".to_string(),
            "-p".to_string(),
            node.http_port().to_string(),
        ])
    }

    /// For commands that name the HTTP port `--http-port` and have no `-p`.
    pub fn http_port_at(self, node: Node) -> Self {
        self.args([
            "-H".to_string(),
            "localhost".to_string(),
            "--http-port".to_string(),
            node.http_port().to_string(),
        ])
    }

    /// Add the observer flags, which every deploying command needs: only the
    /// read-only node serves the endpoints finalization waits on.
    pub fn observing(self, node: Node) -> Self {
        self.args([
            "--observer-host".to_string(),
            "localhost".to_string(),
            "--observer-port".to_string(),
            node.grpc_port().to_string(),
            "--observer-http-port".to_string(),
            node.http_port().to_string(),
        ])
    }

    pub fn key(self, key: &super::keys::Key) -> Self {
        self.args(["--private-key".to_string(), key.private.to_string()])
    }

    pub fn run(self) -> Output {
        let output = Command::new(env!("CARGO_BIN_EXE_node_cli"))
            .args(&self.args)
            .output()
            .unwrap_or_else(|e| panic!("could not run node_cli {:?}: {e}", self.args));

        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        Output {
            status: output.status,
            text,
        }
    }
}
