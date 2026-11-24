# Testing

The default suite runs without Docker. Linux integration tests exercise a
real shard and the production deploy queue.

## Unit tests

```bash
cargo test --release
```

This includes gRPC and HTTP fixtures for queue shutdown, storage failures,
retry metadata, and a failed inclusion lookup after a refused resend.

## Deploy queue integration

```bash
bash scripts/test-deploy-queue.sh
```

The script runs the upstream queue image pinned by digest on Linux with
Docker host networking. Reserve loopback ports 30571, 30573 and 30583, below
Linux's ephemeral range so no outgoing connection can hold them.
A validator fixture holds the chain tip at a fixed height so the actual
queue reaches its deadline and per-deployer limits reproducibly. CI runs
this test separately from the shard suite.

## Integration tests

```bash
cargo test --release --features integration --test integration
```

Requires Docker. The harness pulls the node image, records the digest it
pulled, brings up the shard, waits on chain state, runs the tests, and tears
everything down.

The `integration` feature is what makes the suite honest: without it the
target does not build at all, so a missing shard is a hard error rather than
a pass. The suite's predecessor failed exactly this way — CI set
`F1R3FLY_*` variables while the tests read `FIREFLY_*`, so every test
returned early and 28 of them "passed" in 0.63 seconds without a node.

### Options

| Variable | Effect |
|---|---|
| `FIREFLY_NODE_IMAGE` | Node image override; unset or empty uses `tests/integration/node-image.txt` |
| `FIREFLY_KEEP_SHARD` | Leave the shard up after the run, for debugging |
| `RUST_LOG` | Passed to every node; the node's default filter applies otherwise |

The immutable image in `tests/integration/node-image.txt` is the single pin,
matching the protocol dependencies in `Cargo.toml`. The harness records its
selection in `shard.env` for Compose. PR and branch checks use this pin.
The Build and Test workflow also runs nightly against the moving node `:dev`
image on amd64 and arm64 to detect upstream changes independently of PR gating.
A failed nightly run opens the issue "Nightly run against node :dev is failing",
or comments on it while it is open.
Manual workflow runs can set the `node_image` input to reproduce those checks;
locally set `FIREFLY_NODE_IMAGE=f1r3flyindustries/f1r3fly-rust:dev`.

Run one test by name:

```bash
cargo test --release --features integration --test integration -- pos::
```

The shard publishes each port on a free loopback port Docker picks at startup,
so it runs alongside anything else on the host, including another shard on
404xx. The harness reads the ports back with `docker compose port`.

### The shard

Defined in `tests/integration/topology/`, which the repository owns outright:
the whole shard uses 18 native-token decimals, including existing test groups.

| Service | Role | Published container ports |
|---|---|---|
| `boot` | runs the genesis ceremony, absent from the bond set | gRPC 40402, HTTP 40403 |
| `validator1..3` | genesis validators, stake 100 each | gRPC 40402, HTTP 40403 |
| `readonly` | observer; the only node serving exploratory deploys | gRPC 40402, HTTP 40403 |
| `validator4` | joiner, unbonded at genesis, started by the bonding tests | gRPC 40402, HTTP 40403 |
| `tls-proxy` | Caddy (`caddy:2.10-alpine`) terminating TLS in front of validator1; 40462 serves its HTTP API under `/node1` | gRPC 40460, HTTP 40461, 40462 |

Every key is generated per run and none is committed: the deploying accounts'
keys, the boot and validator keys, the bootstrap's P-256 TLS key and node ID,
and the proxy's CA and server certificate. The harness writes the genesis bonds
and wallets, each node's config carrying its validator key
(`casper.validator-private-key`), the bootstrap key and the certificates into
`target/integration-shard/`, plus `shard.env` (`IT_SHARD_DIR`,
`IT_BOOT_NODE_ID`, `FIREFLY_NODE_IMAGE`), which Compose reads through `--env-file`. To run the
topology by hand after a harness run:

```bash
docker compose --env-file target/integration-shard/shard.env \
  -f tests/integration/topology/compose.yml up -d
docker compose -p rust-client-it port validator1 40403   # validator1's HTTP API
```

The CLI under test gets its key through `FIREFLY_PRIVATE_KEY`, and runs with
every other `FIREFLY_*` variable from the caller's shell removed and no default
config file, so neither can change what is tested.

The `routing` group uses the proxy's certificates for a deploy-and-wait
over TLS with no observer, whose HTTP calls go through the `/node1` base path,
and to check that an untrusted certificate is refused.

Every deploying test owns a key funded in the genesis the harness writes,
because concurrent deploys from one deployer collide. Deploys go only to validators: a node whose validator is not bonded
accepts them and strands them (f1r3node-rust#427).

### Coverage

| Group | What it holds to account |
|---|---|
| `routing` | Which node a command talks to, on which endpoint, and over TLS |
| `deploys` | What a deploy reports on its way to a terminal verdict |
| `transfers` | Amounts moved, rejections, and what the block report says |
| `pos` | The bond set through a real joiner's whole lifecycle |
| `surface` | The remaining commands against the live shard |

`pos` runs last and alone: bonding, activation and payout move shard-wide
state, so nothing else may run beside it.

### When a run is red

A nightly or manual failure against `:dev` may reflect an upstream change.
Re-run without an image override against `tests/integration/node-image.txt`:

- **Green there** — investigate the node change; pinned PR checks remain independent.
- **Red there** — investigate the client change and the failure evidence.

The job records the digest it pulled, because the node's `/api/status` cannot
identify its own build (f1r3node-rust#428).

On failure the harness writes every container's log to
`target/integration-logs/`, and CI uploads that directory. A node that died
mid-run is named before the individual failures, since it takes every test
that talks to it down with it.
