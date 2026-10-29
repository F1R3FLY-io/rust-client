//! Keys the suite deploys and validates with, generated for each run.
//!
//! The harness funds every account in the genesis it writes, so no test funds
//! another, and each deploying test owns its key so concurrent deploys never
//! share a deployer. Nothing here is committed: a key exists only for the run.

use std::sync::LazyLock;

use node_cli::utils::CryptoUtils;
use node_cli::SigningKey;

pub struct Key {
    pub private: String,
    pub public: String,
    pub address: String,
}

impl Key {
    fn generate() -> Self {
        let (secret, public) = CryptoUtils::generate_key_pair();
        let public = CryptoUtils::serialize_public_key(&public, false);
        Key {
            private: CryptoUtils::serialize_private_key(&secret),
            address: CryptoUtils::generate_vault_address(&public).expect("a fresh key's address"),
            public,
        }
    }

    pub fn signing_key(&self) -> SigningKey {
        SigningKey::from_hex(&self.private).expect("a generated key")
    }
}

type Role = LazyLock<Key>;

pub static DEPLOY_A: Role = Role::new(Key::generate);
pub static DEPLOY_B: Role = Role::new(Key::generate);
pub static DEPLOY_C: Role = Role::new(Key::generate);
pub static REPORT_SENDER: Role = Role::new(Key::generate);
pub static TRANSFER_SENDER: Role = Role::new(Key::generate);
pub static TRANSFER_RECIPIENT: Role = Role::new(Key::generate);
/// Funded with far less than the others, so a transfer of an ordinary amount
/// overdraws the vault. Two of them: the tests that overdraw run in the
/// parallel group, and concurrent deploys from one deployer collide.
pub static OVERDRAFT_TRANSFER: Role = Role::new(Key::generate);
pub static OVERDRAFT: Role = Role::new(Key::generate);
/// Sends one signed deploy more than once.
pub static RESUBMIT: Role = Role::new(Key::generate);
/// Sends one signed deploy again after its validity window has closed.
pub static RESEND_EXPIRED: Role = Role::new(Key::generate);
/// Deploys through the TLS proxy.
pub static TLS_DEPLOY: Role = Role::new(Key::generate);
pub static LOAD_TEST: Role = Role::new(Key::generate);
pub static SPARE: Role = Role::new(Key::generate);
/// Nothing deploys from this address and nothing else credits it, so a test
/// can assert that its balance did not move.
pub static IDLE_RECIPIENT: Role = Role::new(Key::generate);
pub static ESTIMATE_COST: Role = Role::new(Key::generate);

/// Runs the genesis ceremony; not bonded.
pub static BOOT: Role = Role::new(Key::generate);
/// Bonded at genesis.
pub static VALIDATOR1: Role = Role::new(Key::generate);
pub static VALIDATOR2: Role = Role::new(Key::generate);
pub static VALIDATOR3: Role = Role::new(Key::generate);
/// The joiner, unbonded at genesis but funded, so bonding needs no transfer
/// first.
pub static VALIDATOR4: Role = Role::new(Key::generate);

/// The secp256k1 generator point: a well-formed public key that no shard can
/// have bonded, for the negative side of the bond-status assertions.
pub const NEVER_BONDED_PUBLIC: &str = "0479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8";

const FUNDED: &str = "50000000000000000";
const SCARCELY_FUNDED: &str = "1000000";

/// Each vault funded at genesis: its role's name, key and balance.
pub fn genesis_wallets() -> [(&'static str, &'static Key, &'static str); 16] {
    [
        ("deploy-a", &DEPLOY_A, FUNDED),
        ("deploy-b", &DEPLOY_B, FUNDED),
        ("deploy-c", &DEPLOY_C, FUNDED),
        ("report-sender", &REPORT_SENDER, FUNDED),
        ("transfer-sender", &TRANSFER_SENDER, FUNDED),
        ("transfer-recipient", &TRANSFER_RECIPIENT, FUNDED),
        ("resubmit", &RESUBMIT, FUNDED),
        ("resend-expired", &RESEND_EXPIRED, FUNDED),
        ("tls-deploy", &TLS_DEPLOY, FUNDED),
        ("load-test", &LOAD_TEST, FUNDED),
        ("spare", &SPARE, FUNDED),
        ("idle-recipient", &IDLE_RECIPIENT, FUNDED),
        ("estimate-cost", &ESTIMATE_COST, FUNDED),
        ("validator4", &VALIDATOR4, FUNDED),
        ("overdraft-transfer", &OVERDRAFT_TRANSFER, SCARCELY_FUNDED),
        ("overdraft", &OVERDRAFT, SCARCELY_FUNDED),
    ]
}

/// The validators bonded at genesis.
pub fn genesis_bonds() -> [&'static Key; 3] {
    [&VALIDATOR1, &VALIDATOR2, &VALIDATOR3]
}
