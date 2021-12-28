#!/bin/sh

set -o xtrace
set -o errexit
set -o nounset

HOST="$1"

cargo build --target armv7-unknown-linux-musleabihf
scp target/armv7-unknown-linux-musleabihf/debug/pitemp "$HOST":
