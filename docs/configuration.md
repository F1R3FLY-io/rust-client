# Configuration

Every endpoint, transport and key setting of the CLI can come from a flag, an
environment variable, or a named profile in a config file. Each setting is
resolved in this order:

1. the flag
2. its environment variable
3. the selected profile
4. the built-in default

The signing key is the exception: a key file, from any of these, together
with `FIREFLY_PRIVATE_KEY` is an error rather than one overriding the other,
so a command never signs with a key you did not mean.

## Config file

TOML at `$XDG_CONFIG_HOME/f1r3fly/node_cli.toml`, else
`~/.config/f1r3fly/node_cli.toml`, or the path given by `--config <PATH>`
(env `FIREFLY_CONFIG`). `--profile <NAME>` (env `FIREFLY_PROFILE`) selects a
profile; without it, the file's `default_profile` is used, and with neither,
no profile applies.

```toml
default_profile = "rgb"

[profiles.rgb]
grpc = "https://validator.example:40401"
http = "https://validator.example:40403"
readonly_http = "https://observer.example:40403"
timeout = 60

[profiles.private]
grpc = "https://node.internal.example:40401"
http = "https://node.internal.example:40403"
readonly_grpc = "https://observer.internal.example:40401"
readonly_http = "https://observer.internal.example:40403"
ca_cert = "private-ca.pem"                   # relative to this file
private_key_file = "~/.keys/private.hex"     # ~/ expands to HOME

[profiles.local]
grpc = "http://localhost:40412"
http = "http://localhost:40413"
internal_grpc = "http://localhost:40412"
observer = "http://localhost:40453"
readonly_grpc = "http://localhost:40452"
readonly_http = "http://localhost:40453"
```

| Key | Sets | Used by |
|---|---|---|
| `grpc` | `--grpc` | commands that deploy or query a node over gRPC |
| `http` | `--http` | commands that query a node over HTTP |
| `internal_grpc` | `propose --grpc` | `propose` |
| `observer` | `--observer` | deploy-and-wait, transfer, bond-validator, unbond-validator |
| `readonly_grpc` | `--grpc` of read-only commands, load-test `--readonly-grpc` | exploratory-deploy, wallet-balance, epoch-info, load-test |
| `readonly_http` | `--http` of read-only commands | bonds, active-validators, validator-status, epoch-rewards, network-consensus, block-transfers |
| `ca_cert` | `--ca-cert` | every command with an endpoint |
| `connect_timeout`, `timeout` | `--connect-timeout`, `--timeout` | every command with an endpoint |
| `shard_id` | `--shard-id` | deploying commands |
| `private_key_file` | `--private-key-file` | commands that sign, and key commands |

A profile's `readonly_grpc` or `readonly_http` satisfies the endpoint the
read-only commands otherwise require. A private key never goes in the file;
only the path of a file holding it does.

The file is read strictly: an unknown key, an unknown profile, an explicit
`--config` that does not exist, and `--profile` with no config file are all
errors. A missing file at the default location is not.

Profiles are a CLI feature. The library reads only the environment, through
`ConnectionConfig::from_env`.

## Environment variables

| Variable | Flag |
|---|---|
| `FIREFLY_GRPC_URL` | `--grpc` (deploy node) |
| `FIREFLY_HTTP_URL` | `--http` (deploy node) |
| `FIREFLY_OBSERVER_URL` | `--observer` |
| `FIREFLY_INTERNAL_GRPC_URL` | `propose --grpc` |
| `FIREFLY_READONLY_GRPC_URL` | `--grpc` of read-only commands, load-test `--readonly-grpc` |
| `FIREFLY_READONLY_HTTP_URL` | `--http` of read-only commands |
| `FIREFLY_CA_CERT` | `--ca-cert` |
| `FIREFLY_CONNECT_TIMEOUT` | `--connect-timeout` |
| `FIREFLY_REQUEST_TIMEOUT` | `--timeout` |
| `FIREFLY_SHARD_ID` | `--shard-id` |
| `FIREFLY_PRIVATE_KEY_FILE` | `--private-key-file` |
| `FIREFLY_PRIVATE_KEY` | none: the key's hex; an error if a key file is also set |
| `FIREFLY_CONFIG` | `--config` |
| `FIREFLY_PROFILE` | `--profile` |

The read-only commands have their own variables because `FIREFLY_HTTP_URL`
names the deploy node, which refuses their queries.

## Signing keys

A key is never passed as a flag value, which would leave it in shell history
and the process list. A command that signs reads it from
`--private-key-file <PATH>` (env `FIREFLY_PRIVATE_KEY_FILE`, or a profile's
`private_key_file`), or from `FIREFLY_PRIVATE_KEY`; both set is an error, so a
command never signs with a key you did not mean. The file holds the key's
64 hex characters; on Unix it is refused if group or others can read it, as ssh
refuses its keys:

```bash
node_cli generate-key-pair --save --output-dir ~/.keys   # writes private_key.hex with mode 0600
node_cli deploy-and-wait -f contract.rho --private-key-file ~/.keys/private_key.hex
```

## config show

`node_cli config show` prints each setting's resolved value and where it came
from, the signing key's source and its vault address (never the key itself). It
takes the same flags and variables as the commands, plus `--internal-grpc`,
`--readonly-grpc` and `--readonly-http`. Here the `rgb` profile above is the
default and `FIREFLY_PRIVATE_KEY` is exported:

```
$ node_cli config show --observer http://localhost:40453

Profile: rgb (/home/user/.config/f1r3fly/node_cli.toml)

  grpc             https://validator.example:40401              (profile rgb)
  http             https://validator.example:40403              (profile rgb)
  observer         http://localhost:40453                       (flag)
  internal grpc    http://localhost:40402                       (default)
  read-only grpc   not set
  read-only http   https://observer.example:40403               (profile rgb)
  ca cert          not set
  connect timeout  10s                                          (default)
  request timeout  60s                                          (profile rgb)
  shard id         root                                         (default)
  signing key      FIREFLY_PRIVATE_KEY                          (environment)
  vault address    11112rf69FZUG2nDGkmb8r4nSQZbb8krBBx18QzZVnQLUXZoCJihpV
```

It exits non-zero, listing each setting that cannot be used, such as
`--ca-cert` with an `http://` endpoint or a key file others can read.
