#!/bin/bash

# Various utility functions used through CI.

# Finds Cargo's `OUT_DIR` directory from the most recent build.
#
# This requires one parameter corresponding to the target directory
# to search for the build output.
cargo_out_dir() {
    # This works by finding the most recent stamp file, which is produced by
    # every build.
    target_dir="$1"
    find "$target_dir" -name git-fixup -print0 \
      | xargs -0 ls -t \
      | head -n1 \
      | xargs dirname
}

is_musl() {
    case "$TARGET" in
        *musl) return 0 ;;
        *)     return 1 ;;
    esac
}
