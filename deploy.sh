#!/bin/sh

set -o xtrace
set -o errexit
set -o nounset

HOST="$1"

cargo build --release --target armv7-unknown-linux-musleabihf
scp target/armv7-unknown-linux-musleabihf/release/strudel "$HOST":
scp ext/strudel.service "$HOST":
