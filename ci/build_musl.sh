#!/bin/bash

set -ex

. "$(dirname "$0")/deploy_utils.sh"

export PROJECT_NAME=git-fixup
export TRAVIS_TAG="$(git describe)"
export TARGET=x86_64-unknown-linux-musl

# This script exists because building libssh/libz for musl is hard, so just use
# the existing docker probject to build it

global_musl_registry() {
    local reg="$HOME/.cargo/${TARGET}-registry"
    mkdir -p "$reg"
    echo "$reg"
}

docker_musl_build() {
    docker run \
        --rm -it \
        -v "$(pwd)":/home/rust/src \
        -v "$(global_musl_registry)":/home/rust/.cargo/registry \
        ekidd/rust-musl-builder \
        cargo build --release
}

main() {
    docker_musl_build
    echo "Packaging"
    mk_tarball
}

main
