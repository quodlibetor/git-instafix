#!/bin/bash

# build, test and generate docs in this phase

set -ex

. "$(dirname "$0")/utils.sh"

main() {
    # Test a normal debug build.
    cargo build --target "$TARGET" --verbose --all

    # sanity check the file type
    file target/"$TARGET"/debug/git-fixup

    # Check that we've generated man page and other shell completions.
    outdir="$(cargo_out_dir "target/$TARGET/debug")"
    file "$outdir/rg.bash"
    file "$outdir/rg.fish"
    file "$outdir/_rg.ps1"
    file "$outdir/rg.1"

    # Run tests
    cargo test --target "$TARGET" --verbose --all
}

main
