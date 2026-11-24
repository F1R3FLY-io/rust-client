#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
cargo test --release --locked --features integration --test live_queue --no-run
image=${FIREFLY_NODE_IMAGE:-$(<tests/integration/node-image.txt)}
queue=$(docker run --detach --network host --entrypoint /opt/docker/bin/deploy-queue \
    "$image" --node-http http://127.0.0.1:30583 --node-grpc http://127.0.0.1:30581 \
    --http-listen 127.0.0.1:30573 --grpc-listen 127.0.0.1:30571 --data-dir /tmp/queue)
trap 'docker rm -f -v "$queue" >/dev/null' EXIT
cargo test --release --locked --features integration --test live_queue || {
    docker logs "$queue"
    exit 1
}
