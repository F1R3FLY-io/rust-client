# Bonding a joiner validator

How to take a fourth validator from "not running" to "producing blocks", and
back out again, using this client. The integration suite performs exactly this
sequence in `tests/integration/pos.rs`, so it is kept honest by CI.

The shard used below is the one this repository owns,
`tests/integration/topology/compose.yml`, which holds the joiner behind a
compose profile. The ports are the standard shard layout, so the commands
apply unchanged to any shard with a fourth validator.

| Node | gRPC | HTTP |
|---|---|---|
| validator1 | 40412 | 40413 |
| validator4 (joiner) | 40442 | 40443 |
| read-only observer | 40452 | 40453 |

Two rules hold throughout:

- **Deploys go to a validator.** A node whose validator is not bonded accepts
  a deploy and strands it, because nothing can ever include it
  (f1r3node-rust#427). The joiner is unbonded until the bond takes effect, so
  its own bond deploy goes to validator1.
- **Read-only queries go to the observer.** Only a read-only node serves the
  exploratory deploys that `bonds`, `active-validators`, `validator-status`
  and `epoch-info` are built on.

## 1. Check the shard before you start

```bash
node_cli bonds -p 40453
node_cli active-validators -p 40453
node_cli epoch-info -p 40452 --http-port 40453
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
where it is not:

```bash
node_cli transfer \
  --to-address <joiner vault address> \
  --amount 100000000 \
  --private-key <funded key> \
  -H localhost -p 40412 --http-port 40413 \
  --observer-host localhost --observer-port 40452 --observer-http-port 40453
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
 WITHDRAWAL REQUESTED
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

**The bond command times out.** It waits for the deploy to reach a terminal
verdict. Confirm the deploy was included at all:

```bash
node_cli deploy-status --sig <deploy id> --http-port 40453
```

If it reports `Pending` with no block, the deploy went somewhere that cannot
include it — check that `-p` pointed at a bonded validator's gRPC port.

**The bond succeeded but the validator never activates.** Compare the bond set
against the active set, and check the shard's number of active validators. If
the cap is already met, the bond will not activate until a slot frees.

**`bonds` fails with `readonly_node_required`.** The query ran against a
validator. Point `-p` at the read-only node's HTTP port (40453).

**The joiner runs but never proposes.** Being bonded is not enough — it has to
be in the *active* set, which only changes at an epoch boundary. Note that
`/api/status` cannot be used to check this: its `isValidator` field reports
whether autopropose is enabled, not whether the node is a validator
(f1r3node-rust#429). Use `validator-status`.
