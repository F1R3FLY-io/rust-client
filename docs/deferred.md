# Deferred Rust Client Work

## Gap 3: Registry Command

Add `registry` CLI command wrapping `GET /api/registry/{uri}`. Currently no way to look up a registry URI from the CLI — users must use `exploratory-deploy` with raw Rholang.

## Gap 4: Use Dedicated HTTP Endpoints

These commands currently construct Rholang and use exploratory deploy. They could use the new dedicated HTTP endpoints instead, which are simpler and in some cases don't require readonly nodes:

| Command | Current approach | Dedicated endpoint | Benefit |
|---|---|---|---|
| `epoch-info` | gRPC explore-deploy with `getEpochLength`/`getQuarantineLength` | `GET /api/epoch` | No readonly needed, faster |
| `epoch-rewards` | HTTP explore-deploy with `getCurrentEpochRewards` | `GET /api/epoch/rewards` | Simpler |
| `estimate-cost` | gRPC exploratory deploy | `POST /api/estimate-cost` | Could use HTTP instead |

`bond-status` already uses `GET /api/bond-status/{pubkey}`. `bonds`, `active-validators`, `validator-status`, and `network-consensus` stay on the PoS snapshot query: `GET /api/validators` and `GET /api/validator/{pubkey}` report a single validator set, without the activation and withdrawal state those commands show.
