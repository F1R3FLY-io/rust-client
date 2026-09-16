# Testing

Two suites. Unit tests need nothing; the integration suite runs every test
against a real shard it creates itself.

## Unit tests

```bash
cargo test --release
```

Pure logic only — key handling, Rholang conversion, PoS and vault response
parsing, signing. No node, no Docker.

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
| `FIREFLY_NODE_IMAGE` | Node image to run. Default `f1r3flyindustries/f1r3fly-rust:dev` |
| `FIREFLY_KEEP_SHARD` | Leave the shard up after the run, for debugging |
| `RUST_LOG` | Passed to every node; the topology's own filter applies otherwise |

Run one test by name:

```bash
cargo test --release --features integration --test integration -- pos::
```

Host ports 40400-40453 must be free — they are the client's own defaults, so
a shard from another project has to be down first.

### The shard

Defined in `tests/integration/topology/`, which the repository owns outright:

| Service | Role | gRPC / HTTP |
|---|---|---|
| `it-boot` | runs the genesis ceremony, absent from the bond set | 40402 / 40403 |
| `it-validator1..3` | genesis validators, stake 100 each | 4041x / 4042x / 4043x |
| `it-readonly` | observer; the only node serving exploratory deploys | 40452 / 40453 |
| `it-validator4` | joiner, unbonded at genesis, started by the bonding tests | 40442 / 40443 |

Every deploying test owns a key funded at genesis in
`topology/genesis/wallets.txt`, because concurrent deploys from one deployer
collide. Deploys go only to validators: a node whose validator is not bonded
accepts them and strands them (f1r3node-rust#427).

### Coverage

| Group | What it holds to account |
|---|---|
| `routing` | Which node a command talks to, and on which port |
| `deploys` | What a deploy reports on its way to a terminal verdict |
| `transfers` | Amounts moved, rejections, and what the block report says |
| `pos` | The bond set through a real joiner's whole lifecycle |
| `surface` | The remaining commands against the live shard |

`pos` runs last and alone: bonding, activation and payout move shard-wide
state, so nothing else may run beside it.

### When a run is red

A failure against `:dev` is not automatically the client's fault — that tag
moves. Re-run against the previous immutable `dev-v...-g<sha>` tag:

- **Green there** — the node changed. File it upstream; do not block the
  client's PR on it.
- **Red there** — the change under test is at fault.

The job records the digest it pulled, because the node's `/api/status` cannot
identify its own build (f1r3node-rust#428).

On failure the harness writes every container's log to
`target/integration-logs/`, and CI uploads that directory. A node that died
mid-run is named before the individual failures, since it takes every test
that talks to it down with it.
