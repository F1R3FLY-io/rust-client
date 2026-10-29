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

        write_shard_files(&shard_dir(), &shard.topology)?;
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
            if let Some(exited) = self.exited_service()? {
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

    /// The compose service of a shard node that is no longer running, if any. A
    /// node that refuses its configuration exits during boot, and one that dies
    /// mid-run fails every test that talks to it - both show up as something
    /// other than their own cause unless this is reported.
    pub fn exited_service(&self) -> Result<Option<String>, String> {
        let listing =
            self.compose_output(&["ps", "--all", "--format", "{{.Service}} {{.State}}"])?;
        Ok(listing
            .lines()
            .filter_map(|line| line.split_once(' '))
            .find(|(_, state)| *state != "running")
            .map(|(name, _)| name.to_string()))
    }

    /// Logs of a compose service. `docker compose logs` takes service names,
    /// such as `boot`, not container names, such as `it-boot`.
    pub fn logs(&self, service: &str) -> String {
        self.compose_output(&["logs", "--no-color", service])
            .unwrap_or_else(|e| format!("could not read logs: {e}"))
    }

    /// Write every node's logs into `dir`, for a failed run's artifacts.
    pub fn dump_logs(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        for service in self
            .compose_output(&["ps", "--all", "--format", "{{.Service}}"])?
            .lines()
        {
            std::fs::write(dir.join(format!("{service}.log")), self.logs(service))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn image_digest(&self, image: &str) -> String {
        Command::new("docker")
            .args([
                "image",
                "inspect",
                "--format",
                "{{index .RepoDigests 0}}",
                image,
            ])
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
        if !output.status.success() {
            return Err(format!(
                "docker compose {}: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn compose_command(&self, args: &[&str]) -> Command {
        let mut command = Command::new("docker");
        command
            .args(["compose", "-p", PROJECT, "--env-file"])
            .arg(shard_dir().join("shard.env"))
            .arg("-f")
            .arg(self.topology.join("compose.yml"))
            .args(args);
        command
    }
}

/// The certificate authority the TLS proxy's certificate is issued by, for
/// `--ca-cert`.
pub fn ca_certificate() -> PathBuf {
    shard_dir().join("tls/ca.pem")
}

/// Where the harness writes everything the topology reads per run.
fn shard_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("target/integration-shard")
}

/// Write the run's generated keys and the files that carry them: genesis
/// bonds and wallets, each node's config with its validator key, the
/// bootstrap's TLS key, the proxy's certificates, and `shard.env` for compose.
fn write_shard_files(dir: &Path, topology: &Path) -> Result<(), String> {
    use super::keys;

    let write = |path: PathBuf, contents: &str| -> Result<(), String> {
        std::fs::create_dir_all(path.parent().expect("a file in a directory"))
            .map_err(|e| e.to_string())?;
        std::fs::write(&path, contents).map_err(|e| format!("{}: {e}", path.display()))
    };
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }

    let bonds: String = keys::genesis_bonds()
        .iter()
        .map(|key| format!("{} 100\n", key.public))
        .collect();
    write(dir.join("genesis/bonds.txt"), &bonds)?;
    let wallets: String = keys::genesis_wallets()
        .iter()
        .map(|(_, key, balance)| format!("{},{balance}\n", key.address))
        .collect();
    write(dir.join("genesis/wallets.txt"), &wallets)?;
    // Each funded key, readable by its owner only, for working the shard by
    // hand, as the bonding guide does.
    for (role, key, _) in keys::genesis_wallets() {
        let path = dir.join(format!("keys/{role}.hex"));
        write(path.clone(), &key.private)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }

    let node_conf = std::fs::read_to_string(topology.join("node.conf"))
        .map_err(|e| format!("node.conf: {e}"))?;
    for (node, key) in [
        ("boot", &*keys::BOOT),
        ("validator1", &*keys::VALIDATOR1),
        ("validator2", &*keys::VALIDATOR2),
        ("validator3", &*keys::VALIDATOR3),
        ("validator4", &*keys::VALIDATOR4),
    ] {
        write(
            dir.join(format!("conf/{node}.conf")),
            &with_validator_key(&node_conf, &key.private)?,
        )?;
    }

    let boot_node_id = write_bootstrap_identity(&dir.join("bootstrap"))?;
    issue_tls_certificates(&dir.join("tls"))?;
    write(
        dir.join("shard.env"),
        &format!(
            "IT_SHARD_DIR={}\nIT_BOOT_NODE_ID={boot_node_id}\n",
            dir.display()
        ),
    )
}

/// `node_conf` with `key` as the node's validator key, inside its `casper`
/// block, so the key reaches the node through its config rather than its
/// command line.
fn with_validator_key(node_conf: &str, key: &str) -> Result<String, String> {
    let block = "casper {";
    let at = node_conf
        .find(block)
        .ok_or("node.conf has no casper block for the validator key")?
        + block.len();
    Ok(format!(
        "{}\n  validator-private-key = \"{key}\"{}",
        &node_conf[..at],
        &node_conf[at..]
    ))
}

/// Generate the bootstrap's TLS key, from which the node builds its
/// certificate, and return the node ID other nodes dial: the last 20 bytes of
/// the Keccak-256 of its uncompressed public key, without the 0x04 prefix.
fn write_bootstrap_identity(dir: &Path) -> Result<String, String> {
    use sha3::Digest;

    let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)
        .map_err(|e| format!("bootstrap key: {e}"))?;
    let public = key.public_key_raw();
    let hash = sha3::Keccak256::digest(&public[1..]);
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("node.key.pem"), key.serialize_pem())
        .map_err(|e| format!("bootstrap key: {e}"))?;
    Ok(hex::encode(&hash[hash.len() - 20..]))
}

/// Issue a fresh CA and a `localhost` server certificate for the TLS proxy.
/// The CA's key is never written, so nothing else can be issued under it.
fn issue_tls_certificates(dir: &Path) -> Result<(), String> {
    use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, KeyPair};

    let failed = |e: rcgen::Error| format!("issuing TLS certificates: {e}");
    let ca_key = KeyPair::generate().map_err(failed)?;
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).map_err(failed)?;
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "rust-client integration CA");
    let ca = ca_params.self_signed(&ca_key).map_err(failed)?;

    let server_key = KeyPair::generate().map_err(failed)?;
    let server = CertificateParams::new(vec!["localhost".to_string()])
        .map_err(failed)?
        .signed_by(&server_key, &ca, &ca_key)
        .map_err(failed)?;

    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for (name, pem) in [
        ("ca.pem", ca.pem()),
        ("server.pem", server.pem()),
        ("server.key", server_key.serialize_pem()),
    ] {
        std::fs::write(dir.join(name), pem).map_err(|e| format!("{name}: {e}"))?;
    }
    Ok(())
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
