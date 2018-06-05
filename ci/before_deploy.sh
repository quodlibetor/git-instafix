#!/bin/bash

# package the build artifacts

set -ex

. "$(dirname "$0")/utils.sh"
. "$(dirname "$0")/deploy_utils.sh"

# Generate artifacts for release
mk_artifacts() {
    cargo build --target "$TARGET" --release
}

main() {
    mk_artifacts
    mk_tarball
}

main
