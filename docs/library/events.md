# WebSocket Events

Real-time deploy finalization notifications via the node's `/ws/events` WebSocket endpoint.

## Usage

```rust
use node_cli::NodeEvents;
use std::time::Duration;

// Connect to node's WebSocket
let events = NodeEvents::connect("ws://localhost:40403");

// Wait for a specific deploy to be finalized
let deploy_id = "3044022075e51b8f...";
match events.wait_for_deploy(deploy_id, Duration::from_secs(60)).await {
    Some(event) => {
        println!("Finalized! Cost: {}, Errored: {}", event.cost, event.errored);
    }
    None => {
        println!("Timeout — deploy not finalized within 60s");
    }
}

// Non-blocking check
if let Some(event) = events.is_finalized(deploy_id).await {
    println!("Already finalized: {:?}", event);
}
```

## DeployEvent

```rust
pub struct DeployEvent {
    pub deploy_id: String,
    pub cost: u64,
    pub errored: bool,
    pub deployer: String,
}
```

## How it works

1. Connects to `ws://{host}/ws/events`
2. Receives JSON events from the node in envelope format
3. Unwraps envelope and deserializes using `f1r3fly_shared::F1r3flyEvent` (type-safe)
4. Tracks `block-finalised` events and their deploy lists
5. Notifies waiters when their deploy ID appears in a finalized block
6. Auto-reconnects on disconnect (5s retry interval)

## When to use

- **Instead of polling `is_finalized`**: WebSocket is push-based, no polling overhead
- **Monitoring**: track all finalized deploys in real-time
- **Integration**: build reactive systems that respond to finalization

## Event format

The node sends events as:
```json
{
  "event": "block-finalised",
  "schema-version": 1,
  "payload": {
    "block-hash": "abc123...",
    "deploys": [
      {"id": "3044...", "cost": 316, "deployer": "04ff...", "errored": false}
    ],
    "creator": "04ff...",
    "seq-num": 42
  }
}
```

The events module unwraps the envelope before deserializing, matching the approach used by the embers client.
