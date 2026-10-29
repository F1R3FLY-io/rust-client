# Key Management Commands

Offline commands for cryptographic key operations. No running node required.

## generate-key-pair

Generate a new secp256k1 private/public key pair.

```bash
node_cli generate-key-pair [--compressed] [--save] [--output-dir DIR]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--compressed` | false | Output compressed public key |
| `--save` | false | Save the keys to `private_key.hex` (mode 0600) and `public_key.hex` instead of printing them |
| `--output-dir` | `.` | Directory for saved keys |

```
$ node_cli generate-key-pair

Private key: <64 hex characters>
Public key (uncompressed): 046adcae2b3d8b22edf207351db32ad481aee1cc9987d2b2bcde9769a3fb52b60ca4da6008653dc056e9c5525405698e17764dcd86ef28ffee5b3fdc89e5d6f9bd
```

```
$ node_cli generate-key-pair --save --output-dir ./keys

Private key saved to: ./keys/private_key.hex
Public key saved to: ./keys/public_key.hex
```

`private_key.hex` is created with mode 0600, so it can be passed straight to
`--private-key-file`.

## generate-public-key

Derive the public key of the signing key.

```bash
node_cli generate-public-key [--private-key-file PATH] [--compressed]
```

The key is read from `--private-key-file` (env `FIREFLY_PRIVATE_KEY_FILE`, or a
profile's `private_key_file`), a file holding its hex that group and others
cannot read, else from `FIREFLY_PRIVATE_KEY`. It is never taken as a flag value.

```
$ node_cli generate-public-key --private-key-file ./keys/private_key.hex

Public key (uncompressed): 04ffc016579a68050d655d55df4e09f04605164543e257c8e6df10361e6068a5336588e9b355ea859c5ab4285a5ef0efdf62bc28b80320ce99e26bb1607b3ad93d
```

## generate-vault-address

Generate a vault address from a public key, or from the signing key.

```bash
node_cli generate-vault-address [--public-key KEY | --private-key-file PATH]
```

With `--public-key`, the address is derived from it. Otherwise the signing key
is read as for `generate-public-key`: `--private-key-file`, else
`FIREFLY_PRIVATE_KEY`.

```
$ node_cli generate-vault-address --private-key-file ./keys/private_key.hex

Public key: 04ffc016579a68050d655d55df4e09f04605164543e257c8e6df10361e6068a5336588e9b355ea859c5ab4285a5ef0efdf62bc28b80320ce99e26bb1607b3ad93d
Vault address: 1111AtahZeefej4tvVR6ti9TJtv8yxLebT31SCEVDCKMNikBk5r3g
```

Vault addresses start with `1111` and are derived from the public key via Keccak-256 + Blake2b + bs58 encoding.

## get-node-id

Extract node ID from a TLS certificate or private key file.

```bash
node_cli get-node-id [--cert-file FILE] [--key-file FILE] [--format hex|rnode-url]
```

| Flag | Description |
|------|-------------|
| `--cert-file` | TLS certificate file (recommended) |
| `--key-file` | TLS private key file |
| `--format` | `hex` (default) or `rnode-url` |
| `--host` | Host for rnode-url format |
| `--protocol-port` | Protocol port for rnode-url (default 40400) |
| `--discovery-port` | Discovery port for rnode-url (default 40404) |

```
$ node_cli get-node-id --cert-file node.certificate.pem

Node ID: 24f315807e49a51b6c5ae18553ddc14f60418db4
```

```
$ node_cli get-node-id --cert-file node.certificate.pem --format rnode-url --host mynode.com

Node ID: 24f315807e49a51b6c5ae18553ddc14f60418db4
RNode URL: rnode://24f315807e49a51b6c5ae18553ddc14f60418db4@mynode.com?protocol=40400&discovery=40404
```
