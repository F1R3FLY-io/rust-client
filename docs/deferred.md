# Deferred Rust Client Work

## Gap 3: Registry Command

Add `registry` CLI command wrapping `GET /api/registry/{uri}`. Currently no way to look up a registry URI from the CLI — users must use `exploratory-deploy` with raw Rholang.

## Gap 4: Use Dedicated HTTP Endpoints

These commands currently construct Rholang and use exploratory deploy. They could use the new dedicated HTTP endpoints instead, which are simpler and in some cases don't require readonly nodes:

| Command | Current approach | Dedicated endpoint | Benefit |
|---|---|---|---|
| `bonds` | HTTP explore-deploy with `@PoS!("getBonds")` | `GET /api/validators` | Simpler, structured response |
| `epoch-info` | gRPC explore-deploy with `getEpochLength`/`getQuarantineLength` | `GET /api/epoch` | No readonly needed, faster |
| `bond-status` | HTTP explore-deploy, parse bonds map | `GET /api/bond-status/{pubkey}` | No readonly needed |
| `validator-status` | Multiple explore-deploys | `GET /api/validator/{pubkey}` | Single request |
| `epoch-rewards` | HTTP explore-deploy with `getCurrentEpochRewards` | `GET /api/epoch/rewards` | Simpler |
| `estimate-cost` | gRPC exploratory deploy | `POST /api/estimate-cost` | Could use HTTP instead |

The existing approach works correctly. Migration is an optimization.
