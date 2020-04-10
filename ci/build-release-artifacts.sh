#!/bin/bash

# package the build artifacts

set -xeuo pipefail

. "$(dirname "$0")/utils.sh"
. "$(dirname "$0")/deploy_utils.sh"

# Generate artifacts for release
mk_artifacts() {
    cargo build --target "$TARGET" --release
}

main() {
    echo "env: $(env)" >&2
    export RELEASE
    RELEASE=$(basename "$REF")
    mk_artifacts
    mk_tarball
}

main
