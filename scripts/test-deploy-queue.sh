#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
cargo test --release --locked --features integration --test live_queue --no-run
image=f1r3flyindustries/f1r3fly-rust@sha256:09f03d3dfef8531c02be1acc8881dee723a814b1af14a9345e604bac169cfd6a
queue=$(docker run --detach --network host --entrypoint /opt/docker/bin/deploy-queue \
    "$image" --node-http http://127.0.0.1:40583 --node-grpc http://127.0.0.1:40581 \
    --http-listen 127.0.0.1:40573 --grpc-listen 127.0.0.1:40571 --data-dir /tmp/queue)
trap 'docker rm -f -v "$queue" >/dev/null' EXIT
cargo test --release --locked --features integration --test live_queue || {
    docker logs "$queue"
    exit 1
}
