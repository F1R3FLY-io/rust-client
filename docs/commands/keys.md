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
| `--save` | false | Save keys to files |
| `--output-dir` | `.` | Directory for saved keys |

```
$ node_cli generate-key-pair

Private key: bd7aa3fa55596353c4f178c2079d50dd20f25534bb057be77a2f5b82f9a05d64
Public key (uncompressed): 046adcae2b3d8b22edf207351db32ad481aee1cc9987d2b2bcde9769a3fb52b60ca4da6008653dc056e9c5525405698e17764dcd86ef28ffee5b3fdc89e5d6f9bd
```

```
$ node_cli generate-key-pair --save --output-dir ./keys

Private key: a1b2c3...
Public key (uncompressed): 04d5e6f7...
Keys saved to: ./keys/private.key, ./keys/public.key
```

## generate-public-key

Derive public key from a private key.

```bash
node_cli generate-public-key [--private-key KEY] [--compressed]
```

```
$ node_cli generate-public-key

Public key (uncompressed): 04ffc016579a68050d655d55df4e09f04605164543e257c8e6df10361e6068a5336588e9b355ea859c5ab4285a5ef0efdf62bc28b80320ce99e26bb1607b3ad93d
```

## generate-vault-address

Generate a vault address from a key.

```bash
node_cli generate-vault-address [--private-key KEY] [--public-key KEY]
```

```
$ node_cli generate-vault-address

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
