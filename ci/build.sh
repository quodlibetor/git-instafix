#!/bin/bash

# build, test and generate docs in this phase

set -ex

. "$(dirname "$0")/utils.sh"

main() {
    # Test a normal debug build.
    cargo build --target "$TARGET" --verbose --all

    # sanity check the file exists
    file target/"$TARGET"/debug/git-fixup

    # Run tests
    cargo test --target "$TARGET" --verbose --all
}

main
