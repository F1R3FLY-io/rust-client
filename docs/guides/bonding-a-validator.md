# Bonding a validator

How to take a new validator from "not running" to "producing blocks", and back
out again, using this client. `tests/integration/pos.rs` covers the bond,
activation and unbond steps; funding (step 3) and payout (step 7) are not
exercised by CI.

The shard used below is the one this repository owns,
`tests/integration/topology/compose.yml`, which holds a joiner behind a compose
profile and calls it validator4. The ports are the standard shard layout, so
the commands apply unchanged to any shard you are adding a validator to.

| Node | gRPC | HTTP |
|---|---|---|
| validator1 | 40412 | 40413 |
| joiner (validator4 here) | 40442 | 40443 |
| read-only observer | 40452 | 40453 |

Three rules hold throughout:

- **Deploys go to a bonded validator.** Any other node accepts a deploy and
  strands it, because nothing can ever include it (f1r3node-rust#427). This
  includes a bootstrap or ceremony-master node, which is typically not in the
  bond set at all. The joiner is unbonded until the bond takes effect, so its
  own bond deploy goes to validator1.
- **Read-only queries go to the observer.** Only a read-only node serves the
  exploratory deploys that `bonds`, `active-validators`, `validator-status`
  and `epoch-info` are built on.
- **Queries are split across two transports.** `bonds`, `active-validators`,
  `validator-status` and `network-consensus` read over HTTP, so they take the
  observer's **HTTP** port; `epoch-info` and `wallet-balance` use gRPC and take
  its **gRPC** port. Passing the wrong one fails with a connection error
  naming `/api/explore-deploy`. See
  [the port and transport table](../commands/advanced.md#port-and-transport).

## 1. Check the shard before you start

```bash
node_cli bonds -p 40453
node_cli active-validators -p 40453
node_cli epoch-info -p 40452
```

`epoch-info` reports the two numbers that decide how long each step below
takes: the epoch length, and the quarantine length. The PoS contract applies
bonds, withdrawals and payouts **only at an epoch boundary** — nothing you do
between boundaries takes effect until the next one.

## 2. Start the joiner

```bash
docker compose -f tests/integration/topology/compose.yml --profile joiner up -d
```

Wait until it answers and has caught up:

```bash
curl -s localhost:40443/api/status
```

The node is running but not bonded. It will not propose, and `bond-status`
reports it as not bonded.

## 3. Fund it, if it is not funded already

The joiner pays for its own bond deploy, so its vault needs a balance. In this
topology it is funded at genesis and this step is unnecessary. On a shard
where it is not, the vault needs

```
phlo limit × phlo price   +   the stake
```

at the moment the bond is submitted. `bond-validator` deploys with the bigger
phlo limit, **5,000,000,000**, and the default phlo price is 1 — so budget at
least 5,000,000,000 plus the stake, not merely enough to cover the stake. An
under-funded vault does not reject the deploy: it is included in a block and
then fails there, with
`systemDeployError: "Deploy payment failed: Insufficient funds"`.

Derive the vault address from the joiner's public key, then transfer:

```bash
node_cli generate-vault-address --public-key <joiner public key>

node_cli transfer \
  --to-address <joiner vault address> \
  --amount 100000000000 \
  --private-key <funded key> \
  -H localhost -p 40412 --http-port 40413 \
  --observer-host localhost --observer-port 40452 --observer-http-port 40453
```

Confirm it landed before bonding — `wallet-balance` takes the observer's gRPC
port:

```bash
node_cli wallet-balance --address <joiner vault address> -p 40452
```

## 4. Bond

```bash
node_cli bond-validator \
  --stake 100 \
  --private-key <joiner private key> \
  -H localhost -p 40412 --http-port 40413 \
  --observer-host localhost --observer-port 40452 --observer-http-port 40453 \
  --max-wait 180
```

The stake must be at least the shard's `bond-minimum`. Bonding twice with the
same key fails with "already bonded".

Immediately afterwards the joiner is in the bond set but **not** in the active
set:

```bash
node_cli bonds -p 40453              # now lists four validators
node_cli active-validators -p 40453  # still lists three
```

`bonds` marks it `(not active)`. That is not a queue position: the active set
is capped at the shard's number of active validators, so a bond can sit there
indefinitely if the cap is already full.

## 5. Wait for the epoch boundary

At the next boundary the active set is recomputed and the joiner enters it:

```bash
node_cli validator-status --public-key <joiner public key> --http-port 40453
```

```
 BONDED: stake 100
 ACTIVE: participating in consensus
```

From here it is a block producer; its public key appears as the `validator`
of blocks it authors:

```bash
node_cli blocks -p 40413
```

## 6. Unbond

```bash
node_cli unbond-validator \
  --private-key <joiner private key> \
  -H localhost -p 40412 --http-port 40413 \
  --observer-host localhost --observer-port 40452 --observer-http-port 40453 \
  --max-wait 180
```

`validator-status` now reports a requested withdrawal and the block its
quarantine ends at:

```
 WITHDRAWAL REQUESTED: leaves the bond set at the next epoch boundary; stake
 and rewards are paid out at the first epoch boundary at or after block <N>
```

The quarantine end is `quarantine-length + epoch-length * (1 + B / epoch-length)`,
where `B` is the block the withdrawal was recorded in — in other words, the
quarantine measured from the end of the epoch that carried the request.

At the next boundary the withdrawal is applied: the joiner leaves the bond set
and the active set, and the shard keeps finalizing without it. Unbonding a key
that is not bonded fails.

## 7. Payout

Stake plus accumulated rewards is returned at the first epoch boundary at or
after the quarantine end. Check the vault:

```bash
node_cli wallet-balance --address <joiner vault address> -p 40452
```

The balance rises by more than the stake, because committed rewards are paid
with it.

## Troubleshooting

**The bond command times out.** `--max-wait` elapsed without a terminal
verdict. The message says nothing about the cause, so check the deploy itself
before assuming the wait was too short:

```bash
curl -s localhost:40453/api/deploy/<deploy id>
```

- `errored: true` with `systemDeployError: "Deploy payment failed: Insufficient
  funds"` — the vault could not cover phlo limit × phlo price plus the stake.
  Top it up (step 3) and bond again. The deploy is spent; it is not retried.
- A block hash and no error — the deploy is fine and the canonical verdict has
  not been written yet. That verdict is deliberately withheld until the
  finalized floor passes the deploy's contestability bound, which is
  `deploy-lifespan` blocks past inclusion at minimum, so a correct deploy can
  outlast a short `--max-wait`. Re-run `deploy-status` rather than re-bonding.
- No block at all — the deploy went somewhere that cannot include it. Check
  that `-p` pointed at a **bonded** validator's gRPC port.

The client does not currently surface the first case; it reports the bare
timeout even when the node has already recorded the failure (rust-client#37).

**The bond succeeded but the validator never activates.** Compare the bond set
against the active set, and check the shard's number of active validators. If
the cap is already met, the bond will not activate until a slot frees.

**A query fails with `Exploratory deploy can only be executed on read-only
node`.** The query ran against a validator. Point `-p` at the read-only node —
the HTTP port (40453) for `bonds`, `active-validators`, `validator-status` and
`network-consensus`, the gRPC port (40452) for `epoch-info` and
`wallet-balance`.

**A query fails with `error sending request for url (…/api/explore-deploy)`.**
The command reads over HTTP but was given a gRPC port. Use 40453.

**The joiner runs but never proposes.** Being bonded is not enough — it has to
be in the *active* set, which only changes at an epoch boundary. Note that
`/api/status` cannot be used to check this: its `isValidator` field reports
whether autopropose is enabled, not whether the node is a validator
(f1r3node-rust#429). Use `validator-status`.
