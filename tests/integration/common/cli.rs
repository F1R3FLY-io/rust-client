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
    env: Vec<(&'static str, String)>,
}

impl Cli {
    pub fn new(command: &str) -> Self {
        Cli {
            args: vec![command.to_string()],
            env: Vec::new(),
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

    /// For commands that reach the node over both gRPC and HTTP.
    pub fn at(self, node: Node) -> Self {
        self.grpc_at(node).http_at(node)
    }

    pub fn grpc_at(self, node: Node) -> Self {
        self.args(["--grpc".to_string(), node.grpc_endpoint()])
    }

    pub fn http_at(self, node: Node) -> Self {
        self.args(["--http".to_string(), node.http_endpoint()])
    }

    /// Poll `node` for the verdict alongside the deploy node.
    pub fn observing(self, node: Node) -> Self {
        self.args(["--observer".to_string(), node.http_endpoint()])
    }

    /// Sign with `key`, passed through the environment as a user would rather
    /// than on the command line.
    pub fn key(mut self, key: &super::keys::Key) -> Self {
        self.env
            .push(("FIREFLY_PRIVATE_KEY", key.private.to_string()));
        self
    }

    /// Run with only this invocation's settings: `FIREFLY_*` variables from
    /// the caller's shell are removed, and the default config file location
    /// points nowhere, so neither can change what is tested.
    pub fn run(self) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_node_cli"));
        for (var, _) in std::env::vars_os() {
            if var.to_string_lossy().starts_with("FIREFLY_") {
                command.env_remove(var);
            }
        }
        command.env(
            "XDG_CONFIG_HOME",
            concat!(env!("CARGO_MANIFEST_DIR"), "/target/integration-no-config"),
        );
        let output = command
            .args(&self.args)
            .envs(self.env.iter().map(|(var, value)| (var, value)))
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
